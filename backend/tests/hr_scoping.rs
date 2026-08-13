//! Whose leave and expenses you can see, and act on.
//!
//! Before this, none of these endpoints checked ownership anywhere — any signed
//! in user could read every claim in the company, delete somebody else's leave
//! request, edit their draft expenses, and file claims in their name.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// A login with an employee record linked to it.
struct Staff {
    token: String,
    employee: String,
}

async fn staff(app: &TestApp, email: &str, first: &str) -> Staff {
    let registered = app.register(email, "supersecret1", first, "Person").await;
    let user_id = registered.field("user.id");

    // The link the whole feature turns on. `employees.user_id` has existed
    // since the HR module was written; nothing populated it until now.
    let employee = app
        .create(
            "/hr/employees",
            json!({
                "first_name": first, "last_name": "Person", "email": email,
                "hire_date": "2024-01-15", "user_id": user_id, "salary": 50000
            }),
        )
        .await;

    Staff { token: registered.field("access_token"), employee }
}

fn leave(employee: &str, start: &str, end: &str) -> serde_json::Value {
    json!({ "employee_id": employee, "leave_type": "annual", "start_date": start, "end_date": end })
}

fn expense(employee: &str, amount: f64) -> serde_json::Value {
    json!({
        "employee_id": employee, "description": "Client visit",
        "lines": [{
            "expense_date": "2026-03-02", "category": "travel",
            "description": "Taxi", "amount": amount
        }]
    })
}

#[sqlx::test]
async fn an_employee_sees_only_their_own_records(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;
    let bart = staff(&app, "bart@erp.test", "Bart").await;

    // Filed by HR on each of their behalves, which HR may do.
    app.create("/hr/leave-requests", leave(&lisa.employee, "2026-08-10", "2026-08-14")).await;
    app.create("/hr/leave-requests", leave(&bart.employee, "2026-09-10", "2026-09-14")).await;
    app.create("/hr/expense-reports", expense(&lisa.employee, 42.50)).await;
    app.create("/hr/expense-reports", expense(&bart.employee, 99.00)).await;

    let hers = app.get_as(&lisa.token, "/hr/leave-requests").await;
    assert_eq!(hers.rows().len(), 1, "{}", hers.body);
    assert_eq!(hers.rows()[0]["employee_id"], lisa.employee.as_str());

    let claims = app.get_as(&lisa.token, "/hr/expense-reports").await;
    assert_eq!(claims.rows().len(), 1, "{}", claims.body);
    assert_eq!(claims.rows()[0]["employee_id"], lisa.employee.as_str());

    // HR sees both people's.
    assert_eq!(app.get("/hr/leave-requests").await.rows().len(), 2);
    assert_eq!(app.get("/hr/expense-reports").await.rows().len(), 2);
}

#[sqlx::test]
async fn a_colleagues_record_is_not_found_rather_than_forbidden(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;
    let bart = staff(&app, "bart@erp.test", "Bart").await;

    let his_leave = app.create("/hr/leave-requests", leave(&bart.employee, "2026-09-10", "2026-09-14")).await;
    let his_claim = app.create("/hr/expense-reports", expense(&bart.employee, 99.00)).await;

    // 404 and not 403: a forbidden would confirm the record exists, which is
    // what somebody probing for it wants to learn.
    for path in [
        format!("/hr/leave-requests/{his_leave}"),
        format!("/hr/expense-reports/{his_claim}"),
        format!("/hr/employees/{}", bart.employee),
        format!("/hr/employees/{}/leave-balance", bart.employee),
    ] {
        assert_eq!(app.get_as(&lisa.token, &path).await.status, 404, "{path}");
    }
}

#[sqlx::test]
async fn one_employee_cannot_delete_or_edit_anothers(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;
    let bart = staff(&app, "bart@erp.test", "Bart").await;

    let his_leave = app.create("/hr/leave-requests", leave(&bart.employee, "2026-09-10", "2026-09-14")).await;
    let his_claim = app.create("/hr/expense-reports", expense(&bart.employee, 99.00)).await;

    // The sharp end of the gap: writes, not just reads.
    let deleted = app
        .delete_as(&lisa.token, &format!("/hr/leave-requests/{his_leave}"))
        .await;
    assert_eq!(deleted.status, 404);

    let edited = app
        .put_as(
            &lisa.token,
            &format!("/hr/expense-reports/{his_claim}"),
            json!({ "description": "Rewritten by someone else" }),
        )
        .await;
    assert_eq!(edited.status, 404);

    // Both survived untouched.
    assert!(app.get(&format!("/hr/leave-requests/{his_leave}")).await.status.is_success());
    assert_eq!(
        app.get(&format!("/hr/expense-reports/{his_claim}")).await.field("description"),
        "Client visit"
    );
}

