use axum::{
    routing::{get, put},
    Router,
};

use crate::infrastructure::state::AppState;
use crate::modules::hr::handlers;

pub fn hr_routes() -> Router<AppState> {
    Router::new()
        .route("/employees", get(handlers::list_employees).post(handlers::create_employee))
        .route(
            "/employees/:id",
            get(handlers::get_employee)
                .put(handlers::update_employee)
                .delete(handlers::delete_employee),
        )
        .route("/employees/:id/leave-balance", get(handlers::get_leave_balance))
        .route(
            "/leave-requests",
            get(handlers::list_leave_requests).post(handlers::create_leave_request),
        )
        .route(
            "/leave-requests/:id",
            get(handlers::get_leave_request).delete(handlers::delete_leave_request),
        )
        .route("/leave-requests/:id/decision", put(handlers::decide_leave_request))
        .route(
            "/expense-reports",
            get(handlers::list_expense_reports).post(handlers::create_expense_report),
        )
        .route(
            "/expense-reports/:id",
            get(handlers::get_expense_report)
                .put(handlers::update_expense_report)
                .delete(handlers::delete_expense_report),
        )
        .route("/expense-reports/:id/status", put(handlers::update_expense_status))
}
