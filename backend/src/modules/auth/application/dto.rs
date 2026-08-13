use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;
use uuid::Uuid;

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub password: String,
    #[validate(length(min = 1, message = "First name is required"))]
    pub first_name: String,
    #[validate(length(min = 1, message = "Last name is required"))]
    pub last_name: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RefreshTokenRequest {
    #[validate(length(min = 1, message = "Refresh token is required"))]
    pub refresh_token: String,
}

/// Query filters for the user list on the settings screen.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UserFilters {
    pub role: Option<String>,
    pub is_active: Option<bool>,
    /// Matches against name and email.
    pub search: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateUserStatusRequest {
    pub is_active: bool,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 1, max = 100, message = "First name is required"))]
    pub first_name: String,
    #[validate(length(min = 1, max = 100, message = "Last name is required"))]
    pub last_name: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 1, message = "Current password is required"))]
    pub current_password: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub new_password: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ForgotPasswordRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResetPasswordRequest {
    #[validate(length(min = 1, message = "Reset token is required"))]
    pub token: String,
    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub password: String,
}

/// Deliberately says nothing about whether the address is registered.
#[derive(Debug, Serialize, ToSchema)]
pub struct ForgotPasswordResponse {
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ResetPasswordResponse {
    pub password_changed: bool,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct VerifyEmailRequest {
    #[validate(length(min = 1, message = "Verification token is required"))]
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyEmailResponse {
    /// The address that was confirmed, so the screen can say which one.
    pub email: String,
    pub email_verified: bool,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResendVerificationRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LogoutResponse {
    /// Always true — the endpoint errors rather than reporting a sign-out that
    /// did not take effect.
    pub signed_out: bool,
}

pub const USER_ROLES: [&str; 6] = ["admin", "manager", "accountant", "hr", "sales", "user"];

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateUserRoleRequest {
    #[validate(custom = "validate_role")]
    pub role: String,
}

fn validate_role(value: &str) -> Result<(), validator::ValidationError> {
    if USER_ROLES.contains(&value) {
        return Ok(());
    }
    let mut err = validator::ValidationError::new("role");
    err.message = Some(format!("Must be one of: {}", USER_ROLES.join(", ")).into());
    Err(err)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    /// Carried on the session because the prompt asking the user to confirm
    /// their address renders from it — without this the client would have to
    /// fetch the full profile before it could know whether to show anything.
    pub email_verified: bool,
}
