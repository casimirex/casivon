use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::purchasing::domain::entities::VendorPayment;
use crate::modules::purchasing::domain::repositories::{
    VendorPaymentFilters, VendorPaymentRepository,
};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["created_at", "payment_date", "amount"];

#[derive(Clone)]
pub struct PgVendorPaymentRepository {
    pool: PgPool,
}

impl PgVendorPaymentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a VendorPaymentFilters) {
    if let Some(po_id) = filters.po_id {
        builder.push(" AND po_id = ").push_bind(po_id);
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
impl VendorPaymentRepository for PgVendorPaymentRepository {
    async fn create(&self, payment: &VendorPayment) -> AppResult<VendorPayment> {
        Ok(sqlx::query_as::<_, VendorPayment>(
            r#"
            INSERT INTO vendor_payments
                (id, org_id, po_id, amount, currency, payment_method,
                 payment_date, reference, notes, created_by, created_at,
                 fx_rate, base_amount, fx_gain_loss)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(payment.id)
        .bind(payment.org_id)
        .bind(payment.po_id)
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

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<VendorPayment>> {
        Ok(sqlx::query_as::<_, VendorPayment>("SELECT * FROM vendor_payments WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM vendor_payments WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &VendorPaymentFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<VendorPayment>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM vendor_payments WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "payment_date")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());

        let rows = query.build_query_as::<VendorPayment>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM vendor_payments WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn total_paid_for_order(&self, po_id: Uuid) -> AppResult<Decimal> {
        Ok(sqlx::query_scalar::<_, Decimal>(
            // The transaction amount, for the same reason the sales side uses
            // it: this total is compared against the order's own `total`, and
            // restating one side of that comparison would break it.
            "SELECT COALESCE(SUM(amount), 0) FROM vendor_payments WHERE po_id = $1",
        )
        .bind(po_id)
        .fetch_one(&self.pool)
        .await?)
    }
}
