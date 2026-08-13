use axum::extract::{Query, State};

use crate::error::AppResult;
use crate::infrastructure::state::AppState;
use crate::modules::auth::application::dto::*;
use crate::modules::auth::application::use_cases::AuthUseCases;
use crate::modules::auth::domain::entities::UserProfile;
use crate::modules::auth::domain::repositories::UserRepository;
use crate::modules::auth::infrastructure::email_verification_repo::PgEmailVerificationTokenRepository;
use crate::modules::auth::infrastructure::password_reset_repo::PgPasswordResetTokenRepository;
use crate::modules::auth::infrastructure::user_repository_impl::PgUserRepository;
use crate::shared::auth::CurrentUser;
use crate::shared::pagination::{PaginatedResponse, PaginationParams};
use crate::shared::response::{ApiResponse, ErrorResponse};
use crate::shared::validation::ValidatedJson;

fn use_cases(
    state: &AppState,
) -> AuthUseCases<
    PgUserRepository,
    PgPasswordResetTokenRepository,
    PgEmailVerificationTokenRepository,
> {
    AuthUseCases::new(
        PgUserRepository::new(state.db.clone()),
        PgPasswordResetTokenRepository::new(state.db.clone()),
        PgEmailVerificationTokenRepository::new(state.db.clone()),
        state.config.clone(),
        state.revoked_tokens.clone(),
        state.email.clone(),
    )
}

/// Confirms an address from the link in the email.
///
/// Unauthenticated: the link is usually opened from a mail client where nobody
/// is signed in, and holding the token is itself the proof.
#[utoipa::path(
    post, path = "/api/v1/auth/verify-email", tag = "Auth",
    request_body = VerifyEmailRequest,
    responses(
        (status = 200, body = ApiResponse<VerifyEmailResponse>),
        (status = 401, description = "The link is unknown, expired or already spent", body = ErrorResponse),
        (status = 422, body = ErrorResponse),
    ),
)]
pub async fn verify_email(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<VerifyEmailRequest>,
) -> AppResult<ApiResponse<VerifyEmailResponse>> {
    Ok(ApiResponse::new(use_cases(&state).verify_email(req).await?))
}

/// Sends another verification link.
///
/// Answers identically whether the address is unknown, already verified or
/// throttled — the same non-enumeration rule `/auth/forgot-password` follows.
#[utoipa::path(
    post, path = "/api/v1/auth/resend-verification", tag = "Auth",
    request_body = ResendVerificationRequest,
    responses(
        (status = 200, body = ApiResponse<ForgotPasswordResponse>),
        (status = 422, body = ErrorResponse),
    ),
)]
pub async fn resend_verification(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<ResendVerificationRequest>,
) -> AppResult<ApiResponse<ForgotPasswordResponse>> {
    Ok(ApiResponse::new(use_cases(&state).resend_verification(req).await?))
}

#[utoipa::path(
    post, path = "/api/v1/auth/register", tag = "Auth",
    request_body = RegisterRequest,
    responses(
        (status = 200, description = "Account created and signed in", body = ApiResponse<AuthResponse>),
        (status = 409, description = "That email is already registered", body = ErrorResponse),
        (status = 422, description = "Validation failed", body = ErrorResponse),
    ),
)]
pub async fn register(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<RegisterRequest>,
) -> AppResult<ApiResponse<AuthResponse>> {
    Ok(ApiResponse::new(use_cases(&state).register(req).await?))
}

#[utoipa::path(
    post, path = "/api/v1/auth/login", tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, body = ApiResponse<AuthResponse>),
        (status = 401, description = "Wrong credentials, or the account is deactivated", body = ErrorResponse),
    ),
)]
pub async fn login(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<LoginRequest>,
) -> AppResult<ApiResponse<AuthResponse>> {
    Ok(ApiResponse::new(use_cases(&state).login(req).await?))
}

#[utoipa::path(
    post, path = "/api/v1/auth/refresh", tag = "Auth",
    request_body = RefreshTokenRequest,
    responses(
        (status = 200, body = ApiResponse<AuthResponse>),
        (status = 401, description = "Expired, revoked by sign-out, or issued before a password change", body = ErrorResponse),
    ),
)]
pub async fn refresh_token(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<RefreshTokenRequest>,
) -> AppResult<ApiResponse<AuthResponse>> {
    Ok(ApiResponse::new(use_cases(&state).refresh(req).await?))
}

/// Emails a reset link. Answers the same way whether or not the address is
/// registered, so it cannot be used to enumerate accounts.
#[utoipa::path(
    post, path = "/api/v1/auth/forgot-password", tag = "Auth",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Always the same answer, registered address or not", body = ApiResponse<ForgotPasswordResponse>),
        (status = 422, body = ErrorResponse),
    ),
)]
pub async fn forgot_password(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<ForgotPasswordRequest>,
) -> AppResult<ApiResponse<ForgotPasswordResponse>> {
    Ok(ApiResponse::new(use_cases(&state).forgot_password(req).await?))
}

/// Spends a reset link and sets the new password.
#[utoipa::path(
    post, path = "/api/v1/auth/reset-password", tag = "Auth",
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, body = ApiResponse<ResetPasswordResponse>),
        (status = 401, description = "The link is unknown, expired or already spent", body = ErrorResponse),
        (status = 422, body = ErrorResponse),
    ),
)]
pub async fn reset_password(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<ResetPasswordRequest>,
) -> AppResult<ApiResponse<ResetPasswordResponse>> {
    Ok(ApiResponse::new(use_cases(&state).reset_password(req).await?))
}

