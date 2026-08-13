use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::sales::domain::entities::{CreditNote, CreditNoteLine};
use crate::modules::sales::domain::repositories::{CreditNoteFilters, CreditNoteRepository};
use crate::shared::pagination::PaginationParams;

#[derive(Clone)]
pub struct PgCreditNoteRepository {
    pool: PgPool,
}

impl PgCreditNoteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a CreditNoteFilters) {
    if let Some(invoice_id) = filters.invoice_id {
        builder.push(" AND invoice_id = ").push_bind(invoice_id);
    }
    if let Some(customer_id) = filters.customer_id {
        builder.push(" AND customer_id = ").push_bind(customer_id);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND issue_date >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND issue_date <= ").push_bind(to);
    }
}

#[async_trait]
impl CreditNoteRepository for PgCreditNoteRepository {
    async fn create(&self, note: &CreditNote, lines: &[CreditNoteLine]) -> AppResult<CreditNote> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, CreditNote>(
            r#"
            INSERT INTO credit_notes
                (id, org_id, credit_note_number, invoice_id, customer_id, issue_date, reason,
                 warehouse_id, subtotal, tax_amount, total, currency, fx_rate, base_total,
                 notes, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            RETURNING *
            "#,
        )
        .bind(note.id)
        .bind(note.org_id)
        .bind(&note.credit_note_number)
        .bind(note.invoice_id)
        .bind(note.customer_id)
        .bind(note.issue_date)
        .bind(&note.reason)
        .bind(note.warehouse_id)
        .bind(note.subtotal)
        .bind(note.tax_amount)
        .bind(note.total)
        .bind(&note.currency)
        .bind(note.fx_rate)
        .bind(note.base_total)
        .bind(&note.notes)
        .bind(note.created_by)
        .bind(note.created_at)
        .fetch_one(&mut *tx)
        .await?;

        for line in lines {
            sqlx::query(
                r#"
                INSERT INTO credit_note_lines
                    (id, credit_note_id, invoice_line_id, product_id, description, quantity,
                     unit_price, discount_percent, tax_rate, line_total, sort_order)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
            )
            .bind(line.id)
            .bind(line.credit_note_id)
            .bind(line.invoice_line_id)
            .bind(line.product_id)
            .bind(&line.description)
            .bind(line.quantity)
            .bind(line.unit_price)
            .bind(line.discount_percent)
            .bind(line.tax_rate)
            .bind(line.line_total)
            .bind(line.sort_order)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<CreditNote>> {
        Ok(sqlx::query_as::<_, CreditNote>("SELECT * FROM credit_notes WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_lines(&self, credit_note_id: Uuid) -> AppResult<Vec<CreditNoteLine>> {
        Ok(sqlx::query_as::<_, CreditNoteLine>(
            "SELECT * FROM credit_note_lines WHERE credit_note_id = $1 ORDER BY sort_order",
        )
        .bind(credit_note_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn list(
        &self,
        filters: &CreditNoteFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<CreditNote>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM credit_notes WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&["created_at", "issue_date", "total"], "issue_date")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<CreditNote>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM credit_notes WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('CN', 'credit_note_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    async fn total_credited_for_invoice(&self, invoice_id: Uuid) -> AppResult<Decimal> {
        // The stored totals, not a recomputation from the lines: the credit note
        // is the document, and what it says it credited is what it credited.
        Ok(sqlx::query_scalar::<_, Decimal>(
            "SELECT COALESCE(SUM(total), 0) FROM credit_notes WHERE invoice_id = $1",
        )
        .bind(invoice_id)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn credited_by_invoice_line(&self, invoice_id: Uuid) -> AppResult<Vec<(Uuid, i64)>> {
        Ok(sqlx::query_as::<_, (Uuid, i64)>(
            r#"
            SELECT cnl.invoice_line_id, SUM(cnl.quantity)::BIGINT
            FROM credit_note_lines cnl
            JOIN credit_notes cn ON cn.id = cnl.credit_note_id
            WHERE cn.invoice_id = $1
            GROUP BY cnl.invoice_line_id
            "#,
        )
        .bind(invoice_id)
        .fetch_all(&self.pool)
        .await?)
    }
}
