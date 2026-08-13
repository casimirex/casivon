//! Sales documents posting themselves to the general ledger.
//!
//! Until this existed, the ledger only ever held what somebody typed into the
//! journal form: a business could invoice all year and its profit and loss would
//! be blank. These tests pin what an invoice and a payment now do to the books,
//! and — just as important — that an installation which has not configured
//! posting is left exactly as it was.

mod common;

use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;

/// Creates every account a complete mapping needs and maps them.
///
/// Returns the account ids by role, in the order of [`ROLES`].
async fn configure_posting(app: &TestApp) -> Vec<String> {
    let ids = create_posting_accounts(app).await;
    let response = app.put("/accounting/posting-accounts", mapping_body(&ids)).await;

    assert!(response.status.is_success(), "mapping failed: {}", response.body);
    assert_eq!(response.field("posting_enabled"), "true");

    ids
}

/// The ten accounts a complete mapping needs, in [`ROLES`] order.
pub async fn create_posting_accounts(app: &TestApp) -> Vec<String> {
    let mut ids = Vec::new();
    for (code, name, account_type) in ROLES {
        ids.push(
            app.create(
                "/accounting/accounts",
                json!({ "account_code": code, "account_name": name, "account_type": account_type }),
            )
            .await,
        );
    }
    ids
}

pub const ROLES: [(&str, &str, &str); 10] = [
    ("1100", "Accounts receivable", "asset"),
    ("1000", "Bank", "asset"),
    ("4000", "Sales revenue", "revenue"),
    ("2100", "Tax payable", "liability"),
    ("4900", "Foreign exchange gain/loss", "revenue"),
    ("2000", "Accounts payable", "liability"),
    ("5000", "Cost of sales", "expense"),
    ("1300", "Purchase tax", "asset"),
    ("2200", "Employee payable", "liability"),
    ("5100", "Employee expense", "expense"),
];

pub fn mapping_body(ids: &[String]) -> Value {
    json!({
        "ar_account_id": ids[0], "bank_account_id": ids[1],
        "sales_revenue_account_id": ids[2], "tax_payable_account_id": ids[3],
        "fx_gain_loss_account_id": ids[4], "accounts_payable_account_id": ids[5],
        "cost_of_sales_account_id": ids[6], "purchase_tax_account_id": ids[7],
        "employee_payable_account_id": ids[8], "employee_expense_account_id": ids[9]
    })
}

fn quote_body(customer: &str, unit_price: f64, tax_rate: f64) -> Value {
    json!({
        "customer_id": customer,
        "issue_date": "2026-03-01",
        "expiry_date": "2026-12-31",
        "lines": [{
            "description": "Widget", "quantity": 1,
            "unit_price": unit_price, "tax_rate": tax_rate
        }]
    })
}

/// Quote -> order -> invoice, left in `draft`.
async fn draft_invoice(app: &TestApp, customer: &str, unit_price: f64, tax_rate: f64) -> String {
    let quote = app.create("/sales/quotes", quote_body(customer, unit_price, tax_rate)).await;
    app.advance(&format!("/sales/quotes/{quote}/status"), &["sent", "accepted"]).await;
    let order = app
        .post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({ "order_date": "2026-03-01" }))
        .await
        .id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed"]).await;

    app.post(
        &format!("/sales/orders/{order}/convert-to-invoice"),
        json!({ "issue_date": "2026-03-01", "payment_terms_days": 30 }),
    )
    .await
    .id()
}

/// Every ledger entry, newest last.
async fn entries(app: &TestApp) -> Vec<Value> {
    app.get("/accounting/ledger-entries?per_page=100").await.rows().clone()
}

/// Net movement on an account across the whole ledger, debit-positive.
async fn net(app: &TestApp, account: &str) -> f64 {
    entries(app)
        .await
        .iter()
        .map(|entry| {
            let amount: f64 = entry["amount"].as_str().unwrap().parse().unwrap();
            match (entry["debit_account_id"].as_str(), entry["credit_account_id"].as_str()) {
                (Some(d), _) if d == account => amount,
                (_, Some(c)) if c == account => -amount,
                _ => 0.0,
            }
        })
        .sum()
}

