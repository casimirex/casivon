//! Double-entry bookkeeping: every entry moves two accounts, and the reports
//! are derived from those movements rather than stored separately.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

struct Chart {
    cash: String,
    revenue: String,
    expense: String,
}

async fn chart_of_accounts(app: &TestApp) -> Chart {
    let account = |code: &str, name: &str, kind: &str| {
        json!({ "account_code": code, "account_name": name, "account_type": kind })
    };
    Chart {
        cash: app.create("/accounting/accounts", account("1000", "Cash", "asset")).await,
        revenue: app.create("/accounting/accounts", account("4000", "Sales Revenue", "revenue")).await,
        expense: app.create("/accounting/accounts", account("5000", "Cost of Sales", "expense")).await,
    }
}

#[sqlx::test]
async fn an_entry_cannot_debit_and_credit_the_same_account(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let chart = chart_of_accounts(&app).await;

    let response = app
        .post(
            "/accounting/ledger-entries",
            json!({
                "entry_date": "2026-08-01",
                "description": "bad",
                "debit_account_id": chart.cash,
                "credit_account_id": chart.cash,
                "amount": 10
            }),
        )
        .await;

    assert!(!response.status.is_success());
}

#[sqlx::test]
async fn entries_move_both_accounts_by_their_normal_balance(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let chart = chart_of_accounts(&app).await;

    // Sale: cash (asset, debit-normal) up 1080, revenue (credit-normal) up 1080.
    app.post(
        "/accounting/ledger-entries",
        json!({
            "entry_date": "2026-08-01",
            "description": "Widget sale",
            "debit_account_id": chart.cash,
            "credit_account_id": chart.revenue,
            "amount": 1080.00
        }),
    )
    .await;

    // Cost: expense up 225, cash down 225.
    app.post(
        "/accounting/ledger-entries",
        json!({
            "entry_date": "2026-08-02",
            "description": "Widget cost",
            "debit_account_id": chart.expense,
            "credit_account_id": chart.cash,
            "amount": 225.00
        }),
    )
    .await;

    app.get(&format!("/accounting/accounts/{}", chart.cash))
        .await
        .assert_money("current_balance", "855.00");
    app.get(&format!("/accounting/accounts/{}", chart.revenue))
        .await
        .assert_money("current_balance", "1080.00");
    app.get(&format!("/accounting/accounts/{}", chart.expense))
        .await
        .assert_money("current_balance", "225.00");
}

#[sqlx::test]
async fn the_trial_balance_balances_and_the_pl_follows_from_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let chart = chart_of_accounts(&app).await;
    for (date, description, debit, credit, amount) in [
        ("2026-08-01", "Widget sale", &chart.cash, &chart.revenue, 1080.00),
        ("2026-08-02", "Widget cost", &chart.expense, &chart.cash, 225.00),
    ] {
        app.post(
            "/accounting/ledger-entries",
            json!({
                "entry_date": date,
                "description": description,
                "debit_account_id": debit,
                "credit_account_id": credit,
                "amount": amount
            }),
        )
        .await;
    }

    let trial_balance = app.get("/accounting/reports/trial-balance").await;
    assert_eq!(trial_balance.data()["is_balanced"], true);
    let debits = trial_balance.field("total_debits");
    assert_eq!(debits, trial_balance.field("total_credits"), "trial balance does not balance");

    let profit_and_loss = app.get("/accounting/reports/profit-and-loss").await;
    profit_and_loss.assert_money("total_revenue", "1080.00");
    profit_and_loss.assert_money("total_expenses", "225.00");
    profit_and_loss.assert_money("net_profit", "855.00");
}

#[sqlx::test]
async fn deleting_an_entry_backs_both_accounts_out(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let chart = chart_of_accounts(&app).await;
    let entry = app
        .create(
            "/accounting/ledger-entries",
            json!({
                "entry_date": "2026-08-01",
                "description": "Widget sale",
                "debit_account_id": chart.cash,
                "credit_account_id": chart.revenue,
                "amount": 1080.00
            }),
        )
        .await;

    app.delete(&format!("/accounting/ledger-entries/{entry}")).await;

    app.get(&format!("/accounting/accounts/{}", chart.cash))
        .await
        .assert_money("current_balance", "0");
    app.get(&format!("/accounting/accounts/{}", chart.revenue))
        .await
        .assert_money("current_balance", "0");
    assert_eq!(app.get("/accounting/reports/trial-balance").await.data()["is_balanced"], true);
}

#[sqlx::test]
async fn recalculating_reproduces_the_running_balances(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let chart = chart_of_accounts(&app).await;
    app.post(
        "/accounting/ledger-entries",
        json!({
            "entry_date": "2026-08-01",
            "description": "Widget sale",
            "debit_account_id": chart.cash,
            "credit_account_id": chart.revenue,
            "amount": 1080.00
        }),
    )
    .await;

    // Balances are maintained incrementally as entries are posted; the rebuild
    // recomputes them from the ledger and must agree.
    let rebuilt = app.post("/accounting/accounts/recalculate", json!({})).await;
    assert!(rebuilt.status.is_success());

    app.get(&format!("/accounting/accounts/{}", chart.cash))
        .await
        .assert_money("current_balance", "1080.00");
}

#[sqlx::test]
async fn a_tax_rate_is_a_whole_percentage(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // The same convention as `tax_rate` on a document line: 20 means 20%.
    let percentage = app
        .post("/accounting/tax-rates", json!({ "name": "VAT", "rate": 20, "tax_type": "vat" }))
        .await;
    assert!(percentage.status.is_success(), "{}", percentage.body);
    percentage.assert_money("rate", "20");

    assert!(app
        .post("/accounting/tax-rates", json!({ "name": "Reduced", "rate": 17.5, "tax_type": "vat" }))
        .await
        .status
        .is_success());

    // Past 100% is a typo, not a tax.
    let absurd = app
        .post("/accounting/tax-rates", json!({ "name": "Oops", "rate": 2000, "tax_type": "vat" }))
        .await;
    assert_eq!(absurd.status, 422);
    assert!(absurd.error_message().contains("percentage"), "{}", absurd.error_message());

    let negative = app
        .post("/accounting/tax-rates", json!({ "name": "Oops", "rate": -1, "tax_type": "vat" }))
        .await;
    assert_eq!(negative.status, 422);
}

#[sqlx::test]
async fn an_account_code_is_unique(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let account = json!({ "account_code": "1000", "account_name": "Cash", "account_type": "asset" });
    app.post("/accounting/accounts", account.clone()).await;

    let duplicate = app.post("/accounting/accounts", account).await;

    assert!(!duplicate.status.is_success());
}

#[sqlx::test]
async fn the_books_are_closed_to_users_without_an_accounting_role(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let bob = app.register("bob@erp.test", "supersecret1", "Bob", "Clerk").await;
    let token = bob.field("access_token");

    // Every one of these was readable by any signed-in user: the role check was
    // only ever on the writes, while the frontend's `RoleRoute` and the OpenAPI
    // tag both claimed the whole module was restricted.
    for path in [
        "/accounting/accounts",
        "/accounting/accounts/tree",
        "/accounting/ledger-entries",
        "/accounting/bank-accounts",
        "/accounting/tax-rates",
        "/accounting/reports/trial-balance",
        "/accounting/reports/profit-and-loss",
        "/accounting/reports/balance-sheet",
    ] {
        assert_eq!(app.get_as(&token, path).await.status, 403, "{path} was readable");
    }

    // The bootstrap admin still reads them all.
    assert!(app.get("/accounting/reports/trial-balance").await.status.is_success());
}
