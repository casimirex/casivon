use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::modules::hr::domain::entities::*;

// ---------------------------------------------------------------- employees

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateEmployeeRequest {
    /// Links the employee record to a login. Optional — not every employee has one.
    pub user_id: Option<Uuid>,
    #[validate(length(max = 50))]
    pub employee_number: Option<String>,
    #[validate(length(min = 1, max = 100, message = "First name is required"))]
    pub first_name: String,
    #[validate(length(min = 1, max = 100, message = "Last name is required"))]
    pub last_name: String,
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
    #[validate(length(max = 50))]
    pub phone: Option<String>,
    pub hire_date: NaiveDate,
    #[validate(length(max = 100))]
    pub department: Option<String>,
    #[validate(length(max = 100))]
    pub job_title: Option<String>,
    pub manager_id: Option<Uuid>,
    pub salary: Option<Decimal>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    #[validate(range(min = 0, max = 365, message = "Entitlement must be between 0 and 365 days"))]
    pub annual_leave_entitlement: Option<i32>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateEmployeeRequest {
    #[validate(length(min = 1, max = 100))]
    pub first_name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub last_name: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    pub phone: Option<String>,
    pub department: Option<String>,
    pub job_title: Option<String>,
    pub manager_id: Option<Uuid>,
    pub salary: Option<Decimal>,
    pub termination_date: Option<NaiveDate>,
    #[validate(custom = "validate_employee_status")]
    pub status: Option<String>,
    #[validate(range(min = 0, max = 365))]
    pub annual_leave_entitlement: Option<i32>,
}

fn validate_employee_status(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &EmployeeStatus::ALL, "status")
}

// ----------------------------------------------------------- leave requests

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateLeaveRequest {
    pub employee_id: Uuid,
    #[validate(custom = "validate_leave_type")]
    pub leave_type: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    /// Defaults to the working days in the range when omitted.
    #[validate(range(min = 1, message = "At least one day must be requested"))]
    pub days_requested: Option<i32>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct DecideLeaveRequest {
    #[validate(custom = "validate_leave_decision")]
    pub status: String,
}

fn validate_leave_type(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &LeaveType::ALL, "leave_type")
}

fn validate_leave_decision(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &[LeaveStatus::APPROVED, LeaveStatus::REJECTED], "status")
}

// --------------------------------------------------------- expense reports

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct ExpenseLineRequest {
    pub expense_date: NaiveDate,
    #[validate(length(min = 1, max = 100, message = "Category is required"))]
    pub category: String,
    #[validate(length(min = 1, max = 1000, message = "Description is required"))]
    pub description: String,
    pub amount: Decimal,
    /// Superseded by `receipt_attachment_id`; still accepted so an existing
    /// client does not break.
    #[validate(url(message = "Receipt must be a valid URL"))]
    pub receipt_url: Option<String>,
    /// The id returned by `POST /api/v1/files`. Must be a file this caller
    /// uploaded — attaching somebody else's is refused, since attaching it is
    /// what would make it readable.
    pub receipt_attachment_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateExpenseReportRequest {
    pub employee_id: Uuid,
    pub description: Option<String>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    #[validate(length(min = 1, message = "An expense report needs at least one line"))]
    #[validate]
    pub lines: Vec<ExpenseLineRequest>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateExpenseReportRequest {
    pub description: Option<String>,
    #[validate]
    pub lines: Option<Vec<ExpenseLineRequest>>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateExpenseStatusRequest {
    #[validate(custom = "validate_expense_status")]
    pub status: String,
}

fn validate_expense_status(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &ExpenseStatus::ALL, "status")
}

// ---------------------------------------------------------------- responses

#[derive(Debug, Serialize, ToSchema)]
pub struct EmployeeDetail {
    #[serde(flatten)]
    pub employee: Employee,
    pub leave_balance: LeaveBalance,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LeaveBalance {
    pub year: i32,
    pub entitlement: i32,
    pub taken: i32,
    pub remaining: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExpenseReportDetail {
    #[serde(flatten)]
    pub report: ExpenseReport,
    pub lines: Vec<ExpenseLine>,
}

fn one_of(
    value: &str,
    allowed: &[&str],
    code: &'static str,
) -> Result<(), validator::ValidationError> {
    if allowed.contains(&value) {
        return Ok(());
    }
    let mut err = validator::ValidationError::new(code);
    err.message = Some(format!("Must be one of: {}", allowed.join(", ")).into());
    Err(err)
}
