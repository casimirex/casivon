use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

// Deliberately not `ToSchema`: `User` carries `password_hash` and is never a
// response body — handlers map it to `UserProfile` first. Documenting it would
// advertise a field the API never returns.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    pub org_id: Option<Uuid>,
    pub is_active: bool,
    pub email_verified: bool,
    /// Stamped into every token issued to this user. Bumped when the password
    /// changes, which ends every session carrying the previous value.
    pub session_epoch: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A single-use password reset link.
///
/// Only the hash of the token reaches the database; `token_hash` is never
/// compared against anything the client sends directly, but against the hash of
/// what it sends.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PasswordResetToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl PasswordResetToken {
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.used_at.is_none() && self.expires_at > now
    }
}

/// Confirms that an address reaches the person who claimed it.
///
/// The same shape as [`PasswordResetToken`], and deliberately so: both are
/// single-use bearer credentials delivered by email, and every rule that applies
/// to one — stored hashed, expiring, spendable exactly once — applies to the
/// other for the same reasons. Kept as its own type rather than shared, because
/// the two live in different tables and a single type would be ambiguous about
/// which.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EmailVerificationToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl EmailVerificationToken {
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.used_at.is_none() && self.expires_at > now
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserProfile {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub role: String,
    pub org_id: Option<Uuid>,
    pub is_active: bool,
    /// Whether the address has been confirmed. Nothing is gated on it, but the
    /// signed-in user is shown a prompt while it is false, so it has to reach
    /// the client — until now the column existed and no endpoint ever returned
    /// it.
    pub email_verified: bool,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserProfile {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            first_name: user.first_name,
            last_name: user.last_name,
            role: user.role,
            org_id: user.org_id,
            is_active: user.is_active,
            email_verified: user.email_verified,
            created_at: user.created_at,
        }
    }
}
