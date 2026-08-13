use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::crm::domain::entities::Company;
use crate::modules::crm::domain::repositories::{CompanyFilters, CompanyRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["created_at", "updated_at", "name"];

#[derive(Clone)]
pub struct PgCompanyRepository {
    pool: PgPool,
}

impl PgCompanyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a CompanyFilters) {
    if let Some(company_type) = &filters.company_type {
        builder.push(" AND company_type = ").push_bind(company_type);
    }
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(assigned_to) = filters.assigned_to {
        builder.push(" AND assigned_to = ").push_bind(assigned_to);
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
impl CompanyRepository for PgCompanyRepository {
    async fn create(&self, company: &Company) -> AppResult<Company> {
        Ok(sqlx::query_as::<_, Company>(
            r#"
            INSERT INTO companies
                (id, org_id, name, legal_name, tax_id, email, phone, website, address, city,
                 country, industry, company_type, status, assigned_to, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            RETURNING *
            "#,
        )
        .bind(company.id)
        .bind(company.org_id)
        .bind(&company.name)
        .bind(&company.legal_name)
        .bind(&company.tax_id)
        .bind(&company.email)
        .bind(&company.phone)
        .bind(&company.website)
        .bind(&company.address)
        .bind(&company.city)
        .bind(&company.country)
        .bind(&company.industry)
        .bind(&company.company_type)
        .bind(&company.status)
        .bind(company.assigned_to)
        .bind(company.created_at)
        .bind(company.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Company>> {
        Ok(sqlx::query_as::<_, Company>("SELECT * FROM companies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, company: &Company) -> AppResult<Company> {
        Ok(sqlx::query_as::<_, Company>(
            r#"
            UPDATE companies SET
                name = $2, legal_name = $3, tax_id = $4, email = $5, phone = $6, website = $7,
                address = $8, city = $9, country = $10, industry = $11, company_type = $12,
                status = $13, assigned_to = $14, updated_at = $15
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(company.id)
        .bind(&company.name)
        .bind(&company.legal_name)
        .bind(&company.tax_id)
        .bind(&company.email)
        .bind(&company.phone)
        .bind(&company.website)
        .bind(&company.address)
        .bind(&company.city)
        .bind(&company.country)
        .bind(&company.industry)
        .bind(&company.company_type)
        .bind(&company.status)
        .bind(company.assigned_to)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM companies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &CompanyFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Company>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM companies WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Company>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM companies WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }
}
