use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::auth::domain::entities::EmailVerificationToken;
use crate::modules::auth::domain::repositories::EmailVerificationTokenRepository;

/// Mirrors `PgPasswordResetTokenRepository` down to the SQL, because the two
/// tables hold the same kind of credential under the same rules.
#[derive(Clone)]
pub struct PgEmailVerificationTokenRepository {
    pool: PgPool,
}

impl PgEmailVerificationTokenRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EmailVerificationTokenRepository for PgEmailVerificationTokenRepository {
    async fn create(&self, token: &EmailVerificationToken) -> AppResult<EmailVerificationToken> {
        let row = sqlx::query_as::<_, EmailVerificationToken>(
            r#"
            INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(token.id)
        .bind(token.user_id)
        .bind(&token.token_hash)
        .bind(token.expires_at)
        .bind(token.created_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    async fn find_by_hash(&self, token_hash: &str) -> AppResult<Option<EmailVerificationToken>> {
        let row = sqlx::query_as::<_, EmailVerificationToken>(
            "SELECT * FROM email_verification_tokens WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    async fn mark_used(&self, id: Uuid, now: DateTime<Utc>) -> AppResult<bool> {
        // `used_at IS NULL` in the WHERE clause is what makes this safe against
        // two requests arriving with the same link: only one updates a row.
        let result = sqlx::query(
            "UPDATE email_verification_tokens SET used_at = $2 WHERE id = $1 AND used_at IS NULL",
        )
        .bind(id)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    async fn last_issued_at(&self, user_id: Uuid) -> AppResult<Option<DateTime<Utc>>> {
        let issued = sqlx::query_scalar::<_, DateTime<Utc>>(
            "SELECT created_at FROM email_verification_tokens
             WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(issued)
    }

    async fn expire_all_for_user(&self, user_id: Uuid, now: DateTime<Utc>) -> AppResult<()> {
        sqlx::query(
            "UPDATE email_verification_tokens SET used_at = $2
             WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind(user_id)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
