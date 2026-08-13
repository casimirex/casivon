//! An order cannot claim its goods have gone before they have.
//!
//! `shipped` and `delivered` were pure labels: order status changes touch stock
//! only on `confirmed` and `cancelled`, so an order could be marked delivered —
//! a terminal state — while its goods sat on the shelf, reserved indefinitely,
//! because only invoicing releases a reservation. On-hand overstated what was
//! physically there and nothing ever put it right.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

async fn configure(app: &TestApp) {
    let roles: [(&str, &str, &str); 12] = [
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

    let mut ids = Vec::new();
    for (code, name, account_type) in roles {
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
                "employee_payable_account_id": ids[8], "employee_expense_account_id": ids[9],
                "inventory_account_id": ids[10], "inventory_adjustment_account_id": ids[11]
            }),
        )
        .await;
    assert!(response.status.is_success(), "{}", response.body);
}

async fn dispatch_from(app: &TestApp, warehouse: &str) {
    let response = app
        .put("/settings/organization", json!({ "default_dispatch_warehouse_id": warehouse }))
        .await;
    assert!(response.status.is_success(), "{}", response.body);
}

/// (quantity, reserved, available) for a product in a warehouse.
async fn level(app: &TestApp, warehouse: &str, product: &str) -> (i64, i64, i64) {
    let rows = app.get(&format!("/inventory/warehouses/{warehouse}/stock")).await;
    let row = rows
        .rows()
        .iter()
        .find(|row| row["product_id"] == product)
        .unwrap_or_else(|| panic!("no stock row: {}", rows.body))
        .clone();

    (
        row["quantity"].as_i64().unwrap(),
        row["reserved_quantity"].as_i64().unwrap(),
        row["available"].as_i64().unwrap(),
    )
}

async fn stocked_product(app: &TestApp, warehouse: &str, quantity: i32) -> String {
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
                            "quantity": quantity, "unit_price": 4.00, "tax_rate": 0 }]
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

/// A confirmed order for `quantity`, ready to be pushed along the lifecycle.
async fn confirmed_order(app: &TestApp, product: &str, quantity: i32) -> String {
    let customer = app.customer().await;
    let created = app
        .post(
            "/sales/orders",
            json!({
                "customer_id": customer, "order_date": "2026-03-01",
                "lines": [{ "product_id": product, "description": "Widget",
                            "quantity": quantity, "unit_price": 20.00, "tax_rate": 0 }]
            }),
        )
        .await;
    assert!(created.status.is_success(), "{}", created.body);
    let order = created.id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed", "processing"]).await;
    order
}

async fn set_status(app: &TestApp, order: &str, status: &str) -> common::TestResponse {
    app.put(&format!("/sales/orders/{order}/status"), json!({ "status": status })).await
}

async fn invoice_and_issue(app: &TestApp, order: &str) -> String {
    let invoice = app
        .post(
            &format!("/sales/orders/{order}/convert-to-invoice"),
            json!({ "issue_date": "2026-03-02", "payment_terms_days": 30 }),
        )
        .await;
    assert!(invoice.status.is_success(), "{}", invoice.body);
    let invoice = invoice.id();

    let issued = app
        .put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" }))
        .await;
    assert!(issued.status.is_success(), "{}", issued.body);
    invoice
}

#[sqlx::test]
async fn an_uninvoiced_order_cannot_be_marked_shipped(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;

    let refused = set_status(&app, &order, "shipped").await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(refused.error_message().contains("still to be invoiced"), "{}", refused.error_message());

    // Left exactly where it was, with its goods still held.
    assert_eq!(app.get(&format!("/sales/orders/{order}")).await.field("status"), "processing");
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));
}

/// `delivered` is only reachable through `shipped`, so this checks the rule
/// applies to it in its own right rather than only by being unreachable.
#[sqlx::test]
async fn the_same_rule_covers_delivered(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let refused = set_status(&app, &order, "delivered").await;

    // Refused for the transition rule as well as this one; what matters is that
    // it does not go through and the order does not move.
    assert!(!refused.status.is_success(), "{}", refused.body);
    assert_eq!(app.get(&format!("/sales/orders/{order}")).await.field("status"), "processing");
}

#[sqlx::test]
async fn a_draft_invoice_is_not_enough(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    // Raised but never sent, so nothing has shipped.
    let invoice = app
        .post(
            &format!("/sales/orders/{order}/convert-to-invoice"),
            json!({ "issue_date": "2026-03-02", "payment_terms_days": 30 }),
        )
        .await;
    assert!(invoice.status.is_success(), "{}", invoice.body);

    let refused = set_status(&app, &order, "shipped").await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    // And the goods really are still on the shelf.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));
}

#[sqlx::test]
async fn an_issued_invoice_opens_the_rest_of_the_lifecycle(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    invoice_and_issue(&app, &order).await;

    assert!(set_status(&app, &order, "shipped").await.status.is_success());
    let delivered = set_status(&app, &order, "delivered").await;
    assert!(delivered.status.is_success(), "{}", delivered.body);

    // The invariant this whole change exists for: by the time an order is
    // delivered, its goods are neither on the shelf nor held.
    assert_eq!(level(&app, &warehouse, &product).await, (90, 0, 90));
}

/// A cancelled invoice has already had its goods put back, so it cannot be the
/// evidence that they left.
#[sqlx::test]
async fn a_cancelled_invoice_does_not_count(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let invoice = invoice_and_issue(&app, &order).await;
    assert_eq!(level(&app, &warehouse, &product).await, (90, 0, 90));

    let cancelled = app
        .put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "cancelled" }))
        .await;
    assert!(cancelled.status.is_success(), "{}", cancelled.body);
    // The goods came back, and so did the order's hold on them — cancelling
    // undoes issuing, and issuing is what released the reservation.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));

    let refused = set_status(&app, &order, "shipped").await;
    assert_eq!(refused.status, 409, "{}", refused.body);
}

/// The compatibility promise: an installation that never opted into automatic
/// dispatch gets no new restriction, because invoicing moves nothing there and
/// so there is no inconsistency to prevent.
#[sqlx::test]
async fn without_automatic_dispatch_the_lifecycle_is_unchanged(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;

    let order = confirmed_order(&app, &product, 10).await;

    assert!(set_status(&app, &order, "shipped").await.status.is_success());
    assert!(set_status(&app, &order, "delivered").await.status.is_success());
}

/// The rule is about the invoice, not about whether anything happened to be
/// held: goods that were never on the shelf have still not left.
#[sqlx::test]
async fn an_order_that_reserved_nothing_is_still_refused(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    // Received then moved straight back out, so the shelf exists but is empty.
    let product = stocked_product(&app, &warehouse, 5).await;
    app.post(
        "/inventory/movements",
        json!({ "product_id": product, "warehouse_id": warehouse,
                "movement_type": "out", "quantity": 5 }),
    )
    .await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    assert_eq!(level(&app, &warehouse, &product).await, (0, 0, 0));

    let refused = set_status(&app, &order, "shipped").await;
    assert_eq!(refused.status, 409, "{}", refused.body);
}

#[sqlx::test]
async fn cancelling_is_still_allowed_and_gives_the_goods_back(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));

    // Cancelling does not claim the goods left, so the guard leaves it alone.
    assert!(set_status(&app, &order, "cancelled").await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await, (100, 0, 100));
}
