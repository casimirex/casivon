use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::purchasing::domain::entities::{PurchaseOrder, PurchaseOrderLine};
use crate::modules::purchasing::domain::repositories::{
    PurchaseOrderFilters, PurchaseOrderRepository,
};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 4] = ["created_at", "order_date", "expected_date", "total"];

#[derive(Clone)]
pub struct PgPurchaseOrderRepository {
    pool: PgPool,
}

impl PgPurchaseOrderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

pub(super) async fn insert_lines(
    tx: &mut Transaction<'_, Postgres>,
    lines: &[PurchaseOrderLine],
) -> AppResult<()> {
    for line in lines {
        sqlx::query(
            r#"
            INSERT INTO purchase_order_lines
                (id, po_id, product_id, description, quantity, unit_price,
                 tax_rate, received_quantity, line_total, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(line.id)
        .bind(line.po_id)
        .bind(line.product_id)
        .bind(&line.description)
        .bind(line.quantity)
        .bind(line.unit_price)
        .bind(line.tax_rate)
        .bind(line.received_quantity)
        .bind(line.line_total)
        .bind(line.sort_order)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a PurchaseOrderFilters) {
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(vendor_id) = filters.vendor_id {
        builder.push(" AND vendor_id = ").push_bind(vendor_id);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND order_date >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND order_date <= ").push_bind(to);
    }
    if let Some(search) = &filters.search {
        builder.push(" AND po_number ILIKE ").push_bind(format!("%{}%", search));
    }
}

#[async_trait]
impl PurchaseOrderRepository for PgPurchaseOrderRepository {
    async fn create(
        &self,
        order: &PurchaseOrder,
        lines: &[PurchaseOrderLine],
    ) -> AppResult<PurchaseOrder> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, PurchaseOrder>(
            r#"
            INSERT INTO purchase_orders
                (id, org_id, po_number, vendor_id, status, order_date, expected_date,
                 shipping_address, subtotal, tax_amount, total, currency, notes,
                 created_by, created_at, updated_at, fx_rate, base_total,
                 amount_paid, amount_due, base_amount_paid, base_amount_due)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
                    $19, $20, $21, $22)
            RETURNING *
            "#,
        )
        .bind(order.id)
        .bind(order.org_id)
        .bind(&order.po_number)
        .bind(order.vendor_id)
        .bind(&order.status)
        .bind(order.order_date)
        .bind(order.expected_date)
        .bind(&order.shipping_address)
        .bind(order.subtotal)
        .bind(order.tax_amount)
        .bind(order.total)
        .bind(&order.currency)
        .bind(&order.notes)
        .bind(order.created_by)
        .bind(order.created_at)
        .bind(order.updated_at)
        .bind(order.fx_rate)
        .bind(order.base_total)
        .bind(order.amount_paid)
        .bind(order.amount_due)
        .bind(order.base_amount_paid)
        .bind(order.base_amount_due)
        .fetch_one(&mut *tx)
        .await?;

        insert_lines(&mut tx, lines).await?;
        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<PurchaseOrder>> {
        Ok(sqlx::query_as::<_, PurchaseOrder>("SELECT * FROM purchase_orders WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_lines(&self, po_id: Uuid) -> AppResult<Vec<PurchaseOrderLine>> {
        Ok(sqlx::query_as::<_, PurchaseOrderLine>(
            "SELECT * FROM purchase_order_lines WHERE po_id = $1 ORDER BY sort_order",
        )
        .bind(po_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn update(
        &self,
        order: &PurchaseOrder,
        lines: Option<&[PurchaseOrderLine]>,
    ) -> AppResult<PurchaseOrder> {
        let mut tx = self.pool.begin().await?;

        let updated = sqlx::query_as::<_, PurchaseOrder>(
            r#"
            UPDATE purchase_orders SET
                vendor_id = $2, expected_date = $3, shipping_address = $4, subtotal = $5,
                tax_amount = $6, total = $7, notes = $8, updated_at = $9,
                fx_rate = $10, base_total = $11,
                -- Re-pricing an unreceived order changes what is owed on it.
                amount_due = $12, base_amount_due = $13
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(order.id)
        .bind(order.vendor_id)
        .bind(order.expected_date)
        .bind(&order.shipping_address)
        .bind(order.subtotal)
        .bind(order.tax_amount)
        .bind(order.total)
        .bind(&order.notes)
        .bind(Utc::now())
        .bind(order.fx_rate)
        .bind(order.base_total)
        .bind(order.amount_due)
        .bind(order.base_amount_due)
        .fetch_one(&mut *tx)
        .await?;

        if let Some(lines) = lines {
            // Receipt lines reference PO lines, so only orders without receipts
            // reach this path (enforced by the `is_editable` check in the use case).
            sqlx::query("DELETE FROM purchase_order_lines WHERE po_id = $1")
                .bind(order.id)
                .execute(&mut *tx)
                .await?;
            insert_lines(&mut tx, lines).await?;
        }

        tx.commit().await?;
        Ok(updated)
    }

    async fn update_status(&self, id: Uuid, status: &str) -> AppResult<PurchaseOrder> {
        Ok(sqlx::query_as::<_, PurchaseOrder>(
            "UPDATE purchase_orders SET status = $2, updated_at = $3 WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(status)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn update_settlement(
        &self,
        id: Uuid,
        amount_paid: rust_decimal::Decimal,
        amount_due: rust_decimal::Decimal,
        base_amount_paid: rust_decimal::Decimal,
        base_amount_due: rust_decimal::Decimal,
    ) -> AppResult<PurchaseOrder> {
        Ok(sqlx::query_as::<_, PurchaseOrder>(
            r#"
            UPDATE purchase_orders
            SET amount_paid = $2, amount_due = $3,
                base_amount_paid = $4, base_amount_due = $5,
                updated_at = $6
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(amount_paid)
        .bind(amount_due)
        .bind(base_amount_paid)
        .bind(base_amount_due)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM purchase_orders WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &PurchaseOrderFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<PurchaseOrder>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM purchase_orders WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<PurchaseOrder>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM purchase_orders WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('PO', 'purchase_order_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }
}
