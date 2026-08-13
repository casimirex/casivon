use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::modules::projects::domain::entities::*;

// ----------------------------------------------------------------- projects

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateProjectRequest {
    #[validate(length(max = 50))]
    pub project_code: Option<String>,
    #[validate(length(min = 1, max = 255, message = "Project name is required"))]
    pub name: String,
    pub description: Option<String>,
    pub customer_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    #[validate(custom = "validate_priority")]
    pub priority: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub budget: Option<Decimal>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateProjectRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub customer_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    #[validate(custom = "validate_priority")]
    pub priority: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub budget: Option<Decimal>,
    #[validate(range(min = 0, max = 100, message = "Progress must be between 0 and 100"))]
    pub progress_percent: Option<i32>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateProjectStatusRequest {
    #[validate(custom = "validate_project_status")]
    pub status: String,
}

fn validate_project_status(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &ProjectStatus::ALL, "status")
}

fn validate_priority(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &Priority::ALL, "priority")
}

// -------------------------------------------------------------------- tasks

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateTaskRequest {
    pub project_id: Uuid,
    pub parent_task_id: Option<Uuid>,
    #[validate(length(max = 50))]
    pub task_code: Option<String>,
    #[validate(length(min = 1, max = 255, message = "Task title is required"))]
    pub title: String,
    pub description: Option<String>,
    pub assigned_to: Option<Uuid>,
    #[validate(custom = "validate_priority")]
    pub priority: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub estimated_hours: Option<Decimal>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateTaskRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: Option<String>,
    pub description: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    #[validate(custom = "validate_priority")]
    pub priority: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub estimated_hours: Option<Decimal>,
    #[validate(range(min = 0, max = 100, message = "Progress must be between 0 and 100"))]
    pub progress_percent: Option<i32>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateTaskStatusRequest {
    #[validate(custom = "validate_task_status")]
    pub status: String,
}

fn validate_task_status(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &TaskStatus::ALL, "status")
}

// ------------------------------------------------------------- time entries

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateTimeEntryRequest {
    pub task_id: Uuid,
    pub employee_id: Uuid,
    pub entry_date: NaiveDate,
    pub hours: Decimal,
    pub description: Option<String>,
    pub is_billable: Option<bool>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateTimeEntryRequest {
    pub entry_date: Option<NaiveDate>,
    pub hours: Option<Decimal>,
    pub description: Option<String>,
    pub is_billable: Option<bool>,
}

// ---------------------------------------------------------------- responses

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectDetail {
    #[serde(flatten)]
    pub project: Project,
    pub task_summary: TaskSummary,
    pub billable_hours: Decimal,
    pub non_billable_hours: Decimal,
}

/// Task counts by status — what the Kanban header and progress bar need.
#[derive(Debug, Default, Serialize, ToSchema)]
pub struct TaskSummary {
    pub total: i64,
    pub todo: i64,
    pub in_progress: i64,
    pub review: i64,
    pub done: i64,
    pub cancelled: i64,
}

impl TaskSummary {
    pub fn from_statuses(statuses: &[(String, i32)]) -> Self {
        let mut summary = Self { total: statuses.len() as i64, ..Default::default() };

        for (status, _) in statuses {
            match status.as_str() {
                TaskStatus::TODO => summary.todo += 1,
                TaskStatus::IN_PROGRESS => summary.in_progress += 1,
                TaskStatus::REVIEW => summary.review += 1,
                TaskStatus::DONE => summary.done += 1,
                TaskStatus::CANCELLED => summary.cancelled += 1,
                _ => {}
            }
        }

        summary
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TaskWithProject {
    #[serde(flatten)]
    pub task: Task,
    /// Refreshed project progress after a task change, so the UI needn't refetch.
    pub project_progress_percent: i32,
}

fn one_of(
    value: &str,
    allowed: &[&str],
    code: &'static str,
) -> Result<(), validator::ValidationError> {
    if allowed.contains(&value) {
        return Ok(());
    }
    let mut err = validator::ValidationError::new(code);
    err.message = Some(format!("Must be one of: {}", allowed.join(", ")).into());
    Err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_summary_counts_each_status() {
        let statuses = vec![
            ("todo".to_string(), 0),
            ("todo".to_string(), 0),
            ("in_progress".to_string(), 50),
            ("done".to_string(), 100),
            ("cancelled".to_string(), 0),
        ];
        let summary = TaskSummary::from_statuses(&statuses);

        assert_eq!(summary.total, 5);
        assert_eq!(summary.todo, 2);
        assert_eq!(summary.in_progress, 1);
        assert_eq!(summary.done, 1);
        assert_eq!(summary.cancelled, 1);
        assert_eq!(summary.review, 0);
    }
}
