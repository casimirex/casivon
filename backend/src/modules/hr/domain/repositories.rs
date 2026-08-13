use async_trait::async_trait;
use utoipa::IntoParams;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::hr::domain::entities::*;
use crate::shared::pagination::PaginationParams;

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EmployeeFilters {
    pub status: Option<String>,
    pub department: Option<String>,
    pub manager_id: Option<Uuid>,
    pub search: Option<String>,
}

#[async_trait]
pub trait EmployeeRepository: Send + Sync {
    async fn create(&self, employee: &Employee) -> AppResult<Employee>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Employee>>;
    async fn find_by_number(&self, number: &str) -> AppResult<Option<Employee>>;
    async fn find_by_user_id(&self, user_id: Uuid) -> AppResult<Option<Employee>>;
    async fn update(&self, employee: &Employee) -> AppResult<Employee>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &EmployeeFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Employee>, i64)>;
    async fn next_number(&self) -> AppResult<String>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LeaveFilters {
    pub employee_id: Option<Uuid>,
    pub status: Option<String>,
    pub leave_type: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait LeaveRequestRepository: Send + Sync {
    async fn create(&self, request: &LeaveRequest) -> AppResult<LeaveRequest>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<LeaveRequest>>;
    async fn update(&self, request: &LeaveRequest) -> AppResult<LeaveRequest>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &LeaveFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<LeaveRequest>, i64)>;
    /// Approved annual-leave days an employee has taken in `year`.
    async fn approved_days_in_year(&self, employee_id: Uuid, year: i32) -> AppResult<i32>;
    /// Pending or approved requests overlapping the given window.
    async fn find_overlapping(
        &self,
        employee_id: Uuid,
        start: NaiveDate,
        end: NaiveDate,
        exclude_id: Option<Uuid>,
    ) -> AppResult<Vec<LeaveRequest>>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExpenseFilters {
    pub employee_id: Option<Uuid>,
    pub status: Option<String>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait ExpenseReportRepository: Send + Sync {
    async fn create(&self, report: &ExpenseReport, lines: &[ExpenseLine])
        -> AppResult<ExpenseReport>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<ExpenseReport>>;
    async fn find_lines(&self, report_id: Uuid) -> AppResult<Vec<ExpenseLine>>;
    async fn update(
        &self,
        report: &ExpenseReport,
        lines: Option<&[ExpenseLine]>,
    ) -> AppResult<ExpenseReport>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &ExpenseFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<ExpenseReport>, i64)>;
    async fn next_number(&self) -> AppResult<String>;
}
