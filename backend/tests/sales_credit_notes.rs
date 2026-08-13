//! Crediting a customer.
//!
//! Before this there was no credit concept in sales at all, and `paid` has no
//! outgoing status transition — so a customer who paid and then sent goods back
//! left nothing to do: no credit note, no partial adjustment, and no way to
//! cancel. These tests pin the money, the stock and the settlement, and the one
//! that matters most is `a_paid_invoice_can_be_credited`.

mod common;

use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;

const ROLES: [(&str, &str, &str); 12] = [
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
    ("1400", "Inventory", "asset"),
    ("5200", "Inventory adjustment", "expense"),
];

const AR: usize = 0;
const REVENUE: usize = 2;
const TAX_PAYABLE: usize = 3;
const COST: usize = 6;
const INVENTORY: usize = 10;

async fn configure(app: &TestApp, perpetual: bool) -> Vec<String> {
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

    let mut body = json!({
        "ar_account_id": ids[0], "bank_account_id": ids[1],
        "sales_revenue_account_id": ids[2], "tax_payable_account_id": ids[3],
        "fx_gain_loss_account_id": ids[4], "accounts_payable_account_id": ids[5],
        "cost_of_sales_account_id": ids[6], "purchase_tax_account_id": ids[7],
        "employee_payable_account_id": ids[8], "employee_expense_account_id": ids[9]
    });
    if perpetual {
        body["inventory_account_id"] = json!(ids[INVENTORY]);
        body["inventory_adjustment_account_id"] = json!(ids[11]);
    }

    let response = app.put("/accounting/posting-accounts", body).await;
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

async fn money(app: &TestApp, path: &str, field: &str) -> f64 {
    app.get(path).await.field(field).parse().unwrap()
}

/// An invoice for `quantity` at `unit_price`, issued. Returns (id, line_id).
async fn issued_invoice(
    app: &TestApp,
    product: Option<&str>,
    quantity: i32,
    unit_price: f64,
    tax_rate: f64,
    currency: Option<&str>,
) -> (String, String) {
    let customer = app.customer().await;

    let mut line = json!({
        "description": "Widget", "quantity": quantity,
        "unit_price": unit_price, "tax_rate": tax_rate
    });
    if let Some(product) = product {
        line["product_id"] = json!(product);
    }

    let mut body = json!({
        "customer_id": customer,
        "issue_date": "2026-03-01",
        "due_date": "2026-03-31",
        "lines": [line]
    });
    if let Some(currency) = currency {
        body["currency"] = json!(currency);
    }

    let created = app.post("/sales/invoices", body).await;
    assert!(created.status.is_success(), "{}", created.body);
    let invoice = created.id();
    let line_id = created.data()["lines"][0]["id"].as_str().unwrap().to_string();

    app.advance(&format!("/sales/invoices/{invoice}/status"), &["sent"]).await;
    (invoice, line_id)
}

async fn credit(
    app: &TestApp,
    invoice: &str,
    line: &str,
    quantity: i32,
    warehouse: Option<&str>,
) -> common::TestResponse {
    let mut body = json!({
        "invoice_id": invoice,
        "issue_date": "2026-03-10",
        "reason": "Returned by the customer",
        "lines": [{ "invoice_line_id": line, "quantity": quantity }]
    });
    if let Some(warehouse) = warehouse {
        body["warehouse_id"] = json!(warehouse);
    }

    app.post("/sales/credit-notes", body).await
}

/// Stock on the shelf, received through a purchase order so it carries a real
/// average cost.
async fn stocked_product(app: &TestApp, warehouse: &str, quantity: i32, cost: f64) -> String {
    let product = app
        .create("/inventory/products", json!({ "sku": "SKU-1", "name": "Widget", "sale_price": 19.99 }))
        .await;
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme" })).await;

    let created = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor, "order_date": "2026-02-01",
                "lines": [{ "product_id": product, "description": "Widget",
                            "quantity": quantity, "unit_price": cost, "tax_rate": 0 }]
            }),
        )
        .await;
    let po = created.id();
    let po_line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();
    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;
    app.post(
        "/purchasing/goods-receipts",
        json!({
            "po_id": po, "warehouse_id": warehouse, "receipt_date": "2026-02-05",
            "lines": [{ "po_line_id": po_line, "quantity_received": quantity }]
        }),
    )
    .await;

    product
}

