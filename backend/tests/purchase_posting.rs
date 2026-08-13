//! The spending side of the books.
//!
//! Sales posting made the profit and loss actively misleading: it reported
//! revenue with no cost against it, so every sale looked like pure profit. These
//! tests pin what buying, receiving and paying now do to the ledger, and end on
//! the one that matters — a P&L with both sides on it.

mod common;

use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;

/// Role order matches `POSTING_ROLES` on the backend.
const ROLES: [(&str, &str, &str); 10] = [
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

const AR: usize = 0;
const BANK: usize = 1;
const REVENUE: usize = 2;
const FX: usize = 4;
const AP: usize = 5;
const COST: usize = 6;
const PURCHASE_TAX: usize = 7;

async fn configure_posting(app: &TestApp) -> Vec<String> {
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

    let response = app
        .put(
            "/accounting/posting-accounts",
            json!({
                "ar_account_id": ids[0], "bank_account_id": ids[1],
                "sales_revenue_account_id": ids[2], "tax_payable_account_id": ids[3],
                "fx_gain_loss_account_id": ids[4], "accounts_payable_account_id": ids[5],
                "cost_of_sales_account_id": ids[6], "purchase_tax_account_id": ids[7],
                "employee_payable_account_id": ids[8], "employee_expense_account_id": ids[9]
            }),
        )
        .await;
    assert!(response.status.is_success(), "mapping failed: {}", response.body);
    ids
}

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

/// A confirmed order for `quantity` at `unit_price`, ready to receive against.
/// Returns (po_id, po_line_id).
async fn confirmed_order(
    app: &TestApp,
    quantity: i32,
    unit_price: f64,
    tax_rate: f64,
    currency: Option<&str>,
) -> (String, String) {
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme Supplies" })).await;

    let mut body = json!({
        "vendor_id": vendor,
        "order_date": "2026-03-01",
        "lines": [{
            "description": "Steel", "quantity": quantity,
            "unit_price": unit_price, "tax_rate": tax_rate
        }]
    });
    if let Some(currency) = currency {
        body["currency"] = json!(currency);
    }

    let created = app.post("/purchasing/purchase-orders", body).await;
    assert!(created.status.is_success(), "{}", created.body);
    let po = created.id();
    let line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();

    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;
    (po, line)
}

async fn receive(app: &TestApp, po: &str, line: &str, quantity: i32, warehouse: &str) -> Value {
    let response = app
        .post(
            "/purchasing/goods-receipts",
            json!({
                "po_id": po, "warehouse_id": warehouse, "receipt_date": "2026-03-05",
                "lines": [{ "po_line_id": line, "quantity_received": quantity }]
            }),
        )
        .await;
    assert!(response.status.is_success(), "receipt failed: {}", response.body);
    response.data().clone()
}

#[sqlx::test]
async fn receiving_goods_incurs_the_cost_and_owes_the_supplier(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;

    let (po, line) = confirmed_order(&app, 10, 100.0, 20.0, None).await;

    // Ordering commits to nothing: the goods have not arrived.
    assert!(entries(&app).await.is_empty(), "a purchase order posted");

    receive(&app, &po, &line, 10, &warehouse).await;

    assert_eq!(net(&app, &ids[COST]).await, 1000.0, "cost of sales");
    assert_eq!(net(&app, &ids[PURCHASE_TAX]).await, 200.0, "recoverable input tax");
    assert_eq!(net(&app, &ids[AP]).await, -1200.0, "owed to the supplier");
}

#[sqlx::test]
async fn a_partial_receipt_posts_only_what_arrived(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;

    let (po, line) = confirmed_order(&app, 10, 100.0, 0.0, None).await;

    receive(&app, &po, &line, 4, &warehouse).await;
    assert_eq!(net(&app, &ids[COST]).await, 400.0, "only the four that arrived");
    assert_eq!(net(&app, &ids[AP]).await, -400.0);

    // The rest arrives later and posts the remainder, leaving the payable at the
    // order total — without either receipt having to know about the other.
    receive(&app, &po, &line, 6, &warehouse).await;
    assert_eq!(net(&app, &ids[COST]).await, 1000.0);
    assert_eq!(net(&app, &ids[AP]).await, -1000.0);
}

#[sqlx::test]
async fn a_foreign_order_receives_at_the_rate_it_was_struck_at(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;

    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": "1.10" })).await;
    let (po, line) = confirmed_order(&app, 10, 100.0, 0.0, Some("EUR")).await;

    // The rate moves before the goods arrive. The delivery is still worth what
    // the order committed to.
    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-03-03", "rate": "1.50" })).await;
    receive(&app, &po, &line, 10, &warehouse).await;

    assert_eq!(net(&app, &ids[COST]).await, 1100.0, "valued at the order's rate, not the day's");
    assert_eq!(net(&app, &ids[AP]).await, -1100.0);
}

#[sqlx::test]
async fn paying_a_vendor_clears_the_payable(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;

    let (po, line) = confirmed_order(&app, 10, 100.0, 0.0, None).await;
    receive(&app, &po, &line, 10, &warehouse).await;

    let payment = app
        .post(
            "/purchasing/vendor-payments",
            json!({
                "po_id": po, "amount": 1000.00,
                "payment_method": "bank_transfer", "payment_date": "2026-03-20"
            }),
        )
        .await;
    assert!(payment.status.is_success(), "{}", payment.body);

    assert_eq!(net(&app, &ids[BANK]).await, -1000.0, "money left the bank");
    assert_eq!(net(&app, &ids[AP]).await, 0.0, "the supplier is owed nothing");

    let order = app.get(&format!("/purchasing/purchase-orders/{po}")).await;
    order.assert_money("amount_paid", "1000.00");
    order.assert_money("amount_due", "0");
}

#[sqlx::test]
async fn a_vendor_payment_cannot_exceed_what_is_owed(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure_posting(&app).await;

    let (po, _) = confirmed_order(&app, 10, 100.0, 0.0, None).await;

    let response = app
        .post(
            "/purchasing/vendor-payments",
            json!({
                "po_id": po, "amount": 9999.00,
                "payment_method": "cash", "payment_date": "2026-03-20"
            }),
        )
        .await;

    assert_eq!(response.status, 422);
    assert!(response.error_message().contains("exceed"), "{}", response.error_message());
}

#[sqlx::test]
async fn settling_a_payable_cheaply_is_a_gain_and_still_clears_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;

    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": "1.10" })).await;
    let (po, line) = confirmed_order(&app, 10, 100.0, 0.0, Some("EUR")).await;
    receive(&app, &po, &line, 10, &warehouse).await;

    // The euro weakens before the invoice is settled, so the debt costs less.
    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-03-15", "rate": "1.05" })).await;

    let payment = app
        .post(
            "/purchasing/vendor-payments",
            json!({
                "po_id": po, "amount": 1000.00,
                "payment_method": "bank_transfer", "payment_date": "2026-03-20"
            }),
        )
        .await;
    payment.assert_money("base_amount", "1050.00");
    payment.assert_money("fx_gain_loss", "50.00");

    assert_eq!(net(&app, &ids[BANK]).await, -1050.0, "only 1,050 actually left");
    // The payable clears at what the order booked, not at what was paid.
    assert_eq!(net(&app, &ids[AP]).await, 0.0);
    assert_eq!(net(&app, &ids[FX]).await, -50.0, "the saving is a gain");
}

