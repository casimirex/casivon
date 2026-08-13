use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::hr::domain::entities::Employee;
use crate::modules::hr::domain::repositories::{EmployeeFilters, EmployeeRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 4] = ["created_at", "hire_date", "last_name", "employee_number"];

#[derive(Clone)]
pub struct PgEmployeeRepository {
    pool: PgPool,
}

impl PgEmployeeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a EmployeeFilters) {
    if let Some(status) = &filters.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(department) = &filters.department {
        builder.push(" AND department = ").push_bind(department);
    }
    if let Some(manager_id) = filters.manager_id {
        builder.push(" AND manager_id = ").push_bind(manager_id);
    }
    if let Some(search) = &filters.search {
        let pattern = format!("%{}%", search);
        builder
            .push(" AND (first_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR last_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR email ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR employee_number ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl EmployeeRepository for PgEmployeeRepository {
    async fn create(&self, employee: &Employee) -> AppResult<Employee> {
        Ok(sqlx::query_as::<_, Employee>(
            r#"
            INSERT INTO employees
                (id, org_id, user_id, employee_number, first_name, last_name, email, phone,
                 hire_date, termination_date, department, job_title, manager_id, salary,
                 currency, status, annual_leave_entitlement, created_at, updated_at,
                 fx_rate, base_salary)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19,
                    $20, $21)
            RETURNING *
            "#,
        )
        .bind(employee.id)
        .bind(employee.org_id)
        .bind(employee.user_id)
        .bind(&employee.employee_number)
        .bind(&employee.first_name)
        .bind(&employee.last_name)
        .bind(&employee.email)
        .bind(&employee.phone)
        .bind(employee.hire_date)
        .bind(employee.termination_date)
        .bind(&employee.department)
        .bind(&employee.job_title)
        .bind(employee.manager_id)
        .bind(employee.salary)
        .bind(&employee.currency)
        .bind(&employee.status)
        .bind(employee.annual_leave_entitlement)
        .bind(employee.created_at)
        .bind(employee.updated_at)
        .bind(employee.fx_rate)
        .bind(employee.base_salary)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Employee>> {
        Ok(sqlx::query_as::<_, Employee>("SELECT * FROM employees WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_by_number(&self, number: &str) -> AppResult<Option<Employee>> {
        Ok(
            sqlx::query_as::<_, Employee>("SELECT * FROM employees WHERE employee_number = $1")
                .bind(number)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    async fn find_by_user_id(&self, user_id: Uuid) -> AppResult<Option<Employee>> {
        Ok(sqlx::query_as::<_, Employee>("SELECT * FROM employees WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, employee: &Employee) -> AppResult<Employee> {
        Ok(sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                first_name = $2, last_name = $3, email = $4, phone = $5, termination_date = $6,
                department = $7, job_title = $8, manager_id = $9, salary = $10, currency = $11,
                status = $12, annual_leave_entitlement = $13, updated_at = $14,
                fx_rate = $15, base_salary = $16
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(employee.id)
        .bind(&employee.first_name)
        .bind(&employee.last_name)
        .bind(&employee.email)
        .bind(&employee.phone)
        .bind(employee.termination_date)
        .bind(&employee.department)
        .bind(&employee.job_title)
        .bind(employee.manager_id)
        .bind(employee.salary)
        .bind(&employee.currency)
        .bind(&employee.status)
        .bind(employee.annual_leave_entitlement)
        .bind(Utc::now())
        .bind(employee.fx_rate)
        .bind(employee.base_salary)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM employees WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &EmployeeFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Employee>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM employees WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "created_at")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Employee>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM employees WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn next_number(&self) -> AppResult<String> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT next_document_number('EMP', 'employee_number_seq')",
        )
        .fetch_one(&self.pool)
        .await?)
    }
}
