//! Project progress and hours are derived from tasks and time entries, never
//! set directly, so these tests drive the derivation.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

async fn project(app: &TestApp) -> String {
    app.create("/projects", json!({ "name": "ERP Rollout", "priority": "high", "budget": 50000 })).await
}

#[sqlx::test]
async fn a_task_cannot_skip_straight_to_done(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let project = project(&app).await;
    let task = app
        .create("/projects/tasks", json!({ "project_id": project, "title": "Design schema" }))
        .await;

    let skipped = app.put(&format!("/projects/tasks/{task}/status"), json!({ "status": "done" })).await;
    assert!(!skipped.status.is_success());

    app.advance(&format!("/projects/tasks/{task}/status"), &["in_progress", "review", "done"]).await;
    assert_eq!(app.get(&format!("/projects/tasks/{task}")).await.field("status"), "done");
}

#[sqlx::test]
async fn project_progress_is_the_average_of_its_tasks(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let project = project(&app).await;
    let first = app
        .create("/projects/tasks", json!({ "project_id": project, "title": "Design schema", "estimated_hours": 8 }))
        .await;
    app.create("/projects/tasks", json!({ "project_id": project, "title": "Build API" })).await;

    let done = app.put(&format!("/projects/tasks/{first}/status"), json!({ "status": "in_progress" })).await;
    assert!(done.status.is_success());
    app.advance(&format!("/projects/tasks/{first}/status"), &["review", "done"]).await;

    // One of two tasks complete: the other is still `todo` at 0%.
    let progress = app.get(&format!("/projects/{project}")).await.field("progress_percent");
    assert_eq!(progress, "50");
}

#[sqlx::test]
async fn a_cancelled_task_is_left_out_of_the_average(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let project = project(&app).await;
    let done_task = app
        .create("/projects/tasks", json!({ "project_id": project, "title": "Design schema" }))
        .await;
    let cancelled = app
        .create("/projects/tasks", json!({ "project_id": project, "title": "Abandoned idea" }))
        .await;

    app.advance(&format!("/projects/tasks/{done_task}/status"), &["in_progress", "review", "done"]).await;
    app.put(&format!("/projects/tasks/{cancelled}/status"), json!({ "status": "cancelled" })).await;

    // Counting the cancelled task would report 50%; it is not work anyone owes.
    assert_eq!(app.get(&format!("/projects/{project}")).await.field("progress_percent"), "100");
}

#[sqlx::test]
async fn time_entries_roll_up_into_task_and_project_hours(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let project = project(&app).await;
    let employee = app.employee("lisa@erp.test").await;
    let task = app
        .create("/projects/tasks", json!({ "project_id": project, "title": "Design schema", "estimated_hours": 8 }))
        .await;

    for (date, hours, billable) in
        [("2026-08-03", 6.5, true), ("2026-08-04", 2.0, false)]
    {
        app.post(
            "/projects/time-entries",
            json!({
                "task_id": task,
                "employee_id": employee,
                "entry_date": date,
                "hours": hours,
                "is_billable": billable
            }),
        )
        .await;
    }

    app.get(&format!("/projects/tasks/{task}")).await.assert_money("actual_hours", "8.5");

    let detail = app.get(&format!("/projects/{project}")).await;
    detail.assert_money("billable_hours", "6.5");
    detail.assert_money("non_billable_hours", "2");
}

#[sqlx::test]
async fn a_time_entry_cannot_exceed_a_day(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let project = project(&app).await;
    let employee = app.employee("lisa@erp.test").await;
    let task = app.create("/projects/tasks", json!({ "project_id": project, "title": "Design schema" })).await;

    let response = app
        .post(
            "/projects/time-entries",
            json!({ "task_id": task, "employee_id": employee, "entry_date": "2026-08-05", "hours": 25 }),
        )
        .await;

    assert_eq!(response.status, 422);
}

#[sqlx::test]
async fn deleting_a_time_entry_reduces_the_hours_again(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let project = project(&app).await;
    let employee = app.employee("lisa@erp.test").await;
    let task = app.create("/projects/tasks", json!({ "project_id": project, "title": "Design schema" })).await;
    let entry = app
        .create(
            "/projects/time-entries",
            json!({ "task_id": task, "employee_id": employee, "entry_date": "2026-08-03", "hours": 6.5 }),
        )
        .await;

    app.delete(&format!("/projects/time-entries/{entry}")).await;

    app.get(&format!("/projects/tasks/{task}")).await.assert_money("actual_hours", "0");
}
