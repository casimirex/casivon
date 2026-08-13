use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::infrastructure::state::AppState;
use crate::modules::purchasing::handlers;

pub fn purchasing_routes() -> Router<AppState> {
    Router::new()
        .route("/vendors", get(handlers::list_vendors).post(handlers::create_vendor))
        .route(
            "/vendors/:id",
            get(handlers::get_vendor).put(handlers::update_vendor).delete(handlers::delete_vendor),
        )
        .route("/purchase-orders", get(handlers::list_pos).post(handlers::create_po))
        .route(
            "/purchase-orders/:id",
            get(handlers::get_po).put(handlers::update_po).delete(handlers::delete_po),
        )
        .route("/purchase-orders/:id/status", put(handlers::update_po_status))
        .route("/goods-receipts", get(handlers::list_receipts).post(handlers::create_receipt))
        .route("/goods-receipts/:id", get(handlers::get_receipt))
        .route(
            "/purchase-returns",
            get(handlers::list_returns).post(handlers::create_return),
        )
        .route("/purchase-returns/:id", get(handlers::get_return))
        .route(
            "/vendor-payments",
            post(handlers::record_vendor_payment).get(handlers::list_vendor_payments),
        )
        .route("/vendor-payments/:id", delete(handlers::delete_vendor_payment))
}
