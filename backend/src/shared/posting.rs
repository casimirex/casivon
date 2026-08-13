use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::AppResult;

/// Posts business events to the general ledger.
///
/// A trait in `shared` rather than a direct call into the accounting module, for
/// the same reason [`crate::shared::email::EmailSender`] and
/// [`crate::shared::currency::CurrencyResolver`] are: sales should not know how
/// the books are kept, only that raising an invoice is an event the books care
/// about. The implementation lives in the accounting module and is injected
/// through `AppState`.
///
/// Every method is a no-op when the organisation has not mapped its accounts.
/// That is what keeps an installation which never configured posting working
/// exactly as it did before this existed.
///
/// Each method is **safe to call twice**. Entries are written with a unique
/// posting key naming the event, so a repeat is refused by the database rather
/// than doubling revenue — see `014_gl_posting.sql`.
#[async_trait]
pub trait DocumentPoster: Send + Sync {
    /// An invoice has been issued: the customer now owes the money, and the
    /// revenue is earned.
    async fn invoice_issued(&self, invoice: &PostableInvoice) -> AppResult<()>;

    /// An invoice has been cancelled. Posts the mirror of what issuing posted;
    /// never deletes, because posted history is not rewritten.
    async fn invoice_cancelled(&self, invoice: &PostableInvoice) -> AppResult<()>;

    /// Money has arrived against an invoice.
    async fn payment_received(&self, payment: &PostablePayment) -> AppResult<()>;

    /// A payment has been reversed. Posts the mirror.
    async fn payment_reversed(&self, payment: &PostablePayment) -> AppResult<()>;

    /// Goods have arrived against a purchase order: the cost is incurred and the
    /// supplier is owed.
    async fn goods_received(&self, receipt: &PostableReceipt) -> AppResult<()>;

    /// Goods have gone back to the supplier: the payable is reduced and
    /// whatever the receipt capitalised or expensed is given back.
    async fn goods_returned(&self, ret: &PostableReturn) -> AppResult<()>;

    /// A credit note has been issued: revenue and tax come back off, the
    /// receivable is reduced, and — when goods came back — stock returns to the
    /// balance sheet at the cost it is carried at.
    async fn credit_note_issued(&self, note: &PostableCreditNote) -> AppResult<()>;

    /// Money has left against a purchase order.
    async fn vendor_payment_made(&self, payment: &PostablePayment) -> AppResult<()>;

    /// A vendor payment has been reversed. Posts the mirror.
    async fn vendor_payment_reversed(&self, payment: &PostablePayment) -> AppResult<()>;

    /// An expense report has been approved: the cost is incurred and the
    /// employee is owed.
    async fn expense_approved(&self, report: &PostableExpenseReport) -> AppResult<()>;

    /// The employee has been paid back.
    async fn expense_reimbursed(&self, report: &PostableExpenseReport) -> AppResult<()>;

    /// An approved report has been withdrawn. Posts the mirror of the approval.
    async fn expense_reversed(&self, report: &PostableExpenseReport) -> AppResult<()>;

    /// Stock has moved. Under perpetual costing this is where the cost of a sale
    /// reaches the profit and loss, and where a hand-made adjustment reaches the
    /// ledger at all.
    ///
    /// Does nothing until the inventory accounts are mapped, which is what keeps
    /// an existing installation on periodic costing until it chooses otherwise.
    async fn stock_moved(&self, movement: &PostableMovement) -> AppResult<()>;

    /// Puts stock that was already on the shelves onto the balance sheet, once,
    /// when an installation switches to perpetual costing.
    ///
    /// Returns whether anything was actually written — `false` means it had been
    /// done already, or there was no stock to open with.
    async fn inventory_opened(&self, opening: &PostableOpening) -> AppResult<bool>;
}

/// The one-time opening of the Inventory account.
#[derive(Debug, Clone)]
pub struct PostableOpening {
    pub org_id: Option<Uuid>,
    /// Total value of stock on hand, in the base currency.
    pub value: Decimal,
    pub on: NaiveDate,
    pub created_by: Uuid,
}

/// What the ledger needs from an invoice.
///
/// A plain struct rather than the sales entity so that `shared` does not depend
/// on any module: the poster needs six figures, not an `Invoice`.
#[derive(Debug, Clone)]
pub struct PostableInvoice {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub number: String,
    /// The date the entry is posted under — the invoice's own issue date, so a
    /// back-dated invoice lands in the period it belongs to.
    pub issue_date: NaiveDate,
    /// Kept for the entry description; the amounts below are already restated.
    pub currency: String,
    pub fx_rate: Decimal,
    /// The whole invoice in base currency, tax included.
    pub base_total: Decimal,
    /// Tax in the *transaction* currency. Restated by the posting rules, which
    /// need it and the total together to keep the two legs adding up.
    pub tax_amount: Decimal,
    pub created_by: Uuid,
}

