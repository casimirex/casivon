use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Vendor {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub name: String,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub payment_terms: Option<String>,
    pub currency: String,
    pub status: String, // active, inactive
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PurchaseOrder {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub po_number: String,
    pub vendor_id: Uuid,
    pub status: String,
    pub order_date: NaiveDate,
    pub expected_date: Option<NaiveDate>,
    pub shipping_address: Option<String>,
    pub subtotal: Option<Decimal>,
    pub tax_amount: Option<Decimal>,
    pub total: Option<Decimal>,
    pub currency: String,
    pub fx_rate: Decimal,
    /// `total` restated in the base currency — what spend reporting adds up.
    pub base_total: Option<Decimal>,
    /// Re-derived from the payments recorded against this order, so the order
    /// can never drift away from its own payment ledger. The `base_` pair is
    /// restated at the *order's* rate, so the two always reconcile against
    /// `base_total`; what each payment was actually worth lives on the payment.
    pub amount_paid: Decimal,
    pub amount_due: Option<Decimal>,
    pub base_amount_paid: Decimal,
    pub base_amount_due: Option<Decimal>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PurchaseOrderLine {
    pub id: Uuid,
    pub po_id: Uuid,
    pub product_id: Option<Uuid>,
    pub description: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    /// A whole percentage: 20 means 20%.
    pub tax_rate: Decimal,
    /// How much of `quantity` has arrived so far.
    pub received_quantity: i32,
    /// Net of tax, matching the sales document lines.
    pub line_total: Decimal,
    pub sort_order: i32,
}

impl PurchaseOrderLine {
    pub fn outstanding(&self) -> i32 {
        (self.quantity - self.received_quantity).max(0)
    }

    pub fn is_fully_received(&self) -> bool {
        self.received_quantity >= self.quantity
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct GoodsReceipt {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub po_id: Uuid,
    pub receipt_number: String,
    pub receipt_date: NaiveDate,
    pub status: String,
    /// Where the goods landed; drives the stock movements.
    pub warehouse_id: Option<Uuid>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct GoodsReceiptLine {
    pub id: Uuid,
    pub receipt_id: Uuid,
    pub po_line_id: Uuid,
    pub product_id: Option<Uuid>,
    pub quantity_received: i32,
    pub notes: Option<String>,
}

/// Goods going back to the supplier.
///
/// The counterpart of a [`GoodsReceipt`], deliberately the same shape. It
/// records quantities and no money of its own: what a return is worth is the
/// purchase order's line price, which is exactly what the receipt brought the
/// goods in at. Valuing it any other way would need a variance account to absorb
/// the difference.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PurchaseReturn {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub po_id: Uuid,
    pub return_number: String,
    pub return_date: NaiveDate,
    /// Which shelf the goods leave from; drives the stock movements.
    pub warehouse_id: Option<Uuid>,
    /// Why they went back, in whatever words fit.
    pub reason: Option<String>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PurchaseReturnLine {
    pub id: Uuid,
    pub return_id: Uuid,
    pub po_line_id: Uuid,
    pub product_id: Option<Uuid>,
    pub quantity_returned: i32,
    pub notes: Option<String>,
}

/// Money going out against a purchase order.
///
/// The mirror of `sales::Payment`, deliberately down to the column names: the
/// two are the same idea pointing in opposite directions, and a reader who
/// understands one should not have to learn a second vocabulary.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct VendorPayment {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub po_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    /// The rate on the day the money left, deliberately not the order's.
    pub fx_rate: Decimal,
    /// What the payment actually cost, at its own rate.
    pub base_amount: Decimal,
    /// The realised FX gain (positive) or loss (negative), in base currency:
    /// what the order said the debt was worth, less what discharging it cost.
    /// A debt settled cheaply is a gain. Zero when both rates agree.
    pub fx_gain_loss: Decimal,
    pub payment_method: String,
    pub payment_date: NaiveDate,
    pub reference: Option<String>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

/// PurchaseOrder:
/// draft -> sent -> confirmed -> [partially_received | fully_received] -> closed,
/// cancellable until goods start arriving.
pub struct PurchaseOrderStatus;

impl PurchaseOrderStatus {
    pub const DRAFT: &'static str = "draft";
    pub const SENT: &'static str = "sent";
    pub const CONFIRMED: &'static str = "confirmed";
    pub const PARTIALLY_RECEIVED: &'static str = "partially_received";
    pub const FULLY_RECEIVED: &'static str = "fully_received";
    pub const CLOSED: &'static str = "closed";
    pub const CANCELLED: &'static str = "cancelled";

    pub const ALL: [&'static str; 7] = [
        Self::DRAFT,
        Self::SENT,
        Self::CONFIRMED,
        Self::PARTIALLY_RECEIVED,
        Self::FULLY_RECEIVED,
        Self::CLOSED,
        Self::CANCELLED,
    ];

    /// Transitions a user may request directly. The receiving statuses are set
    /// by the goods-receipt flow, not by hand.
    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            (Self::DRAFT, Self::SENT)
                | (Self::SENT, Self::CONFIRMED)
                | (Self::FULLY_RECEIVED, Self::CLOSED)
                | (Self::PARTIALLY_RECEIVED, Self::CLOSED)
                | (Self::DRAFT, Self::CANCELLED)
                | (Self::SENT, Self::CANCELLED)
                | (Self::CONFIRMED, Self::CANCELLED)
        )
    }

    pub fn is_editable(status: &str) -> bool {
        matches!(status, Self::DRAFT | Self::SENT)
    }

    /// Goods can only be booked in against a confirmed or part-received order.
    pub fn accepts_receipt(status: &str) -> bool {
        matches!(status, Self::CONFIRMED | Self::PARTIALLY_RECEIVED)
    }

    /// Whether goods can go back against an order in this state.
    ///
    /// Wider than `accepts_receipt`: a fully received order is precisely the one
    /// most likely to have something wrong with it, and a closed order is done
    /// with. Returning drops it back to partially received, so it expects the
    /// replacement.
    pub fn accepts_return(status: &str) -> bool {
        matches!(status, Self::CONFIRMED | Self::PARTIALLY_RECEIVED | Self::FULLY_RECEIVED)
    }

    /// The status implied by how much of the order has arrived.
    ///
    /// Used by returns as well as receipts, and it is the same question either
    /// way: is everything ordered now here?
    pub fn after_receipt(all_lines_complete: bool) -> &'static str {
        if all_lines_complete {
            Self::FULLY_RECEIVED
        } else {
            Self::PARTIALLY_RECEIVED
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn line(quantity: i32, received: i32) -> PurchaseOrderLine {
        PurchaseOrderLine {
            id: Uuid::new_v4(),
            po_id: Uuid::new_v4(),
            product_id: None,
            description: "Bolts".to_string(),
            quantity,
            unit_price: dec!(1.00),
            tax_rate: dec!(20),
            received_quantity: received,
            line_total: dec!(1.00),
            sort_order: 0,
        }
    }

    #[test]
    fn outstanding_never_goes_negative_on_over_receipt() {
        assert_eq!(line(10, 12).outstanding(), 0);
        assert_eq!(line(10, 4).outstanding(), 6);
    }

    #[test]
    fn over_receipt_still_counts_as_complete() {
        assert!(line(10, 12).is_fully_received());
        assert!(!line(10, 9).is_fully_received());
    }

    #[test]
    fn receipts_need_a_confirmed_order() {
        assert!(PurchaseOrderStatus::accepts_receipt(PurchaseOrderStatus::CONFIRMED));
        assert!(PurchaseOrderStatus::accepts_receipt(PurchaseOrderStatus::PARTIALLY_RECEIVED));
        assert!(!PurchaseOrderStatus::accepts_receipt(PurchaseOrderStatus::DRAFT));
        assert!(!PurchaseOrderStatus::accepts_receipt(PurchaseOrderStatus::FULLY_RECEIVED));
    }

    #[test]
    fn order_cannot_be_cancelled_after_goods_arrive() {
        assert!(PurchaseOrderStatus::can_transition(
            PurchaseOrderStatus::CONFIRMED,
            PurchaseOrderStatus::CANCELLED
        ));
        assert!(!PurchaseOrderStatus::can_transition(
            PurchaseOrderStatus::PARTIALLY_RECEIVED,
            PurchaseOrderStatus::CANCELLED
        ));
    }

    #[test]
    fn receiving_status_reflects_completeness() {
        assert_eq!(
            PurchaseOrderStatus::after_receipt(true),
            PurchaseOrderStatus::FULLY_RECEIVED
        );
        assert_eq!(
            PurchaseOrderStatus::after_receipt(false),
            PurchaseOrderStatus::PARTIALLY_RECEIVED
        );
    }
}
