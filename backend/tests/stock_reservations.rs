//! Holding stock for a confirmed order.
//!
//! Issuing an invoice now takes goods off the shelf, so a confirmed order that
//! reserved nothing was a promise with nothing behind it: two orders could both
//! be confirmed against the last unit and the second to invoice was refused, in
//! front of a customer who had already been told they could have it.
//!
//! The sharpest test here is `an_order_does_not_block_its_own_shipment` — the
//! reservation has to be given back at exactly the right moment or the feature
//! deadlocks against itself.

mod common;

use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;

async fn configure(app: &TestApp) -> Vec<String> {
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
    ids
}

async fn dispatch_from(app: &TestApp, warehouse: &str) {
    let response = app
        .put("/settings/organization", json!({ "default_dispatch_warehouse_id": warehouse }))
        .await;
    assert!(response.status.is_success(), "{}", response.body);
}

/// The stock level for a product in a warehouse: (quantity, reserved, available).
async fn level(app: &TestApp, warehouse: &str, product: &str) -> (i64, i64, i64) {
    let rows = app.get(&format!("/inventory/warehouses/{warehouse}/stock")).await;
    let row = rows
        .rows()
        .iter()
        .find(|row| row["product_id"] == product)
        .unwrap_or_else(|| panic!("no stock row for that product: {}", rows.body))
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

/// A draft order for `quantity` of `product`. Returns (order_id, line_id).
async fn draft_order(app: &TestApp, product: &str, quantity: i32) -> (String, String) {
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
    let id = created.id();
    let line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();
    (id, line)
}

async fn confirm(app: &TestApp, order: &str) -> common::TestResponse {
    app.put(&format!("/sales/orders/{order}/status"), json!({ "status": "confirmed" })).await
}

#[sqlx::test]
async fn confirming_an_order_holds_the_stock(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    assert_eq!(level(&app, &warehouse, &product).await, (100, 0, 100));

    let (order, _) = draft_order(&app, &product, 10).await;
    assert!(confirm(&app, &order).await.status.is_success());

    // The goods are still on the shelf — they are simply spoken for.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));
}

/// Selling before buying is ordinary, so a short shelf still confirms.
#[sqlx::test]
async fn a_short_shelf_holds_what_it_has_and_the_order_still_confirms(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 6).await;
    dispatch_from(&app, &warehouse).await;

    let (order, _) = draft_order(&app, &product, 10).await;
    let confirmed = confirm(&app, &order).await;
    assert!(confirmed.status.is_success(), "{}", confirmed.body);
    assert_eq!(confirmed.field("status"), "confirmed");

    // Six held, four still promised and unreserved.
    assert_eq!(level(&app, &warehouse, &product).await, (6, 6, 0));
}

#[sqlx::test]
async fn two_orders_cannot_hold_the_same_unit(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 10).await;
    dispatch_from(&app, &warehouse).await;

    let (first, _) = draft_order(&app, &product, 8).await;
    assert!(confirm(&app, &first).await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await, (10, 8, 2));

    // The second confirms — the promise stands — but only gets what is left.
    let (second, _) = draft_order(&app, &product, 8).await;
    assert!(confirm(&app, &second).await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await, (10, 10, 0));
}

#[sqlx::test]
async fn cancelling_an_order_gives_the_stock_back(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let (order, _) = draft_order(&app, &product, 10).await;
    assert!(confirm(&app, &order).await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));

    let cancelled = app
        .put(&format!("/sales/orders/{order}/status"), json!({ "status": "cancelled" }))
        .await;
    assert!(cancelled.status.is_success(), "{}", cancelled.body);
    assert_eq!(level(&app, &warehouse, &product).await, (100, 0, 100));
}

/// A confirmed order is editable, and rewriting its lines replaces them — the
/// rows a reservation hung off are gone.
#[sqlx::test]
async fn editing_a_confirmed_order_re_reserves_against_the_new_lines(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let (order, _) = draft_order(&app, &product, 10).await;
    assert!(confirm(&app, &order).await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));

    let edited = app
        .put(
            &format!("/sales/orders/{order}"),
            json!({ "lines": [{ "product_id": product, "description": "Widget",
                                "quantity": 3, "unit_price": 20.00, "tax_rate": 0 }] }),
        )
        .await;
    assert!(edited.status.is_success(), "{}", edited.body);

    // Held down to three, not left at ten and not doubled to thirteen.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 3, 97));
}

/// The reservation has to be given back at exactly the right moment, or an
/// order's own goods look unavailable to its own invoice.
#[sqlx::test]
async fn an_order_does_not_block_its_own_shipment(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    // Exactly enough: if the reservation were not released first, there would be
    // nothing available and the invoice would be refused.
    let product = stocked_product(&app, &warehouse, 10).await;
    dispatch_from(&app, &warehouse).await;

    let (order, _) = draft_order(&app, &product, 10).await;
    assert!(confirm(&app, &order).await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await, (10, 10, 0));

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
    assert!(issued.status.is_success(), "the order blocked its own shipment: {}", issued.body);

    // The reservation became a movement: nothing held, nothing on the shelf.
    assert_eq!(level(&app, &warehouse, &product).await, (0, 0, 0));
}

#[sqlx::test]
async fn without_a_dispatch_warehouse_nothing_is_reserved(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;

    let (order, _) = draft_order(&app, &product, 10).await;
    assert!(confirm(&app, &order).await.status.is_success());

    assert_eq!(level(&app, &warehouse, &product).await, (100, 0, 100));
}

/// The payoff for routing every stock change through one door: reservations
/// protect goods from a hand-recorded movement without a line of new code.
#[sqlx::test]
async fn reserved_stock_cannot_be_moved_by_hand(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 10).await;
    dispatch_from(&app, &warehouse).await;

    let (order, _) = draft_order(&app, &product, 8).await;
    assert!(confirm(&app, &order).await.status.is_success());

    let refused = app
        .post(
            "/inventory/movements",
            json!({ "product_id": product, "warehouse_id": warehouse,
                    "movement_type": "out", "quantity": 5 }),
        )
        .await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(refused.error_message().contains("available"), "{}", refused.error_message());

    // The two that are not spoken for still move.
    let allowed = app
        .post(
            "/inventory/movements",
            json!({ "product_id": product, "warehouse_id": warehouse,
                    "movement_type": "out", "quantity": 2 }),
        )
        .await;
    assert!(allowed.status.is_success(), "{}", allowed.body);
}

#[sqlx::test]
async fn the_low_stock_report_counts_what_is_promised(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let policy = app
        .post(
            "/inventory/stock/reorder-policy",
            json!({ "product_id": product, "warehouse_id": warehouse,
                    "reorder_level": 20, "reorder_quantity": 50 }),
        )
        .await;
    assert!(policy.status.is_success(), "{}", policy.body);

    // A hundred on hand against a reorder level of twenty: nothing to see.
    let quiet: Vec<Value> = app.get("/inventory/stock/low").await.rows().clone();
    assert!(quiet.is_empty(), "{quiet:?}");

    // Ninety of them promised, so only ten are really available.
    let (order, _) = draft_order(&app, &product, 90).await;
    assert!(confirm(&app, &order).await.status.is_success());

    let alerted: Vec<Value> = app.get("/inventory/stock/low").await.rows().clone();
    assert_eq!(alerted.len(), 1, "{alerted:?}");
    assert_eq!(alerted[0]["product_id"], product.as_str());
}
