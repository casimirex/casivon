use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::modules::auth::application::dto::UserFilters;
use crate::modules::auth::domain::entities::{EmailVerificationToken, PasswordResetToken, User};
use crate::error::AppResult;
use crate::shared::pagination::PaginationParams;

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, user: &User) -> AppResult<User>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<User>>;
    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>>;
    async fn update(&self, user: &User) -> AppResult<User>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &UserFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<User>, i64)>;
    /// Used to detect a fresh install, where the first account bootstraps as admin.
    async fn count(&self) -> AppResult<i64>;

    /// Sets a new password and moves `sessions_valid_from` to now, so tokens
    /// issued to the old password stop being accepted.
    async fn replace_password(&self, user_id: Uuid, password_hash: &str) -> AppResult<()>;

    /// Records that the address has been confirmed. Idempotent: verifying an
    /// already-verified account is a no-op rather than an error, because a
    /// second click of the same link should not read as a failure.
    async fn mark_email_verified(&self, user_id: Uuid) -> AppResult<()>;
}

#[async_trait]
pub trait PasswordResetTokenRepository: Send + Sync {
    async fn create(&self, token: &PasswordResetToken) -> AppResult<PasswordResetToken>;
    async fn find_by_hash(&self, token_hash: &str) -> AppResult<Option<PasswordResetToken>>;

    /// Marks the token spent. Returns false if it had already been used, which
    /// is how two simultaneous submissions of the same link are resolved.
    async fn mark_used(&self, id: Uuid, now: DateTime<Utc>) -> AppResult<bool>;

    /// When the user last asked for a link, used to throttle requests.
    async fn last_issued_at(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>>;

    /// Invalidates every outstanding link for the user, so a completed reset
    /// does not leave older emails live.
    async fn expire_all_for_user(&self, user_id: Uuid, now: DateTime<Utc>) -> AppResult<()>;
}

/// The verification counterpart, with the same contract for the same reasons.
#[async_trait]
pub trait EmailVerificationTokenRepository: Send + Sync {
    async fn create(&self, token: &EmailVerificationToken) -> AppResult<EmailVerificationToken>;
    async fn find_by_hash(&self, token_hash: &str) -> AppResult<Option<EmailVerificationToken>>;

    /// Marks the token spent. Returns false if it had already been used, which
    /// is how two simultaneous clicks of the same link are resolved.
    async fn mark_used(&self, id: Uuid, now: DateTime<Utc>) -> AppResult<bool>;

    /// When the user was last sent a link, used to throttle resends.
    async fn last_issued_at(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>>;

    /// Invalidates every outstanding link, so verifying does not leave older
    /// emails live.
    async fn expire_all_for_user(&self, user_id: Uuid, now: DateTime<Utc>) -> AppResult<()>;
}

/// Denylist of refresh tokens that have been logged out.
///
/// A JWT is valid because it verifies, not because a server remembers it, so
/// signing out has to be recorded somewhere for the token to stop working. Only
/// the token id is stored, and only until the token would have expired anyway —
/// the list stays proportional to sign-outs in flight, not to users.
#[async_trait]
pub trait RevokedTokenStore: Send + Sync {
    /// Revokes `jti` until `expires_at`, after which the token is dead on its
    /// own and the entry is pointless. Revoking twice is not an error.
    async fn revoke(&self, jti: Uuid, expires_at: DateTime<Utc>) -> AppResult<()>;

    /// Whether the token has been revoked. Callers must treat an error as
    /// "cannot tell" and refuse the token rather than let it through.
    async fn is_revoked(&self, jti: Uuid) -> AppResult<bool>;
}
