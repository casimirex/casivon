//! The rest of the suite runs the denylist in memory so tests stay hermetic;
//! this file checks the Redis implementation the application actually ships.
//!
//! Needs the Redis from `docker compose up -d redis`. `REDIS_URL` is read from
//! `backend/.env`, falling back to localhost.

use chrono::{Duration, Utc};
use casivon_backend::modules::auth::domain::repositories::RevokedTokenStore;
use casivon_backend::modules::auth::infrastructure::redis_revoked_tokens::RedisRevokedTokens;
use uuid::Uuid;

fn store() -> RedisRevokedTokens {
    let url = dotenvy::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    RedisRevokedTokens::connect(&url).expect("failed to parse REDIS_URL")
}

#[tokio::test]
async fn a_revoked_token_reads_back_as_revoked() {
    let store = store();
    let jti = Uuid::new_v4();

    assert!(!store.is_revoked(jti).await.unwrap(), "an untouched token is not revoked");

    store.revoke(jti, Utc::now() + Duration::minutes(5)).await.unwrap();

    assert!(store.is_revoked(jti).await.unwrap());
    // Revoking one session must not touch another.
    assert!(!store.is_revoked(Uuid::new_v4()).await.unwrap());
}

#[tokio::test]
async fn an_entry_expires_with_its_token() {
    let store = store();
    let jti = Uuid::new_v4();

    // Redis expresses TTLs in whole seconds, so this is about as short as the
    // test can be while still observing the entry before it lapses.
    store.revoke(jti, Utc::now() + Duration::seconds(2)).await.unwrap();
    assert!(store.is_revoked(jti).await.unwrap());

    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

    // Once the token would have expired anyway, keeping the entry only wastes
    // memory — Redis drops it for us.
    assert!(!store.is_revoked(jti).await.unwrap());
}

#[tokio::test]
async fn revoking_an_already_expired_token_writes_nothing() {
    let store = store();
    let jti = Uuid::new_v4();

    // A TTL in the past is not a valid `SET ... EX` argument, so this has to be
    // handled before it reaches Redis.
    store.revoke(jti, Utc::now() - Duration::hours(1)).await.unwrap();

    assert!(!store.is_revoked(jti).await.unwrap());
}
