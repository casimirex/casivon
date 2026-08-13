use thiserror::Error;

use crate::error::AppError;

#[derive(Error, Debug)]
pub enum SalesError {
    #[error("This {document} cannot move from '{from}' to '{to}'")]
    InvalidTransition { document: &'static str, from: String, to: String },

    #[error("This {document} can only be edited while it is a draft (current status: '{status}')")]
    NotEditable { document: &'static str, status: String },

    #[error("This {document} needs at least one line item")]
    NoLines { document: &'static str },

    #[error("Quote '{0}' has not been accepted, so it cannot become an order")]
    QuoteNotAccepted(String),

    #[error("Quote '{0}' has already been converted to order '{1}'")]
    QuoteAlreadyConverted(String, String),

    #[error(
        "Order '{order}' cannot be marked '{status}' yet: issuing its invoices is what takes the \
         goods off the shelf, and {outstanding} unit(s) are still to be invoiced. Bill the rest \
         first."
    )]
    GoodsHaveNotLeft { order: String, status: String, outstanding: i32 },

    #[error(
        "Invoice '{invoice}' cannot be cancelled: order '{order}' is '{status}', so cancelling \
         would put goods back on the shelf that the customer already has. Raise a credit note \
         against the invoice instead."
    )]
    GoodsAlreadyGone { invoice: String, order: String, status: String },

    #[error("Invoice '{0}' is a draft — issue it before crediting anything against it")]
    NotCreditable(String),

    #[error("Line '{description}' was invoiced {invoiced} and {already} already credited, so {requested} cannot be credited")]
    OverCredit { description: String, requested: i32, invoiced: i32, already: i32 },

    #[error("Line {0} does not belong to invoice '{1}'")]
    LineNotOnInvoice(String, String),

    #[error("Order '{0}' must be confirmed before it can be invoiced")]
    OrderNotInvoiceable(String),

    #[error("Line '{description}' has {outstanding} left to invoice, so {requested} cannot be")]
    OverInvoice { description: String, requested: i32, outstanding: i32 },

    #[error("Order '{0}' has been invoiced in full; there is nothing left to bill")]
    NothingOutstanding(String),

    #[error("Line {0} does not belong to order '{1}'")]
    LineNotOnOrder(String, String),

    #[error("Invoice '{0}' is a draft — issue it before recording payments against it")]
    NotPayable(String),

    #[error("Invoice '{0}' is {1} and cannot take further payments")]
    InvoiceClosedToPayment(String, String),

    #[error("Payment of {0} exceeds the {1} still outstanding on invoice '{2}'")]
    PaymentExceedsBalance(String, String, String),

    #[error("'{0}' is not a supported payment method")]
    UnsupportedPaymentMethod(String),

    #[error("Expiry date must fall on or after the issue date")]
    ExpiryBeforeIssue,

    #[error("Due date must fall on or after the issue date")]
    DueBeforeIssue,
}

impl From<SalesError> for AppError {
    fn from(err: SalesError) -> Self {
        match err {
            SalesError::InvalidTransition { .. }
            | SalesError::NotEditable { .. }
            | SalesError::QuoteNotAccepted(_)
            | SalesError::OrderNotInvoiceable(_)
            // Both refusals on the payment endpoint answer the same way, so a
            // caller does not have to learn two codes for one rule.
            | SalesError::NotPayable(_)
            | SalesError::InvoiceClosedToPayment(..) => AppError::Conflict(err.to_string()),

            SalesError::QuoteAlreadyConverted(..)
            // A well-formed request the document's own state refuses, which is
            // what 409 is for — the same status an over-issue of stock gives.
            // An order with nothing left to bill is the same shape of answer.
            | SalesError::NothingOutstanding(_)
            | SalesError::GoodsHaveNotLeft { .. }
            | SalesError::GoodsAlreadyGone { .. } => AppError::Conflict(err.to_string()),

            // Asking for more than a line has left is a malformed request
            // rather than a state conflict, which is what `OverCredit` already
            // answers for the same shape of mistake.
            SalesError::NoLines { .. }
            | SalesError::OverInvoice { .. }
            | SalesError::LineNotOnOrder(..)
            | SalesError::OverCredit { .. }
            | SalesError::NotCreditable(_)
            | SalesError::LineNotOnInvoice(..)
            | SalesError::PaymentExceedsBalance(..)
            | SalesError::UnsupportedPaymentMethod(_)
            | SalesError::ExpiryBeforeIssue
            | SalesError::DueBeforeIssue => AppError::Validation(err.to_string()),
        }
    }
}
