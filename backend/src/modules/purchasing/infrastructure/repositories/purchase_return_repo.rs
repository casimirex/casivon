use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::purchasing::domain::entities::{PurchaseReturn, PurchaseReturnLine};
use crate::modules::purchasing::domain::repositories::{
    PurchaseReturnFilters, PurchaseReturnRepository,
};
use crate::shared::pagination::PaginationParams;

#[derive(Clone)]
pub struct PgPurchaseReturnRepository {
    pool: PgPool,
}

impl PgPurchaseReturnRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a PurchaseReturnFilters) {
    if let Some(po_id) = filters.po_id {
        builder.push(" AND po_id = ").push_bind(po_id);
    }
    if let Some(warehouse_id) = filters.warehouse_id {
        builder.push(" AND warehouse_id = ").push_bind(warehouse_id);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND return_date >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND return_date <= ").push_bind(to);
    }
}

#[async_trait]
impl PurchaseReturnRepository for PgPurchaseReturnRepository {
    async fn create(
        &self,
        ret: &PurchaseReturn,
        lines: &[PurchaseReturnLine],
        new_order_status: &str,
    ) -> AppResult<PurchaseReturn> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, PurchaseReturn>(
            r#"
            INSERT INTO purchase_returns
                (id, org_id, po_id, return_number, return_date, warehouse_id, reason,
                 notes, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(ret.id)
        .bind(ret.org_id)
        .bind(ret.po_id)
        .bind(&ret.return_number)
        .bind(ret.return_date)
        .bind(ret.warehouse_id)
        .bind(&ret.reason)
        .bind(&ret.notes)
        .bind(ret.created_by)
        .bind(ret.created_at)
        .fetch_one(&mut *tx)
        .await?;

        for line in lines {
            sqlx::query(
                r#"
                INSERT INTO purchase_return_lines
                    (id, return_id, po_line_id, product_id, quantity_returned, notes)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(line.id)
            .bind(line.return_id)
            .bind(line.po_line_id)
            .bind(line.product_id)
            .bind(line.quantity_returned)
            .bind(&line.notes)
            .execute(&mut *tx)
            .await?;

            // Decremented in SQL for the same reason the receipt increments in
            // SQL: two documents landing at once must not overwrite each other's
            // progress. `GREATEST(…, 0)` is belt and braces — the use case
            // already refuses to return more than is recorded as received — but
            // a negative received quantity would make `outstanding()` nonsense.
            sqlx::query(
                r#"
                UPDATE purchase_order_lines
                SET received_quantity = GREATEST(received_quantity - $2, 0)
                WHERE id = $1
                "#,
            )
            .bind(line.po_line_id)
            .bind(line.quantity_returned)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query("UPDATE purchase_orders SET status = $2, updated_at = $3 WHERE id = $1")
            .bind(ret.po_id)
            .bind(new_order_status)
            .bind(Utc::now())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<PurchaseReturn>> {
        Ok(sqlx::query_as::<_, PurchaseReturn>("SELECT * FROM purchase_returns WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_lines(&self, return_id: Uuid) -> AppResult<Vec<PurchaseReturnLine>> {
        Ok(sqlx::query_as::<_, PurchaseReturnLine>(
            "SELECT * FROM purchase_return_lines WHERE return_id = $1",
        )
        .bind(return_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn list(
        &self,
        filters: &PurchaseReturnFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<PurchaseReturn>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM purchase_returns WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&["created_at", "return_date"], "return_date")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<PurchaseReturn>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM purchase_returns WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('PR', 'purchase_return_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    async fn total_returned_for_order(&self, po_id: Uuid) -> AppResult<Decimal> {
        // Valued exactly as the posting values it: rounded per line, net first
        // and then tax on the rounded net — the same shape as the unposted
        // receipt query. Summing unrounded and rounding once at the end would let
        // what the order says is owed differ from what the ledger says by a cent.
        Ok(sqlx::query_scalar::<_, Decimal>(
            r#"
            SELECT COALESCE(SUM(
                ROUND(rl.quantity_returned * pol.unit_price, 2)
                + ROUND(ROUND(rl.quantity_returned * pol.unit_price, 2) * pol.tax_rate / 100, 2)
            ), 0)
            FROM purchase_return_lines rl
            JOIN purchase_returns r ON r.id = rl.return_id
            JOIN purchase_order_lines pol ON pol.id = rl.po_line_id
            WHERE r.po_id = $1
            "#,
        )
        .bind(po_id)
        .fetch_one(&self.pool)
        .await?)
    }
}
