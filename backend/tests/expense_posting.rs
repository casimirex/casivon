//! Expense reports reaching the books.
//!
//! The one cycle that was already complete before this work: `approved` commits
//! the business to the cost and to owing the employee, `reimbursed` settles it.
//! Both halves post, and either can be the one outstanding.

mod common;

use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;

const ROLES: [(&str, &str, &str); 10] = [
    ("1100", "Accounts receivable", "asset"),
    ("1000", "Bank", "asset"),
    ("4000", "Sales revenue", "revenue"),
    ("2100", "Tax payable", "liability"),
    ("4900", "Foreign exchange gain/loss", "revenue"),
    ("2000", "Accounts payable", "liability"),
    ("5000", "Cost of sales", "expense"),
    ("1300", "Purchase tax", "asset"),
    ("2200", "Employee payable", "liability"),
    ("5100", "Employee expense", "expense"),
];

const BANK: usize = 1;
const EMPLOYEE_PAYABLE: usize = 8;
const EMPLOYEE_EXPENSE: usize = 9;

async fn configure_posting(app: &TestApp) -> Vec<String> {
    let mut ids = Vec::new();
    for (code, name, account_type) in ROLES {
        ids.push(
            app.create(
                "/accounting/accounts",
                json!({ "account_code": code, "account_name": name, "account_type": account_type }),
            )
            .await,
        );
    }

    let response = app
        .put(
            "/accounting/posting-accounts",
            json!({
                "ar_account_id": ids[0], "bank_account_id": ids[1],
                "sales_revenue_account_id": ids[2], "tax_payable_account_id": ids[3],
                "fx_gain_loss_account_id": ids[4], "accounts_payable_account_id": ids[5],
                "cost_of_sales_account_id": ids[6], "purchase_tax_account_id": ids[7],
                "employee_payable_account_id": ids[8], "employee_expense_account_id": ids[9]
            }),
        )
        .await;
    assert!(response.status.is_success(), "mapping failed: {}", response.body);
    ids
}

async fn entries(app: &TestApp) -> Vec<Value> {
    app.get("/accounting/ledger-entries?per_page=100").await.rows().clone()
}

async fn net(app: &TestApp, account: &str) -> f64 {
    entries(app)
        .await
        .iter()
        .map(|entry| {
            let amount: f64 = entry["amount"].as_str().unwrap().parse().unwrap();
            match (entry["debit_account_id"].as_str(), entry["credit_account_id"].as_str()) {
                (Some(d), _) if d == account => amount,
                (_, Some(c)) if c == account => -amount,
                _ => 0.0,
            }
        })
        .sum()
}

/// A submitted report for `amount`, ready to approve.
async fn submitted_report(app: &TestApp, email: &str, amount: f64) -> String {
    let employee = app.employee(email).await;
    let report = app
        .create(
            "/hr/expense-reports",
            json!({
                "employee_id": employee,
                "description": "Client visit",
                "lines": [{
                    "expense_date": "2026-03-02", "category": "travel",
                    "description": "Taxi", "amount": amount
                }]
            }),
        )
        .await;

    app.advance(&format!("/hr/expense-reports/{report}/status"), &["submitted"]).await;
    report
}

#[sqlx::test]
async fn approving_owes_the_employee_and_reimbursing_settles_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;

    let report = submitted_report(&app, "lisa@erp.test", 87.50).await;

    // Submitting is not a commitment: it can still be rejected.
    assert!(entries(&app).await.is_empty(), "a submitted report posted");

    app.put(&format!("/hr/expense-reports/{report}/status"), json!({ "status": "approved" })).await;

    assert_eq!(net(&app, &ids[EMPLOYEE_EXPENSE]).await, 87.50, "the cost is incurred");
    assert_eq!(net(&app, &ids[EMPLOYEE_PAYABLE]).await, -87.50, "the employee is owed");
    assert_eq!(net(&app, &ids[BANK]).await, 0.0, "nothing has been paid out yet");

    app.put(&format!("/hr/expense-reports/{report}/status"), json!({ "status": "reimbursed" })).await;

    assert_eq!(net(&app, &ids[BANK]).await, -87.50, "the employee has been paid");
    assert_eq!(net(&app, &ids[EMPLOYEE_PAYABLE]).await, 0.0, "and is owed nothing");
    // The cost stays where it landed — reimbursing settles a debt, it is not a
    // second expense.
    assert_eq!(net(&app, &ids[EMPLOYEE_EXPENSE]).await, 87.50);
}

