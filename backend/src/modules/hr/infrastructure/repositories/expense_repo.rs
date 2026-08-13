use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::hr::domain::entities::{ExpenseLine, ExpenseReport};
use crate::modules::hr::domain::repositories::{ExpenseFilters, ExpenseReportRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["created_at", "submitted_at", "total_amount"];

#[derive(Clone)]
pub struct PgExpenseReportRepository {
    pool: PgPool,
}

impl PgExpenseReportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

async fn insert_lines(tx: &mut Transaction<'_, Postgres>, lines: &[ExpenseLine]) -> AppResult<()> {
    for line in lines {
        sqlx::query(
            r#"
            INSERT INTO expense_lines
                (id, expense_report_id, expense_date, category, description, amount,
                 receipt_url, sort_order, base_amount, receipt_attachment_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(line.id)
        .bind(line.expense_report_id)
        .bind(line.expense_date)
        .bind(&line.category)
        .bind(&line.description)
        .bind(line.amount)
        .bind(&line.receipt_url)
        .bind(line.sort_order)
        .bind(line.base_amount)
        .bind(line.receipt_attachment_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a ExpenseFilters) {
    if let Some(employee_id) = filters.employee_id {
        builder.push(" AND employee_id = ").push_bind(employee_id);
    }
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND created_at < (").push_bind(to).push("::date + 1)");
    }
}

#[async_trait]
impl ExpenseReportRepository for PgExpenseReportRepository {
    async fn create(
        &self,
        report: &ExpenseReport,
        lines: &[ExpenseLine],
    ) -> AppResult<ExpenseReport> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, ExpenseReport>(
            r#"
            INSERT INTO expense_reports
                (id, org_id, employee_id, report_number, description, total_amount, currency,
                 status, submitted_at, approved_by, approved_at, created_at, updated_at,
                 fx_rate, base_total_amount)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING *
            "#,
        )
        .bind(report.id)
        .bind(report.org_id)
        .bind(report.employee_id)
        .bind(&report.report_number)
        .bind(&report.description)
        .bind(report.total_amount)
        .bind(&report.currency)
        .bind(&report.status)
        .bind(report.submitted_at)
        .bind(report.approved_by)
        .bind(report.approved_at)
        .bind(report.created_at)
        .bind(report.updated_at)
        .bind(report.fx_rate)
        .bind(report.base_total_amount)
        .fetch_one(&mut *tx)
        .await?;

        insert_lines(&mut tx, lines).await?;
        tx.commit().await?;
        Ok(created)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<ExpenseReport>> {
        Ok(sqlx::query_as::<_, ExpenseReport>("SELECT * FROM expense_reports WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_lines(&self, report_id: Uuid) -> AppResult<Vec<ExpenseLine>> {
        Ok(sqlx::query_as::<_, ExpenseLine>(
            "SELECT * FROM expense_lines WHERE expense_report_id = $1 ORDER BY sort_order",
        )
        .bind(report_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn update(
        &self,
        report: &ExpenseReport,
        lines: Option<&[ExpenseLine]>,
    ) -> AppResult<ExpenseReport> {
        let mut tx = self.pool.begin().await?;

        let updated = sqlx::query_as::<_, ExpenseReport>(
            r#"
            UPDATE expense_reports SET
                description = $2, total_amount = $3, status = $4, submitted_at = $5,
                approved_by = $6, approved_at = $7, updated_at = $8,
                fx_rate = $9, base_total_amount = $10
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(report.id)
        .bind(&report.description)
        .bind(report.total_amount)
        .bind(&report.status)
        .bind(report.submitted_at)
        .bind(report.approved_by)
        .bind(report.approved_at)
        .bind(Utc::now())
        .bind(report.fx_rate)
        .bind(report.base_total_amount)
        .fetch_one(&mut *tx)
        .await?;

        if let Some(lines) = lines {
            sqlx::query("DELETE FROM expense_lines WHERE expense_report_id = $1")
                .bind(report.id)
                .execute(&mut *tx)
                .await?;
            insert_lines(&mut tx, lines).await?;
        }

        tx.commit().await?;
        Ok(updated)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM expense_reports WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &ExpenseFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<ExpenseReport>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM expense_reports WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<ExpenseReport>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM expense_reports WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('EXP', 'expense_report_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }
}
