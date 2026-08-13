//! Sending goods back to a supplier.
//!
//! Until this existed there was no return concept at all, and perpetual
//! inventory made that actively wrong: the only tool was a hand-made adjustment,
//! which booked the goods as a loss and left the order still owing for stock that
//! was no longer there. These tests pin both halves — the ledger and the debt —
//! and end on the invariant that ties them together.

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

const AP: usize = 5;
const COST: usize = 6;
const PURCHASE_TAX: usize = 7;
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

async fn product(app: &TestApp, sku: &str) -> String {
    app.create("/inventory/products", json!({ "sku": sku, "name": "Widget", "sale_price": 19.99 }))
        .await
}

/// A confirmed order. Returns (po_id, po_line_id).
async fn order_for(
    app: &TestApp,
    product: &str,
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
            "product_id": product, "description": "Widget",
            "quantity": quantity, "unit_price": unit_price, "tax_rate": tax_rate
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

async fn receive(app: &TestApp, po: &str, line: &str, quantity: i32, warehouse: &str) {
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
}

async fn send_back(
    app: &TestApp,
    po: &str,
    line: &str,
    quantity: i32,
    warehouse: &str,
) -> common::TestResponse {
    app.post(
        "/purchasing/purchase-returns",
        json!({
            "po_id": po, "warehouse_id": warehouse, "return_date": "2026-03-09",
            "reason": "Arrived damaged",
            "lines": [{ "po_line_id": line, "quantity_returned": quantity }]
        }),
    )
    .await
}

#[sqlx::test]
async fn a_return_gives_back_the_stock_and_reduces_what_is_owed(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &sku, 100, 4.00, 0.0, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;

    let sent = send_back(&app, &po, &line, 10, &warehouse).await;
    assert!(sent.status.is_success(), "{}", sent.body);
    assert_eq!(sent.field("return_number").starts_with("PR-"), true, "{}", sent.body);

    // The ledger: stock off the asset, debt reduced.
    assert_eq!(net(&app, &ids[INVENTORY]).await, 360.00);
    assert_eq!(net(&app, &ids[AP]).await, -360.00);
    // And not a cost — the goods were not consumed, they went back.
    assert_eq!(net(&app, &ids[COST]).await, 0.00);

    // The order: 40 of the 400 no longer owed. This is the half that a
    // hand-made stock adjustment could never do.
    let order = app.get(&format!("/purchasing/purchase-orders/{po}")).await;
    order.assert_money("amount_due", "360.00");

    // The shelf agrees.
    assert_eq!(money(&app, "/inventory/stock/valuation", "total_value").await, 360.00);
}

#[sqlx::test]
async fn a_return_under_periodic_costing_credits_the_cost(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, false).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &sku, 100, 4.00, 0.0, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;
    assert_eq!(net(&app, &ids[COST]).await, 400.00);

    assert!(send_back(&app, &po, &line, 10, &warehouse).await.status.is_success());

    // Straight back off the cost it was charged to on arrival.
    assert_eq!(net(&app, &ids[COST]).await, 360.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 0.00);
    assert_eq!(net(&app, &ids[AP]).await, -360.00);
}

#[sqlx::test]
async fn the_input_tax_goes_back_with_the_goods(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &sku, 100, 4.00, 20.0, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;
    assert_eq!(net(&app, &ids[PURCHASE_TAX]).await, 80.00);

    assert!(send_back(&app, &po, &line, 10, &warehouse).await.status.is_success());

    // Leaving it behind would reclaim tax on goods that went back.
    assert_eq!(net(&app, &ids[PURCHASE_TAX]).await, 72.00);
    assert_eq!(net(&app, &ids[AP]).await, -432.00);
}

#[sqlx::test]
async fn more_cannot_go_back_than_arrived(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &sku, 100, 4.00, 0.0, None).await;
    receive(&app, &po, &line, 40, &warehouse).await;

    let refused = send_back(&app, &po, &line, 50, &warehouse).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(refused.error_message().contains("only 40"), "{}", refused.error_message());

    // Nor twice over: the second return sees what the first already took.
    assert!(send_back(&app, &po, &line, 30, &warehouse).await.status.is_success());
    let second = send_back(&app, &po, &line, 20, &warehouse).await;
    assert_eq!(second.status, 422, "{}", second.body);
}

#[sqlx::test]
async fn goods_already_sold_cannot_be_sent_back(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &sku, 100, 4.00, 0.0, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;

    // Ninety-five out of the door.
    app.post(
        "/inventory/movements",
        json!({
            "product_id": sku, "warehouse_id": warehouse,
            "movement_type": "out", "quantity": 95
        }),
    )
    .await;

    // 409 rather than 422: the request is well formed, the shelf simply
    // disagrees — the same status an ordinary outward movement gives.
    let refused = send_back(&app, &po, &line, 10, &warehouse).await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(refused.error_message().contains("available"), "{}", refused.error_message());
}

