//! CRM CRUD, and with it the list behaviour every module shares: filtering,
//! searching, pagination and sorting.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

async fn contact(app: &TestApp, first: &str, last: &str, status: &str, company: &str) -> String {
    app.create(
        "/crm/contacts",
        json!({
            "first_name": first,
            "last_name": last,
            "email": format!("{}@globex.test", first.to_lowercase()),
            "company_id": company,
            "status": status
        }),
    )
    .await
}

#[sqlx::test]
async fn a_contact_round_trips_through_create_update_and_delete(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let company = app.customer().await;
    let id = contact(&app, "Hank", "Scorpio", "customer", &company).await;

    let updated = app.put(&format!("/crm/contacts/{id}"), json!({ "job_title": "CEO" })).await;
    assert_eq!(updated.field("job_title"), "CEO");
    // A partial update leaves the fields it did not mention alone.
    assert_eq!(updated.field("last_name"), "Scorpio");

    assert!(app.delete(&format!("/crm/contacts/{id}")).await.status.is_success());
    assert_eq!(app.get(&format!("/crm/contacts/{id}")).await.status, 404);
}

#[sqlx::test]
async fn contacts_can_be_filtered_and_searched(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let company = app.customer().await;
    contact(&app, "Hank", "Scorpio", "customer", &company).await;
    contact(&app, "Waylon", "Smithers", "lead", &company).await;

    let by_status = app.get("/crm/contacts?status=customer").await;
    assert_eq!(by_status.rows().len(), 1);
    assert_eq!(by_status.rows()[0]["last_name"], "Scorpio");

    // Search is case-insensitive and spans the name fields.
    let by_search = app.get("/crm/contacts?search=scorpio").await;
    assert_eq!(by_search.rows().len(), 1);

    assert_eq!(app.get("/crm/contacts?search=nobody").await.rows().len(), 0);
}

#[sqlx::test]
async fn an_unknown_status_is_rejected(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let response = app
        .post("/crm/contacts", json!({ "first_name": "A", "last_name": "B", "status": "vip" }))
        .await;

    assert_eq!(response.status, 422);
}

#[sqlx::test]
async fn the_pipeline_groups_opportunities_by_stage(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let company = app.customer().await;
    for (title, stage, value) in
        [("Q4 expansion", "proposal", 25000), ("Renewal", "proposal", 5000), ("New site", "qualified", 9000)]
    {
        app.post(
            "/crm/opportunities",
            json!({ "title": title, "company_id": company, "value": value, "stage": stage }),
        )
        .await;
    }

    let pipeline = app.get("/crm/opportunities/pipeline").await;

    let proposal = pipeline
        .rows()
        .iter()
        .find(|row| row["stage"] == "proposal")
        .expect("no proposal stage in the pipeline");
    assert_eq!(proposal["count"], 2);
    assert_eq!(proposal["value"].as_str().unwrap().parse::<f64>().unwrap(), 30000.0);
}

#[sqlx::test]
async fn lists_are_paginated(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let company = app.customer().await;
    for index in 0..3 {
        contact(&app, &format!("Person{index}"), "Test", "lead", &company).await;
    }

    let page = app.get("/crm/contacts?page=1&per_page=2").await;
    assert_eq!(page.rows().len(), 2);
    assert_eq!(page.pagination()["total"], 3);
    assert_eq!(page.pagination()["total_pages"], 2);

    assert_eq!(app.get("/crm/contacts?page=2&per_page=2").await.rows().len(), 1);

    // `per_page` is clamped so one request cannot ask for the whole table.
    let huge = app.get("/crm/contacts?page=1&per_page=100000").await;
    assert_eq!(huge.pagination()["per_page"], 200);
}

#[sqlx::test]
async fn sorting_is_restricted_to_known_columns(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let company = app.customer().await;
    contact(&app, "Aaron", "Aardvark", "lead", &company).await;
    contact(&app, "Zoe", "Zimmerman", "lead", &company).await;

    let ascending = app.get("/crm/contacts?sort=last_name").await;
    assert_eq!(ascending.rows()[0]["last_name"], "Aardvark");

    let descending = app.get("/crm/contacts?sort=-last_name").await;
    assert_eq!(descending.rows()[0]["last_name"], "Zimmerman");

    // A column name cannot be a bound parameter, so anything outside the
    // allow-list must fall back to the default rather than reach SQL.
    let injected = app.get("/crm/contacts?sort=last_name;DROP%20TABLE%20users").await;
    assert!(injected.status.is_success());
    assert_eq!(injected.rows().len(), 2);
    assert!(app.get("/users").await.status.is_success(), "the users table is gone");
}

#[sqlx::test]
async fn a_missing_record_is_a_404_not_a_500(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let missing = app.get("/crm/contacts/00000000-0000-0000-0000-000000000000").await;
    assert_eq!(missing.status, 404);

    // A malformed id is the client's mistake, not a server error.
    assert_eq!(app.get("/crm/contacts/not-a-uuid").await.status, 400);
}
