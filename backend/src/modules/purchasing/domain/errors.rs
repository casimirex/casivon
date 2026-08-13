use thiserror::Error;

use crate::error::AppError;

#[derive(Error, Debug)]
pub enum PurchasingError {
    #[error("A purchase order cannot move from '{from}' to '{to}'")]
    InvalidTransition { from: String, to: String },

    #[error("Purchase order '{0}' can only be edited while it is a draft or awaiting confirmation")]
    NotEditable(String),

    #[error("A purchase order needs at least one line item")]
    NoLines,

    #[error("Purchase order '{0}' is {1}; goods can only be received against a confirmed order")]
    NotReceivable(String, String),

    #[error("A goods receipt needs at least one line")]
    EmptyReceipt,

    #[error("Line {0} does not belong to purchase order '{1}'")]
    LineNotOnOrder(String, String),

    #[error("Cannot receive {requested} of '{description}': only {outstanding} still outstanding")]
    OverReceipt { description: String, requested: i32, outstanding: i32 },

    #[error("Receipt quantities must be positive")]
    NonPositiveQuantity,

    #[error("A goods receipt needs a destination warehouse")]
    ReceiptNeedsWarehouse,

    #[error("Cannot return {requested} of '{description}': only {received} were received and not already sent back")]
    OverReturn { description: String, requested: i32, received: i32 },

    #[error("Nothing has been received against '{0}', so there is nothing to send back")]
    NothingToReturn(String),

    #[error("Vendor '{0}' is inactive")]
    VendorInactive(String),

    #[error("'{0}' is not a supported payment method")]
    UnsupportedPaymentMethod(String),

    #[error("Paying {0} would exceed the {1} still outstanding on purchase order '{2}'")]
    PaymentExceedsBalance(String, String, String),

    #[error("Purchase order '{0}' is a draft; confirm it before paying against it")]
    NotPayable(String),
}

impl From<PurchasingError> for AppError {
    fn from(err: PurchasingError) -> Self {
        match err {
            PurchasingError::InvalidTransition { .. }
            | PurchasingError::NotEditable(_)
            | PurchasingError::NotReceivable(..)
            | PurchasingError::VendorInactive(_)
            | PurchasingError::NotPayable(_) => AppError::Conflict(err.to_string()),
            _ => AppError::Validation(err.to_string()),
        }
    }
}
