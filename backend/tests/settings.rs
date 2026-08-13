//! User administration and the organisation profile — the settings screens.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// Registers a second account and returns `(id, access token)`.
async fn colleague(app: &TestApp, email: &str) -> (String, String) {
    let registered = app.register(email, "supersecret1", "Bob", "Clerk").await;
    (registered.field("user.id"), registered.field("access_token"))
}

#[sqlx::test]
async fn an_admin_sees_every_account(pool: PgPool) {
    let app = TestApp::new(pool).await;
    colleague(&app, "bob@erp.test").await;

    let users = app.get("/users?per_page=50").await;

    assert_eq!(users.pagination()["total"], 2);
    // The list is what the settings screen renders, so it must carry the two
    // things the screen acts on.
    let bob = users.rows().iter().find(|row| row["email"] == "bob@erp.test").unwrap();
    assert_eq!(bob["role"], "user");
    assert_eq!(bob["is_active"], true);
}

#[sqlx::test]
async fn the_user_list_filters_searches_and_sorts_like_every_other_list(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (bob_id, _) = colleague(&app, "bob@erp.test").await;
    colleague(&app, "carol@erp.test").await;
    app.put(&format!("/users/{bob_id}/role"), json!({ "role": "accountant" })).await;
    app.put(&format!("/users/{bob_id}/status"), json!({ "is_active": false })).await;

    assert_eq!(app.get("/users?role=accountant").await.rows().len(), 1);
    assert_eq!(app.get("/users?is_active=false").await.rows().len(), 1);
    assert_eq!(app.get("/users?is_active=true").await.rows().len(), 2);

    let searched = app.get("/users?search=carol").await;
    assert_eq!(searched.rows().len(), 1);
    assert_eq!(searched.rows()[0]["email"], "carol@erp.test");

    let by_email = app.get("/users?sort=email").await;
    assert_eq!(by_email.rows()[0]["email"], "admin@erp.test");
    let descending = app.get("/users?sort=-email").await;
    assert_eq!(descending.rows()[0]["email"], "carol@erp.test");

    // The allow-list keeps an unknown column from reaching SQL.
    let injected = app.get("/users?sort=password_hash").await;
    assert!(injected.status.is_success());
    assert_eq!(injected.rows().len(), 3);
    assert!(!injected.body.to_string().contains("password_hash"));
}

#[sqlx::test]
async fn an_ordinary_user_cannot_list_accounts(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (_, token) = colleague(&app, "bob@erp.test").await;

    assert_eq!(app.get_as(&token, "/users").await.status, 403);
}

#[sqlx::test]
async fn an_admin_retires_and_restores_an_account(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (bob_id, _) = colleague(&app, "bob@erp.test").await;

    let deactivated =
        app.put(&format!("/users/{bob_id}/status"), json!({ "is_active": false })).await;
    assert_eq!(deactivated.field("is_active"), "false");

    // A retired account cannot sign in, but its records survive.
    let refused = app
        .post_anon("/auth/login", json!({ "email": "bob@erp.test", "password": "supersecret1" }))
        .await;
    assert_eq!(refused.status, 401);
    assert!(refused.error_message().contains("disabled"));

    let restored = app.put(&format!("/users/{bob_id}/status"), json!({ "is_active": true })).await;
    assert_eq!(restored.field("is_active"), "true");
    assert!(app
        .post_anon("/auth/login", json!({ "email": "bob@erp.test", "password": "supersecret1" }))
        .await
        .status
        .is_success());
}

#[sqlx::test]
async fn an_admin_cannot_deactivate_themselves(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let response = app
        .put(&format!("/users/{}/status", app.admin_id), json!({ "is_active": false }))
        .await;

    assert!(!response.status.is_success());
    // Locking the last administrator out is not recoverable from inside the app.
    assert_eq!(app.get("/users/me").await.field("is_active"), "true");
}

#[sqlx::test]
async fn changing_a_status_is_admin_only(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (bob_id, bob_token) = colleague(&app, "bob@erp.test").await;

    let response = app
        .put_as(&bob_token, &format!("/users/{bob_id}/status"), json!({ "is_active": false }))
        .await;

    assert_eq!(response.status, 403);
}

#[sqlx::test]
async fn a_user_edits_their_own_name(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let updated = app.put("/users/me", json!({ "first_name": "Ada", "last_name": "Lovelace" })).await;

    assert_eq!(updated.field("last_name"), "Lovelace");
    assert_eq!(app.get("/users/me").await.field("last_name"), "Lovelace");
    // Editing a name is not a route to editing a role.
    assert_eq!(app.get("/users/me").await.field("role"), "admin");
}

