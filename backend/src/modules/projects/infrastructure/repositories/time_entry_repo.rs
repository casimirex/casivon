use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::projects::domain::entities::TimeEntry;
use crate::modules::projects::domain::repositories::{TimeEntryFilters, TimeEntryRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["entry_date", "created_at", "hours"];

#[derive(Clone)]
pub struct PgTimeEntryRepository {
    pool: PgPool,
}

impl PgTimeEntryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a TimeEntryFilters) {
    if let Some(task_id) = filters.task_id {
        builder.push(" AND task_id = ").push_bind(task_id);
    }
    if let Some(project_id) = filters.project_id {
        // time_entries only knows its task, so reach the project through it.
        builder
            .push(" AND task_id IN (SELECT id FROM tasks WHERE project_id = ")
            .push_bind(project_id)
            .push(")");
    }
    if let Some(employee_id) = filters.employee_id {
        builder.push(" AND employee_id = ").push_bind(employee_id);
    }
    if let Some(is_billable) = filters.is_billable {
        builder.push(" AND is_billable = ").push_bind(is_billable);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND entry_date >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND entry_date <= ").push_bind(to);
    }
}

#[async_trait]
impl TimeEntryRepository for PgTimeEntryRepository {
    async fn create(&self, entry: &TimeEntry) -> AppResult<TimeEntry> {
        Ok(sqlx::query_as::<_, TimeEntry>(
            r#"
            INSERT INTO time_entries
                (id, org_id, task_id, employee_id, entry_date, hours, description,
                 is_billable, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(entry.id)
        .bind(entry.org_id)
        .bind(entry.task_id)
        .bind(entry.employee_id)
        .bind(entry.entry_date)
        .bind(entry.hours)
        .bind(&entry.description)
        .bind(entry.is_billable)
        .bind(entry.created_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<TimeEntry>> {
        Ok(sqlx::query_as::<_, TimeEntry>("SELECT * FROM time_entries WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, entry: &TimeEntry) -> AppResult<TimeEntry> {
        Ok(sqlx::query_as::<_, TimeEntry>(
            r#"
            UPDATE time_entries
            SET entry_date = $2, hours = $3, description = $4, is_billable = $5
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(entry.id)
        .bind(entry.entry_date)
        .bind(entry.hours)
        .bind(&entry.description)
        .bind(entry.is_billable)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM time_entries WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &TimeEntryFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<TimeEntry>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM time_entries WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "entry_date")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<TimeEntry>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM time_entries WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }
}
