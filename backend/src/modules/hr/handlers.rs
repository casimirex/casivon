use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Extension;
use uuid::Uuid;

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::files::infrastructure::attachment_repo::PgAttachmentRepository;
use crate::modules::hr::application::dto::*;
use crate::modules::hr::application::use_cases::*;
use crate::modules::hr::domain::entities::{Employee, ExpenseReport, LeaveRequest};
use crate::modules::hr::domain::repositories::*;
use crate::modules::hr::infrastructure::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::{PaginatedResponse, PaginationParams};
use crate::shared::response::{ApiResponse, Created, DeletedResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

fn employees(
    state: &AppState,
) -> EmployeeUseCases<PgEmployeeRepository, PgLeaveRequestRepository> {
    EmployeeUseCases::new(
        PgEmployeeRepository::new(state.db.clone()),
        PgLeaveRequestRepository::new(state.db.clone()), state.fx.clone())
}

fn leave(state: &AppState) -> LeaveUseCases<PgLeaveRequestRepository, PgEmployeeRepository> {
    LeaveUseCases::new(
        PgLeaveRequestRepository::new(state.db.clone()),
        PgEmployeeRepository::new(state.db.clone()),
    )
}

fn expenses(
    state: &AppState,
) -> ExpenseUseCases<PgExpenseReportRepository, PgEmployeeRepository> {
    ExpenseUseCases::new(
        PgExpenseReportRepository::new(state.db.clone()),
        PgEmployeeRepository::new(state.db.clone()),
        state.fx.clone(),
        state.poster.clone(),
        Arc::new(PgAttachmentRepository::new(state.db.clone())),
    )
}

// ---------------------------------------------------------------- employees

#[utoipa::path(
    post, path = "/api/v1/hr/employees", tag = "HR",
    request_body = CreateEmployeeRequest,
    responses((status = 201, body = ApiResponse<Employee>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_employee(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateEmployeeRequest>,
) -> AppResult<Created<Employee>> {
    user.require_any_role(&HR_ROLES)?;
    Ok(Created(employees(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/hr/employees", tag = "HR",
    params(PaginationParams, EmployeeFilters),
    responses((status = 200, body = PaginatedResponse<Employee>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_employees(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<EmployeeFilters>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<PaginatedResponse<Employee>> {
    user.require_any_role(&HR_ROLES)?;

    let (data, total) = employees(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/hr/employees/{id}", tag = "HR",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<EmployeeDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_employee(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<EmployeeDetail>> {
    // No role gate: the use case allows an employee their own record and HR
    // everyone's, which is what the blanket gate should have been.
    Ok(ApiResponse::new(employees(&state).get(id, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/hr/employees/{id}/leave-balance", tag = "HR",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<LeaveBalance>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_leave_balance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<LeaveBalance>> {
    Ok(ApiResponse::new(employees(&state).leave_balance_for(id, &user).await?))
}

#[utoipa::path(
    put, path = "/api/v1/hr/employees/{id}", tag = "HR",
    params(("id" = Uuid, Path)),
    request_body = UpdateEmployeeRequest,
    responses((status = 200, body = ApiResponse<Employee>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_employee(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateEmployeeRequest>,
) -> AppResult<ApiResponse<Employee>> {
    user.require_any_role(&HR_ROLES)?;
    Ok(ApiResponse::new(employees(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/hr/employees/{id}", tag = "HR",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_employee(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    user.require_any_role(&HR_ROLES)?;
    employees(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ----------------------------------------------------------- leave requests

#[utoipa::path(
    post, path = "/api/v1/hr/leave-requests", tag = "HR",
    request_body = CreateLeaveRequest,
    responses((status = 201, body = ApiResponse<LeaveRequest>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_leave_request(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateLeaveRequest>,
) -> AppResult<Created<LeaveRequest>> {
    Ok(Created(leave(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/hr/leave-requests", tag = "HR",
    params(PaginationParams, LeaveFilters),
    responses((status = 200, body = PaginatedResponse<LeaveRequest>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_leave_requests(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<LeaveFilters>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<PaginatedResponse<LeaveRequest>> {
    let (data, total) = leave(&state).list(&filters, &params, &user).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/hr/leave-requests/{id}", tag = "HR",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<LeaveRequest>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_leave_request(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<LeaveRequest>> {
    Ok(ApiResponse::new(leave(&state).get(id, &user).await?))
}

/// Approving or rejecting is a manager's call, never the requester's.
#[utoipa::path(
    put, path = "/api/v1/hr/leave-requests/{id}/decision", tag = "HR",
    params(("id" = Uuid, Path)),
    request_body = DecideLeaveRequest,
    responses((status = 200, body = ApiResponse<LeaveRequest>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn decide_leave_request(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<DecideLeaveRequest>,
) -> AppResult<ApiResponse<LeaveRequest>> {
    user.require_any_role(&HR_ROLES)?;
    Ok(ApiResponse::new(leave(&state).decide(id, &req.status, &user).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/hr/leave-requests/{id}", tag = "HR",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_leave_request(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    leave(&state).delete(id, &user).await?;
    Ok(DeletedResponse::ok())
}

// ---------------------------------------------------------- expense reports

#[utoipa::path(
    post, path = "/api/v1/hr/expense-reports", tag = "HR",
    request_body = CreateExpenseReportRequest,
    responses((status = 201, body = ApiResponse<ExpenseReportDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_expense_report(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateExpenseReportRequest>,
) -> AppResult<Created<ExpenseReportDetail>> {
    Ok(Created(expenses(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/hr/expense-reports", tag = "HR",
    params(PaginationParams, ExpenseFilters),
    responses((status = 200, body = PaginatedResponse<ExpenseReport>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_expense_reports(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<ExpenseFilters>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<PaginatedResponse<ExpenseReport>> {
    let (data, total) = expenses(&state).list(&filters, &params, &user).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/hr/expense-reports/{id}", tag = "HR",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<ExpenseReportDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_expense_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<ExpenseReportDetail>> {
    Ok(ApiResponse::new(expenses(&state).get(id, &user).await?))
}

#[utoipa::path(
    put, path = "/api/v1/hr/expense-reports/{id}", tag = "HR",
    params(("id" = Uuid, Path)),
    request_body = UpdateExpenseReportRequest,
    responses((status = 200, body = ApiResponse<ExpenseReportDetail>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_expense_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<UpdateExpenseReportRequest>,
) -> AppResult<ApiResponse<ExpenseReportDetail>> {
    Ok(ApiResponse::new(expenses(&state).update(id, req, &user).await?))
}

#[utoipa::path(
    put, path = "/api/v1/hr/expense-reports/{id}/status", tag = "HR",
    params(("id" = Uuid, Path)),
    request_body = UpdateExpenseStatusRequest,
    responses((status = 200, body = ApiResponse<ExpenseReport>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_expense_status(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateExpenseStatusRequest>,
) -> AppResult<ApiResponse<ExpenseReport>> {
    // Employees submit and rework their own reports; only HR decides or pays out.
    if matches!(req.status.as_str(), "approved" | "rejected" | "reimbursed") {
        user.require_any_role(&HR_ROLES)?;
    }
    Ok(ApiResponse::new(expenses(&state).set_status(id, &req.status, &user).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/hr/expense-reports/{id}", tag = "HR",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_expense_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    expenses(&state).delete(id, &user).await?;
    Ok(DeletedResponse::ok())
}