#[sqlx::test]
async fn an_unconfigured_installation_posts_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    // No mapping has been set, so this is every installation that existed
    // before automatic posting did.
    let invoice = draft_invoice(&app, &customer, 1000.0, 20.0).await;
    let sent = app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    // The sale still works end to end; it simply does not reach the books.
    assert!(sent.status.is_success(), "{}", sent.body);
    assert_eq!(sent.field("status"), "sent");
    assert!(entries(&app).await.is_empty(), "an unmapped installation posted something");

    let status = app.get("/accounting/posting-accounts").await;
    assert_eq!(status.field("posting_enabled"), "false");
    assert_eq!(status.data()["missing_roles"].as_array().unwrap().len(), ROLES.len());
}

#[sqlx::test]
async fn a_mapping_is_refused_unless_each_account_can_do_its_job(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;

    // Revenue mapped to the receivable account: the type is wrong, and posting
    // to it would put sales into an asset.
    let mut wrong = mapping_body(&ids);
    wrong["sales_revenue_account_id"] = json!(ids[0]);
    let wrong_type = app.put("/accounting/posting-accounts", wrong).await;
    assert_eq!(wrong_type.status, 422);
    assert!(wrong_type.error_message().contains("revenue"), "{}", wrong_type.error_message());

    // An account denominated in something other than the base currency cannot
    // take an automatic posting, because those are made in the base currency.
    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": "1.10" })).await;
    let eur_account = app
        .create(
            "/accounting/accounts",
            json!({ "account_code": "1200", "account_name": "EUR bank", "account_type": "asset", "currency": "EUR" }),
        )
        .await;

    let mut foreign = mapping_body(&ids);
    foreign["bank_account_id"] = json!(eur_account);
    let wrong_currency = app.put("/accounting/posting-accounts", foreign).await;
    assert_eq!(wrong_currency.status, 422);
    let message = wrong_currency.error_message();
    assert!(message.contains("EUR") && message.contains("USD"), "{message}");
}

#[sqlx::test]
async fn issuing_an_invoice_books_the_revenue_and_the_receivable(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let customer = app.customer().await;

    let invoice = draft_invoice(&app, &customer, 1000.0, 20.0).await;

    // A draft has been raised but not issued: nothing is earned or owed yet.
    assert!(entries(&app).await.is_empty(), "a draft invoice posted");

    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    let posted = entries(&app).await;
    assert_eq!(posted.len(), 2, "expected a revenue leg and a tax leg");
    assert_eq!(net(&app, &ids[0]).await, 1200.0, "receivable");
    assert_eq!(net(&app, &ids[2]).await, -1000.0, "revenue");
    assert_eq!(net(&app, &ids[3]).await, -200.0, "tax payable");

    // Every entry traces back to the document that caused it.
    assert!(posted.iter().all(|e| e["reference_type"] == "sales_invoice"));
    assert!(posted.iter().all(|e| e["reference_id"] == invoice.as_str()));
}

#[sqlx::test]
async fn a_zero_rated_invoice_posts_a_single_leg(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let customer = app.customer().await;

    let invoice = draft_invoice(&app, &customer, 500.0, 0.0).await;
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    assert_eq!(entries(&app).await.len(), 1);
    assert_eq!(net(&app, &ids[0]).await, 500.0);
    assert_eq!(net(&app, &ids[3]).await, 0.0, "no tax leg for a zero-rated invoice");
}

#[sqlx::test]
async fn a_foreign_invoice_posts_in_the_base_currency(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let customer = app.customer().await;

    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": "1.10" })).await;

    let quote = app
        .create(
            "/sales/quotes",
            json!({
                "customer_id": customer, "currency": "EUR",
                "issue_date": "2026-03-01", "expiry_date": "2026-12-31",
                "lines": [{ "description": "Widget", "quantity": 1, "unit_price": 1000.00, "tax_rate": 20.0 }]
            }),
        )
        .await;
    app.advance(&format!("/sales/quotes/{quote}/status"), &["sent", "accepted"]).await;
    let order = app
        .post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({ "order_date": "2026-03-01" }))
        .await
        .id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed"]).await;
    let invoice = app
        .post(
            &format!("/sales/orders/{order}/convert-to-invoice"),
            json!({ "issue_date": "2026-03-01", "payment_terms_days": 30 }),
        )
        .await
        .id();

    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    // EUR 1,200 at 1.10. The books are kept in USD, so that is what they show.
    assert_eq!(net(&app, &ids[0]).await, 1320.0, "receivable");
    assert_eq!(net(&app, &ids[2]).await, -1100.0, "revenue");
    assert_eq!(net(&app, &ids[3]).await, -220.0, "tax payable");
    assert!(entries(&app).await.iter().all(|e| e["currency"] == "USD"));
}