#[sqlx::test]
async fn filing_in_someone_elses_name_is_refused(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;
    let bart = staff(&app, "bart@erp.test", "Bart").await;

    let as_bart = app
        .post_as(&lisa.token, "/hr/leave-requests", leave(&bart.employee, "2026-09-10", "2026-09-14"))
        .await;

    // Refused rather than quietly rewritten to her own id: silently correcting
    // the payload would report success for a request nobody made.
    assert_eq!(as_bart.status, 403);
    assert!(as_bart.error_message().contains("only file this for yourself"), "{}", as_bart.error_message());

    let claim_as_bart = app
        .post_as(&lisa.token, "/hr/expense-reports", expense(&bart.employee, 99.00))
        .await;
    assert_eq!(claim_as_bart.status, 403);

    // Her own goes through.
    let hers = app
        .post_as(&lisa.token, "/hr/leave-requests", leave(&lisa.employee, "2026-08-10", "2026-08-14"))
        .await;
    assert!(hers.status.is_success(), "{}", hers.body);

    // And nothing was filed against Bart.
    assert!(app
        .get("/hr/leave-requests")
        .await
        .rows()
        .iter()
        .all(|row| row["employee_id"] != bart.employee.as_str()));
}

#[sqlx::test]
async fn a_login_with_no_employee_record_has_no_records_of_its_own(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;
    app.create("/hr/leave-requests", leave(&lisa.employee, "2026-08-10", "2026-08-14")).await;

    // A contractor or service account: signed in, no HR role, nothing linked.
    let outsider = app.register("outsider@erp.test", "supersecret1", "Ozzy", "Outsider").await;
    let token = outsider.field("access_token");

    assert!(app.get_as(&token, "/hr/leave-requests").await.rows().is_empty());
    assert!(app.get_as(&token, "/hr/expense-reports").await.rows().is_empty());

    // And it says why rather than failing obscurely.
    let filed = app
        .post_as(&token, "/hr/leave-requests", leave(&lisa.employee, "2026-10-10", "2026-10-14"))
        .await;
    assert_eq!(filed.status, 403);
    assert!(filed.error_message().contains("not linked to an employee"), "{}", filed.error_message());
}

#[sqlx::test]
async fn an_employee_can_still_read_their_own_profile(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;

    // Gating the directory to HR in the previous change also stopped an
    // employee reading their own record — everyday self-service that worked
    // before, and does again.
    let profile = app.get_as(&lisa.token, &format!("/hr/employees/{}", lisa.employee)).await;
    assert!(profile.status.is_success(), "{}", profile.body);
    assert_eq!(profile.field("email"), "lisa@erp.test");

    let balance = app
        .get_as(&lisa.token, &format!("/hr/employees/{}/leave-balance", lisa.employee))
        .await;
    assert!(balance.status.is_success(), "{}", balance.body);

    // The directory as a whole is still HR-only.
    assert_eq!(app.get_as(&lisa.token, "/hr/employees").await.status, 403);
}

#[sqlx::test]
async fn hr_still_acts_for_everyone(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;

    // Filing on behalf, reading, and approving all still work for HR — the
    // point was to scope ordinary users, not to break the people whose job
    // this is.
    let request = app.create("/hr/leave-requests", leave(&lisa.employee, "2026-08-10", "2026-08-14")).await;
    assert!(app
        .put(&format!("/hr/leave-requests/{request}/decision"), json!({ "status": "approved" }))
        .await
        .status
        .is_success());

    let claim = app.create("/hr/expense-reports", expense(&lisa.employee, 42.50)).await;
    assert!(app.get(&format!("/hr/expense-reports/{claim}")).await.status.is_success());
}