#[sqlx::test]
async fn a_rejected_report_never_reaches_the_books(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure_posting(&app).await;

    let report = submitted_report(&app, "bart@erp.test", 500.00).await;
    app.put(&format!("/hr/expense-reports/{report}/status"), json!({ "status": "rejected" })).await;

    // Nothing was ever committed to, so there is nothing to reverse either.
    assert!(entries(&app).await.is_empty(), "a rejected report posted");
}

#[sqlx::test]
async fn an_approved_report_cannot_be_deleted_out_from_under_its_entries(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure_posting(&app).await;

    let report = submitted_report(&app, "lisa@erp.test", 60.00).await;
    app.put(&format!("/hr/expense-reports/{report}/status"), json!({ "status": "approved" })).await;

    // The existing draft-only rule is what protects the posting: there is no
    // route out of `approved` that would leave entries behind for a document
    // that no longer exists.
    let refused = app.delete(&format!("/hr/expense-reports/{report}")).await;
    assert!(!refused.status.is_success(), "an approved report was deleted");
    assert_eq!(entries(&app).await.len(), 1, "the entry survived");
}

#[sqlx::test]
async fn reports_approved_before_posting_was_configured_are_repairable(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // Approved and reimbursed while posting was off.
    let settled = submitted_report(&app, "lisa@erp.test", 87.50).await;
    app.advance(
        &format!("/hr/expense-reports/{settled}/status"),
        &["approved", "reimbursed"],
    )
    .await;

    // And one only approved, so the two halves are outstanding independently.
    let owing = submitted_report(&app, "bart@erp.test", 40.00).await;
    app.advance(&format!("/hr/expense-reports/{owing}/status"), &["approved"]).await;

    assert!(entries(&app).await.is_empty());

    let ids = configure_posting(&app).await;

    let outstanding = app.get("/accounting/unposted").await;
    // Three: two approvals and one reimbursement.
    let expense_rows = outstanding.data()["documents"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["kind"] == "expense_report")
        .count();
    assert_eq!(expense_rows, 3, "{}", outstanding.body);

    let run = app.post("/accounting/post-unposted", json!({})).await;
    assert_eq!(run.field("expense_reports_posted"), "3");

    assert_eq!(net(&app, &ids[EMPLOYEE_EXPENSE]).await, 127.50, "both costs");
    assert_eq!(net(&app, &ids[BANK]).await, -87.50, "only the settled one was paid");
    assert_eq!(net(&app, &ids[EMPLOYEE_PAYABLE]).await, -40.00, "the other is still owed");

    // Running it again changes nothing.
    let again = app.post("/accounting/post-unposted", json!({})).await;
    assert_eq!(again.field("expense_reports_posted"), "0");
    assert_eq!(net(&app, &ids[EMPLOYEE_EXPENSE]).await, 127.50);
}

#[sqlx::test]
async fn a_foreign_currency_claim_is_reimbursed_in_base(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure_posting(&app).await;

    app.put("/settings/fx-rates", json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": "1.10" })).await;

    let employee = app.employee("andre@erp.test").await;
    let report = app
        .create(
            "/hr/expense-reports",
            json!({
                "employee_id": employee, "currency": "EUR", "description": "Berlin trip",
                "lines": [{
                    "expense_date": "2026-03-02", "category": "travel",
                    "description": "Taxi", "amount": 100.00
                }]
            }),
        )
        .await;
    app.advance(
        &format!("/hr/expense-reports/{report}/status"),
        &["submitted", "approved"],
    )
    .await;

    // The claim is EUR 100; what the business is out is USD 110.
    assert_eq!(net(&app, &ids[EMPLOYEE_EXPENSE]).await, 110.0);
    assert_eq!(net(&app, &ids[EMPLOYEE_PAYABLE]).await, -110.0);
    assert!(entries(&app).await.iter().all(|e| e["currency"] == "USD"));
}
