use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::infrastructure::state::AppState;
use crate::modules::sales::handlers;

pub fn sales_routes() -> Router<AppState> {
    Router::new()
        .route("/quotes", post(handlers::create_quote).get(handlers::list_quotes))
        .route(
            "/quotes/:id",
            get(handlers::get_quote).put(handlers::update_quote).delete(handlers::delete_quote),
        )
        .route("/quotes/:id/status", put(handlers::update_quote_status))
        .route("/quotes/:id/convert-to-order", post(handlers::convert_quote_to_order))
        .route("/orders", post(handlers::create_order).get(handlers::list_orders))
        .route(
            "/orders/:id",
            get(handlers::get_order).put(handlers::update_order).delete(handlers::delete_order),
        )
        .route("/orders/:id/status", put(handlers::update_order_status))
        .route("/orders/:id/convert-to-invoice", post(handlers::convert_order_to_invoice))
        .route("/invoices", post(handlers::create_invoice).get(handlers::list_invoices))
        .route(
            "/invoices/:id",
            get(handlers::get_invoice)
                .put(handlers::update_invoice)
                .delete(handlers::delete_invoice),
        )
        .route("/invoices/:id/status", put(handlers::update_invoice_status))
        .route("/payments", post(handlers::record_payment).get(handlers::list_payments))
        .route("/payments/:id", delete(handlers::delete_payment))
        .route(
            "/credit-notes",
            post(handlers::create_credit_note).get(handlers::list_credit_notes),
        )
        .route("/credit-notes/:id", get(handlers::get_credit_note))
}