/// What the ledger needs from a credit note.
///
/// Deliberately the same shape as [`PostableInvoice`], because a credit note is
/// that document pointing the other way and has to reverse it exactly — right
/// down to deriving revenue as the remainder rather than restating a subtotal.
#[derive(Debug, Clone)]
pub struct PostableCreditNote {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub number: String,
    pub issue_date: NaiveDate,
    pub fx_rate: Decimal,
    /// The whole credit note in base currency, tax included.
    pub base_total: Decimal,
    /// Tax in the *transaction* currency, restated by the posting rules.
    pub tax_amount: Decimal,
    /// What the returned goods were worth, in base currency, when a warehouse
    /// was named. Zero when the credit is money only — a price dispute or an
    /// over-billing, where nothing came back.
    pub returned_cost: Decimal,
    pub created_by: Uuid,
}

/// What the ledger needs from a payment.
#[derive(Debug, Clone)]
pub struct PostablePayment {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    /// For the entry description — a journal line reading "Payment for INV-014"
    /// is worth more to whoever reads the ledger than a bare uuid.
    pub document_number: String,
    pub payment_date: NaiveDate,
    /// What the money was worth on the day it arrived.
    pub base_amount: Decimal,
    /// Positive is a gain, negative a loss, zero in a single-currency
    /// installation. See `Payment::fx_gain_loss`.
    pub fx_gain_loss: Decimal,
    pub created_by: Uuid,
}

/// What the ledger needs from a goods receipt.
///
/// A receipt records quantities, not prices, so the value comes from the order's
/// lines. `net` and `tax` are already totalled across the received quantities,
/// in the **order's** currency, and `fx_rate` is the order's — a delivery is
/// worth what the order committed to, not what the rate happens to be the day
/// the van arrives.
#[derive(Debug, Clone)]
pub struct PostableReceipt {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub number: String,
    pub receipt_date: NaiveDate,
    pub fx_rate: Decimal,
    /// Lines that name a stocked product. Under perpetual costing this becomes
    /// an asset rather than an expense, which is why it is separated from the
    /// rest of the delivery.
    pub stocked_net: Decimal,
    /// Everything else on the delivery — freight, services, free-text lines.
    /// These hold no inventory and are a cost the moment they arrive, whichever
    /// costing method is in force.
    pub expensed_net: Decimal,
    /// Input tax, kept apart from the cost because it is usually recoverable.
    pub tax: Decimal,
    pub created_by: Uuid,
}

/// What the ledger needs from a purchase return.
///
/// Deliberately the same shape as [`PostableReceipt`], because a return is that
/// document pointing the other way and the two are valued identically — at the
/// purchase order's own line price, restated at the order's rate.
#[derive(Debug, Clone)]
pub struct PostableReturn {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub number: String,
    pub return_date: NaiveDate,
    pub fx_rate: Decimal,
    /// Lines that named a stocked product, and so came off the Inventory asset.
    pub stocked_net: Decimal,
    /// Freight, services and free-text lines, which were a cost on arrival and
    /// are credited back to it.
    pub expensed_net: Decimal,
    /// Input tax, going back with the goods.
    pub tax: Decimal,
    pub created_by: Uuid,
}

/// What the ledger needs from a stock movement.
///
/// Stock moving is the moment cost becomes cost, so this carries the *value* of
/// the movement rather than its quantity and price: the quantity is already
/// spent by the time anyone asks, and the value is what has to reconcile with
/// the Inventory account.
#[derive(Debug, Clone)]
pub struct PostableMovement {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub movement_type: String,
    /// Signed: positive brings stock in, negative takes it out. Adjustments
    /// carry their own sign, which is exactly what decides their direction.
    pub quantity_delta: i32,
    /// Base-currency value moved, already rounded to cents.
    pub value: Decimal,
    pub entry_date: NaiveDate,
    /// What caused the movement, if a document did. A goods receipt has already
    /// posted its own side, so its movements must not post again.
    pub reference_type: Option<String>,
    /// For the journal line, e.g. `SKU-1 — Widget`.
    pub description: String,
    pub created_by: Uuid,
}

/// What the ledger needs from an expense report.
#[derive(Debug, Clone)]
pub struct PostableExpenseReport {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub number: String,
    /// The day the approval or reimbursement happened. An expense report has no
    /// document date of its own — it is a claim, and what matters is when it was
    /// accepted.
    pub on: NaiveDate,
    pub base_total: Decimal,
    pub created_by: Uuid,
}
