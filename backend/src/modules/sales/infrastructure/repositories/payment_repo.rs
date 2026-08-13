use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::sales::domain::entities::Payment;
use crate::modules::sales::domain::repositories::{PaymentFilters, PaymentRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["created_at", "payment_date", "amount"];

#[derive(Clone)]
pub struct PgPaymentRepository {
    pool: PgPool,
}

impl PgPaymentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a PaymentFilters) {
    if let Some(invoice_id) = filters.invoice_id {
        builder.push(" AND invoice_id = ").push_bind(invoice_id);
    }
    if let Some(method) = &filters.payment_method {
        builder.push(" AND payment_method = ").push_bind(method);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND payment_date >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND payment_date <= ").push_bind(to);
    }
}

#[async_trait]
impl PaymentRepository for PgPaymentRepository {
    async fn create(&self, payment: &Payment) -> AppResult<Payment> {
        Ok(sqlx::query_as::<_, Payment>(
            r#"
            INSERT INTO payments
                (id, org_id, invoice_id, amount, currency, payment_method,
                 payment_date, reference, notes, created_by, created_at,
                 fx_rate, base_amount, fx_gain_loss)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(payment.id)
        .bind(payment.org_id)
        .bind(payment.invoice_id)
        .bind(payment.amount)
        .bind(&payment.currency)
        .bind(&payment.payment_method)
        .bind(payment.payment_date)
        .bind(&payment.reference)
        .bind(&payment.notes)
        .bind(payment.created_by)
        .bind(payment.created_at)
        .bind(payment.fx_rate)
        .bind(payment.base_amount)
        .bind(payment.fx_gain_loss)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Payment>> {
        Ok(sqlx::query_as::<_, Payment>("SELECT * FROM payments WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM payments WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &PaymentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Payment>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM payments WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "payment_date")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());

        let rows = query.build_query_as::<Payment>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM payments WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn total_paid_for_invoice(&self, invoice_id: Uuid) -> AppResult<Decimal> {
        Ok(sqlx::query_scalar::<_, Decimal>(
            // Deliberately the transaction amount, not the base one: every
            // payment against an invoice is in that invoice's currency, and this
            // total is compared against the invoice's own `total` to decide what
            // is still outstanding. Restating either side would break that
            // comparison rather than fix it.
            "SELECT COALESCE(SUM(amount), 0) FROM payments WHERE invoice_id = $1",
        )
        .bind(invoice_id)
        .fetch_one(&self.pool)
        .await?)
    }
}
