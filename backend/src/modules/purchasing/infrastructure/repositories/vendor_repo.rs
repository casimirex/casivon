use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::purchasing::domain::entities::Vendor;
use crate::modules::purchasing::domain::repositories::{VendorFilters, VendorRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["created_at", "updated_at", "name"];

#[derive(Clone)]
pub struct PgVendorRepository {
    pool: PgPool,
}

impl PgVendorRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a VendorFilters) {
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(country) = &filters.country {
        builder.push(" AND country = ").push_bind(country);
    }
    if let Some(search) = &filters.search {
        let pattern = format!("%{}%", search);
        builder
            .push(" AND (name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR email ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl VendorRepository for PgVendorRepository {
    async fn create(&self, vendor: &Vendor) -> AppResult<Vendor> {
        Ok(sqlx::query_as::<_, Vendor>(
            r#"
            INSERT INTO vendors
                (id, org_id, name, legal_name, tax_id, email, phone, address, city, country,
                 payment_terms, currency, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING *
            "#,
        )
        .bind(vendor.id)
        .bind(vendor.org_id)
        .bind(&vendor.name)
        .bind(&vendor.legal_name)
        .bind(&vendor.tax_id)
        .bind(&vendor.email)
        .bind(&vendor.phone)
        .bind(&vendor.address)
        .bind(&vendor.city)
        .bind(&vendor.country)
        .bind(&vendor.payment_terms)
        .bind(&vendor.currency)
        .bind(&vendor.status)
        .bind(vendor.created_at)
        .bind(vendor.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Vendor>> {
        Ok(sqlx::query_as::<_, Vendor>("SELECT * FROM vendors WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, vendor: &Vendor) -> AppResult<Vendor> {
        Ok(sqlx::query_as::<_, Vendor>(
            r#"
            UPDATE vendors SET
                name = $2, legal_name = $3, tax_id = $4, email = $5, phone = $6, address = $7,
                city = $8, country = $9, payment_terms = $10, currency = $11, status = $12,
                updated_at = $13
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(vendor.id)
        .bind(&vendor.name)
        .bind(&vendor.legal_name)
        .bind(&vendor.tax_id)
        .bind(&vendor.email)
        .bind(&vendor.phone)
        .bind(&vendor.address)
        .bind(&vendor.city)
        .bind(&vendor.country)
        .bind(&vendor.payment_terms)
        .bind(&vendor.currency)
        .bind(&vendor.status)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM vendors WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &VendorFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Vendor>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM vendors WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Vendor>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM vendors WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }
}