#[sqlx::test]
async fn a_blank_name_is_rejected(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let response = app.put("/users/me", json!({ "first_name": "", "last_name": "Admin" })).await;

    assert_eq!(response.status, 422);
}

#[sqlx::test]
async fn changing_your_password_needs_the_current_one(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let wrong = app
        .put(
            "/users/me/password",
            json!({ "current_password": "not-it", "new_password": "a-brand-new-one" }),
        )
        .await;
    assert_eq!(wrong.status, 401);
    assert!(wrong.error_message().contains("Current password"));

    // The old password still works, so nothing changed.
    assert!(app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await
        .status
        .is_success());
}

#[sqlx::test]
async fn changing_your_password_keeps_you_in_and_evicts_everyone_else(pool: PgPool) {
    let app = TestApp::new(pool).await;
    // A second session — the old laptop this is meant to sign out.
    let elsewhere = app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "supersecret1" }))
        .await
        .field("refresh_token");

    let changed = app
        .put(
            "/users/me/password",
            json!({ "current_password": "supersecret1", "new_password": "a-brand-new-one" }),
        )
        .await;
    assert!(changed.status.is_success());

    // The caller gets a fresh pair rather than being signed out by their own action.
    let renewed = app
        .post_anon("/auth/refresh", json!({ "refresh_token": changed.field("refresh_token") }))
        .await;
    assert!(renewed.status.is_success());

    let other_session = app.post_anon("/auth/refresh", json!({ "refresh_token": elsewhere })).await;
    assert_eq!(other_session.status, 401);

    assert!(app
        .post_anon("/auth/login", json!({ "email": "admin@erp.test", "password": "a-brand-new-one" }))
        .await
        .status
        .is_success());
}

#[sqlx::test]
async fn a_short_new_password_is_rejected(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let response = app
        .put(
            "/users/me/password",
            json!({ "current_password": "supersecret1", "new_password": "short" }),
        )
        .await;

    assert_eq!(response.status, 422);
}

#[sqlx::test]
async fn the_organisation_profile_starts_seeded_and_takes_edits(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // The migration seeds one row, so the screen always has something to edit.
    let seeded = app.get("/settings/organization").await;
    assert_eq!(seeded.field("name"), "My Company");
    assert_eq!(seeded.field("default_currency"), "USD");

    let updated = app
        .put(
            "/settings/organization",
            json!({
                "name": "Globex",
                "email": "hello@globex.test",
                "city": "Springfield",
                "default_currency": "eur"
            }),
        )
        .await;

    assert_eq!(updated.field("name"), "Globex");
    assert_eq!(updated.field("city"), "Springfield");
    // Currency codes are stored upper case whatever the form sent.
    assert_eq!(updated.field("default_currency"), "EUR");
}

#[sqlx::test]
async fn an_omitted_field_is_left_alone_and_a_blank_one_clears(pool: PgPool) {
    let app = TestApp::new(pool).await;
    app.put("/settings/organization", json!({ "name": "Globex", "phone": "555-0100" })).await;

    // `name` is not in this request and must survive it.
    let after = app.put("/settings/organization", json!({ "city": "Springfield" })).await;
    assert_eq!(after.field("name"), "Globex");
    assert_eq!(after.field("phone"), "555-0100");

    let cleared = app.put("/settings/organization", json!({ "phone": "" })).await;
    assert!(cleared.data()["phone"].is_null(), "a blank field should clear, not store \"\"");
}

#[sqlx::test]
async fn the_organisation_profile_is_readable_by_all_and_writable_by_admins(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (_, bob_token) = colleague(&app, "bob@erp.test").await;

    // Everyone sees the company name — it is on the documents they work with.
    assert!(app.get_as(&bob_token, "/settings/organization").await.status.is_success());

    let response = app
        .put_as(&bob_token, "/settings/organization", json!({ "name": "Bob's Company" }))
        .await;
    assert_eq!(response.status, 403);
    assert_eq!(app.get("/settings/organization").await.field("name"), "My Company");
}

#[sqlx::test]
async fn organisation_fields_are_validated(pool: PgPool) {
    let app = TestApp::new(pool).await;

    for body in [
        json!({ "name": "" }),
        json!({ "email": "not-an-email" }),
        json!({ "website": "not-a-url" }),
        json!({ "default_currency": "EUROS" }),
    ] {
        let response = app.put("/settings/organization", body.clone()).await;
        assert_eq!(response.status, 422, "accepted {body}");
    }
}
