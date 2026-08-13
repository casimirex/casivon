//! Leave entitlement and expense approval — both are single-decision workflows
//! where letting a request through twice costs real money.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

fn leave(employee: &str, start: &str, end: &str) -> serde_json::Value {
    json!({ "employee_id": employee, "leave_type": "annual", "start_date": start, "end_date": end })
}

#[sqlx::test]
async fn leave_is_counted_in_working_days(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let employee = app.employee("lisa@erp.test").await;

    // Monday 10 Aug to Friday 14 Aug 2026 is five working days...
    let week = app.post("/hr/leave-requests", leave(&employee, "2026-08-10", "2026-08-14")).await;
    assert_eq!(week.field("days_requested"), "5");

    // ...and stretching it over the weekend to the Monday adds only one.
    let employee = app.employee("bart@erp.test").await;
    let with_weekend =
        app.post("/hr/leave-requests", leave(&employee, "2026-08-10", "2026-08-17")).await;
    assert_eq!(with_weekend.field("days_requested"), "6");
}

#[sqlx::test]
async fn leave_cannot_overlap_an_existing_request(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let employee = app.employee("lisa@erp.test").await;
    app.post("/hr/leave-requests", leave(&employee, "2026-08-10", "2026-08-14")).await;

    let overlapping =
        app.post("/hr/leave-requests", leave(&employee, "2026-08-12", "2026-08-16")).await;
    assert!(!overlapping.status.is_success());

    // A different employee over the same dates is fine.
    let colleague = app.employee("bart@erp.test").await;
    let separate =
        app.post("/hr/leave-requests", leave(&colleague, "2026-08-12", "2026-08-16")).await;
    assert!(separate.status.is_success());
}

#[sqlx::test]
async fn leave_ends_after_it_starts(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let employee = app.employee("lisa@erp.test").await;

    let backwards = app.post("/hr/leave-requests", leave(&employee, "2026-08-14", "2026-08-10")).await;

    assert!(!backwards.status.is_success());
}

#[sqlx::test]
async fn approved_leave_draws_down_the_balance(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let employee = app.employee("lisa@erp.test").await; // 25 days entitlement
    let request = app.create("/hr/leave-requests", leave(&employee, "2026-08-10", "2026-08-14")).await;

    // Pending leave is not yet taken.
    let before = app.get(&format!("/hr/employees/{employee}/leave-balance")).await;
    assert_eq!(before.field("taken"), "0");

    app.put(&format!("/hr/leave-requests/{request}/decision"), json!({ "status": "approved" })).await;

    let after = app.get(&format!("/hr/employees/{employee}/leave-balance")).await;
    assert_eq!(after.field("entitlement"), "25");
    assert_eq!(after.field("taken"), "5");
    assert_eq!(after.field("remaining"), "20");
}

#[sqlx::test]
async fn leave_is_decided_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let employee = app.employee("lisa@erp.test").await;
    let request = app.create("/hr/leave-requests", leave(&employee, "2026-08-10", "2026-08-14")).await;
    app.put(&format!("/hr/leave-requests/{request}/decision"), json!({ "status": "approved" })).await;

    let reversal = app
        .put(&format!("/hr/leave-requests/{request}/decision"), json!({ "status": "rejected" }))
        .await;

    assert!(!reversal.status.is_success());
    assert_eq!(app.get(&format!("/hr/employees/{employee}/leave-balance")).await.field("taken"), "5");
}

#[sqlx::test]
async fn an_expense_report_totals_its_lines_and_follows_the_approval_chain(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let employee = app.employee("lisa@erp.test").await;

    let report = app
        .post(
            "/hr/expense-reports",
            json!({
                "employee_id": employee,
                "description": "Client visit",
                "lines": [
                    { "expense_date": "2026-08-02", "category": "travel", "description": "Taxi", "amount": 42.50 },
                    { "expense_date": "2026-08-02", "category": "meals", "description": "Lunch", "amount": 18.25 }
                ]
            }),
        )
        .await;
    let report_id = report.id();
    report.assert_money("total_amount", "60.75");
    assert!(report.field("report_number").starts_with("EXP-"));

    // Money cannot leave before someone approves it.
    let early = app
        .put(&format!("/hr/expense-reports/{report_id}/status"), json!({ "status": "reimbursed" }))
        .await;
    assert!(!early.status.is_success());

    app.advance(
        &format!("/hr/expense-reports/{report_id}/status"),
        &["submitted", "approved", "reimbursed"],
    )
    .await;
    assert_eq!(app.get(&format!("/hr/expense-reports/{report_id}")).await.field("status"), "reimbursed");
}

#[sqlx::test]
async fn approvals_are_closed_to_users_without_an_hr_role(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let employee = app.employee("lisa@erp.test").await;
    let request = app.create("/hr/leave-requests", leave(&employee, "2026-08-10", "2026-08-14")).await;
    let bob = app.register("bob@erp.test", "supersecret1", "Bob", "Clerk").await;

    let response = app
        .put_as(
            &bob.field("access_token"),
            &format!("/hr/leave-requests/{request}/decision"),
            json!({ "status": "approved" }),
        )
        .await;

    assert_eq!(response.status, 403);
}

#[sqlx::test]
async fn the_employee_directory_is_closed_to_users_without_an_hr_role(pool: PgPool) {
    let app = TestApp::new(pool).await;
    app.employee("lisa@erp.test").await;
    let bob = app.register("bob@erp.test", "supersecret1", "Bob", "Clerk").await;
    let token = bob.field("access_token");

    // These return `salary`. They were readable by any signed-in user until the
    // global search work audited the handlers and found the gate was only ever
    // on the writes — the UI hid the screen and the API did not.
    assert_eq!(app.get_as(&token, "/hr/employees").await.status, 403);

    // An HR role still gets through, so the fix is a gate rather than a wall.
    assert!(app.get("/hr/employees").await.status.is_success());
}
