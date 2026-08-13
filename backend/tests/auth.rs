//! Registration, token handling and role-based access.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn first_account_becomes_admin_and_the_rest_do_not(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // `TestApp::new` registered the first account.
    let me = app.get("/users/me").await;
    assert_eq!(me.field("role"), "admin");

    let second = app.register("bob@erp.test", "supersecret1", "Bob", "Clerk").await;
    assert_eq!(second.field("user.role"), "user");
}

#[sqlx::test]
async fn registration_rejects_invalid_input_field_by_field(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let response = app
        .post_anon(
            "/auth/register",
            json!({ "email": "nope", "password": "x", "first_name": "", "last_name": "" }),
        )
        .await;

    assert_eq!(response.status, 422);
    let message = response.error_message();
    assert!(message.contains("email"), "{message}");
    assert!(message.contains("password"), "{message}");
}

#[sqlx::test]
async fn an_email_can_only_be_registered_once(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let duplicate = app.register("admin@erp.test", "supersecret1", "Imposter", "Admin").await;

    assert!(!duplicate.status.is_success());
    assert!(duplicate.error_message().to_lowercase().contains("already"));
}

#[sqlx::test]
async fn login_returns_a_session_and_rejects_a_wrong_password(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let ok = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await;
    assert!(!ok.field("access_token").is_empty());

    let bad = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "wrong-password" }))
        .await;
    assert_eq!(bad.status, 401);
}

#[sqlx::test]
async fn a_refresh_token_is_not_accepted_as_an_access_token(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let session = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await;
    let refresh = session.field("refresh_token");

    // Both tokens are signed with the same key, so only the `typ` claim stops a
    // refresh token from being used to read data.
    let response = app.get_as(&refresh, "/users/me").await;
    assert_eq!(response.status, 401);

    let renewed = app.post_anon("/auth/refresh", json!({ "refresh_token": refresh })).await;
    assert_eq!(renewed.field("token_type"), "Bearer");
    assert!(!renewed.field("access_token").is_empty());
}

#[sqlx::test]
async fn unauthenticated_requests_are_refused(pool: PgPool) {
    let app = TestApp::new(pool).await;

    assert_eq!(app.get_anon("/crm/contacts").await.status, 401);
    assert_eq!(app.get_as("not-a-jwt", "/crm/contacts").await.status, 401);
}

#[sqlx::test]
async fn role_gated_modules_open_only_after_a_grant(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let bob = app.register("bob@erp.test", "supersecret1", "Bob", "Clerk").await;
    let (bob_id, bob_token) = (bob.field("user.id"), bob.field("access_token"));

    let account = json!({ "account_code": "9999", "account_name": "Sneaky", "account_type": "asset" });
    assert_eq!(app.post_as(&bob_token, "/accounting/accounts", account.clone()).await.status, 403);

    let granted = app.put(&format!("/users/{bob_id}/role"), json!({ "role": "accountant" })).await;
    assert_eq!(granted.field("role"), "accountant");

    // The old token still carries `role: user`; the client re-logs in after a
    // grant, which is what the frontend does.
    let bob_token = app
        .post_anon("/auth/login", json!({ "email": "bob@erp.test", "password": "supersecret1" }))
        .await
        .field("access_token");
    assert!(app.post_as(&bob_token, "/accounting/accounts", account).await.status.is_success());
}

#[sqlx::test]
async fn an_admin_cannot_demote_themselves(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let response = app
        .put(&format!("/users/{}/role", app.admin_id), json!({ "role": "user" }))
        .await;

    assert!(!response.status.is_success());
    assert_eq!(app.get("/users/me").await.field("role"), "admin");
}

#[sqlx::test]
async fn granting_a_role_requires_an_admin(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let bob = app.register("bob@erp.test", "supersecret1", "Bob", "Clerk").await;
    let bob_token = bob.field("access_token");

    let response = app
        .put_as(&bob_token, &format!("/users/{}/role", bob.field("user.id")), json!({ "role": "admin" }))
        .await;

    assert_eq!(response.status, 403);
}

#[sqlx::test]
async fn signing_out_stops_the_refresh_token_working(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let session = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await;
    let refresh = session.field("refresh_token");

    // It works right up until the moment it is revoked.
    assert!(app
        .post_anon("/auth/refresh", json!({ "refresh_token": refresh }))
        .await
        .status
        .is_success());

    let signed_out = app.post_anon("/auth/logout", json!({ "refresh_token": refresh })).await;
    assert!(signed_out.status.is_success());
    assert_eq!(signed_out.data()["signed_out"], true);

    let after = app.post_anon("/auth/refresh", json!({ "refresh_token": refresh })).await;
    assert_eq!(after.status, 401);
    assert!(after.error_message().contains("signed out"), "{}", after.error_message());
}

#[sqlx::test]
async fn signing_out_is_idempotent(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let refresh = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await
        .field("refresh_token");

    app.post_anon("/auth/logout", json!({ "refresh_token": refresh.clone() })).await;

    // A client retrying after a dropped connection should not see an error for
    // having succeeded twice.
    let again = app.post_anon("/auth/logout", json!({ "refresh_token": refresh })).await;
    assert!(again.status.is_success());
}

#[sqlx::test]
async fn signing_out_one_session_leaves_the_others_alone(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let login = || {
        app.post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
    };
    let laptop = login().await.field("refresh_token");
    let phone = login().await.field("refresh_token");

    app.post_anon("/auth/logout", json!({ "refresh_token": laptop.clone() })).await;

    assert_eq!(app.post_anon("/auth/refresh", json!({ "refresh_token": laptop })).await.status, 401);
    assert!(
        app.post_anon("/auth/refresh", json!({ "refresh_token": phone })).await.status.is_success(),
        "signing out one device ended a session on another"
    );
}

#[sqlx::test]
async fn signing_out_needs_a_real_refresh_token(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let session = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await;

    assert_eq!(app.post_anon("/auth/logout", json!({ "refresh_token": "garbage" })).await.status, 401);
    assert_eq!(app.post_anon("/auth/logout", json!({})).await.status, 400);
    assert_eq!(
        app.post_anon("/auth/logout", json!({ "refresh_token": "" })).await.status,
        422,
        "an empty token should fail validation, not verification"
    );

    // The access token is not a refresh token, and the `typ` claim says so.
    let with_access_token = app
        .post_anon("/auth/logout", json!({ "refresh_token": session.field("access_token") }))
        .await;
    assert_eq!(with_access_token.status, 401);
}

/// Documents a deliberate limit rather than an oversight: revoking access
/// tokens too would mean a denylist lookup on every authenticated request, so
/// they are left to expire on their own instead.
#[sqlx::test]
async fn signing_out_leaves_the_access_token_valid_until_it_expires(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let session = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await;
    let access = session.field("access_token");

    app.post_anon("/auth/logout", json!({ "refresh_token": session.field("refresh_token") })).await;

    assert!(app.get_as(&access, "/users/me").await.status.is_success());
}

#[sqlx::test]
async fn a_password_hash_is_never_serialised(pool: PgPool) {
    let app = TestApp::new(pool).await;
    app.register("bob@erp.test", "supersecret1", "Bob", "Clerk").await;

    // The entity carries the hash, so the risk is a handler returning it by
    // accident; check every shape that can carry a user.
    for path in ["/users?per_page=50", "/users/me"] {
        let body = app.get(path).await.body.to_string();
        assert!(!body.contains("password_hash"), "{path} leaked the hash: {body}");
        assert!(!body.contains("$argon2"), "{path} leaked the hash: {body}");
    }

    let login = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await;
    assert!(!login.body.to_string().contains("password_hash"));
}
