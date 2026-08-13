use thiserror::Error;

use crate::error::AppError;

#[derive(Error, Debug)]
pub enum AccountingError {
    #[error("'{0}' is not a valid account type")]
    UnknownAccountType(String),

    #[error("Account code '{0}' is already in use")]
    DuplicateAccountCode(String),

    #[error("A journal entry must debit and credit two different accounts")]
    SameDebitAndCreditAccount,

    #[error("Journal entry amounts must be greater than zero")]
    NonPositiveAmount,

    #[error("Account '{0}' is inactive and cannot be posted to")]
    InactiveAccount(String),

    #[error("Account '{0}' has ledger entries and cannot be deleted; deactivate it instead")]
    AccountHasEntries(String),

    #[error("Account '{0}' has child accounts and cannot be deleted")]
    AccountHasChildren(String),

    #[error("An account cannot be its own parent")]
    SelfParent,

    #[error("Making '{0}' a child of '{1}' would create a cycle in the chart of accounts")]
    CircularHierarchy(String, String),

    #[error("Tax rate must be a percentage between 0 and 100 (20 means 20%)")]
    TaxRateOutOfRange,

    #[error("The reporting period must start on or before it ends")]
    InvalidPeriod,

    #[error("'{role}' must be mapped to a {expected} account, but '{code}' is {actual}")]
    WrongAccountTypeForRole {
        role: &'static str,
        expected: &'static str,
        code: String,
        actual: String,
    },

    #[error(
        "'{role}' is mapped to '{code}', which is denominated in {currency} rather than the \
         base currency {base}. Automatic postings are made in {base}, and an entry has to \
         agree with the accounts it touches."
    )]
    PostingAccountNotInBaseCurrency {
        role: &'static str,
        code: String,
        currency: String,
        base: String,
    },

    #[error("A posted entry cannot be deleted. Cancel or reverse {0} instead, which posts the mirror and leaves the audit trail intact")]
    PostedEntryNotDeletable(String),
}

impl From<AccountingError> for AppError {
    fn from(err: AccountingError) -> Self {
        match err {
            AccountingError::DuplicateAccountCode(_)
            | AccountingError::InactiveAccount(_)
            | AccountingError::AccountHasEntries(_)
            | AccountingError::AccountHasChildren(_)
            // A conflict rather than a validation failure: the request is
            // well-formed, it is the state of the entry that refuses it.
            | AccountingError::PostedEntryNotDeletable(_) => AppError::Conflict(err.to_string()),
            _ => AppError::Validation(err.to_string()),
        }
    }
}
