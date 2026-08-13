//! Goods receipts are the seam between purchasing and inventory: they move the
//! purchase order forward and post stock in the same transaction.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// A vendor, a warehouse, a product and a confirmed PO for 50 units.
async fn confirmed_po(app: &TestApp) -> (String, String, String, String) {
    let vendor = app
        .create("/purchasing/vendors", json!({ "name": "Acme Supplies", "email": "sales@acme.test" }))
        .await;
    let warehouse = app.warehouse("WH1", "Main Warehouse").await;
    let product = app.product("WIDGET-1", "Widget").await;

    let po = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor,
                "order_date": "2026-08-01",
                "lines": [{
                    "product_id": product,
                    "description": "Widget restock",
                    "quantity": 50,
                    "unit_price": 4.50,
                    "tax_rate": 20
                }]
            }),
        )
        .await;

    let po_id = po.id();
    let line_id = po.data()["lines"][0]["id"].as_str().unwrap().to_string();
    (po_id, line_id, warehouse, product)
}

#[sqlx::test]
async fn a_purchase_order_totals_its_lines(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let vendor = app
        .create("/purchasing/vendors", json!({ "name": "Acme Supplies", "email": "sales@acme.test" }))
        .await;
    let product = app.product("WIDGET-1", "Widget").await;

    // 50 x 4.50 = 225.00 net, plus 20% tax = 45.00.
    let po = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor,
                "order_date": "2026-08-01",
                "lines": [{ "product_id": product, "description": "Widget restock", "quantity": 50, "unit_price": 4.50, "tax_rate": 20 }]
            }),
        )
        .await;

    po.assert_money("subtotal", "225.00");
    po.assert_money("tax_amount", "45.00");
    po.assert_money("total", "270.00");
    assert!(po.field("po_number").starts_with("PO-"));
}

#[sqlx::test]
async fn goods_cannot_be_received_against_an_unconfirmed_order(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (po, line, warehouse, _) = confirmed_po(&app).await;

    let response = app
        .post(
            "/purchasing/goods-receipts",
            json!({
                "po_id": po,
                "warehouse_id": warehouse,
                "lines": [{ "po_line_id": line, "quantity_received": 10 }]
            }),
        )
        .await;

    assert!(!response.status.is_success(), "a draft PO accepted a receipt");
}

#[sqlx::test]
async fn more_cannot_be_received_than_was_ordered(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (po, line, warehouse, _) = confirmed_po(&app).await;
    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;

    let response = app
        .post(
            "/purchasing/goods-receipts",
            json!({
                "po_id": po,
                "warehouse_id": warehouse,
                "lines": [{ "po_line_id": line, "quantity_received": 999 }]
            }),
        )
        .await;

    assert!(!response.status.is_success());
}

#[sqlx::test]
async fn receiving_posts_stock_and_advances_the_order(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (po, line, warehouse, product) = confirmed_po(&app).await;
    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;

    let receipt = |quantity: i32| {
        json!({
            "po_id": po,
            "warehouse_id": warehouse,
            "lines": [{ "po_line_id": line, "quantity_received": quantity }]
        })
    };

    let partial = app.post("/purchasing/goods-receipts", receipt(20)).await;
    assert_eq!(partial.field("order_status"), "partially_received");
    assert_eq!(app.get(&format!("/inventory/products/{product}")).await.field("total_on_hand"), "20");

    let final_receipt = app.post("/purchasing/goods-receipts", receipt(30)).await;
    assert_eq!(final_receipt.field("order_status"), "fully_received");
    assert_eq!(app.get(&format!("/inventory/products/{product}")).await.field("total_on_hand"), "50");

    // The order is now complete, so nothing further can be booked against it.
    let extra = app.post("/purchasing/goods-receipts", receipt(1)).await;
    assert!(!extra.status.is_success());
}

#[sqlx::test]
async fn a_rejected_receipt_leaves_no_stock_behind(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (po, line, warehouse, product) = confirmed_po(&app).await;
    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;

    // The second line is over-ordered, so the whole receipt must roll back —
    // including the stock the first line would otherwise have posted.
    let response = app
        .post(
            "/purchasing/goods-receipts",
            json!({
                "po_id": po,
                "warehouse_id": warehouse,
                "lines": [
                    { "po_line_id": line, "quantity_received": 10 },
                    { "po_line_id": line, "quantity_received": 999 }
                ]
            }),
        )
        .await;

    assert!(!response.status.is_success());
    assert_eq!(app.get(&format!("/inventory/products/{product}")).await.field("total_on_hand"), "0");
}

#[sqlx::test]
async fn a_purchase_order_line_keeps_its_tax_rate(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (po, _, _, _) = confirmed_po(&app).await;

    // The rate used to be accepted, folded into the total and then dropped, so
    // the line could not be shown or recomputed afterwards.
    let reloaded = app.get(&format!("/purchasing/purchase-orders/{po}")).await;
    let stored = &reloaded.data()["lines"][0]["tax_rate"];
    assert_eq!(
        stored.as_str().unwrap().parse::<f64>().unwrap(),
        20.0,
        "the line came back without the rate it was created with"
    );
}

#[sqlx::test]
async fn a_purchase_order_line_rate_must_be_a_percentage(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let vendor = app
        .create("/purchasing/vendors", json!({ "name": "Acme Supplies", "email": "sales@acme.test" }))
        .await;

    let response = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor,
                "order_date": "2026-08-01",
                "lines": [{ "description": "Widget", "quantity": 1, "unit_price": 4.50, "tax_rate": 2000 }]
            }),
        )
        .await;

    assert_eq!(response.status, 422);
    assert!(response.error_message().contains("percentage"), "message was: {}", response.error_message());
}
