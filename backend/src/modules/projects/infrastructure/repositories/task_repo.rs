use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::projects::domain::entities::Task;
use crate::modules::projects::domain::repositories::{TaskFilters, TaskRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 5] =
    ["created_at", "due_date", "start_date", "priority", "progress_percent"];

#[derive(Clone)]
pub struct PgTaskRepository {
    pool: PgPool,
}

impl PgTaskRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a TaskFilters) {
    if let Some(project_id) = filters.project_id {
        builder.push(" AND project_id = ").push_bind(project_id);
    }
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(priority) = &filters.priority {
        builder.push(" AND priority = ").push_bind(priority);
    }
    if let Some(assigned_to) = filters.assigned_to {
        builder.push(" AND assigned_to = ").push_bind(assigned_to);
    }
    if let Some(parent_task_id) = filters.parent_task_id {
        builder.push(" AND parent_task_id = ").push_bind(parent_task_id);
    }
    if let Some(search) = &filters.search {
        let pattern = format!("%{}%", search);
        builder
            .push(" AND (title ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR task_code ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl TaskRepository for PgTaskRepository {
    async fn create(&self, task: &Task) -> AppResult<Task> {
        Ok(sqlx::query_as::<_, Task>(
            r#"
            INSERT INTO tasks
                (id, org_id, project_id, parent_task_id, task_code, title, description,
                 assigned_to, status, priority, start_date, due_date, estimated_hours,
                 actual_hours, progress_percent, created_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            RETURNING *
            "#,
        )
        .bind(task.id)
        .bind(task.org_id)
        .bind(task.project_id)
        .bind(task.parent_task_id)
        .bind(&task.task_code)
        .bind(&task.title)
        .bind(&task.description)
        .bind(task.assigned_to)
        .bind(&task.status)
        .bind(&task.priority)
        .bind(task.start_date)
        .bind(task.due_date)
        .bind(task.estimated_hours)
        .bind(task.actual_hours)
        .bind(task.progress_percent)
        .bind(task.created_by)
        .bind(task.created_at)
        .bind(task.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Task>> {
        Ok(sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Task>> {
        Ok(
            sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE project_id = $1 AND task_code = $2")
                .bind(project_id)
                .bind(code)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn update(&self, task: &Task) -> AppResult<Task> {
        Ok(sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks SET
                parent_task_id = $2, title = $3, description = $4, assigned_to = $5,
                status = $6, priority = $7, start_date = $8, due_date = $9,
                estimated_hours = $10, progress_percent = $11, updated_at = $12
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(task.id)
        .bind(task.parent_task_id)
        .bind(&task.title)
        .bind(&task.description)
        .bind(task.assigned_to)
        .bind(&task.status)
        .bind(&task.priority)
        .bind(task.start_date)
        .bind(task.due_date)
        .bind(task.estimated_hours)
        .bind(task.progress_percent)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM tasks WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &TaskFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Task>, i64)> {
        let mut query: QueryBuilder<Postgres> = QueryBuilder::new("SELECT * FROM tasks WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Task>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM tasks WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn count_subtasks(&self, id: Uuid) -> AppResult<i64> {
        Ok(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tasks WHERE parent_task_id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    async fn progress_snapshot(&self, project_id: Uuid) -> AppResult<Vec<(String, i32)>> {
        let rows = sqlx::query("SELECT status, progress_percent FROM tasks WHERE project_id = $1")
            .bind(project_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get::<String, _>("status"), row.get::<i32, _>("progress_percent")))
            .collect())
    }

    async fn next_code(&self, project_id: Uuid) -> AppResult<String> {
        // Task codes are per-project (TASK-1, TASK-2, ...), so they come from a
        // count rather than a global sequence.
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tasks WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(format!("TASK-{}", count + 1))
    }

    async fn sync_actual_hours(&self, task_id: Uuid) -> AppResult<Task> {
        Ok(sqlx::query_as::<_, Task>(
            r#"
            UPDATE tasks
            SET actual_hours = (
                    SELECT COALESCE(SUM(hours), 0) FROM time_entries WHERE task_id = $1
                ),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await?)
    }
}
