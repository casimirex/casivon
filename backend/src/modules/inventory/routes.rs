use axum::{
    routing::{get, post, put},
    Router,
};

use crate::infrastructure::state::AppState;
use crate::modules::inventory::handlers;

pub fn inventory_routes() -> Router<AppState> {
    Router::new()
        .route("/products", get(handlers::list_products).post(handlers::create_product))
        .route(
            "/products/:id",
            get(handlers::get_product)
                .put(handlers::update_product)
                .delete(handlers::delete_product),
        )
        .route("/categories", get(handlers::list_categories).post(handlers::create_category))
        .route(
            "/categories/:id",
            put(handlers::update_category).delete(handlers::delete_category),
        )
        .route("/warehouses", get(handlers::list_warehouses).post(handlers::create_warehouse))
        .route(
            "/warehouses/:id",
            get(handlers::get_warehouse)
                .put(handlers::update_warehouse)
                .delete(handlers::delete_warehouse),
        )
        .route("/warehouses/:id/stock", get(handlers::warehouse_stock))
        .route("/movements", get(handlers::list_movements).post(handlers::record_movement))
        .route("/stock/low", get(handlers::low_stock))
        .route("/stock/reorder-policy", post(handlers::set_reorder_policy))
        .route("/stock/valuation", get(handlers::stock_valuation))
        .route("/boms", get(handlers::list_boms).post(handlers::create_bom))
        .route(
            "/boms/:id",
            get(handlers::get_bom).put(handlers::update_bom).delete(handlers::delete_bom),
        )
}
