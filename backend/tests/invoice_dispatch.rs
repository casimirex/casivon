//! Stock leaving when an invoice is issued.
//!
//! Selling never moved stock: receiving created a movement automatically,
//! invoicing did not. The credit note made that awkward — it puts goods *back*
//! automatically, reversing a movement nothing ever made. These tests pin both
//! halves, and the compatibility promise that matters most: with no dispatch
//! warehouse chosen, invoicing behaves exactly as it always did.

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

const REVENUE: usize = 2;
const COST: usize = 6;
const INVENTORY: usize = 10;
const ADJUSTMENT: usize = 11;

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
        body["inventory_adjustment_account_id"] = json!(ids[ADJUSTMENT]);
    }

    let response = app.put("/accounting/posting-accounts", body).await;
    assert!(response.status.is_success(), "mapping failed: {}", response.body);
    ids
}

/// Chooses the warehouse goods ship from — the switch this whole feature is off
/// without.
async fn dispatch_from(app: &TestApp, warehouse: &str) {
    let response = app
        .put("/settings/organization", json!({ "default_dispatch_warehouse_id": warehouse }))
        .await;
    assert!(response.status.is_success(), "{}", response.body);
    assert_eq!(response.field("default_dispatch_warehouse_id"), warehouse);
}

async fn entries(app: &TestApp) -> Vec<Value> {
    app.get("/accounting/ledger-entries?per_page=100").await.rows().clone()
}

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

async fn on_hand(app: &TestApp, product: &str) -> i64 {
    app.get(&format!("/inventory/products/{product}")).await.field("total_on_hand").parse().unwrap()
}

/// Stock on the shelf, received through a purchase order so it carries a real
/// average cost.
async fn stocked_product(app: &TestApp, warehouse: &str, quantity: i32, cost: f64) -> String {
    let product = app
        .create("/inventory/products", json!({ "sku": "SKU-1", "name": "Widget", "sale_price": 20.00 }))
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

/// A draft invoice. Returns (id, line_id).
async fn draft_invoice(app: &TestApp, lines: Value) -> (String, String) {
    let customer = app.customer().await;
    let created = app
        .post(
            "/sales/invoices",
            json!({
                "customer_id": customer, "issue_date": "2026-03-01",
                "due_date": "2026-03-31", "lines": lines
            }),
        )
        .await;
    assert!(created.status.is_success(), "{}", created.body);
    let id = created.id();
    let line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();
    (id, line)
}

fn product_line(product: &str, quantity: i32) -> Value {
    json!([{ "product_id": product, "description": "Widget",
             "quantity": quantity, "unit_price": 20.00, "tax_rate": 0 }])
}

async fn issue(app: &TestApp, invoice: &str) -> common::TestResponse {
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await
}

#[sqlx::test]
async fn issuing_an_invoice_ships_the_stock_and_posts_the_cost(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;
    dispatch_from(&app, &warehouse).await;

    let (invoice, _) = draft_invoice(&app, product_line(&product, 10)).await;
    assert!(issue(&app, &invoice).await.status.is_success());

    // Revenue and its cost in one go, which is what tying the movement to the
    // issue buys.
    assert_eq!(net(&app, &ids[REVENUE]).await, -200.00);
    assert_eq!(net(&app, &ids[COST]).await, 40.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 360.00);
    assert_eq!(on_hand(&app, &product).await, 90);

    // And not as shrinkage.
    assert_eq!(net(&app, &ids[ADJUSTMENT]).await, 0.00);

    let valuation = money(&app, "/inventory/stock/valuation", "total_value").await;
    assert_eq!(valuation, net(&app, &ids[INVENTORY]).await);
}

/// The compatibility promise: choose no warehouse and nothing changes.
#[sqlx::test]
async fn without_a_dispatch_warehouse_invoicing_moves_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;

    let (invoice, _) = draft_invoice(&app, product_line(&product, 10)).await;
    assert!(issue(&app, &invoice).await.status.is_success());

    assert_eq!(net(&app, &ids[REVENUE]).await, -200.00);
    // No movement, so no cost — exactly as it behaved before this existed.
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
    assert_eq!(on_hand(&app, &product).await, 100);
}

#[sqlx::test]
async fn an_invoice_the_shelf_cannot_cover_is_refused(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 5, 4.00).await;
    dispatch_from(&app, &warehouse).await;

    let (invoice, _) = draft_invoice(&app, product_line(&product, 10)).await;
    let refused = issue(&app, &invoice).await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(refused.error_message().contains("available"), "{}", refused.error_message());

    // The invoice is still a draft and nothing moved: refusing has to leave both
    // halves untouched, or the failure is worse than the error it reports. The
    // ledger is not empty — the purchase that stocked the shelf is in it — but
    // nothing of the invoice's reached it.
    assert_eq!(app.get(&format!("/sales/invoices/{invoice}")).await.field("status"), "draft");
    assert_eq!(on_hand(&app, &product).await, 5);
    assert_eq!(net(&app, &ids[REVENUE]).await, 0.00);
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
}

