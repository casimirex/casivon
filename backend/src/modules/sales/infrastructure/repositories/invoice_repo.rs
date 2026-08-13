use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::sales::domain::entities::{Invoice, InvoiceLine, InvoiceStatus};
use crate::modules::sales::domain::repositories::{InvoiceRepository, SalesDocumentFilters};
use crate::modules::sales::infrastructure::repositories::quote_repo::push_document_filters;
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 6] =
    ["created_at", "issue_date", "due_date", "total", "amount_due", "invoice_number"];

#[derive(Clone)]
pub struct PgInvoiceRepository {
    pool: PgPool,
}

impl PgInvoiceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

async fn insert_lines(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    lines: &[InvoiceLine],
) -> AppResult<()> {
    for line in lines {
        sqlx::query(
            r#"
            INSERT INTO invoice_lines
                (id, invoice_id, order_line_id, product_id, description, quantity, unit_price,
                 discount_percent, tax_rate, line_total, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(line.id)
        .bind(line.invoice_id)
        .bind(line.order_line_id)
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
impl InvoiceRepository for PgInvoiceRepository {
    async fn create(&self, invoice: &Invoice, lines: &[InvoiceLine]) -> AppResult<Invoice> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, Invoice>(
            r#"
            INSERT INTO invoices
                (id, org_id, invoice_number, customer_id, order_id, status, issue_date, due_date,
                 subtotal, tax_amount, total, amount_paid, amount_due, currency, notes,
                 created_by, created_at, updated_at,
                 fx_rate, base_total, base_amount_paid, base_amount_due)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
                    $19, $20, $21, $22)
            RETURNING *
            "#,
        )
        .bind(invoice.id)
        .bind(invoice.org_id)
        .bind(&invoice.invoice_number)
        .bind(invoice.customer_id)
        .bind(invoice.order_id)
        .bind(&invoice.status)
        .bind(invoice.issue_date)
        .bind(invoice.due_date)
        .bind(invoice.subtotal)
        .bind(invoice.tax_amount)
        .bind(invoice.total)
        .bind(invoice.amount_paid)
        .bind(invoice.amount_due)
        .bind(&invoice.currency)
        .bind(&invoice.notes)
        .bind(invoice.created_by)
        .bind(invoice.created_at)
        .bind(invoice.updated_at)
        .bind(invoice.fx_rate)
        .bind(invoice.base_total)
        .bind(invoice.base_amount_paid)
        .bind(invoice.base_amount_due)
        .fetch_one(&mut *tx)
        .await?;

        insert_lines(&mut tx, lines).await?;
        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Invoice>> {
        Ok(sqlx::query_as::<_, Invoice>("SELECT * FROM invoices WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_lines(&self, invoice_id: Uuid) -> AppResult<Vec<InvoiceLine>> {
        Ok(sqlx::query_as::<_, InvoiceLine>(
            "SELECT * FROM invoice_lines WHERE invoice_id = $1 ORDER BY sort_order",
        )
        .bind(invoice_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn update(&self, invoice: &Invoice, lines: Option<&[InvoiceLine]>) -> AppResult<Invoice> {
        let mut tx = self.pool.begin().await?;

        let updated = sqlx::query_as::<_, Invoice>(
            r#"
            UPDATE invoices SET
                customer_id = $2, issue_date = $3, due_date = $4, subtotal = $5,
                tax_amount = $6, total = $7, amount_due = $8, notes = $9, updated_at = $10,
                fx_rate = $11, base_total = $12, base_amount_due = $13
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(invoice.id)
        .bind(invoice.customer_id)
        .bind(invoice.issue_date)
        .bind(invoice.due_date)
        .bind(invoice.subtotal)
        .bind(invoice.tax_amount)
        .bind(invoice.total)
        .bind(invoice.amount_due)
        .bind(&invoice.notes)
        .bind(Utc::now())
        .bind(invoice.fx_rate)
        .bind(invoice.base_total)
        .bind(invoice.base_amount_due)
        .fetch_one(&mut *tx)
        .await?;

        if let Some(lines) = lines {
            sqlx::query("DELETE FROM invoice_lines WHERE invoice_id = $1")
                .bind(invoice.id)
                .execute(&mut *tx)
                .await?;
            insert_lines(&mut tx, lines).await?;
        }

        tx.commit().await?;
        Ok(updated)
    }

    async fn update_status(&self, id: Uuid, status: &str) -> AppResult<Invoice> {
        Ok(sqlx::query_as::<_, Invoice>(
            "UPDATE invoices SET status = $2, updated_at = $3 WHERE id = $1 RETURNING *",
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
        amount_paid: Decimal,
        amount_due: Decimal,
        base_amount_paid: Decimal,
        base_amount_due: Decimal,
        status: &str,
    ) -> AppResult<Invoice> {
        Ok(sqlx::query_as::<_, Invoice>(
            r#"
            UPDATE invoices
            SET amount_paid = $2, amount_due = $3, status = $4, updated_at = $5,
                base_amount_paid = $6, base_amount_due = $7
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(amount_paid)
        .bind(amount_due)
        .bind(status)
        .bind(Utc::now())
        .bind(base_amount_paid)
        .bind(base_amount_due)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM invoices WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Invoice>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM invoices WHERE 1 = 1");
        push_document_filters(&mut query, filters, "issue_date");
        if let Some(search) = &filters.search {
            query.push(" AND invoice_number ILIKE ").push_bind(format!("%{}%", search));
        }
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());

        let rows = query.build_query_as::<Invoice>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM invoices WHERE 1 = 1");
        push_document_filters(&mut count, filters, "issue_date");
        if let Some(search) = &filters.search {
            count.push(" AND invoice_number ILIKE ").push_bind(format!("%{}%", search));
        }
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('INV', 'invoice_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    async fn mark_overdue(&self, today: NaiveDate) -> AppResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE invoices
            SET status = $1, updated_at = NOW()
            WHERE status = $2 AND due_date < $3
            "#,
        )
        .bind(InvoiceStatus::OVERDUE)
        .bind(InvoiceStatus::SENT)
        .bind(today)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
