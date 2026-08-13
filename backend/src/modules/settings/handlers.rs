use axum::extract::{Path, Query, State};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::infrastructure::state::AppState;
use crate::modules::settings::application::dto::{
    AvailableCurrencies, FxRateFilter, UpdateOrganizationRequest, UpsertFxRateRequest,
};
use crate::modules::settings::application::use_cases::{FxRateUseCases, SettingsUseCases};
use crate::modules::settings::domain::entities::{FxRate, OrganizationSettings};
use crate::modules::settings::infrastructure::fx_rate_repo::PgFxRateRepository;
use crate::modules::settings::infrastructure::organization_repo::PgOrganizationRepository;
use crate::shared::auth::CurrentUser;
use crate::shared::response::{ApiResponse, DeletedResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

fn use_cases(state: &AppState) -> SettingsUseCases<PgOrganizationRepository> {
    SettingsUseCases::new(PgOrganizationRepository::new(state.db.clone()))
}

fn fx_use_cases(
    state: &AppState,
) -> FxRateUseCases<PgFxRateRepository, PgOrganizationRepository> {
    FxRateUseCases::new(
        PgFxRateRepository::new(state.db.clone()),
        PgOrganizationRepository::new(state.db.clone()),
    )
}

/// Readable by any signed-in user: the company name and address show up on
/// documents they work with every day.
#[utoipa::path(
    get, path = "/api/v1/settings/organization", tag = "Settings",
    responses((status = 200, body = ApiResponse<OrganizationSettings>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn get_organization(
    State(state): State<AppState>,
) -> AppResult<ApiResponse<OrganizationSettings>> {
    Ok(ApiResponse::new(use_cases(&state).get_organization().await?))
}

/// Admin-only. These details appear on everything the company sends out.
#[utoipa::path(
    put, path = "/api/v1/settings/organization", tag = "Settings",
    request_body = UpdateOrganizationRequest,
    responses((status = 200, body = ApiResponse<OrganizationSettings>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn update_organization(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<UpdateOrganizationRequest>,
) -> AppResult<ApiResponse<OrganizationSettings>> {
    if !user.is_admin() {
        return Err(AppError::Forbidden(
            "Only an administrator can change the organisation settings".into(),
        ));
    }

    Ok(ApiResponse::new(use_cases(&state).update_organization(req).await?))
}

/// What any document's currency picker may offer. Readable by any signed-in
/// user, because every create form needs it.
#[utoipa::path(
    get, path = "/api/v1/settings/currencies", tag = "Settings",
    responses((status = 200, body = ApiResponse<AvailableCurrencies>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn available_currencies(
    State(state): State<AppState>,
) -> AppResult<ApiResponse<AvailableCurrencies>> {
    Ok(ApiResponse::new(fx_use_cases(&state).available().await?))
}

/// Readable by any signed-in user: a document shows the rate it was raised at,
/// and that figure has to be explainable without admin rights.
#[utoipa::path(
    get, path = "/api/v1/settings/fx-rates", tag = "Settings",
    params(FxRateFilter),
    responses((status = 200, body = ApiResponse<Vec<FxRate>>), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn list_fx_rates(
    State(state): State<AppState>,
    Query(filter): Query<FxRateFilter>,
) -> AppResult<ApiResponse<Vec<FxRate>>> {
    Ok(ApiResponse::new(fx_use_cases(&state).list(filter.currency).await?))
}

/// Admin-only. A rate decides what every document raised in that currency is
/// worth to the business.
///
/// `PUT` rather than `POST`: a currency and a date identify exactly one rate,
/// so sending the same pair twice corrects it rather than creating a duplicate.
#[utoipa::path(
    put, path = "/api/v1/settings/fx-rates", tag = "Settings",
    request_body = UpsertFxRateRequest,
    responses((status = 200, body = ApiResponse<FxRate>), (status = 422, description = "Validation failed", body = ErrorResponse), (status = 403, description = "Admin only", body = ErrorResponse), (status = 401, description = "Missing or invalid token", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn upsert_fx_rate(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<UpsertFxRateRequest>,
) -> AppResult<ApiResponse<FxRate>> {
    if !user.is_admin() {
        return Err(AppError::Forbidden(
            "Only an administrator can set exchange rates".into(),
        ));
    }

    Ok(ApiResponse::new(fx_use_cases(&state).upsert(req).await?))
}

/// Admin-only.
#[utoipa::path(
    delete, path = "/api/v1/settings/fx-rates/{id}", tag = "Settings",
    params(("id" = Uuid, Path, description = "Exchange rate id")),
    responses((status = 200, body = ApiResponse<DeletedResponse>), (status = 422, description = "Still in use", body = ErrorResponse), (status = 404, description = "Not found", body = ErrorResponse), (status = 403, description = "Admin only", body = ErrorResponse)),
    security(("bearer" = [])),
)]
pub async fn delete_fx_rate(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<DeletedResponse>> {
    if !user.is_admin() {
        return Err(AppError::Forbidden(
            "Only an administrator can remove exchange rates".into(),
        ));
    }

    fx_use_cases(&state).delete(id).await?;
    Ok(DeletedResponse::ok())
}