#[sqlx::test]
async fn lines_that_hold_no_stock_are_skipped(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    dispatch_from(&app, &warehouse).await;

    let service = app
        .create(
            "/inventory/products",
            json!({ "sku": "CONSULT", "name": "A day of advice", "product_type": "service", "sale_price": 500.00 }),
        )
        .await;

    // A service line and a free-text line: neither was ever on a shelf.
    let (invoice, _) = draft_invoice(
        &app,
        json!([
            { "product_id": service, "description": "Consulting", "quantity": 2, "unit_price": 500.00, "tax_rate": 0 },
            { "description": "Delivery", "quantity": 1, "unit_price": 25.00, "tax_rate": 0 }
        ]),
    )
    .await;

    assert!(issue(&app, &invoice).await.status.is_success());
    assert_eq!(net(&app, &ids[REVENUE]).await, -1025.00);
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
}

#[sqlx::test]
async fn cancelling_brings_the_goods_back_as_a_cost_not_as_shrinkage(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;
    dispatch_from(&app, &warehouse).await;

    let (invoice, _) = draft_invoice(&app, product_line(&product, 10)).await;
    assert!(issue(&app, &invoice).await.status.is_success());

    let cancelled = app
        .put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "cancelled" }))
        .await;
    assert!(cancelled.status.is_success(), "{}", cancelled.body);

    assert_eq!(on_hand(&app, &product).await, 100);
    // Back where it started, on both sides.
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 400.00);
    assert_eq!(net(&app, &ids[REVENUE]).await, 0.00);

    // Crucially not booked as shrinkage, which is what the movement rule would
    // have done for a plain inward movement.
    assert_eq!(net(&app, &ids[ADJUSTMENT]).await, 0.00);

    let valuation = money(&app, "/inventory/stock/valuation", "total_value").await;
    assert_eq!(valuation, net(&app, &ids[INVENTORY]).await);
    assert_eq!(valuation, 400.00);
}

/// Issuing ships and crediting returns. They are separate events, and running
/// both must not move the goods twice.
#[sqlx::test]
async fn issuing_then_crediting_moves_the_goods_once_each_way(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;
    dispatch_from(&app, &warehouse).await;

    let (invoice, line) = draft_invoice(&app, product_line(&product, 10)).await;
    assert!(issue(&app, &invoice).await.status.is_success());
    assert_eq!(on_hand(&app, &product).await, 90);

    let credited = app
        .post(
            "/sales/credit-notes",
            json!({
                "invoice_id": invoice, "warehouse_id": warehouse,
                "reason": "Two came back",
                "lines": [{ "invoice_line_id": line, "quantity": 2 }]
            }),
        )
        .await;
    assert!(credited.status.is_success(), "{}", credited.body);

    // Ten out, two back.
    assert_eq!(on_hand(&app, &product).await, 92);
    assert_eq!(net(&app, &ids[COST]).await, 32.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 368.00);
    assert_eq!(
        money(&app, "/inventory/stock/valuation", "total_value").await,
        net(&app, &ids[INVENTORY]).await
    );
}

#[sqlx::test]
async fn under_periodic_costing_stock_moves_but_nothing_posts_for_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, false).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;
    dispatch_from(&app, &warehouse).await;

    let (invoice, _) = draft_invoice(&app, product_line(&product, 10)).await;
    assert!(issue(&app, &invoice).await.status.is_success());

    // The goods physically left.
    assert_eq!(on_hand(&app, &product).await, 90);
    // But the cost was taken when they were bought, so there is nothing to post.
    assert_eq!(net(&app, &ids[COST]).await, 400.00); // the purchase, untouched
    assert_eq!(net(&app, &ids[INVENTORY]).await, 0.00);
}

#[sqlx::test]
async fn an_invoice_issued_before_the_setting_is_untouched_by_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;

    // Issued while dispatch was off.
    let (invoice, _) = draft_invoice(&app, product_line(&product, 10)).await;
    assert!(issue(&app, &invoice).await.status.is_success());
    assert_eq!(on_hand(&app, &product).await, 100);

    // Turning it on now does not reach back — a movement records something
    // physical happening, and nothing happened.
    dispatch_from(&app, &warehouse).await;
    assert_eq!(on_hand(&app, &product).await, 100);
}

#[sqlx::test]
async fn dispatch_can_be_switched_off_again(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100, 4.00).await;
    dispatch_from(&app, &warehouse).await;

    let cleared = app
        .put("/settings/organization", json!({ "default_dispatch_warehouse_id": "" }))
        .await;
    assert!(cleared.status.is_success(), "{}", cleared.body);
    assert!(cleared.data()["default_dispatch_warehouse_id"].is_null(), "{}", cleared.body);

    let (invoice, _) = draft_invoice(&app, product_line(&product, 10)).await;
    assert!(issue(&app, &invoice).await.status.is_success());
    assert_eq!(on_hand(&app, &product).await, 100);
}
