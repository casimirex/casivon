use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::error::AppError;

pub const TOKEN_TYPE_ACCESS: &str = "access";
pub const TOKEN_TYPE_REFRESH: &str = "refresh";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub email: String,
    pub role: String,
    pub org_id: Option<Uuid>,
    /// `access` or `refresh` — checked so a refresh token cannot be replayed
    /// against a protected endpoint, and vice versa.
    #[serde(default = "default_token_type")]
    pub typ: String,
    /// The user's session generation when this token was issued. `/auth/refresh`
    /// compares it against the stored one, so bumping that ends every session at
    /// once — what a password reset needs and a per-token denylist cannot do.
    #[serde(default)]
    pub epoch: i32,
    pub exp: i64,
    pub iat: i64,
    pub jti: Uuid,
}

fn default_token_type() -> String {
    TOKEN_TYPE_ACCESS.to_string()
}

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub org_id: Option<Uuid>,
}

impl CurrentUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// Admins pass every check; everyone else must hold one of `roles`.
    pub fn require_any_role(&self, roles: &[&str]) -> Result<(), AppError> {
        if self.is_admin() || roles.contains(&self.role.as_str()) {
            Ok(())
        } else {
            Err(AppError::Forbidden(format!(
                "This action needs one of these roles: {}",
                roles.join(", ")
            )))
        }
    }
}

fn create_token(
    user_id: Uuid,
    email: String,
    role: String,
    org_id: Option<Uuid>,
    epoch: i32,
    token_type: &str,
    secret: &str,
    expiry_secs: i64,
) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id,
        email,
        role,
        org_id,
        typ: token_type.to_string(),
        epoch,
        iat: now.timestamp(),
        exp: (now + Duration::seconds(expiry_secs)).timestamp(),
        jti: Uuid::new_v4(),
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|_| AppError::Auth("Token creation failed".to_string()))
}

pub fn create_access_token(
    user_id: Uuid,
    email: String,
    role: String,
    org_id: Option<Uuid>,
    epoch: i32,
    secret: &str,
    expiry_secs: i64,
) -> Result<String, AppError> {
    create_token(user_id, email, role, org_id, epoch, TOKEN_TYPE_ACCESS, secret, expiry_secs)
}

pub fn create_refresh_token(
    user_id: Uuid,
    email: String,
    role: String,
    org_id: Option<Uuid>,
    epoch: i32,
    secret: &str,
    expiry_secs: i64,
) -> Result<String, AppError> {
    create_token(user_id, email, role, org_id, epoch, TOKEN_TYPE_REFRESH, secret, expiry_secs)
}

pub fn verify_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    let validation = Validation::default();
    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)
        .map_err(|e| AppError::Auth(format!("Invalid token: {}", e)))?;

    Ok(token_data.claims)
}

/// Verifies a token *and* asserts its `typ`, so callers cannot mix up the two kinds.
pub fn verify_token_of_type(token: &str, secret: &str, expected: &str) -> Result<Claims, AppError> {
    let claims = verify_token(token, secret)?;
    if claims.typ != expected {
        return Err(AppError::Auth(format!("Expected a {} token", expected)));
    }
    Ok(claims)
}

pub async fn auth_middleware(
    State(config): State<AppConfig>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let token = auth_header.ok_or_else(|| AppError::Auth("Missing authorization header".to_string()))?;
    let claims = verify_token_of_type(token, &config.jwt_secret, TOKEN_TYPE_ACCESS)?;

    let current_user = CurrentUser {
        id: claims.sub,
        email: claims.email,
        role: claims.role,
        org_id: claims.org_id,
    };

    request.extensions_mut().insert(current_user);
    Ok(next.run(request).await)
}

/// Route-level RBAC, layered after `auth_middleware`:
/// `.layer(middleware::from_fn(require_roles(&["manager"])))`
pub fn require_roles(
    roles: &'static [&'static str],
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, AppError>> + Send>>
       + Clone {
    move |request: Request, next: Next| {
        Box::pin(async move {
            let user = request
                .extensions()
                .get::<CurrentUser>()
                .cloned()
                .ok_or_else(|| AppError::Auth("Missing authentication context".to_string()))?;
            user.require_any_role(roles)?;
            Ok(next.run(request).await)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-value-at-least-32-bytes-long";

    fn user() -> (Uuid, String, String) {
        (Uuid::new_v4(), "a@example.com".to_string(), "user".to_string())
    }

    #[test]
    fn access_token_round_trips() {
        let (id, email, role) = user();
        let token = create_access_token(id, email.clone(), role, None, 0, SECRET, 60).unwrap();
        let claims = verify_token_of_type(&token, SECRET, TOKEN_TYPE_ACCESS).unwrap();
        assert_eq!(claims.sub, id);
        assert_eq!(claims.email, email);
    }

    #[test]
    fn refresh_token_is_rejected_as_access_token() {
        let (id, email, role) = user();
        let token = create_refresh_token(id, email, role, None, 0, SECRET, 60).unwrap();
        assert!(verify_token_of_type(&token, SECRET, TOKEN_TYPE_ACCESS).is_err());
        assert!(verify_token_of_type(&token, SECRET, TOKEN_TYPE_REFRESH).is_ok());
    }

    #[test]
    fn token_signed_with_another_secret_fails() {
        let (id, email, role) = user();
        let token = create_access_token(id, email, role, None, 0, SECRET, 60).unwrap();
        assert!(verify_token(&token, "a-different-secret-value-32-bytes!!").is_err());
    }

    #[test]
    fn admin_passes_every_role_check() {
        let admin = CurrentUser {
            id: Uuid::new_v4(),
            email: "admin@example.com".into(),
            role: "admin".into(),
            org_id: None,
        };
        assert!(admin.require_any_role(&["accountant"]).is_ok());

        let plain = CurrentUser { role: "user".into(), ..admin.clone() };
        assert!(plain.require_any_role(&["accountant"]).is_err());
        assert!(plain.require_any_role(&["user"]).is_ok());
    }
}
