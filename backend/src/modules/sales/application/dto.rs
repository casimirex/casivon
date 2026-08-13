use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::modules::sales::domain::entities::*;

/// One line on a quote, order or invoice. The three documents share a line
/// shape, so they share a request type.
///
/// `Serialize` is required by `validator`, which echoes the offending value back
/// in the error payload when a `length` rule fails on the containing `Vec`.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct DocumentLineRequest {
    pub product_id: Option<Uuid>,
    #[validate(length(min = 1, max = 1000, message = "Description is required"))]
    pub description: String,
    #[validate(range(min = 1, message = "Quantity must be at least 1"))]
    pub quantity: i32,
    pub unit_price: Decimal,
    /// A whole percentage: 10 means 10% off.
    #[serde(default)]
    #[validate(custom = "crate::shared::validation::validate_percentage")]
    pub discount_percent: Decimal,
    /// A whole percentage: 20 means 20%, the same convention as
    /// `accounting.tax_rates.rate`.
    #[serde(default)]
    #[validate(custom = "crate::shared::validation::validate_percentage")]
    pub tax_rate: Decimal,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateQuoteRequest {
    pub customer_id: Uuid,
    pub contact_id: Option<Uuid>,
    pub issue_date: NaiveDate,
    pub expiry_date: NaiveDate,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    pub notes: Option<String>,
    pub terms: Option<String>,
    #[validate(length(min = 1, message = "A quote needs at least one line item"))]
    #[validate]
    pub lines: Vec<DocumentLineRequest>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateQuoteRequest {
    pub customer_id: Option<Uuid>,
    pub contact_id: Option<Uuid>,
    pub issue_date: Option<NaiveDate>,
    pub expiry_date: Option<NaiveDate>,
    pub notes: Option<String>,
    pub terms: Option<String>,
    /// Omit to leave the existing lines alone; send a list to replace them all.
    #[validate]
    pub lines: Option<Vec<DocumentLineRequest>>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateOrderRequest {
    pub customer_id: Uuid,
    pub contact_id: Option<Uuid>,
    pub quote_id: Option<Uuid>,
    pub order_date: NaiveDate,
    pub required_date: Option<NaiveDate>,
    pub shipping_address: Option<String>,
    pub billing_address: Option<String>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    pub notes: Option<String>,
    #[validate(length(min = 1, message = "An order needs at least one line item"))]
    #[validate]
    pub lines: Vec<DocumentLineRequest>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateOrderRequest {
    pub customer_id: Option<Uuid>,
    pub contact_id: Option<Uuid>,
    pub required_date: Option<NaiveDate>,
    pub shipping_address: Option<String>,
    pub billing_address: Option<String>,
    pub notes: Option<String>,
    #[validate]
    pub lines: Option<Vec<DocumentLineRequest>>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateInvoiceRequest {
    pub customer_id: Uuid,
    pub order_id: Option<Uuid>,
    pub issue_date: NaiveDate,
    pub due_date: NaiveDate,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    pub notes: Option<String>,
    #[validate(length(min = 1, message = "An invoice needs at least one line item"))]
    #[validate]
    pub lines: Vec<DocumentLineRequest>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateInvoiceRequest {
    pub customer_id: Option<Uuid>,
    pub issue_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub notes: Option<String>,
    #[validate]
    pub lines: Option<Vec<DocumentLineRequest>>,
}

/// Body for the status endpoints (`POST /quotes/:id/status`).
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateStatusRequest {
    #[validate(length(min = 1, message = "Status is required"))]
    pub status: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ConvertQuoteRequest {
    pub order_date: Option<NaiveDate>,
    pub required_date: Option<NaiveDate>,
    pub shipping_address: Option<String>,
    pub billing_address: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ConvertOrderRequest {
    pub issue_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    /// Used when `due_date` is absent: issue date plus this many days.
    #[validate(range(min = 0, max = 365))]
    pub payment_terms_days: Option<i64>,
    /// What to bill this time, per line.
    ///
    /// Omitted means everything still outstanding, which is what the one-click
    /// conversion has always done and what an order billed in one go still does.
    #[validate]
    pub lines: Option<Vec<ConvertLineRequest>>,
}

/// One line's worth of an instalment.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct ConvertLineRequest {
    pub order_line_id: Uuid,
    #[validate(range(min = 1, message = "Invoiced quantity must be at least 1"))]
    pub quantity: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct CreditNoteLineRequest {
    pub invoice_line_id: Uuid,
    #[validate(range(min = 1, message = "Credited quantity must be at least 1"))]
    pub quantity: i32,
}

/// Crediting a customer against an invoice.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateCreditNoteRequest {
    pub invoice_id: Uuid,
    /// Where returned goods land. Omit it and the credit is money only, which is
    /// what a price dispute or an over-billing needs.
    pub warehouse_id: Option<Uuid>,
    pub issue_date: Option<NaiveDate>,
    pub reason: Option<String>,
    pub notes: Option<String>,
    #[validate(length(min = 1, message = "A credit note needs at least one line"))]
    #[validate]
    pub lines: Vec<CreditNoteLineRequest>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RecordPaymentRequest {
    pub invoice_id: Uuid,
    pub amount: Decimal,
    #[validate(custom = "validate_payment_method")]
    pub payment_method: String,
    pub payment_date: NaiveDate,
    #[validate(length(max = 255))]
    pub reference: Option<String>,
    pub notes: Option<String>,
}

fn validate_payment_method(value: &str) -> Result<(), validator::ValidationError> {
    if PaymentMethod::is_valid(value) {
        Ok(())
    } else {
        let mut err = validator::ValidationError::new("payment_method");
        err.message = Some(
            format!("Must be one of: {}", PaymentMethod::ALL.join(", ")).into(),
        );
        Err(err)
    }
}

// ---------------------------------------------------------------- responses

/// A document plus its lines, for the detail pages.
#[derive(Debug, Serialize, ToSchema)]
pub struct QuoteDetail {
    #[serde(flatten)]
    pub quote: Quote,
    pub lines: Vec<QuoteLine>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderDetail {
    #[serde(flatten)]
    pub order: SalesOrder,
    pub lines: Vec<OrderLineView>,
}

/// An order line with how much of it is still to be billed.
///
/// The mirror of `PurchaseOrderLineView`, except that `invoiced` is derived from
/// the order's live invoices rather than read off the row, so the view is built
/// with it rather than `From` the line alone.
#[derive(Debug, Serialize, ToSchema)]
pub struct OrderLineView {
    #[serde(flatten)]
    pub line: OrderLine,
    pub invoiced_quantity: i32,
    pub outstanding: i32,
    pub is_fully_invoiced: bool,
}

impl OrderLineView {
    pub fn new(line: OrderLine, invoiced: i32) -> Self {
        Self {
            invoiced_quantity: invoiced,
            outstanding: line.outstanding(invoiced),
            is_fully_invoiced: line.is_fully_invoiced(invoiced),
            line,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvoiceDetail {
    #[serde(flatten)]
    pub invoice: Invoice,
    pub lines: Vec<InvoiceLine>,
    pub payments: Vec<Payment>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreditNoteDetail {
    #[serde(flatten)]
    pub credit_note: CreditNote,
    pub lines: Vec<CreditNoteLine>,
}
