use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::crm::domain::entities::Opportunity;
use crate::modules::crm::domain::repositories::{OpportunityFilters, OpportunityRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 4] = ["created_at", "updated_at", "value", "expected_close_date"];

#[derive(Clone)]
pub struct PgOpportunityRepository {
    pool: PgPool,
}

impl PgOpportunityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a OpportunityFilters) {
    if let Some(stage) = &filters.stage {
        builder.push(" AND stage = ").push_bind(stage);
    }
    if let Some(company_id) = filters.company_id {
        builder.push(" AND company_id = ").push_bind(company_id);
    }
    if let Some(assigned_to) = filters.assigned_to {
        builder.push(" AND assigned_to = ").push_bind(assigned_to);
    }
    if let Some(search) = &filters.search {
        builder.push(" AND title ILIKE ").push_bind(format!("%{}%", search));
    }
}

#[async_trait]
impl OpportunityRepository for PgOpportunityRepository {
    async fn create(&self, opp: &Opportunity) -> AppResult<Opportunity> {
        Ok(sqlx::query_as::<_, Opportunity>(
            r#"
            INSERT INTO opportunities
                (id, org_id, title, company_id, contact_id, stage, value, currency, probability,
                 expected_close_date, description, assigned_to, source, created_at, updated_at,
                 fx_rate, base_value)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            RETURNING *
            "#,
        )
        .bind(opp.id)
        .bind(opp.org_id)
        .bind(&opp.title)
        .bind(opp.company_id)
        .bind(opp.contact_id)
        .bind(&opp.stage)
        .bind(opp.value)
        .bind(&opp.currency)
        .bind(opp.probability)
        .bind(opp.expected_close_date)
        .bind(&opp.description)
        .bind(opp.assigned_to)
        .bind(&opp.source)
        .bind(opp.created_at)
        .bind(opp.updated_at)
        .bind(opp.fx_rate)
        .bind(opp.base_value)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Opportunity>> {
        Ok(sqlx::query_as::<_, Opportunity>("SELECT * FROM opportunities WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, opp: &Opportunity) -> AppResult<Opportunity> {
        Ok(sqlx::query_as::<_, Opportunity>(
            r#"
            UPDATE opportunities SET
                title = $2, company_id = $3, contact_id = $4, stage = $5, value = $6,
                currency = $7, probability = $8, expected_close_date = $9, description = $10,
                assigned_to = $11, source = $12, updated_at = $13,
                fx_rate = $14, base_value = $15
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(opp.id)
        .bind(&opp.title)
        .bind(opp.company_id)
        .bind(opp.contact_id)
        .bind(&opp.stage)
        .bind(opp.value)
        .bind(&opp.currency)
        .bind(opp.probability)
        .bind(opp.expected_close_date)
        .bind(&opp.description)
        .bind(opp.assigned_to)
        .bind(&opp.source)
        .bind(Utc::now())
        .bind(opp.fx_rate)
        .bind(opp.base_value)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM opportunities WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &OpportunityFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Opportunity>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM opportunities WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Opportunity>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM opportunities WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn pipeline_by_stage(&self) -> AppResult<Vec<(String, i64, Decimal)>> {
        let rows = sqlx::query(
            r#"
            -- Base currency, not `value`: a pipeline that added EUR and USD
            -- deals together would produce a number that is not money.
            SELECT stage, COUNT(*) AS count, COALESCE(SUM(base_value), 0) AS value
            FROM opportunities
            WHERE stage NOT IN ('closed_won', 'closed_lost')
            GROUP BY stage
            ORDER BY stage
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("stage"),
                    row.get::<i64, _>("count"),
                    row.get::<Decimal, _>("value"),
                )
            })
            .collect())
    }
}
