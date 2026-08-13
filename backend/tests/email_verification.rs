//! Email verification.
//!
//! `users.email_verified` existed from the first migration and was never once
//! set — the schema advertised a feature that was not there. These tests pin the
//! flow that fills it in, and the things it deliberately does not do: it gates
//! nothing, and it never reveals whether an address is registered.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// Pulls the token out of the link, the way a person clicking it would.
fn token_from_email(app: &TestApp, address: &str) -> String {
    let message = app.email.last_to(address).expect("no verification email was sent");
    let (_, token) = message.body.split_once("verify-email?token=").expect("no link in the email");
    token.split_whitespace().next().unwrap().to_string()
}

async fn register(app: &TestApp, email: &str) {
    let response = app.register(email, "supersecret1", "New", "Person").await;
    assert!(response.status.is_success(), "{}", response.body);
}

/// Whether the account behind `email` reports itself verified.
async fn is_verified(app: &TestApp, email: &str, password: &str) -> bool {
    let login = app.post_anon("/auth/login", json!({ "email": email, "password": password })).await;
    assert!(login.status.is_success(), "{}", login.body);
    let token = login.field("access_token");
    app.get_as(&token, "/users/me").await.data()["email_verified"] == true
}

#[sqlx::test]
async fn registering_sends_a_link_that_confirms_the_address(pool: PgPool) {
    let app = TestApp::new(pool).await;
    register(&app, "new@erp.test").await;

    // A fresh account is unverified, and says so.
    assert!(!is_verified(&app, "new@erp.test", "supersecret1").await);

    let token = token_from_email(&app, "new@erp.test");
    let verified = app.post_anon("/auth/verify-email", json!({ "token": token })).await;

    assert!(verified.status.is_success(), "{}", verified.body);
    assert_eq!(verified.data()["email_verified"], true);
    // The response names the address, so the screen can say which one was confirmed.
    assert_eq!(verified.field("email"), "new@erp.test");

    assert!(is_verified(&app, "new@erp.test", "supersecret1").await);
}

#[sqlx::test]
async fn an_unverified_account_can_still_do_everything(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // Verification is recorded, not enforced. Gating sign-in would have locked
    // out every account that existed before this feature shipped.
    let customer = app.customer().await;
    assert!(!customer.is_empty());
    assert!(!is_verified(&app, "admin@erp.test", "supersecret1").await);
}

#[sqlx::test]
async fn a_link_works_exactly_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    register(&app, "new@erp.test").await;
    let token = token_from_email(&app, "new@erp.test");

    assert!(app
        .post_anon("/auth/verify-email", json!({ "token": token.clone() }))
        .await
        .status
        .is_success());

    // Spent. A link that kept working would be a standing credential sitting in
    // an inbox forever.
    let again = app.post_anon("/auth/verify-email", json!({ "token": token })).await;
    assert_eq!(again.status, 401);
}

#[sqlx::test]
async fn an_unknown_token_is_refused(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let response = app
        .post_anon("/auth/verify-email", json!({ "token": "0".repeat(64) }))
        .await;

    assert_eq!(response.status, 401);
    // Says nothing about whether the token was unknown, expired or spent.
    assert!(
        response.error_message().contains("invalid or has expired"),
        "{}",
        response.error_message()
    );
}

// Not covered here: verifying expires any *other* live link for the same
// account (`expire_all_for_user`). Reaching two live tokens means two issues
// more than the throttle's minute apart, and a test that sleeps for a minute
// is a test people start skipping. The single-use guarantee above is what
// protects the token that was actually clicked.

#[sqlx::test]
async fn resending_never_says_whether_the_address_exists(pool: PgPool) {
    let app = TestApp::new(pool).await;
    register(&app, "new@erp.test").await;
    let token = token_from_email(&app, "new@erp.test");
    app.post_anon("/auth/verify-email", json!({ "token": token })).await;

    // Unknown address, already-verified address, and a real unverified one all
    // answer identically — otherwise this is a way to test who has an account.
    let unknown = app
        .post_anon("/auth/resend-verification", json!({ "email": "nobody@erp.test" }))
        .await;
    let verified = app
        .post_anon("/auth/resend-verification", json!({ "email": "new@erp.test" }))
        .await;

    assert!(unknown.status.is_success() && verified.status.is_success());
    assert_eq!(unknown.field("message"), verified.field("message"));
}

#[sqlx::test]
async fn resending_does_not_flood_the_inbox(pool: PgPool) {
    let app = TestApp::new(pool).await;
    register(&app, "new@erp.test").await;

    for _ in 0..5 {
        let response =
            app.post_anon("/auth/resend-verification", json!({ "email": "new@erp.test" })).await;
        // Throttled or not, the caller is told the same thing — the throttle
        // must not become a way to distinguish accounts either.
        assert!(response.status.is_success());
    }

    // Just the one from registering. The link sent seconds ago is still in the
    // inbox, and the endpoint is public, so without this it is a way to bury
    // someone in mail.
    let sent = app
        .email
        .sent()
        .iter()
        .filter(|m| m.to == "new@erp.test" && m.subject.contains("Confirm"))
        .count();
    assert_eq!(sent, 1, "a resend within the throttle window sent another email");
}

#[sqlx::test]
async fn a_malformed_request_is_rejected_before_anything_is_looked_up(pool: PgPool) {
    let app = TestApp::new(pool).await;

    assert_eq!(app.post_anon("/auth/verify-email", json!({ "token": "" })).await.status, 422);
    assert_eq!(
        app.post_anon("/auth/resend-verification", json!({ "email": "not-an-address" })).await.status,
        422
    );
}
