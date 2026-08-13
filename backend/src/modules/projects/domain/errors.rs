use thiserror::Error;

use crate::error::AppError;

#[derive(Error, Debug)]
pub enum ProjectsError {
    #[error("This {document} cannot move from '{from}' to '{to}'")]
    InvalidTransition { document: &'static str, from: String, to: String },

    #[error("Project code '{0}' is already in use")]
    DuplicateProjectCode(String),

    #[error("Task code '{0}' is already used on this project")]
    DuplicateTaskCode(String),

    #[error("Project '{0}' is {1} and no longer accepts work")]
    ProjectClosed(String, String),

    #[error("A task cannot be its own parent")]
    SelfParentTask,

    #[error("Parent task '{0}' belongs to a different project")]
    ParentTaskInAnotherProject(String),

    #[error("Making '{0}' a subtask of '{1}' would create a cycle")]
    CircularTaskHierarchy(String, String),

    #[error("Logged hours must be greater than zero")]
    NonPositiveHours,

    #[error("A single time entry cannot exceed 24 hours")]
    HoursExceedDay,

    #[error("Project end date must fall on or after the start date")]
    EndBeforeStart,

    #[error("Task '{0}' has subtasks and cannot be deleted")]
    TaskHasSubtasks(String),
}

impl From<ProjectsError> for AppError {
    fn from(err: ProjectsError) -> Self {
        match err {
            ProjectsError::InvalidTransition { .. }
            | ProjectsError::DuplicateProjectCode(_)
            | ProjectsError::DuplicateTaskCode(_)
            | ProjectsError::ProjectClosed(..)
            | ProjectsError::TaskHasSubtasks(_) => AppError::Conflict(err.to_string()),
            _ => AppError::Validation(err.to_string()),
        }
    }
}
