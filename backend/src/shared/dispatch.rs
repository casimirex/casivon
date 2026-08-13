//! Stock leaving and returning because a sales document said so.
//!
//! A trait in `shared` rather than a direct call into the inventory module, for
//! the same reason [`crate::shared::posting::DocumentPoster`] is one: sales
//! should not know what a warehouse is, only that issuing an invoice is an event
//! the shelves care about. The implementation lives in the inventory module and
//! is injected through `AppState`.

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::AppResult;
use crate::shared::auth::CurrentUser;

#[async_trait]
pub trait StockDispatcher: Send + Sync {
    /// An invoice has been issued, so the goods on it have left.
    ///
    /// **Does nothing at all** until a default dispatch warehouse is configured,
    /// which is what keeps an installation that never chose one behaving exactly
    /// as it did before this existed.
    ///
    /// Once one *is* configured this can fail: issuing an invoice for stock that
    /// is not on the shelf is refused, naming the SKU and what is available.
    /// That is the point — it catches the error where it is made — but it is why
    /// the setting is opt-in.
    async fn invoice_issued(
        &self,
        invoice: &DispatchableInvoice,
        user: &CurrentUser,
    ) -> AppResult<()>;

    /// An invoice has been cancelled, so the goods come back.
    async fn invoice_cancelled(
        &self,
        invoice: &DispatchableInvoice,
        user: &CurrentUser,
    ) -> AppResult<()>;

    /// An order has been confirmed, so its goods are promised to someone.
    ///
    /// Holds what each shelf can cover and leaves the rest of the promise
    /// unreserved — selling before buying is ordinary, and refusing to confirm
    /// would block it outright. Like everything else here, does nothing until a
    /// dispatch warehouse is configured.
    async fn order_confirmed(&self, order: &DispatchableOrder) -> AppResult<()>;

    /// An order is no longer promising anything — cancelled, or about to be
    /// re-reserved after an edit. Gives back exactly what was held.
    async fn order_released(&self, order_id: Uuid) -> AppResult<()>;

    /// Whether issuing an invoice actually moves stock on this installation.
    ///
    /// Sales asks this rather than asking about warehouses, because what it
    /// needs to know is whether invoicing is the moment goods leave — and if it
    /// is, an order cannot claim they have left before it has been invoiced.
    async fn ships_automatically(&self) -> AppResult<bool>;
}

/// What the shelves need from a sales order.
#[derive(Debug, Clone)]
pub struct DispatchableOrder {
    pub id: Uuid,
    pub lines: Vec<ReservableLine>,
}

#[derive(Debug, Clone, Copy)]
pub struct ReservableLine {
    pub order_line_id: Uuid,
    pub product_id: Option<Uuid>,
    pub quantity: i32,
}

/// What the shelves need from an invoice.
///
/// Carries no warehouse: which shelf the goods leave from is the
/// implementation's business, read from the organisation's settings. Sales
/// knows what was sold, not where it was kept.
#[derive(Debug, Clone)]
pub struct DispatchableInvoice {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    /// The order this invoice came from, if any.
    ///
    /// Its goods are reserved *by that order*, and the availability check would
    /// otherwise see them as unavailable — an order blocking its own shipment.
    /// So the reservation is released in the same transaction as the movements.
    pub order_id: Option<Uuid>,
    /// For the movement's note, so a stock ledger reads "Invoice INV-014"
    /// rather than a bare uuid.
    pub number: String,
    pub lines: Vec<DispatchableLine>,
}

/// One line of an invoice, as far as stock is concerned.
///
/// `product_id` is optional because a free-text line — a delivery charge, a
/// consulting day — is perfectly ordinary and holds no stock.
#[derive(Debug, Clone, Copy)]
pub struct DispatchableLine {
    pub product_id: Option<Uuid>,
    pub quantity: i32,
    /// The order line this bills, when the invoice came from an order.
    ///
    /// An invoice may cover only part of its order, so shipping it releases only
    /// what it ships: releasing the whole order would hand back stock still owed
    /// on lines this instalment does not touch.
    pub order_line_id: Option<Uuid>,
}
