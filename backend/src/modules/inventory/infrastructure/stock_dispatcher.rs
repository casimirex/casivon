use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::inventory::application::dto::RecordMovementRequest;
use crate::modules::inventory::application::use_cases::StockUseCases;
use crate::modules::inventory::domain::entities::{MovementType, ProductType};
use crate::modules::inventory::domain::repositories::{
    ProductRepository, ReservationRelease, ReservationRequest, StockRepository,
};
use crate::modules::inventory::infrastructure::repositories::{
    PgProductRepository, PgStockRepository, PgWarehouseRepository,
};
use crate::shared::auth::CurrentUser;
use crate::shared::dispatch::{
    DispatchableInvoice, DispatchableLine, DispatchableOrder, ReservableLine, StockDispatcher,
};
use crate::shared::posting::DocumentPoster;

/// Tags the movements an invoice produces, so they can be traced back — and so
/// the posting rules can tell a cancelled sale coming back onto the shelf from
/// somebody adding stock by hand.
const INVOICE_REFERENCE: &str = "sales_invoice";

/// Moves stock when a sales invoice is issued or cancelled.
///
/// Deliberately thin: it works out *which* lines hold stock and *which* shelf
/// they leave from, then hands each one to
/// [`StockUseCases::record_movement`] — which already refuses to move more than
/// is available, maintains the moving average, and posts the movement. Doing the
/// movement any other way would mean a second implementation of those rules,
/// free to drift from the one a person gets when they record a movement by hand.
pub struct PgStockDispatcher {
    pool: PgPool,
    poster: Arc<dyn DocumentPoster>,
}

impl PgStockDispatcher {
    pub fn new(pool: PgPool, poster: Arc<dyn DocumentPoster>) -> Self {
        Self { pool, poster }
    }

