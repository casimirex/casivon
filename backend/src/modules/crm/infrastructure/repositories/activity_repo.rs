use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::crm::domain::entities::Activity;
use crate::modules::crm::domain::repositories::{ActivityFilters, ActivityRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["created_at", "scheduled_at", "completed_at"];

#[derive(Clone)]
pub struct PgActivityRepository {
    pool: PgPool,
}

impl PgActivityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a ActivityFilters) {
    if let Some(related_type) = &filters.related_to_type {
        builder.push(" AND related_to_type = ").push_bind(related_type);
    }
    if let Some(related_id) = filters.related_to_id {
        builder.push(" AND related_to_id = ").push_bind(related_id);
    }
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(assigned_to) = filters.assigned_to {
        builder.push(" AND assigned_to = ").push_bind(assigned_to);
    }
}

#[async_trait]
impl ActivityRepository for PgActivityRepository {
    async fn create(&self, activity: &Activity) -> AppResult<Activity> {
        Ok(sqlx::query_as::<_, Activity>(
            r#"
            INSERT INTO activities
                (id, org_id, activity_type, subject, description, related_to_type, related_to_id,
                 scheduled_at, completed_at, status, assigned_to, created_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(activity.id)
        .bind(activity.org_id)
        .bind(&activity.activity_type)
        .bind(&activity.subject)
        .bind(&activity.description)
        .bind(&activity.related_to_type)
        .bind(activity.related_to_id)
        .bind(activity.scheduled_at)
        .bind(activity.completed_at)
        .bind(&activity.status)
        .bind(activity.assigned_to)
        .bind(activity.created_by)
        .bind(activity.created_at)
        .bind(activity.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Activity>> {
        Ok(sqlx::query_as::<_, Activity>("SELECT * FROM activities WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, activity: &Activity) -> AppResult<Activity> {
        Ok(sqlx::query_as::<_, Activity>(
            r#"
            UPDATE activities SET
                subject = $2, description = $3, scheduled_at = $4, completed_at = $5,
                status = $6, assigned_to = $7, updated_at = $8
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(activity.id)
        .bind(&activity.subject)
        .bind(&activity.description)
        .bind(activity.scheduled_at)
        .bind(activity.completed_at)
        .bind(&activity.status)
        .bind(activity.assigned_to)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM activities WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &ActivityFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Activity>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM activities WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Activity>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM activities WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }
}