#[sqlx::test]
async fn the_order_expects_the_goods_again(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &sku, 100, 4.00, 0.0, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;
    assert_eq!(app.get(&format!("/purchasing/purchase-orders/{po}")).await.field("status"), "fully_received");

    let sent = send_back(&app, &po, &line, 10, &warehouse).await;
    assert_eq!(sent.field("order_status"), "partially_received");

    let order = app.get(&format!("/purchasing/purchase-orders/{po}")).await;
    assert_eq!(order.field("status"), "partially_received");
    assert_eq!(order.data()["lines"][0]["outstanding"], 10);

    // And the replacement is accepted, which is the point of reopening it.
    receive(&app, &po, &line, 10, &warehouse).await;
    assert_eq!(app.get(&format!("/purchasing/purchase-orders/{po}")).await.field("status"), "fully_received");
}

/// The invariant, with quantities chosen so the un-blended average repeats:
/// 620 / 140 = 4.428571… — the case most likely to drift a cent.
#[sqlx::test]
async fn the_valuation_still_matches_the_inventory_account_after_a_return(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (first, first_line) = order_for(&app, &sku, 100, 4.00, 0.0, None).await;
    receive(&app, &first, &first_line, 100, &warehouse).await;
    let (second, second_line) = order_for(&app, &sku, 50, 5.50, 0.0, None).await;
    receive(&app, &second, &second_line, 50, &warehouse).await;

    // 150 @ 4.50 = 675.00. Send back 10 of the dearer delivery.
    assert!(send_back(&app, &second, &second_line, 10, &warehouse).await.status.is_success());

    let inventory = net(&app, &ids[INVENTORY]).await;
    assert_eq!(inventory, 620.00);

    // The average un-blended rather than staying at 4.50, or the two would
    // disagree by the difference.
    assert_eq!(app.get(&format!("/inventory/products/{sku}")).await.field("average_cost"), "4.4286");

    let valuation = money(&app, "/inventory/stock/valuation", "total_value").await;
    assert_eq!(valuation, inventory, "valuation and ledger disagree after a return");
}

#[sqlx::test]
async fn a_foreign_return_uses_the_orders_rate(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let rate = app
        .put(
            "/settings/fx-rates",
            json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": 1.10 }),
        )
        .await;
    assert!(rate.status.is_success(), "{}", rate.body);

    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &sku, 100, 5.00, 0.0, Some("EUR")).await;
    receive(&app, &po, &line, 100, &warehouse).await;
    assert_eq!(net(&app, &ids[INVENTORY]).await, 550.00);

    // EUR 50 of goods back, at the order's 1.10 — not at today's rate, and not
    // at the foreign figure.
    assert!(send_back(&app, &po, &line, 10, &warehouse).await.status.is_success());
    assert_eq!(net(&app, &ids[INVENTORY]).await, 495.00);
    assert_eq!(net(&app, &ids[AP]).await, -495.00);
}

#[sqlx::test]
async fn returning_what_was_already_paid_for_leaves_the_supplier_owing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &sku, 100, 4.00, 0.0, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;

    let paid = app
        .post(
            "/purchasing/vendor-payments",
            json!({ "po_id": po, "amount": 400.00, "payment_method": "bank_transfer", "payment_date": "2026-03-06" }),
        )
        .await;
    assert!(paid.status.is_success(), "{}", paid.body);
    app.get(&format!("/purchasing/purchase-orders/{po}")).await.assert_money("amount_due", "0");

    assert!(send_back(&app, &po, &line, 10, &warehouse).await.status.is_success());

    // Negative: the supplier owes you, which nets against the next purchase.
    // Getting the money back is a refund, which does not exist yet.
    app.get(&format!("/purchasing/purchase-orders/{po}")).await.assert_money("amount_due", "-40.00");

    // And nothing further can be paid on it.
    let overpaid = app
        .post(
            "/purchasing/vendor-payments",
            json!({ "po_id": po, "amount": 10.00, "payment_method": "bank_transfer", "payment_date": "2026-03-10" }),
        )
        .await;
    assert_eq!(overpaid.status, 422, "{}", overpaid.body);
}

#[sqlx::test]
async fn a_return_raised_before_posting_was_configured_is_repairable(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let sku = product(&app, "SKU-1").await;

    // Bought, received and returned while posting was off.
    let (po, line) = order_for(&app, &sku, 100, 4.00, 20.0, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;
    assert!(send_back(&app, &po, &line, 10, &warehouse).await.status.is_success());
    assert!(entries(&app).await.is_empty());

    let ids = configure(&app, true).await;

    let unposted = app.get("/accounting/unposted").await;
    let kinds: Vec<&str> =
        unposted.data()["documents"].as_array().unwrap().iter().map(|d| d["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"purchase_return"), "{:?}", kinds);

    let run = app.post("/accounting/post-unposted", json!({})).await;
    assert!(run.status.is_success(), "{}", run.body);

    // The same figures the live path would have produced.
    assert_eq!(net(&app, &ids[INVENTORY]).await, 360.00);
    assert_eq!(net(&app, &ids[PURCHASE_TAX]).await, 72.00);
    assert_eq!(net(&app, &ids[AP]).await, -432.00);

    // And running it again changes nothing.
    app.post("/accounting/post-unposted", json!({})).await;
    assert_eq!(net(&app, &ids[INVENTORY]).await, 360.00);
    assert!(app.get("/accounting/unposted").await.data()["documents"].as_array().unwrap().is_empty());
}
