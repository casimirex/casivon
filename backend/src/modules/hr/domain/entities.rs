use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Employee {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub employee_number: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub hire_date: NaiveDate,
    pub termination_date: Option<NaiveDate>,
    pub department: Option<String>,
    pub job_title: Option<String>,
    pub manager_id: Option<Uuid>,
    pub salary: Option<Decimal>,
    pub currency: String,
    pub fx_rate: Decimal,
    /// `salary` restated in the base currency — payroll cost is only additive
    /// across a workforce paid in different currencies once restated.
    pub base_salary: Option<Decimal>,
    pub status: String, // active, on_leave, terminated
    /// Paid leave days granted per calendar year.
    pub annual_leave_entitlement: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct LeaveRequest {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub employee_id: Uuid,
    pub leave_type: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub days_requested: i32,
    pub reason: Option<String>,
    pub status: String, // pending, approved, rejected
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct ExpenseReport {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub employee_id: Uuid,
    pub report_number: String,
    pub description: Option<String>,
    pub total_amount: Decimal,
    pub currency: String,
    pub fx_rate: Decimal,
    pub base_total_amount: Decimal,
    pub status: String, // draft, submitted, approved, rejected, reimbursed
    pub submitted_at: Option<DateTime<Utc>>,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct ExpenseLine {
    pub id: Uuid,
    pub expense_report_id: Uuid,
    pub expense_date: NaiveDate,
    pub category: String,
    pub description: String,
    pub amount: Decimal,
    /// Restated at the report's rate — a line has no currency of its own.
    pub base_amount: Option<Decimal>,
    /// Superseded by `receipt_attachment_id`. Kept because dropping a column
    /// cannot be undone; nothing writes it any more and the UI no longer offers
    /// it.
    pub receipt_url: Option<String>,
    /// The uploaded receipt, if there is one. Fetch it from
    /// `GET /api/v1/files/{id}`, which answers with a short-lived link.
    pub receipt_attachment_id: Option<Uuid>,
    pub sort_order: i32,
}

pub struct EmployeeStatus;

impl EmployeeStatus {
    pub const ACTIVE: &'static str = "active";
    pub const ON_LEAVE: &'static str = "on_leave";
    pub const TERMINATED: &'static str = "terminated";

    pub const ALL: [&'static str; 3] = [Self::ACTIVE, Self::ON_LEAVE, Self::TERMINATED];

    pub fn is_valid(value: &str) -> bool {
        Self::ALL.contains(&value)
    }
}

/// LeaveRequest: pending -> [approved | rejected]
pub struct LeaveStatus;

impl LeaveStatus {
    pub const PENDING: &'static str = "pending";
    pub const APPROVED: &'static str = "approved";
    pub const REJECTED: &'static str = "rejected";

    pub const ALL: [&'static str; 3] = [Self::PENDING, Self::APPROVED, Self::REJECTED];

    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            (Self::PENDING, Self::APPROVED) | (Self::PENDING, Self::REJECTED)
        )
    }
}

pub struct LeaveType;

impl LeaveType {
    pub const ANNUAL: &'static str = "annual";

    pub const ALL: [&'static str; 5] =
        [Self::ANNUAL, "sick", "maternity", "paternity", "unpaid"];

    pub fn is_valid(value: &str) -> bool {
        Self::ALL.contains(&value)
    }

    /// Only annual leave draws down the yearly entitlement.
    pub fn counts_against_entitlement(value: &str) -> bool {
        value == Self::ANNUAL
    }
}

/// ExpenseReport: draft -> submitted -> [approved | rejected] -> reimbursed
pub struct ExpenseStatus;

impl ExpenseStatus {
    pub const DRAFT: &'static str = "draft";
    pub const SUBMITTED: &'static str = "submitted";
    pub const APPROVED: &'static str = "approved";
    pub const REJECTED: &'static str = "rejected";
    pub const REIMBURSED: &'static str = "reimbursed";

    pub const ALL: [&'static str; 5] = [
        Self::DRAFT,
        Self::SUBMITTED,
        Self::APPROVED,
        Self::REJECTED,
        Self::REIMBURSED,
    ];

    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            (Self::DRAFT, Self::SUBMITTED)
                | (Self::SUBMITTED, Self::APPROVED)
                | (Self::SUBMITTED, Self::REJECTED)
                | (Self::APPROVED, Self::REIMBURSED)
                // A rejected report goes back to the employee to fix and resubmit.
                | (Self::REJECTED, Self::DRAFT)
        )
    }

    pub fn is_editable(status: &str) -> bool {
        matches!(status, Self::DRAFT | Self::REJECTED)
    }

    pub fn requires_approver(status: &str) -> bool {
        matches!(status, Self::APPROVED | Self::REJECTED)
    }
}

/// Whole days inclusive of both endpoints — the way people count leave.
pub fn inclusive_days(start: NaiveDate, end: NaiveDate) -> i32 {
    (end - start).num_days() as i32 + 1
}

/// Working days (Mon–Fri) between two dates, inclusive.
pub fn working_days(start: NaiveDate, end: NaiveDate) -> i32 {
    start
        .iter_days()
        .take_while(|d| *d <= end)
        .filter(|d| d.weekday().num_days_from_monday() < 5)
        .count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn a_single_day_of_leave_counts_as_one() {
        assert_eq!(inclusive_days(date(2026, 8, 10), date(2026, 8, 10)), 1);
        assert_eq!(inclusive_days(date(2026, 8, 10), date(2026, 8, 14)), 5);
    }

    #[test]
    fn working_days_skip_the_weekend() {
        // Mon 10 Aug 2026 through Sun 16 Aug: 5 working days out of 7.
        assert_eq!(working_days(date(2026, 8, 10), date(2026, 8, 16)), 5);
        // A weekend on its own is zero.
        assert_eq!(working_days(date(2026, 8, 15), date(2026, 8, 16)), 0);
    }

    #[test]
    fn leave_is_decided_once() {
        assert!(LeaveStatus::can_transition(LeaveStatus::PENDING, LeaveStatus::APPROVED));
        assert!(!LeaveStatus::can_transition(LeaveStatus::APPROVED, LeaveStatus::REJECTED));
        assert!(!LeaveStatus::can_transition(LeaveStatus::REJECTED, LeaveStatus::PENDING));
    }

    #[test]
    fn only_annual_leave_uses_the_entitlement() {
        assert!(LeaveType::counts_against_entitlement("annual"));
        assert!(!LeaveType::counts_against_entitlement("sick"));
        assert!(!LeaveType::counts_against_entitlement("unpaid"));
    }

    #[test]
    fn expenses_follow_the_approval_chain() {
        assert!(ExpenseStatus::can_transition(ExpenseStatus::DRAFT, ExpenseStatus::SUBMITTED));
        assert!(ExpenseStatus::can_transition(
            ExpenseStatus::APPROVED,
            ExpenseStatus::REIMBURSED
        ));
        // Cannot pay out something nobody approved.
        assert!(!ExpenseStatus::can_transition(
            ExpenseStatus::SUBMITTED,
            ExpenseStatus::REIMBURSED
        ));
        // A rejected report can be reworked.
        assert!(ExpenseStatus::can_transition(ExpenseStatus::REJECTED, ExpenseStatus::DRAFT));
    }

    #[test]
    fn submitted_reports_are_locked_for_editing() {
        assert!(ExpenseStatus::is_editable(ExpenseStatus::DRAFT));
        assert!(ExpenseStatus::is_editable(ExpenseStatus::REJECTED));
        assert!(!ExpenseStatus::is_editable(ExpenseStatus::SUBMITTED));
        assert!(!ExpenseStatus::is_editable(ExpenseStatus::REIMBURSED));
    }
}
