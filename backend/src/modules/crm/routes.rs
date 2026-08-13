use axum::{routing::get, Router};

use crate::infrastructure::state::AppState;
use crate::modules::crm::handlers;

pub fn crm_routes() -> Router<AppState> {
    Router::new()
        .route("/contacts", get(handlers::list_contacts).post(handlers::create_contact))
        .route(
            "/contacts/:id",
            get(handlers::get_contact)
                .put(handlers::update_contact)
                .delete(handlers::delete_contact),
        )
        .route("/companies", get(handlers::list_companies).post(handlers::create_company))
        .route(
            "/companies/:id",
            get(handlers::get_company)
                .put(handlers::update_company)
                .delete(handlers::delete_company),
        )
        .route(
            "/opportunities",
            get(handlers::list_opportunities).post(handlers::create_opportunity),
        )
        .route("/opportunities/pipeline", get(handlers::opportunity_pipeline))
        .route(
            "/opportunities/:id",
            get(handlers::get_opportunity)
                .put(handlers::update_opportunity)
                .delete(handlers::delete_opportunity),
        )
        .route("/activities", get(handlers::list_activities).post(handlers::create_activity))
        .route(
            "/activities/:id",
            get(handlers::get_activity)
                .put(handlers::update_activity)
                .delete(handlers::delete_activity),
        )
}
