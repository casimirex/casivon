use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::sales::domain::entities::{Invoice, OrderLine, SalesOrder};
use crate::modules::sales::domain::repositories::{SalesDocumentFilters, SalesOrderRepository};
use crate::modules::sales::infrastructure::repositories::quote_repo::push_document_filters;
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 5] =
    ["created_at", "order_date", "required_date", "total", "order_number"];

#[derive(Clone)]
pub struct PgSalesOrderRepository {
    pool: PgPool,
}

impl PgSalesOrderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

async fn insert_lines(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    lines: &[OrderLine],
) -> AppResult<()> {
    for line in lines {
        sqlx::query(
            r#"
            INSERT INTO sales_order_lines
                (id, order_id, product_id, description, quantity, unit_price,
                 discount_percent, tax_rate, line_total, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(line.id)
        .bind(line.order_id)
        .bind(line.product_id)
        .bind(&line.description)
        .bind(line.quantity)
        .bind(line.unit_price)
        .bind(line.discount_percent)
        .bind(line.tax_rate)
        .bind(line.line_total)
        .bind(line.sort_order)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[async_trait]
impl SalesOrderRepository for PgSalesOrderRepository {
    async fn create(&self, order: &SalesOrder, lines: &[OrderLine]) -> AppResult<SalesOrder> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, SalesOrder>(
            r#"
            INSERT INTO sales_orders
                (id, org_id, order_number, customer_id, contact_id, quote_id, status,
                 order_date, required_date, shipping_address, billing_address,
                 subtotal, tax_amount, total, currency, notes, created_by, created_at, updated_at,
                 fx_rate, base_total)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19,
                    $20, $21)
            RETURNING *
            "#,
        )
        .bind(order.id)
        .bind(order.org_id)
        .bind(&order.order_number)
        .bind(order.customer_id)
        .bind(order.contact_id)
        .bind(order.quote_id)
        .bind(&order.status)
        .bind(order.order_date)
        .bind(order.required_date)
        .bind(&order.shipping_address)
        .bind(&order.billing_address)
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
        .fetch_one(&mut *tx)
        .await?;

        insert_lines(&mut tx, lines).await?;
        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<SalesOrder>> {
        Ok(sqlx::query_as::<_, SalesOrder>("SELECT * FROM sales_orders WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_lines(&self, order_id: Uuid) -> AppResult<Vec<OrderLine>> {
        Ok(sqlx::query_as::<_, OrderLine>(
            "SELECT * FROM sales_order_lines WHERE order_id = $1 ORDER BY sort_order",
        )
        .bind(order_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn update(&self, order: &SalesOrder, lines: Option<&[OrderLine]>) -> AppResult<SalesOrder> {
        let mut tx = self.pool.begin().await?;

        let updated = sqlx::query_as::<_, SalesOrder>(
            r#"
            UPDATE sales_orders SET
                customer_id = $2, contact_id = $3, required_date = $4, shipping_address = $5,
                billing_address = $6, subtotal = $7, tax_amount = $8, total = $9,
                notes = $10, updated_at = $11, fx_rate = $12, base_total = $13
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(order.id)
        .bind(order.customer_id)
        .bind(order.contact_id)
        .bind(order.required_date)
        .bind(&order.shipping_address)
        .bind(&order.billing_address)
        .bind(order.subtotal)
        .bind(order.tax_amount)
        .bind(order.total)
        .bind(&order.notes)
        .bind(Utc::now())
        .bind(order.fx_rate)
        .bind(order.base_total)
        .fetch_one(&mut *tx)
        .await?;

        if let Some(lines) = lines {
            sqlx::query("DELETE FROM sales_order_lines WHERE order_id = $1")
                .bind(order.id)
                .execute(&mut *tx)
                .await?;
            insert_lines(&mut tx, lines).await?;
        }

        tx.commit().await?;
        Ok(updated)
    }

    async fn update_status(&self, id: Uuid, status: &str) -> AppResult<SalesOrder> {
        Ok(sqlx::query_as::<_, SalesOrder>(
            "UPDATE sales_orders SET status = $2, updated_at = $3 WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(status)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM sales_orders WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<SalesOrder>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM sales_orders WHERE 1 = 1");
        push_document_filters(&mut query, filters, "order_date");
        if let Some(search) = &filters.search {
            query.push(" AND order_number ILIKE ").push_bind(format!("%{}%", search));
        }
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());

        let rows = query.build_query_as::<SalesOrder>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM sales_orders WHERE 1 = 1");
        push_document_filters(&mut count, filters, "order_date");
        if let Some(search) = &filters.search {
            count.push(" AND order_number ILIKE ").push_bind(format!("%{}%", search));
        }
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('SO', 'sales_order_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    async fn invoiced_by_order_line(&self, order_id: Uuid) -> AppResult<Vec<(Uuid, i64)>> {
        Ok(sqlx::query_as::<_, (Uuid, i64)>(
            r#"
            SELECT il.order_line_id, SUM(il.quantity)::BIGINT
            FROM invoice_lines il
            JOIN invoices i ON i.id = il.invoice_id
            WHERE i.order_id = $1
              AND i.status <> 'cancelled'
              AND il.order_line_id IS NOT NULL
            GROUP BY il.order_line_id
            "#,
        )
        .bind(order_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn invoiced_by_invoices(&self, invoice_ids: &[Uuid]) -> AppResult<Vec<(Uuid, i64)>> {
        if invoice_ids.is_empty() {
            return Ok(Vec::new());
        }

        Ok(sqlx::query_as::<_, (Uuid, i64)>(
            r#"
            SELECT il.order_line_id, SUM(il.quantity)::BIGINT
            FROM invoice_lines il
            WHERE il.invoice_id = ANY($1)
              AND il.order_line_id IS NOT NULL
            GROUP BY il.order_line_id
            "#,
        )
        .bind(invoice_ids)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn find_invoices_for_order(&self, order_id: Uuid) -> AppResult<Vec<Invoice>> {
        // Ordered, unlike the `LIMIT 1` this replaced: once an order can carry a
        // cancelled invoice alongside a live one, which row comes back first
        // decides whether the order may ship. That must not be up to the
        // planner.
        Ok(sqlx::query_as::<_, Invoice>(
            "SELECT * FROM invoices WHERE order_id = $1 ORDER BY created_at",
        )
        .bind(order_id)
        .fetch_all(&self.pool)
        .await?)
    }
}
