use thiserror::Error;

use crate::error::AppError;

#[derive(Error, Debug)]
pub enum HrError {
    #[error(
        "You can only file this for yourself. Ask someone in HR to raise it on \
         another employee's behalf."
    )]
    NotYoursToFile,

    #[error(
        "This login is not linked to an employee record, so it has no leave or \
         expenses of its own. Ask HR to link it."
    )]
    NoEmployeeRecord,

    #[error(
        "That receipt was not uploaded by you, so it cannot be attached to this \
         claim. Upload the file again."
    )]
    ReceiptNotYours,

    #[error("This {document} cannot move from '{from}' to '{to}'")]
    InvalidTransition { document: &'static str, from: String, to: String },

    #[error("Leave must end on or after it starts")]
    EndBeforeStart,

    #[error("Employee '{employee}' has only {remaining} day(s) of annual leave left, {requested} requested")]
    InsufficientLeaveBalance { employee: String, remaining: i32, requested: i32 },

    #[error("Leave from {0} to {1} overlaps an existing request")]
    OverlappingLeave(String, String),

    #[error("'{0}' is not a valid leave type")]
    UnknownLeaveType(String),

    #[error("Employee '{0}' has been terminated")]
    EmployeeTerminated(String),

    #[error("Employee number '{0}' is already in use")]
    DuplicateEmployeeNumber(String),

    #[error("An employee cannot be their own manager")]
    SelfManaged,

    #[error("An expense report can only be edited while it is a draft (current status: '{0}')")]
    ExpenseNotEditable(String),

    #[error("An expense report needs at least one line")]
    EmptyExpenseReport,

    #[error("Expense amounts must be greater than zero")]
    NonPositiveExpense,
}

impl From<HrError> for AppError {
    fn from(err: HrError) -> Self {
        match err {
            HrError::InvalidTransition { .. }
            | HrError::InsufficientLeaveBalance { .. }
            | HrError::OverlappingLeave(..)
            | HrError::EmployeeTerminated(_)
            | HrError::DuplicateEmployeeNumber(_)
            | HrError::ExpenseNotEditable(_) => AppError::Conflict(err.to_string()),

            // Well-formed requests the caller simply may not make.
            HrError::NotYoursToFile | HrError::NoEmployeeRecord | HrError::ReceiptNotYours => {
                AppError::Forbidden(err.to_string())
            }
            _ => AppError::Validation(err.to_string()),
        }
    }
}
