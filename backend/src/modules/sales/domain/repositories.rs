use async_trait::async_trait;
use utoipa::IntoParams;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::sales::domain::entities::*;
use crate::shared::pagination::PaginationParams;

/// Filters shared by every sales document list endpoint.
#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SalesDocumentFilters {
    pub status: Option<String>,
    pub customer_id: Option<Uuid>,
    /// Matches the document number, case-insensitively.
    pub search: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait QuoteRepository: Send + Sync {
    /// Inserts header and lines in one transaction — a quote with half its lines
    /// written would be worse than no quote at all.
    async fn create(&self, quote: &Quote, lines: &[QuoteLine]) -> AppResult<Quote>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Quote>>;
    async fn find_lines(&self, quote_id: Uuid) -> AppResult<Vec<QuoteLine>>;
    /// Replaces the header and, when `lines` is `Some`, the whole line set.
    async fn update(&self, quote: &Quote, lines: Option<&[QuoteLine]>) -> AppResult<Quote>;
    async fn update_status(&self, id: Uuid, status: &str) -> AppResult<Quote>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Quote>, i64)>;
    async fn next_number(&self) -> AppResult<String>;
    /// The order a quote was converted into, if any.
    async fn find_converted_order(&self, quote_id: Uuid) -> AppResult<Option<SalesOrder>>;
}

#[async_trait]
pub trait SalesOrderRepository: Send + Sync {
    async fn create(&self, order: &SalesOrder, lines: &[OrderLine]) -> AppResult<SalesOrder>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<SalesOrder>>;
    async fn find_lines(&self, order_id: Uuid) -> AppResult<Vec<OrderLine>>;
    async fn update(&self, order: &SalesOrder, lines: Option<&[OrderLine]>) -> AppResult<SalesOrder>;
    async fn update_status(&self, id: Uuid, status: &str) -> AppResult<SalesOrder>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<SalesOrder>, i64)>;
    async fn next_number(&self) -> AppResult<String>;
    /// How much of each line this order's live invoices have billed.
    ///
    /// Derived rather than counted into a column, so cancelling an invoice,
    /// deleting a draft or editing a draft's lines all give the quantity back
    /// without a decrement path to write. Cancelled invoices are excluded for
    /// exactly that reason — their goods came back and their posting was
    /// mirrored, so they bill nothing.
    ///
    /// The shape `CreditNoteRepository::credited_by_invoice_line` uses to decide
    /// what an invoice line has left to credit. Lines with no `order_line_id` —
    /// an invoice raised straight against a customer — do not appear.
    async fn invoiced_by_order_line(&self, order_id: Uuid) -> AppResult<Vec<(Uuid, i64)>>;

    /// How much of each order line the given invoices bill.
    ///
    /// The same sum as `invoiced_by_order_line` narrowed to a chosen set, so the
    /// lifecycle guard can ask what has actually *shipped* — issued invoices
    /// only — rather than what has been billed on paper.
    async fn invoiced_by_invoices(&self, invoice_ids: &[Uuid]) -> AppResult<Vec<(Uuid, i64)>>;

    /// Every invoice ever raised against this order, oldest first.
    ///
    /// Plural because a cancelled invoice does not stop the order being
    /// invoiced again, so "the order's invoice" is not a well-formed question.
    /// Callers apply their own rule — `InvoiceStatus::is_live` to find the one
    /// that still stands, `is_issued` to find one whose goods actually went.
    async fn find_invoices_for_order(&self, order_id: Uuid) -> AppResult<Vec<Invoice>>;
}

#[async_trait]
pub trait InvoiceRepository: Send + Sync {
    async fn create(&self, invoice: &Invoice, lines: &[InvoiceLine]) -> AppResult<Invoice>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Invoice>>;
    async fn find_lines(&self, invoice_id: Uuid) -> AppResult<Vec<InvoiceLine>>;
    async fn update(&self, invoice: &Invoice, lines: Option<&[InvoiceLine]>) -> AppResult<Invoice>;
    async fn update_status(&self, id: Uuid, status: &str) -> AppResult<Invoice>;
    /// Writes the settlement columns after a payment is recorded or removed.
    ///
    /// `base_amount_paid` and `base_amount_due` are restated at the *invoice's*
    /// rate, so the two always reconcile against its base total.
    async fn update_settlement(
        &self,
        id: Uuid,
        amount_paid: rust_decimal::Decimal,
        amount_due: rust_decimal::Decimal,
        base_amount_paid: rust_decimal::Decimal,
        base_amount_due: rust_decimal::Decimal,
        status: &str,
    ) -> AppResult<Invoice>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Invoice>, i64)>;
    async fn next_number(&self) -> AppResult<String>;
    /// Flags sent invoices whose due date has passed; returns how many changed.
    async fn mark_overdue(&self, today: NaiveDate) -> AppResult<u64>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PaymentFilters {
    pub invoice_id: Option<Uuid>,
    pub payment_method: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait PaymentRepository: Send + Sync {
    async fn create(&self, payment: &Payment) -> AppResult<Payment>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Payment>>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &PaymentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Payment>, i64)>;
    /// Total settled against one invoice — the source of truth for `amount_paid`.
    async fn total_paid_for_invoice(&self, invoice_id: Uuid) -> AppResult<rust_decimal::Decimal>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CreditNoteFilters {
    pub invoice_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait CreditNoteRepository: Send + Sync {
    async fn create(
        &self,
        note: &CreditNote,
        lines: &[CreditNoteLine],
    ) -> AppResult<CreditNote>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<CreditNote>>;
    async fn find_lines(&self, credit_note_id: Uuid) -> AppResult<Vec<CreditNoteLine>>;
    async fn list(
        &self,
        filters: &CreditNoteFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<CreditNote>, i64)>;
    async fn next_number(&self) -> AppResult<String>;

    /// Total credited against one invoice, in its own currency. Used the same
    /// way `total_paid_for_invoice` is: settlement is derived from these ledgers
    /// rather than accumulated on the invoice.
    async fn total_credited_for_invoice(
        &self,
        invoice_id: Uuid,
    ) -> AppResult<rust_decimal::Decimal>;

    /// How many units of each invoice line have already been credited.
    ///
    /// A purchase return needs no equivalent because it decrements
    /// `received_quantity` on the order line. Invoice lines are immutable, so
    /// this tally is the only thing standing between a customer and being
    /// credited twice for the same goods.
    async fn credited_by_invoice_line(&self, invoice_id: Uuid) -> AppResult<Vec<(Uuid, i64)>>;
}