#[sqlx::test]
async fn settling_at_a_moved_rate_recognises_the_difference(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let customer = app.customer().await;

    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": "1.10" })).await;

    let quote = app
        .create(
            "/sales/quotes",
            json!({
                "customer_id": customer, "currency": "EUR",
                "issue_date": "2026-03-01", "expiry_date": "2026-12-31",
                "lines": [{ "description": "Widget", "quantity": 1, "unit_price": 1000.00, "tax_rate": 0.0 }]
            }),
        )
        .await;
    app.advance(&format!("/sales/quotes/{quote}/status"), &["sent", "accepted"]).await;
    let order = app
        .post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({ "order_date": "2026-03-01" }))
        .await
        .id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed"]).await;
    let invoice = app
        .post(
            &format!("/sales/orders/{order}/convert-to-invoice"),
            json!({ "issue_date": "2026-03-01", "payment_terms_days": 60 }),
        )
        .await
        .id();
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    // The euro strengthens, then the customer pays in full.
    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-04-01", "rate": "1.15" })).await;
    app.post(
        "/sales/payments",
        json!({
            "invoice_id": invoice, "amount": 1000.00,
            "payment_method": "bank_transfer", "payment_date": "2026-04-10"
        }),
    )
    .await;

    assert_eq!(net(&app, &ids[1]).await, 1150.0, "the bank received what the money was worth");
    // The receivable clears at the rate it was raised at, leaving nothing owing.
    assert_eq!(net(&app, &ids[0]).await, 0.0, "receivable did not clear");
    assert_eq!(net(&app, &ids[4]).await, -50.0, "the gain was not recognised");

    // And the P&L now shows the sale, which is the whole point.
    let pl = app.get("/accounting/reports/profit-and-loss?date_from=2026-01-01&date_to=2026-12-31").await;
    pl.assert_money("total_revenue", "1150.00");
}

#[sqlx::test]
async fn cancelling_and_reversing_post_mirrors_rather_than_deleting(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let customer = app.customer().await;

    let invoice = draft_invoice(&app, &customer, 1000.0, 20.0).await;
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    let payment = app
        .post(
            "/sales/payments",
            json!({
                "invoice_id": invoice, "amount": 200.00,
                "payment_method": "cash", "payment_date": "2026-03-10"
            }),
        )
        .await
        .id();
    let after_payment = entries(&app).await.len();

    app.delete(&format!("/sales/payments/{payment}")).await;

    // The reversal is added, not substituted: the ledger still shows that money
    // arrived and went back, which is what an audit trail is for.
    assert!(entries(&app).await.len() > after_payment, "reversal deleted rather than posted");
    assert_eq!(net(&app, &ids[1]).await, 0.0, "bank did not return to zero");
    assert_eq!(net(&app, &ids[0]).await, 1200.0, "the whole invoice is owing again");

    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "cancelled" })).await;

    // Cancelling unwinds the issue, so every account it touched is back where it
    // started — without a single row having been removed.
    for account in [&ids[0], &ids[2], &ids[3]] {
        assert_eq!(net(&app, account).await, 0.0, "account {account} did not unwind");
    }
}

