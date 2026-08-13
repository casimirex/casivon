use axum::{
    routing::{delete, get, put},
    Router,
};

use crate::infrastructure::state::AppState;
use crate::modules::settings::handlers;

pub fn settings_routes() -> Router<AppState> {
    Router::new()
        .route("/organization", get(handlers::get_organization))
        .route("/organization", put(handlers::update_organization))
        .route("/currencies", get(handlers::available_currencies))
        .route("/fx-rates", get(handlers::list_fx_rates))
        .route("/fx-rates", put(handlers::upsert_fx_rate))
        .route("/fx-rates/:id", delete(handlers::delete_fx_rate))
}