#[sqlx::test]
async fn a_credit_note_takes_back_revenue_tax_and_the_receivable(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, false).await;

    // 10 @ 20.00 with 20% tax = 240.00.
    let (invoice, line) = issued_invoice(&app, None, 10, 20.00, 20.0, None).await;
    assert_eq!(net(&app, &ids[AR]).await, 240.00);

    let note = credit(&app, &invoice, &line, 2, None).await;
    assert!(note.status.is_success(), "{}", note.body);
    assert!(note.field("credit_note_number").starts_with("CN-"), "{}", note.body);
    note.assert_money("total", "48.00");

    assert_eq!(net(&app, &ids[REVENUE]).await, -160.00);
    assert_eq!(net(&app, &ids[TAX_PAYABLE]).await, -32.00);
    assert_eq!(net(&app, &ids[AR]).await, 192.00);

    // And the invoice knows it is owed less.
    app.get(&format!("/sales/invoices/{invoice}")).await.assert_money("amount_due", "192.00");
}

/// The case with no answer before this existed: `paid` has no outgoing status
/// transition, so the invoice could be neither cancelled nor adjusted.
#[sqlx::test]
async fn a_paid_invoice_can_be_credited(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, false).await;

    let (invoice, line) = issued_invoice(&app, None, 10, 20.00, 20.0, None).await;
    let paid = app
        .post(
            "/sales/payments",
            json!({ "invoice_id": invoice, "amount": 240.00, "payment_method": "bank_transfer", "payment_date": "2026-03-05" }),
        )
        .await;
    assert!(paid.status.is_success(), "{}", paid.body);

    let settled = app.get(&format!("/sales/invoices/{invoice}")).await;
    assert_eq!(settled.field("status"), "paid");
    settled.assert_money("amount_due", "0");

    // The status machine is not involved at all — this is a settlement change.
    let note = credit(&app, &invoice, &line, 2, None).await;
    assert!(note.status.is_success(), "{}", note.body);

    let after = app.get(&format!("/sales/invoices/{invoice}")).await;
    // Negative: the money is owed back to the customer, and nets against their
    // next invoice. Refunding it is a document that does not exist yet.
    after.assert_money("amount_due", "-48.00");
    after.assert_money("amount_paid", "240.00");
}

#[sqlx::test]
async fn more_cannot_be_credited_than_was_invoiced(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, false).await;
    let (invoice, line) = issued_invoice(&app, None, 10, 20.00, 0.0, None).await;

    let refused = credit(&app, &invoice, &line, 11, None).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(refused.error_message().contains("invoiced 10"), "{}", refused.error_message());

    // Nor across two notes, which is what the per-line tally is for.
    assert!(credit(&app, &invoice, &line, 7, None).await.status.is_success());
    let second = credit(&app, &invoice, &line, 4, None).await;
    assert_eq!(second.status, 422, "{}", second.body);
    assert!(second.error_message().contains("7 already credited"), "{}", second.error_message());
}

#[sqlx::test]
async fn a_draft_invoice_cannot_be_credited(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, false).await;
    let customer = app.customer().await;

    let created = app
        .post(
            "/sales/invoices",
            json!({
                "customer_id": customer, "issue_date": "2026-03-01", "due_date": "2026-03-31",
                "lines": [{ "description": "Widget", "quantity": 10, "unit_price": 20.00, "tax_rate": 0 }]
            }),
        )
        .await;
    let invoice = created.id();
    let line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();

    // Never issued, so there is no receivable to relieve.
    let refused = credit(&app, &invoice, &line, 2, None).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(refused.error_message().contains("draft"), "{}", refused.error_message());
}

#[sqlx::test]
async fn goods_coming_back_return_to_stock_and_relieve_the_cost(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;

    // Sell ten: the stock leaves through a movement, which is how selling works
    // here — invoicing does not move stock.
    let (invoice, line) = issued_invoice(&app, Some(&product), 10, 20.00, 0.0, None).await;
    app.post(
        "/inventory/movements",
        json!({ "product_id": product, "warehouse_id": warehouse, "movement_type": "out", "quantity": 10 }),
    )
    .await;
    assert_eq!(net(&app, &ids[COST]).await, 40.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 360.00);

    let note = credit(&app, &invoice, &line, 2, Some(&warehouse)).await;
    assert!(note.status.is_success(), "{}", note.body);

    // Two units back on the shelf at 4.00, and the cost relieved to match.
    assert_eq!(net(&app, &ids[INVENTORY]).await, 368.00);
    assert_eq!(net(&app, &ids[COST]).await, 32.00);

    // The money legs are independent of the goods.
    assert_eq!(net(&app, &ids[REVENUE]).await, -160.00);

    // And the invariant still holds.
    let valuation = money(&app, "/inventory/stock/valuation", "total_value").await;
    assert_eq!(valuation, net(&app, &ids[INVENTORY]).await);
    assert_eq!(valuation, 368.00);
}

