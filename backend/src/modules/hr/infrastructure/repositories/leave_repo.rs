use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::hr::domain::entities::{LeaveRequest, LeaveStatus, LeaveType};
use crate::modules::hr::domain::repositories::{LeaveFilters, LeaveRequestRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["created_at", "start_date", "end_date"];

#[derive(Clone)]
pub struct PgLeaveRequestRepository {
    pool: PgPool,
}

impl PgLeaveRequestRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a LeaveFilters) {
    if let Some(employee_id) = filters.employee_id {
        builder.push(" AND employee_id = ").push_bind(employee_id);
    }
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(leave_type) = &filters.leave_type {
        builder.push(" AND leave_type = ").push_bind(leave_type);
    }
    // Any request that touches the window, not only ones fully inside it.
    if let Some(from) = filters.date_from {
        builder.push(" AND end_date >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND start_date <= ").push_bind(to);
    }
}

#[async_trait]
impl LeaveRequestRepository for PgLeaveRequestRepository {
    async fn create(&self, request: &LeaveRequest) -> AppResult<LeaveRequest> {
        Ok(sqlx::query_as::<_, LeaveRequest>(
            r#"
            INSERT INTO leave_requests
                (id, org_id, employee_id, leave_type, start_date, end_date, days_requested,
                 reason, status, approved_by, approved_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING *
            "#,
        )
        .bind(request.id)
        .bind(request.org_id)
        .bind(request.employee_id)
        .bind(&request.leave_type)
        .bind(request.start_date)
        .bind(request.end_date)
        .bind(request.days_requested)
        .bind(&request.reason)
        .bind(&request.status)
        .bind(request.approved_by)
        .bind(request.approved_at)
        .bind(request.created_at)
        .bind(request.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<LeaveRequest>> {
        Ok(sqlx::query_as::<_, LeaveRequest>("SELECT * FROM leave_requests WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, request: &LeaveRequest) -> AppResult<LeaveRequest> {
        Ok(sqlx::query_as::<_, LeaveRequest>(
            r#"
            UPDATE leave_requests SET
                leave_type = $2, start_date = $3, end_date = $4, days_requested = $5,
                reason = $6, status = $7, approved_by = $8, approved_at = $9, updated_at = $10
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(request.id)
        .bind(&request.leave_type)
        .bind(request.start_date)
        .bind(request.end_date)
        .bind(request.days_requested)
        .bind(&request.reason)
        .bind(&request.status)
        .bind(request.approved_by)
        .bind(request.approved_at)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM leave_requests WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &LeaveFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<LeaveRequest>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM leave_requests WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "start_date")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<LeaveRequest>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM leave_requests WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn approved_days_in_year(&self, employee_id: Uuid, year: i32) -> AppResult<i32> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(SUM(days_requested), 0)
            FROM leave_requests
            WHERE employee_id = $1
              AND status = $2
              AND leave_type = $3
              AND EXTRACT(YEAR FROM start_date) = $4
            "#,
        )
        .bind(employee_id)
        .bind(LeaveStatus::APPROVED)
        .bind(LeaveType::ANNUAL)
        .bind(f64::from(year))
        .fetch_one(&self.pool)
        .await? as i32)
    }

    async fn find_overlapping(
        &self,
        employee_id: Uuid,
        start: NaiveDate,
        end: NaiveDate,
        exclude_id: Option<Uuid>,
    ) -> AppResult<Vec<LeaveRequest>> {
        // Two ranges overlap when each starts before the other ends. Rejected
        // requests are ignored — those days were never actually booked.
        Ok(sqlx::query_as::<_, LeaveRequest>(
            r#"
            SELECT * FROM leave_requests
            WHERE employee_id = $1
              AND status <> $2
              AND start_date <= $4
              AND end_date >= $3
              AND ($5::uuid IS NULL OR id <> $5)
            "#,
        )
        .bind(employee_id)
        .bind(LeaveStatus::REJECTED)
        .bind(start)
        .bind(end)
        .bind(exclude_id)
        .fetch_all(&self.pool)
        .await?)
    }
}
