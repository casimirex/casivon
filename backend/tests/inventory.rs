//! Stock levels only ever move through a movement, so these tests check the
//! arithmetic that movement performs.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn a_sku_is_unique(pool: PgPool) {
    let app = TestApp::new(pool).await;
    app.product("WIDGET-1", "Widget").await;

    let duplicate = app.post("/inventory/products", json!({ "sku": "WIDGET-1", "name": "Dup" })).await;

    assert!(!duplicate.status.is_success());
    assert!(duplicate.error_message().to_lowercase().contains("sku"));
}

#[sqlx::test]
async fn stock_cannot_go_negative(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let warehouse = app.warehouse("WH1", "Main Warehouse").await;
    let product = app.product("WIDGET-1", "Widget").await;

    let response = app
        .post(
            "/inventory/movements",
            json!({ "product_id": product, "warehouse_id": warehouse, "movement_type": "out", "quantity": 5 }),
        )
        .await;

    assert!(!response.status.is_success());
    // The refusal names the shortfall so the operator can act on it.
    let message = response.error_message();
    assert!(message.contains("WIDGET-1") && message.contains("5 requested"), "{message}");
}

#[sqlx::test]
async fn movements_add_and_remove_stock(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let warehouse = app.warehouse("WH1", "Main Warehouse").await;
    let product = app.product("WIDGET-1", "Widget").await;

    let received = app
        .post(
            "/inventory/movements",
            json!({ "product_id": product, "warehouse_id": warehouse, "movement_type": "in", "quantity": 100 }),
        )
        .await;
    assert_eq!(received.field("stock_level.quantity"), "100");

    let issued = app
        .post(
            "/inventory/movements",
            json!({ "product_id": product, "warehouse_id": warehouse, "movement_type": "out", "quantity": 40 }),
        )
        .await;
    assert_eq!(issued.field("stock_level.quantity"), "60");
}

#[sqlx::test]
async fn a_transfer_moves_stock_between_warehouses_without_creating_any(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let source = app.warehouse("WH1", "Main Warehouse").await;
    let destination = app.warehouse("WH2", "Overflow").await;
    let product = app.product("WIDGET-1", "Widget").await;

    app.post(
        "/inventory/movements",
        json!({ "product_id": product, "warehouse_id": source, "movement_type": "in", "quantity": 100 }),
    )
    .await;

    app.post(
        "/inventory/movements",
        json!({
            "product_id": product,
            "warehouse_id": source,
            "to_warehouse_id": destination,
            "movement_type": "transfer",
            "quantity": 30
        }),
    )
    .await;

    let at_source = app.get(&format!("/inventory/warehouses/{source}/stock")).await;
    let at_destination = app.get(&format!("/inventory/warehouses/{destination}/stock")).await;
    assert_eq!(at_source.data()[0]["quantity"], 70);
    assert_eq!(at_destination.data()[0]["quantity"], 30);

    // The total on hand is unchanged: a transfer is not a source of goods.
    let product_detail = app.get(&format!("/inventory/products/{product}")).await;
    assert_eq!(product_detail.field("total_on_hand"), "100");
}

#[sqlx::test]
async fn a_transfer_needs_a_destination(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let source = app.warehouse("WH1", "Main Warehouse").await;
    let product = app.product("WIDGET-1", "Widget").await;
    app.post(
        "/inventory/movements",
        json!({ "product_id": product, "warehouse_id": source, "movement_type": "in", "quantity": 10 }),
    )
    .await;

    let response = app
        .post(
            "/inventory/movements",
            json!({ "product_id": product, "warehouse_id": source, "movement_type": "transfer", "quantity": 5 }),
        )
        .await;

    assert!(!response.status.is_success());
}

#[sqlx::test]
async fn stock_cannot_be_transferred_to_the_warehouse_it_is_already_in(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let warehouse = app.warehouse("WH1", "Main Warehouse").await;
    let product = app.product("WIDGET-1", "Widget").await;
    app.post(
        "/inventory/movements",
        json!({ "product_id": product, "warehouse_id": warehouse, "movement_type": "in", "quantity": 10 }),
    )
    .await;

    let response = app
        .post(
            "/inventory/movements",
            json!({
                "product_id": product,
                "warehouse_id": warehouse,
                "to_warehouse_id": warehouse,
                "movement_type": "transfer",
                "quantity": 5
            }),
        )
        .await;

    assert!(!response.status.is_success());
}

#[sqlx::test]
async fn valuation_prices_stock_at_cost(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let warehouse = app.warehouse("WH1", "Main Warehouse").await;
    let product = app.product("WIDGET-1", "Widget").await; // cost 4.50
    app.post(
        "/inventory/movements",
        json!({ "product_id": product, "warehouse_id": warehouse, "movement_type": "in", "quantity": 100 }),
    )
    .await;

    let valuation = app.get("/inventory/stock/valuation").await;

    valuation.assert_money("total_value", "450.00");
}

#[sqlx::test]
async fn a_bom_cannot_contain_itself(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let product = app.product("WIDGET-1", "Widget").await;

    let response = app
        .post(
            "/inventory/boms",
            json!({
                "product_id": product,
                "name": "Self-referencing",
                "components": [{ "component_product_id": product, "quantity": 1 }]
            }),
        )
        .await;

    assert!(!response.status.is_success());
}
