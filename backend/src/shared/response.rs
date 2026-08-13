use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;
use utoipa::ToSchema;

/// The success half of the API envelope documented in the roadmap:
/// `{ "success": true, "data": { ... } }`. The error half lives in `AppError`.
///
/// Generic in the schema too, so every documented operation can say
/// `body = ApiResponse<Contact>` rather than needing a wrapper type each.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiResponse<T: Serialize> {
    /// Always `true`; failures carry the `ErrorResponse` shape instead.
    pub success: bool,
    pub data: T,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn new(data: T) -> Self {
        Self { success: true, data }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

/// 201 Created wrapper for POST handlers.
pub struct Created<T: Serialize>(pub T);

impl<T: Serialize> IntoResponse for Created<T> {
    fn into_response(self) -> Response {
        (StatusCode::CREATED, Json(ApiResponse::new(self.0))).into_response()
    }
}

/// Response for DELETE handlers, which have no body to return.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeletedResponse {
    pub deleted: bool,
}

/// The error half of the envelope. Not produced by this type — `AppError`
/// builds it directly — but declared here so the documented responses have
/// something to point at.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Always `false`.
    pub success: bool,
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Repeats the HTTP status, so a client that only kept the body still knows.
    #[schema(example = 422)]
    pub code: u16,
    /// Human-readable. Validation failures list every offending field as
    /// `field: reason`, separated by `; `.
    #[schema(example = "email: Invalid email format")]
    pub message: String,
}

impl DeletedResponse {
    pub fn ok() -> ApiResponse<Self> {
        ApiResponse::new(Self { deleted: true })
    }
}
