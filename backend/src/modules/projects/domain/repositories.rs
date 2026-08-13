use async_trait::async_trait;
use utoipa::IntoParams;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::projects::domain::entities::*;
use crate::shared::pagination::PaginationParams;

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProjectFilters {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub customer_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    pub search: Option<String>,
}

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn create(&self, project: &Project) -> AppResult<Project>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Project>>;
    async fn find_by_code(&self, code: &str) -> AppResult<Option<Project>>;
    async fn update(&self, project: &Project) -> AppResult<Project>;
    async fn update_progress(&self, id: Uuid, progress: i32) -> AppResult<Project>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &ProjectFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Project>, i64)>;
    async fn next_code(&self) -> AppResult<String>;
    /// Hours logged against the project, split billable / non-billable.
    async fn logged_hours(&self, id: Uuid) -> AppResult<(Decimal, Decimal)>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TaskFilters {
    pub project_id: Option<Uuid>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    pub search: Option<String>,
}

#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn create(&self, task: &Task) -> AppResult<Task>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Task>>;
    async fn find_by_code(&self, project_id: Uuid, code: &str) -> AppResult<Option<Task>>;
    async fn update(&self, task: &Task) -> AppResult<Task>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &TaskFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Task>, i64)>;
    async fn count_subtasks(&self, id: Uuid) -> AppResult<i64>;
    /// (status, progress_percent) for every task on a project — the roll-up input.
    async fn progress_snapshot(&self, project_id: Uuid) -> AppResult<Vec<(String, i32)>>;
    async fn next_code(&self, project_id: Uuid) -> AppResult<String>;
    /// Recomputes `actual_hours` from the task's time entries.
    async fn sync_actual_hours(&self, task_id: Uuid) -> AppResult<Task>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TimeEntryFilters {
    pub task_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub employee_id: Option<Uuid>,
    pub is_billable: Option<bool>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

#[async_trait]
pub trait TimeEntryRepository: Send + Sync {
    async fn create(&self, entry: &TimeEntry) -> AppResult<TimeEntry>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<TimeEntry>>;
    async fn update(&self, entry: &TimeEntry) -> AppResult<TimeEntry>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &TimeEntryFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<TimeEntry>, i64)>;
}
