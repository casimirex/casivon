use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::inventory::domain::costing::{average_after_removal, moving_average};
use crate::modules::inventory::domain::entities::{
    MovementType, StockLevel, StockMovement, StockReservation,
};
use crate::modules::inventory::domain::errors::InventoryError;
use crate::modules::inventory::domain::repositories::{
    MovementFilters, ReservationRelease, ReservationRequest, StockRepository,
};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 2] = ["created_at", "quantity"];

#[derive(Clone)]
pub struct PgStockRepository {
    pool: PgPool,
}

impl PgStockRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Adds `delta` to one product/warehouse pair, creating the row if this is the
/// first movement for that pair. `FOR UPDATE` via ON CONFLICT keeps concurrent
/// movements from losing each other's writes.
async fn adjust_level(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    warehouse_id: Uuid,
    delta: i32,
) -> AppResult<StockLevel> {
    Ok(sqlx::query_as::<_, StockLevel>(
        r#"
        INSERT INTO stock_levels (product_id, warehouse_id, quantity, updated_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (product_id, warehouse_id) DO UPDATE
            SET quantity = stock_levels.quantity + EXCLUDED.quantity,
                updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(product_id)
    .bind(warehouse_id)
    .bind(delta)
    .fetch_one(&mut **tx)
    .await?)
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a MovementFilters) {
    if let Some(product_id) = filters.product_id {
        builder.push(" AND product_id = ").push_bind(product_id);
    }
    if let Some(warehouse_id) = filters.warehouse_id {
        // A transfer shows up in both the source and the destination log.
        builder
            .push(" AND (warehouse_id = ")
            .push_bind(warehouse_id)
            .push(" OR to_warehouse_id = ")
            .push_bind(warehouse_id)
            .push(")");
    }
    if let Some(movement_type) = &filters.movement_type {
        builder.push(" AND movement_type = ").push_bind(movement_type);
    }
    if let Some(reference_type) = &filters.reference_type {
        builder.push(" AND reference_type = ").push_bind(reference_type);
    }
    if let Some(reference_id) = filters.reference_id {
        builder.push(" AND reference_id = ").push_bind(reference_id);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        // Inclusive of the whole end day.
        builder.push(" AND created_at < (").push_bind(to).push("::date + 1)");
    }
}

/// Gives back part of what an order line holds, inside a transaction the caller
/// owns.
///
/// Per line and per quantity, because an invoice may ship only part of what the
/// order promised: releasing the whole order would hand back stock still owed on
/// lines this instalment does not touch.
///
/// The row is decremented and deleted only when it reaches zero, which is what
/// keeps `UNIQUE (order_line_id)` meaningful — one hold per line at a time,
/// shrinking as the line ships.
async fn release_line(
    tx: &mut Transaction<'_, Postgres>,
    order_line_id: Uuid,
    quantity: i32,
) -> AppResult<i32> {
    let Some(held) = sqlx::query_as::<_, StockReservation>(
        "SELECT * FROM stock_reservations WHERE order_line_id = $1 FOR UPDATE",
    )
    .bind(order_line_id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        // Nothing held for this line: ordinary for an order confirmed against an
        // empty shelf, or one confirmed before reservations existed.
        return Ok(0);
    };

    // Never more than is actually held. A line can be invoiced for more than was
    // reserved — confirming short of stock reserves what there is and leaves the
    // rest of the promise unheld — and giving back what was never taken would
    // drive `reserved_quantity` below zero.
    let giving_back = quantity.min(held.quantity).max(0);
    if giving_back == 0 {
        return Ok(0);
    }

    if held.quantity > giving_back {
        sqlx::query("UPDATE stock_reservations SET quantity = quantity - $2 WHERE id = $1")
            .bind(held.id)
            .bind(giving_back)
            .execute(&mut **tx)
            .await?;
    } else {
        sqlx::query("DELETE FROM stock_reservations WHERE id = $1")
            .bind(held.id)
            .execute(&mut **tx)
            .await?;
    }

    sqlx::query(
        r#"
        UPDATE stock_levels
        SET reserved_quantity = GREATEST(reserved_quantity - $3, 0), updated_at = NOW()
        WHERE product_id = $1 AND warehouse_id = $2
        "#,
    )
    .bind(held.product_id)
    .bind(held.warehouse_id)
    .bind(giving_back)
    .execute(&mut **tx)
    .await?;

    Ok(giving_back)
}

/// Gives back everything an order holds, inside a transaction the caller owns.
///
/// Used when an order is cancelled or its lines replaced, where the whole
/// promise goes at once.
async fn release_into(tx: &mut Transaction<'_, Postgres>, order_id: Uuid) -> AppResult<u64> {
    // Deleted first and the rows used to drive the give-back, so exactly what
    // was taken is what comes back — the reserved quantity was never the
    // ordered quantity.
    let released = sqlx::query_as::<_, StockReservation>(
        "DELETE FROM stock_reservations WHERE order_id = $1 RETURNING *",
    )
    .bind(order_id)
    .fetch_all(&mut **tx)
    .await?;

    for reservation in &released {
        sqlx::query(
            r#"
            UPDATE stock_levels
            SET reserved_quantity = GREATEST(reserved_quantity - $3, 0), updated_at = NOW()
            WHERE product_id = $1 AND warehouse_id = $2
            "#,
        )
        .bind(reservation.product_id)
        .bind(reservation.warehouse_id)
        .bind(reservation.quantity)
        .execute(&mut **tx)
        .await?;
    }

    Ok(released.len() as u64)
}

/// Refuses a movement the shelf cannot cover, naming what it is short of.
///
/// Only ever called on the way to refusing, so the two lookups cost nothing in
/// the ordinary case. The alternative — every caller carrying a SKU and a
/// warehouse name it does not otherwise need, on the chance of an error — put
/// the cost on the path that succeeds.
async fn insufficient(
    tx: &mut Transaction<'_, Postgres>,
    movement: &StockMovement,
    available: i32,
    requested: i32,
) -> AppError {
    let names: Result<(String, String), _> = sqlx::query_as(
        r#"
        SELECT p.sku, w.name
        FROM products p, warehouses w
        WHERE p.id = $1 AND w.id = $2
        "#,
    )
    .bind(movement.product_id)
    .bind(movement.warehouse_id)
    .fetch_one(&mut **tx)
    .await;

    match names {
        Ok((sku, warehouse)) => {
            InventoryError::InsufficientStock { sku, warehouse, available, requested }.into()
        }
        Err(err) => err.into(),
    }
}

/// Applies one movement inside a transaction the caller owns.
///
/// Availability is re-checked here rather than trusted from the caller: the
/// level is locked for the rest of the transaction, so two shipments of the last
/// unit cannot both find it free. A caller that checked earlier gets a faster
/// refusal in the ordinary case and this one when it matters.
async fn apply_into(
    tx: &mut Transaction<'_, Postgres>,
    movement: &StockMovement,
) -> AppResult<(StockMovement, StockLevel)> {
    if MovementType::removes_from_source(&movement.movement_type) || movement.quantity < 0 {
        let level = sqlx::query_as::<_, StockLevel>(
            "SELECT * FROM stock_levels WHERE product_id = $1 AND warehouse_id = $2 FOR UPDATE",
        )
        .bind(movement.product_id)
        .bind(movement.warehouse_id)
        .fetch_optional(&mut **tx)
        .await?;

        let available = level.map(|l| l.available()).unwrap_or(0);
        let requested = movement.quantity.abs();
        if available < requested {
            return Err(insufficient(tx, movement, available, requested).await);
        }
    }

    apply_costed(tx, movement).await
}

/// The costing half: what the movement was worth, and what it does to the
/// running average.
async fn apply_costed(
    tx: &mut Transaction<'_, Postgres>,
    movement: &StockMovement,
) -> AppResult<(StockMovement, StockLevel)> {
    // Locked, not merely read: the average is a read-modify-write, and two
    // deliveries of the same product arriving at once would otherwise each
    // compute a new average from the same old one and the second would
    // discard the first.
    let (on_hand, average): (Option<i64>, Option<Decimal>) = sqlx::query_as(
        r#"
        SELECT (SELECT COALESCE(SUM(quantity), 0) FROM stock_levels WHERE product_id = $1),
               average_cost
        FROM products WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(movement.product_id)
    .fetch_one(&mut **tx)
    .await?;
    let on_hand = on_hand.unwrap_or(0) as i32;

    // A movement that names its own cost is one where the price is a fact of
    // the document — a delivery, or goods going back at what they arrived
    // at. One that does not is an ordinary sale, which consumes at whatever
    // the average is now: exactly what the caller cannot know without this
    // lock.
    let names_its_own_cost = movement.base_unit_cost.is_some();
    let base_unit_cost = movement.base_unit_cost.or(average);

    let source_delta = MovementType::source_delta(&movement.movement_type, movement.quantity);

    // A transfer changes which shelf stock sits on, not what it cost.
    if movement.movement_type != MovementType::TRANSFER {
        if let Some(cost) = base_unit_cost {
            let updated = if source_delta > 0 {
                // Stock arriving blends into the average.
                Some(moving_average(on_hand, average, source_delta, cost))
            } else if names_its_own_cost {
                // Goods going back to a supplier leave at the price they
                // arrived at, so the average has to un-blend to match — or
                // the valuation report and the Inventory account, which is
                // credited with that same price, stop agreeing.
                Some(average_after_removal(on_hand, average, -source_delta, cost))
            } else {
                // An ordinary sale. Consuming at the average is what leaves
                // it alone, which is the whole idea of a weighted average.
                None
            };

            if let Some(updated) = updated {
                sqlx::query("UPDATE products SET average_cost = $2 WHERE id = $1")
                    .bind(movement.product_id)
                    .bind(updated)
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }

    let created = sqlx::query_as::<_, StockMovement>(
        r#"
        INSERT INTO stock_movements
            (id, org_id, product_id, warehouse_id, to_warehouse_id, movement_type, quantity,
             unit_cost, base_unit_cost, reference_type, reference_id, notes, created_by,
             created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING *
        "#,
    )
    .bind(movement.id)
    .bind(movement.org_id)
    .bind(movement.product_id)
    .bind(movement.warehouse_id)
    .bind(movement.to_warehouse_id)
    .bind(&movement.movement_type)
    .bind(movement.quantity)
    .bind(movement.unit_cost)
    // Recorded on the row rather than looked up again later: the average
    // moves with the next delivery, and what this movement was worth has to
    // stay what it was worth.
    .bind(base_unit_cost)
    .bind(&movement.reference_type)
    .bind(movement.reference_id)
    .bind(&movement.notes)
    .bind(movement.created_by)
    .bind(movement.created_at)
    .fetch_one(&mut **tx)
    .await?;

    let level =
        adjust_level(tx, movement.product_id, movement.warehouse_id, source_delta).await?;

    // The other half of a transfer.
    if let Some(destination) = movement.to_warehouse_id {
        adjust_level(tx, movement.product_id, destination, movement.quantity).await?;
    }

    Ok((created, level))
}

#[async_trait]
impl StockRepository for PgStockRepository {
    async fn apply_movements(
        &self,
        release: &[ReservationRelease],
        movements: &[StockMovement],
    ) -> AppResult<Vec<(StockMovement, StockLevel)>> {
        let mut tx = self.pool.begin().await?;

        // Before the movements, because shipping checks what is *available* and
        // an order's own hold would otherwise block its own shipment. Inside the
        // same transaction, because a refused shipment must leave the hold
        // exactly where it was.
        for line in release {
            release_line(&mut tx, line.order_line_id, line.quantity).await?;
        }

        let mut applied = Vec::with_capacity(movements.len());
        for movement in movements {
            applied.push(apply_into(&mut tx, movement).await?);
        }

        tx.commit().await?;
        Ok(applied)
    }

    async fn reserve_for_order(
        &self,
        order_id: Uuid,
        warehouse_id: Uuid,
        wanted: &[ReservationRequest],
    ) -> AppResult<Vec<StockReservation>> {
        let mut tx = self.pool.begin().await?;
        let mut held = Vec::new();

        for request in wanted {
            if request.quantity <= 0 {
                continue;
            }

            // Locked for the length of the transaction, so two orders confirmed
            // at the same moment cannot both be told the same unit is free.
            let level = sqlx::query_as::<_, StockLevel>(
                "SELECT * FROM stock_levels WHERE product_id = $1 AND warehouse_id = $2 FOR UPDATE",
            )
            .bind(request.product_id)
            .bind(warehouse_id)
            .fetch_optional(&mut *tx)
            .await?;

            // Nothing on the shelf at all: the promise stands, unreserved.
            let Some(level) = level else {
                continue;
            };

            let quantity = level.reservable(request.quantity);
            if quantity == 0 {
                continue;
            }

            let reservation = sqlx::query_as::<_, StockReservation>(
                r#"
                INSERT INTO stock_reservations
                    (order_id, order_line_id, product_id, warehouse_id, quantity)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING *
                "#,
            )
            .bind(order_id)
            .bind(request.order_line_id)
            .bind(request.product_id)
            .bind(warehouse_id)
            .bind(quantity)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                r#"
                UPDATE stock_levels
                SET reserved_quantity = reserved_quantity + $3, updated_at = NOW()
                WHERE product_id = $1 AND warehouse_id = $2
                "#,
            )
            .bind(request.product_id)
            .bind(warehouse_id)
            .bind(quantity)
            .execute(&mut *tx)
            .await?;

            held.push(reservation);
        }

        tx.commit().await?;
        Ok(held)
    }

    async fn release_order(&self, order_id: Uuid) -> AppResult<u64> {
        let mut tx = self.pool.begin().await?;
        let released = release_into(&mut tx, order_id).await?;
        tx.commit().await?;
        Ok(released)
    }

    async fn find_level(
        &self,
        product_id: Uuid,
        warehouse_id: Uuid,
    ) -> AppResult<Option<StockLevel>> {
        Ok(sqlx::query_as::<_, StockLevel>(
            "SELECT * FROM stock_levels WHERE product_id = $1 AND warehouse_id = $2",
        )
        .bind(product_id)
        .bind(warehouse_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn levels_for_product(&self, product_id: Uuid) -> AppResult<Vec<StockLevel>> {
        Ok(sqlx::query_as::<_, StockLevel>(
            "SELECT * FROM stock_levels WHERE product_id = $1 ORDER BY warehouse_id",
        )
        .bind(product_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn levels_for_warehouse(
        &self,
        warehouse_id: Uuid,
        params: &PaginationParams,
    ) -> AppResult<(Vec<StockLevel>, i64)> {
        let rows = sqlx::query_as::<_, StockLevel>(
            "SELECT * FROM stock_levels WHERE warehouse_id = $1 ORDER BY updated_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(warehouse_id)
        .bind(params.per_page())
        .bind(params.offset())
        .fetch_all(&self.pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM stock_levels WHERE warehouse_id = $1",
        )
        .bind(warehouse_id)
        .fetch_one(&self.pool)
        .await?;

        Ok((rows, total))
    }

    async fn low_stock(&self, params: &PaginationParams) -> AppResult<(Vec<StockLevel>, i64)> {
        let condition =
            "reorder_level IS NOT NULL AND (quantity - reserved_quantity) <= reorder_level";

        let rows = sqlx::query_as::<_, StockLevel>(&format!(
            "SELECT * FROM stock_levels WHERE {} ORDER BY (quantity - reserved_quantity) ASC LIMIT $1 OFFSET $2",
            condition
        ))
        .bind(params.per_page())
        .bind(params.offset())
        .fetch_all(&self.pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM stock_levels WHERE {}",
            condition
        ))
        .fetch_one(&self.pool)
        .await?;

        Ok((rows, total))
    }

    async fn set_reorder_policy(
        &self,
        product_id: Uuid,
        warehouse_id: Uuid,
        reorder_level: i32,
        reorder_quantity: i32,
    ) -> AppResult<StockLevel> {
        Ok(sqlx::query_as::<_, StockLevel>(
            r#"
            INSERT INTO stock_levels
                (product_id, warehouse_id, quantity, reorder_level, reorder_quantity, updated_at)
            VALUES ($1, $2, 0, $3, $4, NOW())
            ON CONFLICT (product_id, warehouse_id) DO UPDATE
                SET reorder_level = EXCLUDED.reorder_level,
                    reorder_quantity = EXCLUDED.reorder_quantity,
                    updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(product_id)
        .bind(warehouse_id)
        .bind(reorder_level)
        .bind(reorder_quantity)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn list_movements(
        &self,
        filters: &MovementFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<StockMovement>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM stock_movements WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<StockMovement>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM stock_movements WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    /// Stock valuation, in the base currency.
    ///
    /// At the moving average rather than the standing `cost_price`, so that with
    /// the inventory accounts mapped this figure and the Inventory account
    /// balance are the same number — which is the invariant to reach for first
    /// when the books look wrong.
    ///
    /// No restatement happens here because `products` carries no currency:
    /// product prices are base-currency by definition until price lists exist.
    /// See the closing note in `013_multi_currency.sql`.
    async fn valuation(&self) -> AppResult<Decimal> {
        Ok(sqlx::query_scalar::<_, Decimal>(
            r#"
            -- Rounded to cents because this is a money figure on the wire, and
            -- the average it is built from carries four decimal places: without
            -- this the endpoint reports "675.0000" where every other amount in
            -- the API reads "675.00".
            SELECT ROUND(COALESCE(SUM(sl.quantity * COALESCE(p.average_cost, 0)), 0), 2)
            FROM stock_levels sl
            JOIN products p ON p.id = sl.product_id
            "#,
        )
        .fetch_one(&self.pool)
        .await?)
    }
}