#[sqlx::test]
async fn reversing_a_vendor_payment_puts_the_debt_back(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;

    let (po, line) = confirmed_order(&app, 10, 100.0, 0.0, None).await;
    receive(&app, &po, &line, 10, &warehouse).await;

    let payment = app
        .post(
            "/purchasing/vendor-payments",
            json!({
                "po_id": po, "amount": 400.00,
                "payment_method": "cash", "payment_date": "2026-03-20"
            }),
        )
        .await
        .id();

    let before = entries(&app).await.len();
    app.delete(&format!("/purchasing/vendor-payments/{payment}")).await;

    // Added, not substituted: the ledger still shows the money went and came back.
    assert!(entries(&app).await.len() > before, "reversal deleted rather than posted");
    assert_eq!(net(&app, &ids[BANK]).await, 0.0);
    assert_eq!(net(&app, &ids[AP]).await, -1000.0, "the whole debt is owed again");

    let order = app.get(&format!("/purchasing/purchase-orders/{po}")).await;
    order.assert_money("amount_paid", "0");
}

#[sqlx::test]
async fn receipts_and_payments_made_before_posting_was_configured_are_repairable(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let warehouse = app.warehouse("MAIN", "Main").await;

    // Bought, received and paid while posting was off.
    let (po, line) = confirmed_order(&app, 10, 100.0, 20.0, None).await;
    receive(&app, &po, &line, 10, &warehouse).await;
    app.post(
        "/purchasing/vendor-payments",
        json!({
            "po_id": po, "amount": 500.00,
            "payment_method": "cash", "payment_date": "2026-03-20"
        }),
    )
    .await;
    assert!(entries(&app).await.is_empty());

    let ids = configure_posting(&app).await;

    let outstanding = app.get("/accounting/unposted").await;
    let kinds: Vec<&str> = outstanding.data()["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"goods_receipt"), "{kinds:?}");
    assert!(kinds.contains(&"vendor_payment"), "{kinds:?}");

    let run = app.post("/accounting/post-unposted", json!({})).await;
    assert_eq!(run.field("receipts_posted"), "1");
    assert_eq!(run.field("vendor_payments_posted"), "1");

    // A repaired receipt values the same as a live one would have.
    assert_eq!(net(&app, &ids[COST]).await, 1000.0);
    assert_eq!(net(&app, &ids[PURCHASE_TAX]).await, 200.0);
    assert_eq!(net(&app, &ids[AP]).await, -700.0, "1,200 owed less 500 paid");
    assert_eq!(net(&app, &ids[BANK]).await, -500.0);

    // And running it again changes nothing.
    let again = app.post("/accounting/post-unposted", json!({})).await;
    assert_eq!(again.field("receipts_posted"), "0");
    assert_eq!(again.field("vendor_payments_posted"), "0");
    assert_eq!(net(&app, &ids[COST]).await, 1000.0, "a second run doubled the cost");
}

#[sqlx::test]
async fn the_profit_and_loss_finally_has_both_sides(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;

    // Buy for 600, sell for 1,000.
    let (po, line) = confirmed_order(&app, 10, 60.0, 0.0, None).await;
    receive(&app, &po, &line, 10, &warehouse).await;

    let customer = app.customer().await;
    let quote = app
        .create(
            "/sales/quotes",
            json!({
                "customer_id": customer, "issue_date": "2026-03-10", "expiry_date": "2026-12-31",
                "lines": [{ "description": "Widget", "quantity": 10, "unit_price": 100.00, "tax_rate": 0.0 }]
            }),
        )
        .await;
    app.advance(&format!("/sales/quotes/{quote}/status"), &["sent", "accepted"]).await;
    let order = app.post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({})).await.id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed"]).await;
    let invoice = app
        .post(&format!("/sales/orders/{order}/convert-to-invoice"), json!({}))
        .await
        .id();
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await;

    let pl = app
        .get("/accounting/reports/profit-and-loss?date_from=2026-01-01&date_to=2026-12-31")
        .await;

    // The whole point: before this change net profit equalled revenue, because
    // nothing in the system booked a cost.
    pl.assert_money("total_revenue", "1000.00");
    pl.assert_money("total_expenses", "600.00");
    pl.assert_money("net_profit", "400.00");

    let trial = app
        .get("/accounting/reports/trial-balance?date_from=2026-01-01&date_to=2026-12-31")
        .await;
    assert_eq!(trial.field("is_balanced"), "true", "{}", trial.body);

    // And the balance sheet has a liability on it, not just receivables.
    assert_eq!(net(&app, &ids[AP]).await, -600.0);
    assert_eq!(net(&app, &ids[AR]).await, 1000.0);
    assert_eq!(net(&app, &ids[REVENUE]).await, -1000.0);
}
