use axum::{
    routing::{get, post},
    Router,
};
use crate::infrastructure::state::AppState;
use crate::modules::auth::handlers;

pub fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(handlers::get_me).put(handlers::update_me))
        .route("/me/password", axum::routing::put(handlers::change_my_password))
        .route("/", get(handlers::list_users))
        .route("/:id/role", axum::routing::put(handlers::update_user_role))
        .route("/:id/status", axum::routing::put(handlers::update_user_status))
}

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(handlers::register))
        .route("/login", post(handlers::login))
        .route("/refresh", post(handlers::refresh_token))
        .route("/logout", post(handlers::logout))
        .route("/forgot-password", post(handlers::forgot_password))
        .route("/reset-password", post(handlers::reset_password))
}
