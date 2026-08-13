use axum::extract::{Path, Query, State};
use axum::Extension;
use uuid::Uuid;

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::crm::application::dto::*;
use crate::modules::crm::application::use_cases::*;
use crate::modules::crm::domain::entities::{Activity, Company, Contact, Opportunity};
use crate::modules::crm::domain::repositories::*;
use crate::modules::crm::infrastructure::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::{PaginatedResponse, PaginationParams};
use crate::shared::response::{ApiResponse, Created, DeletedResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

fn contacts(state: &AppState) -> ContactUseCases<PgContactRepository> {
    ContactUseCases::new(PgContactRepository::new(state.db.clone()))
}

fn companies(state: &AppState) -> CompanyUseCases<PgCompanyRepository> {
    CompanyUseCases::new(PgCompanyRepository::new(state.db.clone()))
}

fn opportunities(state: &AppState) -> OpportunityUseCases<PgOpportunityRepository> {
    OpportunityUseCases::new(PgOpportunityRepository::new(state.db.clone()), state.fx.clone())
}

fn activities(state: &AppState) -> ActivityUseCases<PgActivityRepository> {
    ActivityUseCases::new(PgActivityRepository::new(state.db.clone()))
}

// ----------------------------------------------------------------- contacts

#[utoipa::path(
    post, path = "/api/v1/crm/contacts", tag = "CRM",
    request_body = CreateContactRequest,
    responses((status = 201, body = ApiResponse<Contact>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_contact(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateContactRequest>,
) -> AppResult<Created<Contact>> {
    Ok(Created(contacts(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/crm/contacts", tag = "CRM",
    params(PaginationParams, ContactFilters),
    responses((status = 200, body = PaginatedResponse<Contact>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_contacts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<ContactFilters>,
) -> AppResult<PaginatedResponse<Contact>> {
    let (data, total) = contacts(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/crm/contacts/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<Contact>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_contact(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<Contact>> {
    Ok(ApiResponse::new(contacts(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/crm/contacts/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    request_body = UpdateContactRequest,
    responses((status = 200, body = ApiResponse<Contact>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_contact(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateContactRequest>,
) -> AppResult<ApiResponse<Contact>> {
    Ok(ApiResponse::new(contacts(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/crm/contacts/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_contact(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    contacts(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ---------------------------------------------------------------- companies

#[utoipa::path(
    post, path = "/api/v1/crm/companies", tag = "CRM",
    request_body = CreateCompanyRequest,
    responses((status = 201, body = ApiResponse<Company>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_company(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateCompanyRequest>,
) -> AppResult<Created<Company>> {
    Ok(Created(companies(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/crm/companies", tag = "CRM",
    params(PaginationParams, CompanyFilters),
    responses((status = 200, body = PaginatedResponse<Company>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_companies(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<CompanyFilters>,
) -> AppResult<PaginatedResponse<Company>> {
    let (data, total) = companies(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/crm/companies/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<Company>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_company(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<Company>> {
    Ok(ApiResponse::new(companies(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/crm/companies/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    request_body = UpdateCompanyRequest,
    responses((status = 200, body = ApiResponse<Company>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_company(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateCompanyRequest>,
) -> AppResult<ApiResponse<Company>> {
    Ok(ApiResponse::new(companies(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/crm/companies/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_company(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    companies(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

// ------------------------------------------------------------ opportunities

#[utoipa::path(
    post, path = "/api/v1/crm/opportunities", tag = "CRM",
    request_body = CreateOpportunityRequest,
    responses((status = 201, body = ApiResponse<Opportunity>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_opportunity(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateOpportunityRequest>,
) -> AppResult<Created<Opportunity>> {
    Ok(Created(opportunities(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/crm/opportunities", tag = "CRM",
    params(PaginationParams, OpportunityFilters),
    responses((status = 200, body = PaginatedResponse<Opportunity>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_opportunities(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<OpportunityFilters>,
) -> AppResult<PaginatedResponse<Opportunity>> {
    let (data, total) = opportunities(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/crm/opportunities/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<Opportunity>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_opportunity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<Opportunity>> {
    Ok(ApiResponse::new(opportunities(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/crm/opportunities/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    request_body = UpdateOpportunityRequest,
    responses((status = 200, body = ApiResponse<Opportunity>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_opportunity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateOpportunityRequest>,
) -> AppResult<ApiResponse<Opportunity>> {
    Ok(ApiResponse::new(opportunities(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/crm/opportunities/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_opportunity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    opportunities(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}

#[utoipa::path(
    get, path = "/api/v1/crm/opportunities/pipeline", tag = "CRM",
    responses((status = 200, body = ApiResponse<Vec<PipelineStage>>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn opportunity_pipeline(
    State(state): State<AppState>,
) -> AppResult<ApiResponse<Vec<PipelineStage>>> {
    Ok(ApiResponse::new(opportunities(&state).pipeline().await?))
}

// --------------------------------------------------------------- activities

#[utoipa::path(
    post, path = "/api/v1/crm/activities", tag = "CRM",
    request_body = CreateActivityRequest,
    responses((status = 201, body = ApiResponse<Activity>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn create_activity(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<CreateActivityRequest>,
) -> AppResult<Created<Activity>> {
    Ok(Created(activities(&state).create(req, &user).await?))
}

#[utoipa::path(
    get, path = "/api/v1/crm/activities", tag = "CRM",
    params(PaginationParams, ActivityFilters),
    responses((status = 200, body = PaginatedResponse<Activity>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_activities(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<ActivityFilters>,
) -> AppResult<PaginatedResponse<Activity>> {
    let (data, total) = activities(&state).list(&filters, &params).await?;
    Ok(PaginatedResponse::new(data, total, &params))
}

#[utoipa::path(
    get, path = "/api/v1/crm/activities/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<Activity>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_activity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<Activity>> {
    Ok(ApiResponse::new(activities(&state).get(id).await?))
}

#[utoipa::path(
    put, path = "/api/v1/crm/activities/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    request_body = UpdateActivityRequest,
    responses((status = 200, body = ApiResponse<Activity>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_activity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateActivityRequest>,
) -> AppResult<ApiResponse<Activity>> {
    Ok(ApiResponse::new(activities(&state).update(id, req).await?))
}

#[utoipa::path(
    delete, path = "/api/v1/crm/activities/{id}", tag = "CRM",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 404, description = "Not found", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_activity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    activities(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}