#[sqlx::test]
async fn a_credit_with_no_warehouse_moves_no_stock(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;

    let (invoice, line) = issued_invoice(&app, Some(&product), 10, 20.00, 0.0, None).await;
    let before = money(&app, "/inventory/stock/valuation", "total_value").await;

    // A price dispute: money back, nothing physical.
    assert!(credit(&app, &invoice, &line, 2, None).await.status.is_success());

    assert_eq!(money(&app, "/inventory/stock/valuation", "total_value").await, before);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 400.00);
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
    // The money still moved: 200 invoiced less the 40 credited.
    assert_eq!(net(&app, &ids[REVENUE]).await, -160.00);
}

/// Under periodic costing the cost was taken when the goods were *bought*, so
/// there is nothing here to reverse.
#[sqlx::test]
async fn returned_goods_post_no_stock_leg_under_periodic_costing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, false).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;

    let (invoice, line) = issued_invoice(&app, Some(&product), 10, 20.00, 0.0, None).await;
    let cost_before = net(&app, &ids[COST]).await;

    assert!(credit(&app, &invoice, &line, 2, Some(&warehouse)).await.status.is_success());

    // The stock still comes back — that is a physical fact — but nothing is
    // posted for it.
    assert_eq!(net(&app, &ids[COST]).await, cost_before);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 0.00);
    let levels = app.get(&format!("/inventory/products/{product}")).await;
    assert_eq!(levels.field("total_on_hand"), "102");
}

#[sqlx::test]
async fn a_foreign_invoice_credits_at_its_own_rate(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, false).await;
    let rate = app
        .put(
            "/settings/fx-rates",
            json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": 1.10 }),
        )
        .await;
    assert!(rate.status.is_success(), "{}", rate.body);

    // EUR 200 invoiced at 1.10 -> 220.00 base.
    let (invoice, line) = issued_invoice(&app, None, 10, 20.00, 0.0, Some("EUR")).await;
    assert_eq!(net(&app, &ids[AR]).await, 220.00);

    // EUR 40 credited, at the invoice's rate rather than today's.
    assert!(credit(&app, &invoice, &line, 2, None).await.status.is_success());
    assert_eq!(net(&app, &ids[AR]).await, 176.00);
    // 220 booked less the 44 credited, both at the invoice's 1.10.
    assert_eq!(net(&app, &ids[REVENUE]).await, -176.00);
}

#[sqlx::test]
async fn a_payment_cannot_exceed_what_is_left_after_crediting(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, false).await;

    let (invoice, line) = issued_invoice(&app, None, 10, 20.00, 0.0, None).await;
    assert!(credit(&app, &invoice, &line, 5, None).await.status.is_success());

    // 200 invoiced, 100 credited — so 100 is the most that can be collected.
    let refused = app
        .post(
            "/sales/payments",
            json!({ "invoice_id": invoice, "amount": 150.00, "payment_method": "bank_transfer", "payment_date": "2026-03-12" }),
        )
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);

    let accepted = app
        .post(
            "/sales/payments",
            json!({ "invoice_id": invoice, "amount": 100.00, "payment_method": "bank_transfer", "payment_date": "2026-03-12" }),
        )
        .await;
    assert!(accepted.status.is_success(), "{}", accepted.body);
    app.get(&format!("/sales/invoices/{invoice}")).await.assert_money("amount_due", "0");
}

#[sqlx::test]
async fn a_credit_note_raised_before_posting_was_configured_is_repairable(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let (invoice, line) = issued_invoice(&app, None, 10, 20.00, 20.0, None).await;
    assert!(credit(&app, &invoice, &line, 2, None).await.status.is_success());
    assert!(entries(&app).await.is_empty());

    let ids = configure(&app, false).await;

    let unposted = app.get("/accounting/unposted").await;
    let kinds: Vec<&str> =
        unposted.data()["documents"].as_array().unwrap().iter().map(|d| d["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"sales_credit_note"), "{:?}", kinds);

    assert!(app.post("/accounting/post-unposted", json!({})).await.status.is_success());

    assert_eq!(net(&app, &ids[AR]).await, 192.00);
    assert_eq!(net(&app, &ids[REVENUE]).await, -160.00);

    // Running it again changes nothing.
    app.post("/accounting/post-unposted", json!({})).await;
    assert_eq!(net(&app, &ids[AR]).await, 192.00);
    assert!(app.get("/accounting/unposted").await.data()["documents"].as_array().unwrap().is_empty());
}