/// Ends a session. Idempotent: signing out an already-revoked token succeeds,
/// so a client retrying after a dropped connection is not shown an error.
#[utoipa::path(
    post, path = "/api/v1/auth/logout", tag = "Auth",
    request_body = RefreshTokenRequest,
    responses(
        (status = 200, body = ApiResponse<LogoutResponse>),
        (status = 401, description = "Not a valid refresh token", body = ErrorResponse),
    ),
)]
pub async fn logout(
    State(state): State<AppState>,
    ValidatedJson(req): ValidatedJson<RefreshTokenRequest>,
) -> AppResult<ApiResponse<LogoutResponse>> {
    use_cases(&state).logout(req).await?;
    Ok(ApiResponse::new(LogoutResponse { signed_out: true }))
}

#[utoipa::path(
    get, path = "/api/v1/users/me", tag = "Users",
    responses((status = 200, body = ApiResponse<UserProfile>)),
    security(("bearer" = [])),
)]
pub async fn get_me(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
) -> AppResult<ApiResponse<UserProfile>> {
    Ok(ApiResponse::new(use_cases(&state).get_user_profile(user.id).await?))
}

/// Grants a role. Admin-only: this is how the bootstrap admin creates the
/// accountants, HR staff and managers the role-gated modules expect.
#[utoipa::path(
    put, path = "/api/v1/users/{id}/role", tag = "Users",
    params(("id" = Uuid, Path, description = "User id")),
    request_body = UpdateUserRoleRequest,
    responses(
        (status = 200, body = ApiResponse<UserProfile>),
        (status = 400, description = "An admin cannot remove their own admin role", body = ErrorResponse),
        (status = 403, description = "Admins only", body = ErrorResponse),
        (status = 404, body = ErrorResponse),
    ),
    security(("bearer" = [])),
)]
pub async fn update_user_role(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateUserRoleRequest>,
) -> AppResult<ApiResponse<UserProfile>> {
    if !user.is_admin() {
        return Err(crate::error::AppError::Forbidden("Only an administrator can manage user accounts".into()));
    }

    Ok(ApiResponse::new(
        use_cases(&state).set_user_role(id, &req.role, user.id).await?,
    ))
}

/// Activates or retires an account. Admin-only, and never your own.
#[utoipa::path(
    put, path = "/api/v1/users/{id}/status", tag = "Users",
    params(("id" = Uuid, Path, description = "User id")),
    request_body = UpdateUserStatusRequest,
    responses(
        (status = 200, body = ApiResponse<UserProfile>),
        (status = 400, description = "An admin cannot deactivate their own account", body = ErrorResponse),
        (status = 403, description = "Admins only", body = ErrorResponse),
        (status = 404, body = ErrorResponse),
    ),
    security(("bearer" = [])),
)]
pub async fn update_user_status(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateUserStatusRequest>,
) -> AppResult<ApiResponse<UserProfile>> {
    if !user.is_admin() {
        return Err(crate::error::AppError::Forbidden("Only an administrator can manage user accounts".into()));
    }

    Ok(ApiResponse::new(
        use_cases(&state).set_user_status(id, req.is_active, user.id).await?,
    ))
}

#[utoipa::path(
    put, path = "/api/v1/users/me", tag = "Users",
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, body = ApiResponse<UserProfile>),
        (status = 422, body = ErrorResponse),
    ),
    security(("bearer" = [])),
)]
pub async fn update_me(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<UpdateProfileRequest>,
) -> AppResult<ApiResponse<UserProfile>> {
    Ok(ApiResponse::new(use_cases(&state).update_profile(user.id, req).await?))
}

/// Changes your own password and hands back a fresh token pair, since the
/// change ends every session that was running — including this one.
#[utoipa::path(
    put, path = "/api/v1/users/me/password", tag = "Users",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "A fresh token pair: the change ends every session, including this one", body = ApiResponse<AuthResponse>),
        (status = 401, description = "The current password is wrong", body = ErrorResponse),
        (status = 422, body = ErrorResponse),
    ),
    security(("bearer" = [])),
)]
pub async fn change_my_password(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
    ValidatedJson(req): ValidatedJson<ChangePasswordRequest>,
) -> AppResult<ApiResponse<AuthResponse>> {
    Ok(ApiResponse::new(use_cases(&state).change_password(user.id, req).await?))
}

#[utoipa::path(
    get, path = "/api/v1/users", tag = "Users",
    params(PaginationParams, UserFilters),
    responses(
        (status = 200, body = PaginatedResponse<UserProfile>),
        (status = 403, description = "Managers and admins only", body = ErrorResponse),
    ),
    security(("bearer" = [])),
)]
pub async fn list_users(
    State(state): State<AppState>,
    user: axum::Extension<CurrentUser>,
    Query(params): Query<PaginationParams>,
    Query(filters): Query<UserFilters>,
) -> AppResult<PaginatedResponse<UserProfile>> {
    user.require_any_role(&["manager"])?;

    let repo = PgUserRepository::new(state.db.clone());
    let (users, total) = repo.list(&filters, &params).await?;

    // Map to profiles — `User` carries `password_hash`, which must never be serialized.
    let profiles = users.into_iter().map(UserProfile::from).collect();
    Ok(PaginatedResponse::new(profiles, total, &params))
}
