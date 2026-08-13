use axum::{routing::get, Router};

use crate::infrastructure::state::AppState;
use crate::modules::search::handlers;

pub fn search_routes() -> Router<AppState> {
    Router::new().route("/", get(handlers::search))
}
