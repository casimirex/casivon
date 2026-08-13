use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::modules::purchasing::domain::entities::*;

// ------------------------------------------------------------------ vendors

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateVendorRequest {
    #[validate(length(min = 1, max = 255, message = "Vendor name is required"))]
    pub name: String,
    #[validate(length(max = 255))]
    pub legal_name: Option<String>,
    #[validate(length(max = 100))]
    pub tax_id: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    #[validate(length(max = 50))]
    pub phone: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    #[validate(length(max = 100))]
    pub payment_terms: Option<String>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateVendorRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub payment_terms: Option<String>,
    #[validate(length(min = 3, max = 3))]
    pub currency: Option<String>,
    #[validate(custom = "validate_vendor_status")]
    pub status: Option<String>,
}

pub const VENDOR_STATUSES: [&str; 2] = ["active", "inactive"];

fn validate_vendor_status(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &VENDOR_STATUSES, "status")
}

// ---------------------------------------------------------- purchase orders

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct PurchaseOrderLineRequest {
    pub product_id: Option<Uuid>,
    #[validate(length(min = 1, max = 1000, message = "Description is required"))]
    pub description: String,
    #[validate(range(min = 1, message = "Quantity must be at least 1"))]
    pub quantity: i32,
    pub unit_price: Decimal,
    /// A whole percentage: 20 means 20%, the same convention as
    /// `accounting.tax_rates.rate`.
    #[serde(default)]
    #[validate(custom = "crate::shared::validation::validate_percentage")]
    pub tax_rate: Decimal,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreatePurchaseOrderRequest {
    pub vendor_id: Uuid,
    pub order_date: NaiveDate,
    pub expected_date: Option<NaiveDate>,
    pub shipping_address: Option<String>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    pub notes: Option<String>,
    #[validate(length(min = 1, message = "A purchase order needs at least one line item"))]
    #[validate]
    pub lines: Vec<PurchaseOrderLineRequest>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdatePurchaseOrderRequest {
    pub vendor_id: Option<Uuid>,
    pub expected_date: Option<NaiveDate>,
    pub shipping_address: Option<String>,
    pub notes: Option<String>,
    #[validate]
    pub lines: Option<Vec<PurchaseOrderLineRequest>>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateStatusRequest {
    #[validate(custom = "validate_po_status")]
    pub status: String,
}

fn validate_po_status(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &PurchaseOrderStatus::ALL, "status")
}

// ----------------------------------------------------------- goods receipts

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct GoodsReceiptLineRequest {
    pub po_line_id: Uuid,
    #[validate(range(min = 1, message = "Received quantity must be at least 1"))]
    pub quantity_received: i32,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateGoodsReceiptRequest {
    pub po_id: Uuid,
    pub warehouse_id: Uuid,
    pub receipt_date: Option<NaiveDate>,
    pub notes: Option<String>,
    #[validate(length(min = 1, message = "A goods receipt needs at least one line"))]
    #[validate]
    pub lines: Vec<GoodsReceiptLineRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub struct PurchaseReturnLineRequest {
    pub po_line_id: Uuid,
    #[validate(range(min = 1, message = "Returned quantity must be at least 1"))]
    pub quantity_returned: i32,
    pub notes: Option<String>,
}

/// Sending goods back. The mirror of [`CreateGoodsReceiptRequest`], down to the
/// warehouse — stock has to leave from somewhere in particular.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreatePurchaseReturnRequest {
    pub po_id: Uuid,
    pub warehouse_id: Uuid,
    pub return_date: Option<NaiveDate>,
    /// Why the goods went back, in whatever words fit.
    pub reason: Option<String>,
    pub notes: Option<String>,
    #[validate(length(min = 1, message = "A purchase return needs at least one line"))]
    #[validate]
    pub lines: Vec<PurchaseReturnLineRequest>,
}

// ---------------------------------------------------------------- responses

#[derive(Debug, Serialize, ToSchema)]
pub struct PurchaseOrderDetail {
    #[serde(flatten)]
    pub order: PurchaseOrder,
    pub lines: Vec<PurchaseOrderLineView>,
}

/// A PO line with the outstanding quantity the receipt screen needs.
#[derive(Debug, Serialize, ToSchema)]
pub struct PurchaseOrderLineView {
    #[serde(flatten)]
    pub line: PurchaseOrderLine,
    pub outstanding: i32,
    pub is_fully_received: bool,
}

impl From<PurchaseOrderLine> for PurchaseOrderLineView {
    fn from(line: PurchaseOrderLine) -> Self {
        Self {
            outstanding: line.outstanding(),
            is_fully_received: line.is_fully_received(),
            line,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GoodsReceiptDetail {
    #[serde(flatten)]
    pub receipt: GoodsReceipt,
    pub lines: Vec<GoodsReceiptLine>,
    /// Status the parent order ended up in after this receipt.
    pub order_status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PurchaseReturnDetail {
    #[serde(flatten)]
    pub purchase_return: PurchaseReturn,
    pub lines: Vec<PurchaseReturnLine>,
    /// Status the parent order ended up in after this return — a fully received
    /// order drops back to partially received, and expects the goods again.
    pub order_status: String,
}

// --------------------------------------------------------- vendor payments

/// Money going out against a purchase order.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RecordVendorPaymentRequest {
    pub po_id: Uuid,
    pub amount: Decimal,
    #[validate(custom = "validate_payment_method")]
    pub payment_method: String,
    pub payment_date: NaiveDate,
    #[validate(length(max = 255))]
    pub reference: Option<String>,
    pub notes: Option<String>,
}

/// The same closed set the sales side accepts — a payment is a payment
/// whichever way the money is going.
fn validate_payment_method(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &PAYMENT_METHODS, "payment_method")
}

pub const PAYMENT_METHODS: [&str; 6] =
    ["bank_transfer", "credit_card", "cash", "check", "stripe", "paypal"];

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
