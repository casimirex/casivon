use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::crm::domain::entities::Contact;
use crate::modules::crm::domain::repositories::{ContactFilters, ContactRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 4] = ["created_at", "updated_at", "first_name", "last_name"];

#[derive(Clone)]
pub struct PgContactRepository {
    pool: PgPool,
}

impl PgContactRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a ContactFilters) {
    if let Some(company_id) = filters.company_id {
        builder.push(" AND company_id = ").push_bind(company_id);
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
            .push(" AND (first_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR last_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR email ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl ContactRepository for PgContactRepository {
    async fn create(&self, contact: &Contact) -> AppResult<Contact> {
        Ok(sqlx::query_as::<_, Contact>(
            r#"
            INSERT INTO contacts
                (id, org_id, first_name, last_name, email, phone, mobile, address, city, country,
                 job_title, company_id, status, tags, notes, assigned_to, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            RETURNING *
            "#,
        )
        .bind(contact.id)
        .bind(contact.org_id)
        .bind(&contact.first_name)
        .bind(&contact.last_name)
        .bind(&contact.email)
        .bind(&contact.phone)
        .bind(&contact.mobile)
        .bind(&contact.address)
        .bind(&contact.city)
        .bind(&contact.country)
        .bind(&contact.job_title)
        .bind(contact.company_id)
        .bind(&contact.status)
        .bind(&contact.tags)
        .bind(&contact.notes)
        .bind(contact.assigned_to)
        .bind(contact.created_at)
        .bind(contact.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Contact>> {
        Ok(sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, contact: &Contact) -> AppResult<Contact> {
        Ok(sqlx::query_as::<_, Contact>(
            r#"
            UPDATE contacts SET
                first_name = $2, last_name = $3, email = $4, phone = $5, mobile = $6,
                address = $7, city = $8, country = $9, job_title = $10, company_id = $11,
                status = $12, tags = $13, notes = $14, assigned_to = $15, updated_at = $16
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(contact.id)
        .bind(&contact.first_name)
        .bind(&contact.last_name)
        .bind(&contact.email)
        .bind(&contact.phone)
        .bind(&contact.mobile)
        .bind(&contact.address)
        .bind(&contact.city)
        .bind(&contact.country)
        .bind(&contact.job_title)
        .bind(contact.company_id)
        .bind(&contact.status)
        .bind(&contact.tags)
        .bind(&contact.notes)
        .bind(contact.assigned_to)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM contacts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &ContactFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Contact>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM contacts WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Contact>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM contacts WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }
}