#[sqlx::test]
async fn a_posted_entry_cannot_be_deleted_by_hand(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure_posting(&app).await;
    let customer = app.customer().await;

    let invoice = draft_invoice(&app, &customer, 100.0, 0.0).await;
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    let entry = entries(&app).await[0]["id"].as_str().unwrap().to_string();
    let refused = app.delete(&format!("/accounting/ledger-entries/{entry}")).await;

    // Deleting it would leave the invoice believing it is posted while the books
    // disagreed, and its posting key would stop a repost ever putting it back.
    assert_eq!(refused.status, 409);
    assert!(refused.error_message().contains("sales invoice"), "{}", refused.error_message());

    // A manual entry is still deletable: the guard is about provenance, not
    // about locking the ledger.
    let accounts = app.get("/accounting/accounts?per_page=100").await;
    let (a, b) = (accounts.rows()[0]["id"].as_str().unwrap(), accounts.rows()[1]["id"].as_str().unwrap());
    let manual = app
        .create(
            "/accounting/ledger-entries",
            json!({
                "entry_date": "2026-03-01", "description": "Adjustment",
                "debit_account_id": a, "credit_account_id": b, "amount": 10
            }),
        )
        .await;
    assert!(app.delete(&format!("/accounting/ledger-entries/{manual}")).await.status.is_success());
}

#[sqlx::test]
async fn documents_raised_before_posting_was_configured_are_reported_and_repairable(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    // Invoiced and settled while posting was off — the state every existing
    // installation upgrades from.
    let invoice = draft_invoice(&app, &customer, 1000.0, 20.0).await;
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;
    app.post(
        "/sales/payments",
        json!({
            "invoice_id": invoice, "amount": 400.00,
            "payment_method": "cash", "payment_date": "2026-03-10"
        }),
    )
    .await;
    assert!(entries(&app).await.is_empty());

    let ids = configure_posting(&app).await;

    let outstanding = app.get("/accounting/unposted").await;
    assert_eq!(outstanding.data()["documents"].as_array().unwrap().len(), 2);
    assert_eq!(outstanding.field("posting_enabled"), "true");

    let run = app.post("/accounting/post-unposted", json!({})).await;
    assert!(run.status.is_success(), "{}", run.body);
    assert_eq!(run.field("invoices_posted"), "1");
    assert_eq!(run.field("payments_posted"), "1");

    assert_eq!(net(&app, &ids[0]).await, 800.0, "receivable net of the payment");
    assert_eq!(net(&app, &ids[2]).await, -1000.0, "revenue");
    assert_eq!(net(&app, &ids[1]).await, 400.0, "bank");

    // Nothing is owed to the ledger any more.
    assert!(app.get("/accounting/unposted").await.data()["documents"].as_array().unwrap().is_empty());
}

#[sqlx::test]
async fn repairing_twice_does_not_post_twice(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    let invoice = draft_invoice(&app, &customer, 1000.0, 20.0).await;
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    let ids = configure_posting(&app).await;
    app.post("/accounting/post-unposted", json!({})).await;

    let after_first = entries(&app).await.len();
    let receivable = net(&app, &ids[0]).await;

    // The second run finds nothing outstanding; even if it did, the posting key
    // would stop the entries being written a second time.
    let second = app.post("/accounting/post-unposted", json!({})).await;
    assert!(second.status.is_success());
    assert_eq!(second.field("invoices_posted"), "0");

    assert_eq!(entries(&app).await.len(), after_first, "entries were doubled");
    assert_eq!(net(&app, &ids[0]).await, receivable, "the receivable was doubled");
}

#[sqlx::test]
async fn repairing_is_refused_while_posting_is_unconfigured(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // Reporting success with nowhere to post to would be a lie.
    let response = app.post("/accounting/post-unposted", json!({})).await;
    assert_eq!(response.status, 422);
    assert!(response.error_message().contains("not configured"), "{}", response.error_message());
}

#[sqlx::test]
async fn the_trial_balance_still_balances_after_a_full_cycle(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure_posting(&app).await;
    let customer = app.customer().await;

    let invoice = draft_invoice(&app, &customer, 1000.0, 20.0).await;
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;
    app.post(
        "/sales/payments",
        json!({
            "invoice_id": invoice, "amount": 1200.00,
            "payment_method": "bank_transfer", "payment_date": "2026-03-15"
        }),
    )
    .await;

    let trial = app.get("/accounting/reports/trial-balance?date_from=2026-01-01&date_to=2026-12-31").await;
    assert_eq!(trial.field("is_balanced"), "true", "{}", trial.body);
    assert_eq!(trial.field("total_debits"), trial.field("total_credits"));

    // The invoice is settled, and the ledger agrees.
    assert_eq!(app.get(&format!("/sales/invoices/{invoice}")).await.field("status"), "paid");
}
