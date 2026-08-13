use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, QueryBuilder, Postgres};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::sales::domain::entities::{Quote, QuoteLine, SalesOrder};
use crate::modules::sales::domain::repositories::{QuoteRepository, SalesDocumentFilters};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 5] = ["created_at", "issue_date", "expiry_date", "total", "quote_number"];

#[derive(Clone)]
pub struct PgQuoteRepository {
    pool: PgPool,
}

impl PgQuoteRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Appends the shared document filters to a query being built.
pub(super) fn push_document_filters<'a>(
    builder: &mut QueryBuilder<'a, Postgres>,
    filters: &'a SalesDocumentFilters,
    date_column: &str,
) {
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(customer_id) = filters.customer_id {
        builder.push(" AND customer_id = ").push_bind(customer_id);
    }
    if let Some(from) = filters.date_from {
        builder.push(format!(" AND {} >= ", date_column)).push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(format!(" AND {} <= ", date_column)).push_bind(to);
    }
}

async fn insert_lines(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    lines: &[QuoteLine],
) -> AppResult<()> {
    for line in lines {
        sqlx::query(
            r#"
            INSERT INTO quote_lines
                (id, quote_id, product_id, description, quantity, unit_price,
                 discount_percent, tax_rate, line_total, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(line.id)
        .bind(line.quote_id)
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
impl QuoteRepository for PgQuoteRepository {
    async fn create(&self, quote: &Quote, lines: &[QuoteLine]) -> AppResult<Quote> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, Quote>(
            r#"
            INSERT INTO quotes
                (id, org_id, quote_number, customer_id, contact_id, status, issue_date,
                 expiry_date, subtotal, tax_amount, total, currency, notes, terms,
                 created_by, created_at, updated_at, fx_rate, base_total)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                    $18, $19)
            RETURNING *
            "#,
        )
        .bind(quote.id)
        .bind(quote.org_id)
        .bind(&quote.quote_number)
        .bind(quote.customer_id)
        .bind(quote.contact_id)
        .bind(&quote.status)
        .bind(quote.issue_date)
        .bind(quote.expiry_date)
        .bind(quote.subtotal)
        .bind(quote.tax_amount)
        .bind(quote.total)
        .bind(&quote.currency)
        .bind(&quote.notes)
        .bind(&quote.terms)
        .bind(quote.created_by)
        .bind(quote.created_at)
        .bind(quote.updated_at)
        .bind(quote.fx_rate)
        .bind(quote.base_total)
        .fetch_one(&mut *tx)
        .await?;

        insert_lines(&mut tx, lines).await?;
        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Quote>> {
        Ok(sqlx::query_as::<_, Quote>("SELECT * FROM quotes WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_lines(&self, quote_id: Uuid) -> AppResult<Vec<QuoteLine>> {
        Ok(sqlx::query_as::<_, QuoteLine>(
            "SELECT * FROM quote_lines WHERE quote_id = $1 ORDER BY sort_order",
        )
        .bind(quote_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn update(&self, quote: &Quote, lines: Option<&[QuoteLine]>) -> AppResult<Quote> {
        let mut tx = self.pool.begin().await?;

        let updated = sqlx::query_as::<_, Quote>(
            r#"
            UPDATE quotes SET
                customer_id = $2, contact_id = $3, issue_date = $4, expiry_date = $5,
                subtotal = $6, tax_amount = $7, total = $8, currency = $9,
                notes = $10, terms = $11, updated_at = $12,
                fx_rate = $13, base_total = $14
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(quote.id)
        .bind(quote.customer_id)
        .bind(quote.contact_id)
        .bind(quote.issue_date)
        .bind(quote.expiry_date)
        .bind(quote.subtotal)
        .bind(quote.tax_amount)
        .bind(quote.total)
        .bind(&quote.currency)
        .bind(&quote.notes)
        .bind(&quote.terms)
        .bind(Utc::now())
        .bind(quote.fx_rate)
        .bind(quote.base_total)
        .fetch_one(&mut *tx)
        .await?;

        // Lines are replaced wholesale: the client always sends the full set.
        if let Some(lines) = lines {
            sqlx::query("DELETE FROM quote_lines WHERE quote_id = $1")
                .bind(quote.id)
                .execute(&mut *tx)
                .await?;
            insert_lines(&mut tx, lines).await?;
        }

        tx.commit().await?;
        Ok(updated)
    }

    async fn update_status(&self, id: Uuid, status: &str) -> AppResult<Quote> {
        Ok(sqlx::query_as::<_, Quote>(
            "UPDATE quotes SET status = $2, updated_at = $3 WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(status)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        // quote_lines cascade on the foreign key.
        sqlx::query("DELETE FROM quotes WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &SalesDocumentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Quote>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM quotes WHERE 1 = 1");
        push_document_filters(&mut query, filters, "issue_date");
        if let Some(search) = &filters.search {
            query.push(" AND quote_number ILIKE ").push_bind(format!("%{}%", search));
        }
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());

        let rows = query.build_query_as::<Quote>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM quotes WHERE 1 = 1");
        push_document_filters(&mut count, filters, "issue_date");
        if let Some(search) = &filters.search {
            count.push(" AND quote_number ILIKE ").push_bind(format!("%{}%", search));
        }
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('QUO', 'quote_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_converted_order(&self, quote_id: Uuid) -> AppResult<Option<SalesOrder>> {
        Ok(
            sqlx::query_as::<_, SalesOrder>("SELECT * FROM sales_orders WHERE quote_id = $1 LIMIT 1")
                .bind(quote_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
}
