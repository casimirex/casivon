use axum::{
    routing::{get, put},
    Router,
};

use crate::infrastructure::state::AppState;
use crate::modules::projects::handlers;

pub fn project_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::list_projects).post(handlers::create_project))
        // Registered before "/:id" so these literal segments win the match.
        .route("/tasks", get(handlers::list_tasks).post(handlers::create_task))
        .route(
            "/tasks/:id",
            get(handlers::get_task).put(handlers::update_task).delete(handlers::delete_task),
        )
        .route("/tasks/:id/status", put(handlers::update_task_status))
        .route(
            "/time-entries",
            get(handlers::list_time_entries).post(handlers::create_time_entry),
        )
        .route(
            "/time-entries/:id",
            get(handlers::get_time_entry)
                .put(handlers::update_time_entry)
                .delete(handlers::delete_time_entry),
        )
        .route(
            "/:id",
            get(handlers::get_project)
                .put(handlers::update_project)
                .delete(handlers::delete_project),
        )
        .route("/:id/status", put(handlers::update_project_status))
        .route("/:id/tasks", get(handlers::list_project_tasks))
        .route("/:id/time-entries", get(handlers::list_project_time_entries))
}
