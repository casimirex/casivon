use axum::extract::{Query, State};
use axum::Extension;

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::search::application::dto::{SearchQuery, SearchResults};
use crate::modules::search::application::use_cases::SearchUseCases;
use crate::modules::search::infrastructure::search_repo::PgSearchRepository;
use crate::shared::auth::CurrentUser;
use crate::shared::response::{ApiResponse, ErrorResponse};

/// Finds records across every module the caller is allowed to see.
///
/// No role gate of its own: what a user may find is decided per kind, so a
/// salesperson and an accountant get different answers to the same query rather
/// than one of them getting a 403.
#[utoipa::path(
    get, path = "/api/v1/search", tag = "Search",
    params(SearchQuery),
    responses(
        (status = 200, body = ApiResponse<SearchResults>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearer" = [])),
)]
pub async fn search(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<SearchQuery>,
) -> AppResult<ApiResponse<SearchResults>> {
    let use_cases = SearchUseCases::new(PgSearchRepository::new(state.db.clone()));
    Ok(ApiResponse::new(use_cases.search(query, &user).await?))
}
