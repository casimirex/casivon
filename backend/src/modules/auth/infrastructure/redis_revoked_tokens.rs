use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::auth::domain::repositories::RevokedTokenStore;

/// Redis-backed denylist.
///
/// Redis is the right home for this: every entry is worthless past its expiry,
/// and `SET ... EX` makes that the storage engine's problem instead of a
/// scheduled clean-up job.
pub struct RedisRevokedTokens {
    client: redis::Client,
}

impl RedisRevokedTokens {
    pub fn connect(redis_url: &str) -> anyhow::Result<Self> {
        // `Client::open` only parses the URL; connections are opened per call.
        Ok(Self { client: redis::Client::open(redis_url)? })
    }

    fn key(jti: Uuid) -> String {
        format!("revoked_token:{jti}")
    }

    async fn connection(&self) -> AppResult<redis::aio::Connection> {
        self.client.get_async_connection().await.map_err(|e| {
            tracing::error!("token denylist unavailable: {e}");
            AppError::Auth("Sign-out service unavailable, please try again".to_string())
        })
    }
}

#[async_trait]
impl RevokedTokenStore for RedisRevokedTokens {
    async fn revoke(&self, jti: Uuid, expires_at: DateTime<Utc>) -> AppResult<()> {
        let remaining_ms = (expires_at - Utc::now()).num_milliseconds();
        if remaining_ms <= 0 {
            // Already expired: the signature check will reject it regardless.
            return Ok(());
        }
        // Round the TTL up. Truncating would let the entry lapse up to a second
        // before the token it revokes, leaving a window where a signed-out token
        // is accepted again.
        let ttl = (remaining_ms as u64).div_ceil(1000);

        let mut connection = self.connection().await?;
        connection
            .set_ex::<_, _, ()>(Self::key(jti), 1, ttl)
            .await
            .map_err(|e| {
                tracing::error!("failed to revoke token: {e}");
                // Reporting success here would tell someone they had signed out
                // while their token stayed usable.
                AppError::Auth("Sign-out could not be recorded, please try again".to_string())
            })
    }

    async fn is_revoked(&self, jti: Uuid) -> AppResult<bool> {
        let mut connection = self.connection().await?;
        connection.exists(Self::key(jti)).await.map_err(|e| {
            tracing::error!("failed to read the token denylist: {e}");
            AppError::Auth("Could not verify the session, please sign in again".to_string())
        })
    }
}
