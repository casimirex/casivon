use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::projects::domain::entities::Project;
use crate::modules::projects::domain::repositories::{ProjectFilters, ProjectRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 5] =
    ["created_at", "name", "start_date", "end_date", "progress_percent"];

#[derive(Clone)]
pub struct PgProjectRepository {
    pool: PgPool,
}

impl PgProjectRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a ProjectFilters) {
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(priority) = &filters.priority {
        builder.push(" AND priority = ").push_bind(priority);
    }
    if let Some(customer_id) = filters.customer_id {
        builder.push(" AND customer_id = ").push_bind(customer_id);
    }
    if let Some(manager_id) = filters.manager_id {
        builder.push(" AND manager_id = ").push_bind(manager_id);
    }
    if let Some(search) = &filters.search {
        let pattern = format!("%{}%", search);
        builder
            .push(" AND (name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR project_code ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl ProjectRepository for PgProjectRepository {
    async fn create(&self, project: &Project) -> AppResult<Project> {
        Ok(sqlx::query_as::<_, Project>(
            r#"
            INSERT INTO projects
                (id, org_id, project_code, name, description, customer_id, manager_id, status,
                 priority, start_date, end_date, budget, currency, progress_percent,
                 created_by, created_at, updated_at, fx_rate, base_budget)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            RETURNING *
            "#,
        )
        .bind(project.id)
        .bind(project.org_id)
        .bind(&project.project_code)
        .bind(&project.name)
        .bind(&project.description)
        .bind(project.customer_id)
        .bind(project.manager_id)
        .bind(&project.status)
        .bind(&project.priority)
        .bind(project.start_date)
        .bind(project.end_date)
        .bind(project.budget)
        .bind(&project.currency)
        .bind(project.progress_percent)
        .bind(project.created_by)
        .bind(project.created_at)
        .bind(project.updated_at)
        .bind(project.fx_rate)
        .bind(project.base_budget)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Project>> {
        Ok(sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_by_code(&self, code: &str) -> AppResult<Option<Project>> {
        Ok(sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE project_code = $1")
            .bind(code)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, project: &Project) -> AppResult<Project> {
        Ok(sqlx::query_as::<_, Project>(
            r#"
            UPDATE projects SET
                name = $2, description = $3, customer_id = $4, manager_id = $5, status = $6,
                priority = $7, start_date = $8, end_date = $9, budget = $10,
                progress_percent = $11, updated_at = $12, fx_rate = $13, base_budget = $14
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(project.id)
        .bind(&project.name)
        .bind(&project.description)
        .bind(project.customer_id)
        .bind(project.manager_id)
        .bind(&project.status)
        .bind(&project.priority)
        .bind(project.start_date)
        .bind(project.end_date)
        .bind(project.budget)
        .bind(project.progress_percent)
        .bind(Utc::now())
        .bind(project.fx_rate)
        .bind(project.base_budget)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn update_progress(&self, id: Uuid, progress: i32) -> AppResult<Project> {
        Ok(sqlx::query_as::<_, Project>(
            "UPDATE projects SET progress_percent = $2, updated_at = $3 WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(progress)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &ProjectFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Project>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM projects WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Project>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM projects WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_code(&self) -> AppResult<String> {
        Ok(
            sqlx::query_scalar::<_, String>("SELECT next_document_number('PRJ', 'project_code_seq')")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    async fn logged_hours(&self, id: Uuid) -> AppResult<(Decimal, Decimal)> {
        let row = sqlx::query(
            r#"
            SELECT
                COALESCE(SUM(te.hours) FILTER (WHERE te.is_billable), 0) AS billable,
                COALESCE(SUM(te.hours) FILTER (WHERE NOT te.is_billable), 0) AS non_billable
            FROM time_entries te
            JOIN tasks t ON t.id = te.task_id
            WHERE t.project_id = $1
            "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok((row.get::<Decimal, _>("billable"), row.get::<Decimal, _>("non_billable")))
    }
}