    /// The organisation's dispatch warehouse, or `None` if nobody chose one.
    ///
    /// Read per invoice rather than cached: an admin can set it under Settings,
    /// and the next invoice must pick that up without a restart — the same
    /// reasoning the posting mapping is read per posting.
    async fn dispatch_warehouse(&self) -> AppResult<Option<Uuid>> {
        Ok(sqlx::query_scalar::<_, Option<Uuid>>(
            "SELECT default_dispatch_warehouse_id FROM organization_settings WHERE singleton",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    fn stock(&self) -> StockUseCases<PgStockRepository, PgProductRepository, PgWarehouseRepository> {
        StockUseCases::new(
            PgStockRepository::new(self.pool.clone()),
            PgProductRepository::new(self.pool.clone()),
            PgWarehouseRepository::new(self.pool.clone()),
            self.poster.clone(),
        )
    }

    /// The lines that actually hold stock, with their quantities.
    ///
    /// A free-text line — a delivery charge, a consulting day — names no
    /// product, and a `service` product names one that was never on a shelf.
    /// Both are ordinary on an invoice and neither moves.
    async fn stocked_lines(&self, lines: &[DispatchableLine]) -> AppResult<Vec<(Uuid, i32)>> {
        let products = PgProductRepository::new(self.pool.clone());
        let mut stocked = Vec::new();

        for line in lines {
            let Some(product_id) = line.product_id else {
                continue;
            };
            if line.quantity <= 0 {
                continue;
            }
            let Some(product) = products.find_by_id(product_id).await? else {
                continue;
            };
            if !ProductType::is_stocked(&product.product_type) {
                continue;
            }
            stocked.push((product_id, line.quantity));
        }

        Ok(stocked)
    }

    /// The stocked lines of an order, with the quantities to hold.
    async fn reservable_lines(&self, lines: &[ReservableLine]) -> AppResult<Vec<ReservationRequest>> {
        let products = PgProductRepository::new(self.pool.clone());
        let mut wanted = Vec::new();

        for line in lines {
            let Some(product_id) = line.product_id else {
                continue;
            };
            if line.quantity <= 0 {
                continue;
            }
            let Some(product) = products.find_by_id(product_id).await? else {
                continue;
            };
            if !ProductType::is_stocked(&product.product_type) {
                continue;
            }
            wanted.push(ReservationRequest {
                order_line_id: line.order_line_id,
                product_id,
                quantity: line.quantity,
            });
        }

        Ok(wanted)
    }

    async fn move_lines(
        &self,
        invoice: &DispatchableInvoice,
        user: &CurrentUser,
        movement_type: &str,
        note: &str,
    ) -> AppResult<()> {
        let Some(warehouse_id) = self.dispatch_warehouse().await? else {
            // Nobody has opted in, so invoicing moves nothing — exactly as it
            // behaved before this existed.
            return Ok(());
        };

        let requests: Vec<RecordMovementRequest> = self
            .stocked_lines(&invoice.lines)
            .await?
            .into_iter()
            .map(|(product_id, quantity)| RecordMovementRequest {
                product_id,
                warehouse_id,
                to_warehouse_id: None,
                movement_type: movement_type.to_string(),
                quantity,
                // No cost of its own. Going out, that means consuming at the
                // running average; coming back, blending in at it.
                unit_cost: None,
                reference_type: Some(INVOICE_REFERENCE.to_string()),
                reference_id: Some(invoice.id),
                notes: Some(format!("{note} {}", invoice.number)),
            })
            .collect();

        // Only what this invoice ships, line by line. An invoice may cover part
        // of its order, and releasing the whole order would hand back stock
        // still owed on the lines this instalment does not touch.
        //
        // Only when goods are going *out*: bringing them back on a cancellation
        // takes no hold with it, and the order's re-hold is a separate decision
        // sales makes afterwards.
        let release: Vec<ReservationRelease> = if movement_type == MovementType::OUT {
            invoice
                .lines
                .iter()
                .filter_map(|line| {
                    line.order_line_id.map(|order_line_id| ReservationRelease {
                        order_line_id,
                        quantity: line.quantity,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        // One call, so the whole invoice moves or none of it does. A line the
        // shelf cannot cover used to leave the earlier lines already gone. The
        // release rides inside that same transaction: the goods this invoice
        // ships are reserved by the order it came from, and moving stock checks
        // what is *available*, so without the release an order blocks its own
        // shipment — while releasing separately left the hold gone whenever the
        // shipment was refused.
        self.stock().record_movements(requests, &release, user).await?;

        Ok(())
    }
}

#[async_trait]
impl StockDispatcher for PgStockDispatcher {
    async fn order_confirmed(&self, order: &DispatchableOrder) -> AppResult<()> {
        let Some(warehouse_id) = self.dispatch_warehouse().await? else {
            return Ok(());
        };

        let wanted = self.reservable_lines(&order.lines).await?;
        PgStockRepository::new(self.pool.clone())
            .reserve_for_order(order.id, warehouse_id, &wanted)
            .await?;

        Ok(())
    }

    async fn ships_automatically(&self) -> AppResult<bool> {
        Ok(self.dispatch_warehouse().await?.is_some())
    }

    async fn order_released(&self, order_id: Uuid) -> AppResult<()> {
        // Deliberately not gated on the warehouse setting: an order confirmed
        // while dispatch was on must still be able to give its goods back after
        // somebody switches it off.
        PgStockRepository::new(self.pool.clone()).release_order(order_id).await?;
        Ok(())
    }

    async fn invoice_issued(
        &self,
        invoice: &DispatchableInvoice,
        user: &CurrentUser,
    ) -> AppResult<()> {
        self.move_lines(invoice, user, MovementType::OUT, "Invoice").await
    }

    async fn invoice_cancelled(
        &self,
        invoice: &DispatchableInvoice,
        user: &CurrentUser,
    ) -> AppResult<()> {
        self.move_lines(invoice, user, MovementType::IN, "Cancelled invoice").await
    }
}
