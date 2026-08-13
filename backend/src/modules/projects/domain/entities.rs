use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Project {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub project_code: String,
    pub name: String,
    pub description: Option<String>,
    pub customer_id: Option<Uuid>,
    pub manager_id: Option<Uuid>,
    pub status: String, // planning, active, on_hold, completed, cancelled
    pub priority: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub budget: Option<Decimal>,
    pub currency: String,
    pub fx_rate: Decimal,
    /// `budget` restated in the base currency.
    pub base_budget: Option<Decimal>,
    pub progress_percent: i32,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Task {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub project_id: Uuid,
    pub parent_task_id: Option<Uuid>,
    pub task_code: String,
    pub title: String,
    pub description: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub status: String, // todo, in_progress, review, done, cancelled
    pub priority: String,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub estimated_hours: Option<Decimal>,
    pub actual_hours: Option<Decimal>,
    pub progress_percent: i32,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TimeEntry {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub task_id: Uuid,
    pub employee_id: Uuid,
    pub entry_date: NaiveDate,
    pub hours: Decimal,
    pub description: Option<String>,
    pub is_billable: bool,
    pub created_at: DateTime<Utc>,
}

pub struct ProjectStatus;

impl ProjectStatus {
    pub const PLANNING: &'static str = "planning";
    pub const ACTIVE: &'static str = "active";
    pub const ON_HOLD: &'static str = "on_hold";
    pub const COMPLETED: &'static str = "completed";
    pub const CANCELLED: &'static str = "cancelled";

    pub const ALL: [&'static str; 5] = [
        Self::PLANNING,
        Self::ACTIVE,
        Self::ON_HOLD,
        Self::COMPLETED,
        Self::CANCELLED,
    ];

    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            (Self::PLANNING, Self::ACTIVE)
                | (Self::ACTIVE, Self::ON_HOLD)
                | (Self::ON_HOLD, Self::ACTIVE)
                | (Self::ACTIVE, Self::COMPLETED)
                | (Self::PLANNING, Self::CANCELLED)
                | (Self::ACTIVE, Self::CANCELLED)
                | (Self::ON_HOLD, Self::CANCELLED)
        )
    }

    /// Work can only be logged against a project that is actually running.
    pub fn accepts_work(status: &str) -> bool {
        matches!(status, Self::PLANNING | Self::ACTIVE)
    }
}

/// Task: todo -> in_progress -> review -> done, cancellable before it's done.
pub struct TaskStatus;

impl TaskStatus {
    pub const TODO: &'static str = "todo";
    pub const IN_PROGRESS: &'static str = "in_progress";
    pub const REVIEW: &'static str = "review";
    pub const DONE: &'static str = "done";
    pub const CANCELLED: &'static str = "cancelled";

    pub const ALL: [&'static str; 5] =
        [Self::TODO, Self::IN_PROGRESS, Self::REVIEW, Self::DONE, Self::CANCELLED];

    pub fn can_transition(from: &str, to: &str) -> bool {
        matches!(
            (from, to),
            (Self::TODO, Self::IN_PROGRESS)
                | (Self::IN_PROGRESS, Self::REVIEW)
                | (Self::REVIEW, Self::DONE)
                // Review can bounce work back.
                | (Self::REVIEW, Self::IN_PROGRESS)
                // Reopening finished work.
                | (Self::DONE, Self::IN_PROGRESS)
                | (Self::TODO, Self::CANCELLED)
                | (Self::IN_PROGRESS, Self::CANCELLED)
                | (Self::REVIEW, Self::CANCELLED)
        )
    }

    /// Progress percentage implied by a status change.
    pub fn implied_progress(status: &str, current: i32) -> i32 {
        match status {
            Self::TODO => 0,
            Self::DONE => 100,
            // in_progress / review / cancelled keep whatever was reported.
            _ => current,
        }
    }

    /// Cancelled tasks are excluded from a project's progress roll-up.
    pub fn counts_towards_progress(status: &str) -> bool {
        status != Self::CANCELLED
    }
}

pub struct Priority;

impl Priority {
    pub const ALL: [&'static str; 4] = ["low", "medium", "high", "urgent"];

    pub fn is_valid(value: &str) -> bool {
        Self::ALL.contains(&value)
    }
}

/// A project's progress is the mean of its tasks' progress, ignoring cancelled
/// ones. A project with no live tasks keeps whatever progress was set by hand.
pub fn roll_up_progress(task_progress: &[(String, i32)], current: i32) -> i32 {
    let live: Vec<i32> = task_progress
        .iter()
        .filter(|(status, _)| TaskStatus::counts_towards_progress(status))
        .map(|(_, progress)| *progress)
        .collect();

    if live.is_empty() {
        return current;
    }

    (live.iter().sum::<i32>() as f64 / live.len() as f64).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_can_send_work_back() {
        assert!(TaskStatus::can_transition(TaskStatus::REVIEW, TaskStatus::IN_PROGRESS));
        assert!(TaskStatus::can_transition(TaskStatus::REVIEW, TaskStatus::DONE));
        // But work cannot skip review.
        assert!(!TaskStatus::can_transition(TaskStatus::IN_PROGRESS, TaskStatus::DONE));
        assert!(!TaskStatus::can_transition(TaskStatus::TODO, TaskStatus::DONE));
    }

    #[test]
    fn cancelled_tasks_are_terminal() {
        assert!(!TaskStatus::can_transition(TaskStatus::CANCELLED, TaskStatus::TODO));
        assert!(!TaskStatus::can_transition(TaskStatus::DONE, TaskStatus::CANCELLED));
    }

    #[test]
    fn finishing_a_task_sets_it_to_a_hundred_percent() {
        assert_eq!(TaskStatus::implied_progress(TaskStatus::DONE, 40), 100);
        assert_eq!(TaskStatus::implied_progress(TaskStatus::TODO, 40), 0);
        assert_eq!(TaskStatus::implied_progress(TaskStatus::IN_PROGRESS, 40), 40);
    }

    #[test]
    fn project_progress_averages_its_live_tasks() {
        let tasks = vec![
            ("done".to_string(), 100),
            ("in_progress".to_string(), 50),
            ("todo".to_string(), 0),
        ];
        assert_eq!(roll_up_progress(&tasks, 0), 50);
    }

    #[test]
    fn cancelled_tasks_do_not_drag_progress_down() {
        let tasks = vec![
            ("done".to_string(), 100),
            ("cancelled".to_string(), 0),
        ];
        assert_eq!(roll_up_progress(&tasks, 0), 100);
    }

    #[test]
    fn a_project_with_no_tasks_keeps_its_manual_progress() {
        assert_eq!(roll_up_progress(&[], 35), 35);
        // Same when every task is cancelled.
        assert_eq!(roll_up_progress(&[("cancelled".to_string(), 0)], 35), 35);
    }

    #[test]
    fn completed_projects_take_no_more_work() {
        assert!(ProjectStatus::accepts_work(ProjectStatus::ACTIVE));
        assert!(!ProjectStatus::accepts_work(ProjectStatus::COMPLETED));
        assert!(!ProjectStatus::accepts_work(ProjectStatus::CANCELLED));
    }
}
