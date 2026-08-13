use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Quote {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub quote_number: String,
    pub customer_id: Uuid,
    pub contact_id: Option<Uuid>,
    pub status: String, // draft, sent, accepted, rejected, expired
    pub issue_date: NaiveDate,
    pub expiry_date: NaiveDate,
    pub subtotal: Option<Decimal>,
    pub tax_amount: Option<Decimal>,
    pub total: Option<Decimal>,
    pub currency: String,
    /// Units of base currency per one unit of `currency`, frozen when the quote
    /// was raised so that reopening it never restates it at today's rate.
    pub fx_rate: Decimal,
    /// `total` restated in the base currency. What any cross-currency report
    /// adds up; see `013_multi_currency.sql`.
    pub base_total: Option<Decimal>,
    pub notes: Option<String>,
    pub terms: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct QuoteLine {
    pub id: Uuid,
    pub quote_id: Uuid,
    pub product_id: Option<Uuid>,
    pub description: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub discount_percent: Decimal,
    pub tax_rate: Decimal,
    pub line_total: Decimal,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SalesOrder {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub order_number: String,
    pub customer_id: Uuid,
    pub contact_id: Option<Uuid>,
    pub quote_id: Option<Uuid>,
    pub status: String, // draft, confirmed, processing, shipped, delivered, cancelled
    pub order_date: NaiveDate,
    pub required_date: Option<NaiveDate>,
    pub shipping_address: Option<String>,
    pub billing_address: Option<String>,
    pub subtotal: Option<Decimal>,
    pub tax_amount: Option<Decimal>,
    pub total: Option<Decimal>,
    pub currency: String,
    pub fx_rate: Decimal,
    pub base_total: Option<Decimal>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct OrderLine {
    pub id: Uuid,
    pub order_id: Uuid,
    pub product_id: Option<Uuid>,
    pub description: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub discount_percent: Decimal,
    pub tax_rate: Decimal,
    pub line_total: Decimal,
    pub sort_order: i32,
}

impl OrderLine {
    /// How much of this line is still to be billed.
    ///
    /// `invoiced` is passed in rather than stored on the row: it is summed from
    /// the live invoice lines pointing at this one, so cancelling an invoice or
    /// editing a draft moves it without anything having to remember to. The
    /// mirror of `PurchaseOrderLine::outstanding`, which reads a column because
    /// receiving has no cancellation to unwind.
    pub fn outstanding(&self, invoiced: i32) -> i32 {
        (self.quantity - invoiced).max(0)
    }

    pub fn is_fully_invoiced(&self, invoiced: i32) -> bool {
        invoiced >= self.quantity
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Invoice {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub invoice_number: String,
    pub customer_id: Uuid,
    pub order_id: Option<Uuid>,
    pub status: String, // draft, sent, paid, overdue, cancelled
    pub issue_date: NaiveDate,
    pub due_date: NaiveDate,
    pub subtotal: Option<Decimal>,
    pub tax_amount: Option<Decimal>,
    pub total: Option<Decimal>,
    pub amount_paid: Option<Decimal>,
    pub amount_due: Option<Decimal>,
    pub currency: String,
    pub fx_rate: Decimal,
    pub base_total: Option<Decimal>,
    /// Paid and due, restated at the *invoice's* rate — not at each payment's.
    /// These answer "what is still outstanding on this invoice", which is a
    /// question about the invoice. What the money was actually worth when it
    /// arrived lives on the payment, along with the difference between the two.
    pub base_amount_paid: Option<Decimal>,
    pub base_amount_due: Option<Decimal>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct InvoiceLine {
    pub id: Uuid,
    pub invoice_id: Uuid,
    /// The order line this is billing, when the invoice came from an order.
    ///
    /// What an order line has left to bill is summed from these, so the link is
    /// the whole mechanism behind partial fulfilment. `None` for an invoice
    /// raised straight against a customer.
    pub order_line_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub description: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub discount_percent: Decimal,
    pub tax_rate: Decimal,
    pub line_total: Decimal,
    pub sort_order: i32,
}

/// Crediting a customer: the counterpart of an [`Invoice`].
///
/// Stores its own totals, unlike a purchase return which derives them from the
/// order. The reason is not convention: `invoice_entries` derives its revenue leg
/// as `base_total − base_tax` so that rounding cannot leave the legs disagreeing
/// with the receivable, and a credit note has to reverse that identically —
/// which needs a stored total to be the remainder's anchor.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct CreditNote {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub credit_note_number: String,
    pub invoice_id: Uuid,
    pub customer_id: Uuid,
    pub issue_date: NaiveDate,
    /// Why it was issued, in whatever words fit.
    pub reason: Option<String>,
    /// Where returned goods landed. `None` is ordinary and means the credit is
    /// money only — a price dispute or an over-billing.
    pub warehouse_id: Option<Uuid>,
    pub subtotal: Decimal,
    pub tax_amount: Decimal,
    pub total: Decimal,
    pub currency: String,
    /// The *invoice's* rate. The receivable was raised at it, so relieving it at
    /// any other would leave a difference nothing accounts for.
    pub fx_rate: Decimal,
    pub base_total: Decimal,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct CreditNoteLine {
    pub id: Uuid,
    pub credit_note_id: Uuid,
    /// Everything about the price comes from here — a credit is worth what was
    /// charged, not what the item is worth today.
    pub invoice_line_id: Uuid,
    pub product_id: Option<Uuid>,
    pub description: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub discount_percent: Decimal,
    pub tax_rate: Decimal,
    pub line_total: Decimal,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Payment {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub invoice_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    /// The rate on the day the money arrived, which is deliberately not the
    /// invoice's rate.
    pub fx_rate: Decimal,
    /// What the payment was actually worth, at the payment's own rate.
    pub base_amount: Decimal,
    /// The realised FX gain (positive) or loss (negative), in base currency:
    /// what the money turned out to be worth, less what the invoice said it
    /// would be worth. Zero whenever both rates agree.
    pub fx_gain_loss: Decimal,
    pub payment_method: String, // bank_transfer, credit_card, cash, check, paypal, stripe
    pub payment_date: NaiveDate,
    pub reference: Option<String>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------- workflows

/// Quote: draft -> sent -> [accepted | rejected | expired]
pub struct QuoteStatus;

impl QuoteStatus {
    pub const DRAFT: &'static str = "draft";
    pub const SENT: &'static str = "sent";
    pub const ACCEPTED: &'static str = "accepted";
    pub const REJECTED: &'static str = "rejected";
    pub const EXPIRED: &'static str = "expired";

    pub const ALL: [&'static str; 5] =
        [Self::DRAFT, Self::SENT, Self::ACCEPTED, Self::REJECTED, Self::EXPIRED];

    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            (Self::DRAFT, Self::SENT)
                | (Self::DRAFT, Self::EXPIRED)
                | (Self::SENT, Self::ACCEPTED)
                | (Self::SENT, Self::REJECTED)
                | (Self::SENT, Self::EXPIRED)
        )
    }

    /// Only a draft quote may have its lines or dates rewritten.
    pub fn is_editable(status: &str) -> bool {
        status == Self::DRAFT
    }
}

/// SalesOrder: draft -> confirmed -> processing -> [partially_shipped ->]
/// shipped -> delivered, with cancellation available until it ships in full.
pub struct OrderStatus;

impl OrderStatus {
    pub const DRAFT: &'static str = "draft";
    pub const CONFIRMED: &'static str = "confirmed";
    pub const PROCESSING: &'static str = "processing";
    /// Some of the order has been invoiced and shipped; some is still owed.
    ///
    /// Set by the invoicing flow, never requested by hand — the mirror of
    /// `PurchaseOrderStatus::PARTIALLY_RECEIVED`, which the goods-receipt flow
    /// sets the same way.
    pub const PARTIALLY_SHIPPED: &'static str = "partially_shipped";
    pub const SHIPPED: &'static str = "shipped";
    pub const DELIVERED: &'static str = "delivered";
    pub const CANCELLED: &'static str = "cancelled";

    pub const ALL: [&'static str; 7] = [
        Self::DRAFT,
        Self::CONFIRMED,
        Self::PROCESSING,
        Self::PARTIALLY_SHIPPED,
        Self::SHIPPED,
        Self::DELIVERED,
        Self::CANCELLED,
    ];

    /// Transitions a user may request directly.
    ///
    /// `partially_shipped` is absent as a *destination* on purpose: an order
    /// reaches it by being part-invoiced, not by somebody saying so. It appears
    /// only as a source, because an order in that state can still ship the rest
    /// or be cancelled.
    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            (Self::DRAFT, Self::CONFIRMED)
                | (Self::CONFIRMED, Self::PROCESSING)
                | (Self::PROCESSING, Self::SHIPPED)
                | (Self::PARTIALLY_SHIPPED, Self::SHIPPED)
                | (Self::SHIPPED, Self::DELIVERED)
                | (Self::DRAFT, Self::CANCELLED)
                | (Self::CONFIRMED, Self::CANCELLED)
                | (Self::PROCESSING, Self::CANCELLED)
                | (Self::PARTIALLY_SHIPPED, Self::CANCELLED)
        )
    }

    pub fn is_editable(status: &str) -> bool {
        matches!(status, Self::DRAFT | Self::CONFIRMED)
    }

    /// An order has to be at least confirmed before it can be billed.
    pub fn is_invoiceable(status: &str) -> bool {
        matches!(
            status,
            Self::CONFIRMED
                | Self::PROCESSING
                | Self::PARTIALLY_SHIPPED
                | Self::SHIPPED
                | Self::DELIVERED
        )
    }

    /// Where an order lands once an instalment has been issued against it.
    ///
    /// Only moves an order that was still being worked on: one already marked
    /// `shipped` or `delivered` has said its goods are gone, and one that is
    /// `cancelled` is not coming back. The mirror of
    /// `PurchaseOrderStatus::after_receipt`.
    pub fn after_invoice(current: &str, all_lines_invoiced: bool) -> Option<&'static str> {
        if !matches!(current, Self::CONFIRMED | Self::PROCESSING | Self::PARTIALLY_SHIPPED) {
            return None;
        }
        // Fully invoiced does not mean shipped: marking it so stays the
        // operator's call, and the lifecycle guard now lets them make it.
        if all_lines_invoiced {
            None
        } else if current == Self::PARTIALLY_SHIPPED {
            None
        } else {
            Some(Self::PARTIALLY_SHIPPED)
        }
    }

    /// Whether this status claims the goods have physically left.
    ///
    /// Both of these were pure labels: order status changes touch stock only on
    /// `confirmed` and `cancelled`, so an order could be marked delivered while
    /// its goods sat on the shelf, reserved indefinitely — `delivered` is
    /// terminal, and only invoicing releases a reservation.
    ///
    /// `shipped` is included as well as `delivered`. Guarding only the terminal
    /// status would leave the same problem one step earlier, with an order
    /// sitting in `shipped` and its goods still on the shelf.
    pub fn asserts_goods_have_left(status: &str) -> bool {
        matches!(status, Self::SHIPPED | Self::DELIVERED)
    }

    /// Whether the order is still waiting for goods it has been promised.
    ///
    /// Read when an invoice is cancelled and its stock comes back: the hold
    /// issuing released is retaken only for an order that still expects the
    /// goods. A `cancelled` order would strand stock against a dead document,
    /// and one that `asserts_goods_have_left` cannot reach that path at all —
    /// cancelling its invoice is refused.
    ///
    /// `draft` is excluded because nothing is held until an order is confirmed.
    pub fn still_expects_goods(status: &str) -> bool {
        matches!(status, Self::CONFIRMED | Self::PROCESSING | Self::PARTIALLY_SHIPPED)
    }
}

/// Invoice: draft -> sent -> [paid | overdue] -> cancelled.
/// `paid` is reached by recording payments, not by a direct status call.
pub struct InvoiceStatus;

impl InvoiceStatus {
    pub const DRAFT: &'static str = "draft";
    pub const SENT: &'static str = "sent";
    pub const PAID: &'static str = "paid";
    pub const OVERDUE: &'static str = "overdue";
    pub const CANCELLED: &'static str = "cancelled";

    pub const ALL: [&'static str; 5] =
        [Self::DRAFT, Self::SENT, Self::PAID, Self::OVERDUE, Self::CANCELLED];

    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            (Self::DRAFT, Self::SENT)
                | (Self::DRAFT, Self::CANCELLED)
                | (Self::SENT, Self::PAID)
                | (Self::SENT, Self::OVERDUE)
                | (Self::SENT, Self::CANCELLED)
                | (Self::OVERDUE, Self::PAID)
                | (Self::OVERDUE, Self::CANCELLED)
        )
    }

    pub fn is_editable(status: &str) -> bool {
        status == Self::DRAFT
    }

    /// Whether the invoice went out: the goods left, the revenue is recognised
    /// and the receivable exists.
    ///
    /// One definition for the three places that need it — the order lifecycle
    /// guard asking whether an order may claim its goods have gone, the
    /// cancellation path asking whether there is anything to unwind, and
    /// anything later that means "this document has had effects". A **draft**
    /// has done none of it; a **cancelled** invoice has had it all undone.
    pub fn is_issued(status: &str) -> bool {
        matches!(status, Self::SENT | Self::PAID | Self::OVERDUE)
    }

    /// Whether this invoice still stands as its order's invoice.
    ///
    /// A cancelled one does not: it has been unwound on both sides, so it must
    /// not keep the order from being invoiced again. Everything else — a draft
    /// included — does, which is why an order gets one live invoice at a time.
    pub fn is_live(status: &str) -> bool {
        status != Self::CANCELLED
    }

    /// Whether the invoice is a live receivable, and so has a debt to settle.
    ///
    /// Stated positively, because the negative form let a **draft** take money:
    /// it relieved a receivable that had never been raised, and settlement then
    /// wrote `sent` or `paid` straight onto the document — issuing it without
    /// shipping its goods or recognising its revenue. A draft has raised nothing
    /// to settle, a paid one has nothing outstanding, a cancelled one is closed.
    pub fn accepts_payment(status: &str) -> bool {
        matches!(status, Self::SENT | Self::OVERDUE)
    }
}

pub struct PaymentMethod;

impl PaymentMethod {
    pub const ALL: [&'static str; 6] =
        ["bank_transfer", "credit_card", "cash", "check", "stripe", "paypal"];

    pub fn is_valid(value: &str) -> bool {
        Self::ALL.contains(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_shipped_and_delivered_claim_the_goods_have_left() {
        assert!(OrderStatus::asserts_goods_have_left(OrderStatus::SHIPPED));
        assert!(OrderStatus::asserts_goods_have_left(OrderStatus::DELIVERED));

        for status in [
            OrderStatus::DRAFT,
            OrderStatus::CONFIRMED,
            OrderStatus::PROCESSING,
            // Cancelling gives the goods back rather than sending them out.
            OrderStatus::CANCELLED,
        ] {
            assert!(!OrderStatus::asserts_goods_have_left(status), "{status}");
        }
    }

    #[test]
    fn a_line_knows_what_is_left_to_bill() {
        let line = OrderLine {
            id: Uuid::new_v4(),
            order_id: Uuid::new_v4(),
            product_id: None,
            description: "Widget".to_string(),
            quantity: 10,
            unit_price: Decimal::from(20),
            discount_percent: Decimal::ZERO,
            tax_rate: Decimal::ZERO,
            line_total: Decimal::from(200),
            sort_order: 0,
        };

        assert_eq!(line.outstanding(0), 10);
        assert_eq!(line.outstanding(6), 4);
        assert_eq!(line.outstanding(10), 0);
        // Over-invoicing is refused before it can happen, but the figure must
        // not go negative if it ever does.
        assert_eq!(line.outstanding(12), 0);

        assert!(!line.is_fully_invoiced(9));
        assert!(line.is_fully_invoiced(10));
    }

    #[test]
    fn an_instalment_moves_a_live_order_to_partially_shipped() {
        assert_eq!(
            OrderStatus::after_invoice(OrderStatus::PROCESSING, false),
            Some(OrderStatus::PARTIALLY_SHIPPED)
        );
        assert_eq!(
            OrderStatus::after_invoice(OrderStatus::CONFIRMED, false),
            Some(OrderStatus::PARTIALLY_SHIPPED)
        );

        // Already there: nothing to write.
        assert_eq!(OrderStatus::after_invoice(OrderStatus::PARTIALLY_SHIPPED, false), None);
        // Fully invoiced is not the same as shipped; that stays a decision.
        assert_eq!(OrderStatus::after_invoice(OrderStatus::PROCESSING, true), None);
        // And a closed order is not reopened by a late instalment.
        for status in [OrderStatus::SHIPPED, OrderStatus::DELIVERED, OrderStatus::CANCELLED] {
            assert_eq!(OrderStatus::after_invoice(status, false), None, "{status}");
        }
    }

    #[test]
    fn partially_shipped_is_reached_only_by_invoicing() {
        // Never a destination a caller may ask for...
        for from in OrderStatus::ALL {
            assert!(
                !OrderStatus::can_transition(from, OrderStatus::PARTIALLY_SHIPPED),
                "{from} -> partially_shipped"
            );
        }
        // ...but an order sitting in it can still finish or be called off.
        assert!(OrderStatus::can_transition(
            OrderStatus::PARTIALLY_SHIPPED,
            OrderStatus::SHIPPED
        ));
        assert!(OrderStatus::can_transition(
            OrderStatus::PARTIALLY_SHIPPED,
            OrderStatus::CANCELLED
        ));
    }

    #[test]
    fn only_a_live_order_expects_its_goods_back() {
        assert!(OrderStatus::still_expects_goods(OrderStatus::CONFIRMED));
        assert!(OrderStatus::still_expects_goods(OrderStatus::PROCESSING));
        // Part-shipped still wants the rest, so a cancelled instalment gives
        // its hold back.
        assert!(OrderStatus::still_expects_goods(OrderStatus::PARTIALLY_SHIPPED));

        for status in [
            // Nothing is held until an order is confirmed.
            OrderStatus::DRAFT,
            // Holding stock against a dead order would strand it.
            OrderStatus::CANCELLED,
            // These two say the goods are with the customer.
            OrderStatus::SHIPPED,
            OrderStatus::DELIVERED,
        ] {
            assert!(!OrderStatus::still_expects_goods(status), "{status}");
        }
    }

    #[test]
    fn only_an_issued_invoice_has_had_effects() {
        for status in [InvoiceStatus::SENT, InvoiceStatus::PAID, InvoiceStatus::OVERDUE] {
            assert!(InvoiceStatus::is_issued(status), "{status}");
        }

        // A draft has shipped nothing and posted nothing; a cancelled invoice
        // has had both undone. Neither has anything left to unwind.
        assert!(!InvoiceStatus::is_issued(InvoiceStatus::DRAFT));
        assert!(!InvoiceStatus::is_issued(InvoiceStatus::CANCELLED));
    }

    #[test]
    fn a_cancelled_invoice_no_longer_stands() {
        for status in [
            InvoiceStatus::DRAFT,
            InvoiceStatus::SENT,
            InvoiceStatus::PAID,
            InvoiceStatus::OVERDUE,
        ] {
            assert!(InvoiceStatus::is_live(status), "{status}");
        }

        assert!(!InvoiceStatus::is_live(InvoiceStatus::CANCELLED));
    }

    #[test]
    fn quote_follows_the_documented_path() {
        assert!(QuoteStatus::can_transition(QuoteStatus::DRAFT, QuoteStatus::SENT));
        assert!(QuoteStatus::can_transition(QuoteStatus::SENT, QuoteStatus::ACCEPTED));
        // Cannot skip `sent`, and terminal states are terminal.
        assert!(!QuoteStatus::can_transition(QuoteStatus::DRAFT, QuoteStatus::ACCEPTED));
        assert!(!QuoteStatus::can_transition(QuoteStatus::ACCEPTED, QuoteStatus::SENT));
        assert!(!QuoteStatus::can_transition(QuoteStatus::REJECTED, QuoteStatus::ACCEPTED));
    }

    #[test]
    fn order_cannot_be_cancelled_once_shipped() {
        assert!(OrderStatus::can_transition(OrderStatus::PROCESSING, OrderStatus::CANCELLED));
        assert!(!OrderStatus::can_transition(OrderStatus::SHIPPED, OrderStatus::CANCELLED));
        assert!(!OrderStatus::can_transition(OrderStatus::DELIVERED, OrderStatus::CANCELLED));
    }

    #[test]
    fn draft_order_is_not_invoiceable() {
        assert!(!OrderStatus::is_invoiceable(OrderStatus::DRAFT));
        assert!(OrderStatus::is_invoiceable(OrderStatus::CONFIRMED));
    }

    #[test]
    fn invoice_reaches_paid_only_from_sent_or_overdue() {
        assert!(InvoiceStatus::can_transition(InvoiceStatus::SENT, InvoiceStatus::PAID));
        assert!(InvoiceStatus::can_transition(InvoiceStatus::OVERDUE, InvoiceStatus::PAID));
        assert!(!InvoiceStatus::can_transition(InvoiceStatus::DRAFT, InvoiceStatus::PAID));
        assert!(!InvoiceStatus::can_transition(InvoiceStatus::CANCELLED, InvoiceStatus::SENT));
    }

    #[test]
    fn only_a_live_receivable_takes_money() {
        // The two states with a debt outstanding.
        assert!(InvoiceStatus::accepts_payment(InvoiceStatus::SENT));
        assert!(InvoiceStatus::accepts_payment(InvoiceStatus::OVERDUE));

        assert!(!InvoiceStatus::accepts_payment(InvoiceStatus::CANCELLED));
        assert!(!InvoiceStatus::accepts_payment(InvoiceStatus::PAID));
        // The one this rule was widened to catch: a draft has raised no
        // receivable, so there is nothing for money to settle.
        assert!(!InvoiceStatus::accepts_payment(InvoiceStatus::DRAFT));
    }

    #[test]
    fn payment_methods_are_closed_set() {
        assert!(PaymentMethod::is_valid("stripe"));
        assert!(!PaymentMethod::is_valid("crypto"));
    }
}
