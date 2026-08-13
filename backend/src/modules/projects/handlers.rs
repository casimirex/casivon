use axum::extract::{Path, Query, State};
use axum::Extension;
use uuid::Uuid;

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::projects::application::dto::*;
use crate::modules::projects::application::use_cases::*;
use crate::modules::projects::domain::entities::{Project, Task, TimeEntry};
use crate::modules::projects::domain::repositories::*;
use crate::modules::projects::infrastructure::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::{PaginatedResponse, PaginationParams};
use crate::shared::response::{ApiResponse, Created, DeletedResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

fn projects(state: &AppState) -> ProjectUseCases<PgProjectRepository, PgTaskRepository> {
    ProjectUseCases::new(
        PgProjectRepository::new(state.db.clone()),
        PgTaskRepository::new(state.db.clone()), state.fx.clone())
}

fn tasks(state: &AppState) -> TaskUseCases<PgTaskRepository, PgProjectRepository> {
    TaskUseCases::new(
        PgTaskRepository::new(state.db.clone()),
        PgProjectRepository::new(state.db.clone()),
    )
}

fn time_entries(
    state: &AppState,
) -> TimeEntryUseCases<PgTimeEntryRepository, PgTaskRepository, PgProjectRepository> {
    TimeEntryUseCases::new(
        PgTimeEntryRepository::new(state.db.clone()),
        PgTaskRepository::new(state.db.clone()),
        PgProjectRepository::new(state.db.clone()),
    )
}

// ----------------------------------------------------------------- projects

#[utoipa::path(
    post, path = "/api/v1/projects", tag = "Projects",
    request_body = CreateProjectRequest,
    responses((status = 201, body = ApiResponse<Project>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_project(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateProjectRequest>,
) -> AppResult<Created<Project>> {
    Ok(Created(projects(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/projects", tag = "Projects",
    params(PaginationParams, ProjectFilters),
    responses((status = 200, body = PaginatedResponse<Project>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_projects(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<ProjectFilters>,
) -> AppResult<PaginatedResponse<Project>> {
    let (data, total) = projects(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/projects/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<ProjectDetail>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<ProjectDetail>> {
    Ok(ApiResponse::new(projects(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/projects/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    request_body = UpdateProjectRequest,
    responses((status = 200, body = ApiResponse<Project>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateProjectRequest>,
) -> AppResult<ApiResponse<Project>> {
    Ok(ApiResponse::new(projects(&state).update(id, req).await?))
}

#[utoipa::path(
    put, path = "/api/v1/projects/{id}/status", tag = "Projects",
    params(("id" = Uuid, Path)),
    request_body = UpdateProjectStatusRequest,
    responses((status = 200, body = ApiResponse<Project>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_project_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateProjectStatusRequest>,
) -> AppResult<ApiResponse<Project>> {
    Ok(ApiResponse::new(projects(&state).set_status(id, &req.status).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/projects/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    projects(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// -------------------------------------------------------------------- tasks

#[utoipa::path(
    post, path = "/api/v1/projects/tasks", tag = "Projects",
    request_body = CreateTaskRequest,
    responses((status = 201, body = ApiResponse<TaskWithProject>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_task(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateTaskRequest>,
) -> AppResult<Created<TaskWithProject>> {
    Ok(Created(tasks(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/projects/tasks", tag = "Projects",
    params(PaginationParams, TaskFilters),
    responses((status = 200, body = PaginatedResponse<Task>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_tasks(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<TaskFilters>,
) -> AppResult<PaginatedResponse<Task>> {
    let (data, total) = tasks(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

/// Tasks scoped to one project — what the Kanban board loads.
#[utoipa::path(
    get, path = "/api/v1/projects/{id}/tasks", tag = "Projects",
    params(("project_id" = Uuid, Path), PaginationParams),
    responses((status = 200, body = PaginatedResponse<Task>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_project_tasks(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
    Query(mut filters): Query<TaskFilters>,
) -> AppResult<PaginatedResponse<Task>> {
    filters.project_id = Some(project_id);
    let (data, total) = tasks(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/projects/tasks/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<Task>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<Task>> {
    Ok(ApiResponse::new(tasks(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/projects/tasks/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    request_body = UpdateTaskRequest,
    responses((status = 200, body = ApiResponse<TaskWithProject>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateTaskRequest>,
) -> AppResult<ApiResponse<TaskWithProject>> {
    Ok(ApiResponse::new(tasks(&state).update(id, req).await?))
}

#[utoipa::path(
    put, path = "/api/v1/projects/tasks/{id}/status", tag = "Projects",
    params(("id" = Uuid, Path)),
    request_body = UpdateTaskStatusRequest,
    responses((status = 200, body = ApiResponse<TaskWithProject>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_task_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateTaskStatusRequest>,
) -> AppResult<ApiResponse<TaskWithProject>> {
    Ok(ApiResponse::new(tasks(&state).set_status(id, &req.status).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/projects/tasks/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    tasks(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ------------------------------------------------------------- time entries

#[utoipa::path(
    post, path = "/api/v1/projects/time-entries", tag = "Projects",
    request_body = CreateTimeEntryRequest,
    responses((status = 201, body = ApiResponse<TimeEntry>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_time_entry(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateTimeEntryRequest>,
) -> AppResult<Created<TimeEntry>> {
    Ok(Created(time_entries(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/projects/time-entries", tag = "Projects",
    params(PaginationParams, TimeEntryFilters),
    responses((status = 200, body = PaginatedResponse<TimeEntry>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_time_entries(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<TimeEntryFilters>,
) -> AppResult<PaginatedResponse<TimeEntry>> {
    let (data, total) = time_entries(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/projects/{id}/time-entries", tag = "Projects",
    params(("project_id" = Uuid, Path), PaginationParams),
    responses((status = 200, body = PaginatedResponse<TimeEntry>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_project_time_entries(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Query(params): Query<PaginationParams>,
    Query(mut filters): Query<TimeEntryFilters>,
) -> AppResult<PaginatedResponse<TimeEntry>> {
    filters.project_id = Some(project_id);
    let (data, total) = time_entries(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/projects/time-entries/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<TimeEntry>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_time_entry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<TimeEntry>> {
    Ok(ApiResponse::new(time_entries(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/projects/time-entries/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    request_body = UpdateTimeEntryRequest,
    responses((status = 200, body = ApiResponse<TimeEntry>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_time_entry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateTimeEntryRequest>,
) -> AppResult<ApiResponse<TimeEntry>> {
    Ok(ApiResponse::new(time_entries(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/projects/time-entries/{id}", tag = "Projects",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_time_entry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    time_entries(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}
