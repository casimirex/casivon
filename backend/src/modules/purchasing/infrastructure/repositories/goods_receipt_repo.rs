use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::purchasing::domain::entities::{GoodsReceipt, GoodsReceiptLine};
use crate::modules::purchasing::domain::repositories::{
    GoodsReceiptFilters, GoodsReceiptRepository,
};
use crate::shared::pagination::PaginationParams;

#[derive(Clone)]
pub struct PgGoodsReceiptRepository {
    pool: PgPool,
}

impl PgGoodsReceiptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a GoodsReceiptFilters) {
    if let Some(po_id) = filters.po_id {
        builder.push(" AND po_id = ").push_bind(po_id);
    }
    if let Some(warehouse_id) = filters.warehouse_id {
        builder.push(" AND warehouse_id = ").push_bind(warehouse_id);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND receipt_date >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND receipt_date <= ").push_bind(to);
    }
}

#[async_trait]
impl GoodsReceiptRepository for PgGoodsReceiptRepository {
    async fn create(
        &self,
        receipt: &GoodsReceipt,
        lines: &[GoodsReceiptLine],
        new_order_status: &str,
    ) -> AppResult<GoodsReceipt> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, GoodsReceipt>(
            r#"
            INSERT INTO goods_receipts
                (id, org_id, po_id, receipt_number, receipt_date, status, warehouse_id,
                 notes, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
        )
        .bind(receipt.id)
        .bind(receipt.org_id)
        .bind(receipt.po_id)
        .bind(&receipt.receipt_number)
        .bind(receipt.receipt_date)
        .bind(&receipt.status)
        .bind(receipt.warehouse_id)
        .bind(&receipt.notes)
        .bind(receipt.created_by)
        .bind(receipt.created_at)
        .fetch_one(&mut *tx)
        .await?;

        for line in lines {
            sqlx::query(
                r#"
                INSERT INTO goods_receipt_lines
                    (id, receipt_id, po_line_id, product_id, quantity_received, notes)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(line.id)
            .bind(line.receipt_id)
            .bind(line.po_line_id)
            .bind(line.product_id)
            .bind(line.quantity_received)
            .bind(&line.notes)
            .execute(&mut *tx)
            .await?;

            // Increment in SQL rather than writing a value computed earlier, so
            // two concurrent receipts cannot overwrite each other's progress.
            sqlx::query(
                r#"
                UPDATE purchase_order_lines
                SET received_quantity = received_quantity + $2
                WHERE id = $1
                "#,
            )
            .bind(line.po_line_id)
            .bind(line.quantity_received)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query("UPDATE purchase_orders SET status = $2, updated_at = $3 WHERE id = $1")
            .bind(receipt.po_id)
            .bind(new_order_status)
            .bind(Utc::now())
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<GoodsReceipt>> {
        Ok(sqlx::query_as::<_, GoodsReceipt>("SELECT * FROM goods_receipts WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_lines(&self, receipt_id: Uuid) -> AppResult<Vec<GoodsReceiptLine>> {
        Ok(sqlx::query_as::<_, GoodsReceiptLine>(
            "SELECT * FROM goods_receipt_lines WHERE receipt_id = $1",
        )
        .bind(receipt_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn list(
        &self,
        filters: &GoodsReceiptFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<GoodsReceipt>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM goods_receipts WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&["created_at", "receipt_date"], "receipt_date")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<GoodsReceipt>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM goods_receipts WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('GR', 'goods_receipt_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }
}
