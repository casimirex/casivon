use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::projects::application::dto::*;
use crate::modules::projects::domain::entities::*;
use crate::modules::projects::domain::errors::ProjectsError;
use crate::modules::projects::domain::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::currency::{CurrencyResolver, DocumentCurrency};
use crate::shared::pagination::PaginationParams;

const DEFAULT_PRIORITY: &str = "medium";
const MAX_HOURS_PER_ENTRY: i64 = 24;

pub struct ProjectUseCases<P: ProjectRepository, T: TaskRepository> {
    projects: P,
    tasks: T,    fx: Arc<dyn CurrencyResolver>,
}

impl<P: ProjectRepository, T: TaskRepository> ProjectUseCases<P, T> {
    pub fn new(projects: P, tasks: T, fx: Arc<dyn CurrencyResolver>) -> Self {
        Self { projects, tasks, fx }
    }

    /// The currency a document is raised in, together with the rate frozen onto
    /// it. Read at the point of use rather than cached, so a change under
    /// Settings applies to the next document raised.
    ///
    /// `on` is the document's own date: the rate that applied when it was
    /// raised is the rate it keeps.
    async fn currency(
        &self,
        requested: Option<String>,
        on: NaiveDate,
    ) -> AppResult<DocumentCurrency> {
        self.fx.resolve(requested.as_deref(), on).await
    }

    pub async fn create(&self, req: CreateProjectRequest, user: &CurrentUser) -> AppResult<Project> {
        if let (Some(start), Some(end)) = (req.start_date, req.end_date) {
            if end < start {
                return Err(ProjectsError::EndBeforeStart.into());
            }
        }

        let project_code = match req.project_code {
            Some(code) => {
                if self.projects.find_by_code(&code).await?.is_some() {
                    return Err(ProjectsError::DuplicateProjectCode(code).into());
                }
                code
            }
            None => self.projects.next_code().await?,
        };

        let now = Utc::now();

        // A budget is a forecast rather than a posting, and `start_date` is
        // optional and often in the past, so the rate is the one in force when
        // the project was set up.
        let currency = self.currency(req.currency.clone(), now.date_naive()).await?;

        let project = Project {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            project_code,
            name: req.name,
            description: req.description,
            customer_id: req.customer_id,
            manager_id: req.manager_id.or(Some(user.id)),
            status: ProjectStatus::PLANNING.to_string(),
            priority: req.priority.unwrap_or_else(|| DEFAULT_PRIORITY.to_string()),
            start_date: req.start_date,
            end_date: req.end_date,
            budget: req.budget,
            base_budget: currency.to_base_opt(req.budget),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            progress_percent: 0,
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };

        self.projects.create(&project).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<ProjectDetail> {
        let project = self.require_project(id).await?;
        let snapshot = self.tasks.progress_snapshot(id).await?;
        let (billable, non_billable) = self.projects.logged_hours(id).await?;

        Ok(ProjectDetail {
            project,
            task_summary: TaskSummary::from_statuses(&snapshot),
            billable_hours: billable,
            non_billable_hours: non_billable,
        })
    }

    pub async fn list(
        &self,
        filters: &ProjectFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Project>, i64)> {
        self.projects.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateProjectRequest) -> AppResult<Project> {
        let mut project = self.require_project(id).await?;

        if let Some(v) = req.name {
            project.name = v;
        }
        if req.description.is_some() {
            project.description = req.description;
        }
        if req.customer_id.is_some() {
            project.customer_id = req.customer_id;
        }
        if req.manager_id.is_some() {
            project.manager_id = req.manager_id;
        }
        if let Some(v) = req.priority {
            project.priority = v;
        }
        if req.start_date.is_some() {
            project.start_date = req.start_date;
        }
        if req.end_date.is_some() {
            project.end_date = req.end_date;
        }
        if req.budget.is_some() {
            project.budget = req.budget;
        }
        if let Some(v) = req.progress_percent {
            project.progress_percent = v;
        }

        if let (Some(start), Some(end)) = (project.start_date, project.end_date) {
            if end < start {
                return Err(ProjectsError::EndBeforeStart.into());
            }
        }
        project.updated_at = Utc::now();

        self.projects.update(&project).await
    }

    pub async fn set_status(&self, id: Uuid, status: &str) -> AppResult<Project> {
        let mut project = self.require_project(id).await?;

        if !ProjectStatus::can_transition(&project.status, status) {
            return Err(ProjectsError::InvalidTransition {
                document: "project",
                from: project.status,
                to: status.to_string(),
            }
            .into());
        }

        // Marking a project complete completes its progress bar too.
        if status == ProjectStatus::COMPLETED {
            project.progress_percent = 100;
        }
        project.status = status.to_string();
        project.updated_at = Utc::now();

        self.projects.update(&project).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.require_project(id).await?;
        // tasks and time entries cascade from the project foreign keys.
        self.projects.delete(id).await
    }

    async fn require_project(&self, id: Uuid) -> AppResult<Project> {
        self.projects
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Project {} not found", id)))
    }
}

pub struct TaskUseCases<T: TaskRepository, P: ProjectRepository> {
    tasks: T,
    projects: P,
}

impl<T: TaskRepository, P: ProjectRepository> TaskUseCases<T, P> {
    pub fn new(tasks: T, projects: P) -> Self {
        Self { tasks, projects }
    }

    pub async fn create(
        &self,
        req: CreateTaskRequest,
        user: &CurrentUser,
    ) -> AppResult<TaskWithProject> {
        let project = self.require_project(req.project_id).await?;

        if !ProjectStatus::accepts_work(&project.status) {
            return Err(
                ProjectsError::ProjectClosed(project.project_code, project.status).into()
            );
        }

        if let Some(parent_id) = req.parent_task_id {
            let parent = self.require_task(parent_id).await?;
            if parent.project_id != req.project_id {
                return Err(
                    ProjectsError::ParentTaskInAnotherProject(parent.task_code).into()
                );
            }
        }

        let task_code = match req.task_code {
            Some(code) => {
                if self.tasks.find_by_code(req.project_id, &code).await?.is_some() {
                    return Err(ProjectsError::DuplicateTaskCode(code).into());
                }
                code
            }
            None => self.tasks.next_code(req.project_id).await?,
        };

        let now = Utc::now();
        let task = Task {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            project_id: req.project_id,
            parent_task_id: req.parent_task_id,
            task_code,
            title: req.title,
            description: req.description,
            assigned_to: req.assigned_to,
            status: TaskStatus::TODO.to_string(),
            priority: req.priority.unwrap_or_else(|| DEFAULT_PRIORITY.to_string()),
            start_date: req.start_date,
            due_date: req.due_date,
            estimated_hours: req.estimated_hours,
            actual_hours: Some(Decimal::ZERO),
            progress_percent: 0,
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };

        let task = self.tasks.create(&task).await?;
        let project_progress = self.refresh_project_progress(req.project_id).await?;
        Ok(TaskWithProject { task, project_progress_percent: project_progress })
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Task> {
        self.require_task(id).await
    }

    pub async fn list(
        &self,
        filters: &TaskFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Task>, i64)> {
        self.tasks.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateTaskRequest) -> AppResult<TaskWithProject> {
        let mut task = self.require_task(id).await?;

        if let Some(v) = req.title {
            task.title = v;
        }
        if req.description.is_some() {
            task.description = req.description;
        }
        if req.assigned_to.is_some() {
            task.assigned_to = req.assigned_to;
        }
        if let Some(parent_id) = req.parent_task_id {
            if parent_id == id {
                return Err(ProjectsError::SelfParentTask.into());
            }
            let parent = self.require_task(parent_id).await?;
            if parent.project_id != task.project_id {
                return Err(
                    ProjectsError::ParentTaskInAnotherProject(parent.task_code).into()
                );
            }
            self.assert_no_cycle(id, parent_id).await?;
            task.parent_task_id = Some(parent_id);
        }
        if let Some(v) = req.priority {
            task.priority = v;
        }
        if req.start_date.is_some() {
            task.start_date = req.start_date;
        }
        if req.due_date.is_some() {
            task.due_date = req.due_date;
        }
        if req.estimated_hours.is_some() {
            task.estimated_hours = req.estimated_hours;
        }
        if let Some(v) = req.progress_percent {
            task.progress_percent = v;
        }
        task.updated_at = Utc::now();

        let task = self.tasks.update(&task).await?;
        let project_progress = self.refresh_project_progress(task.project_id).await?;
        Ok(TaskWithProject { task, project_progress_percent: project_progress })
    }

    pub async fn set_status(&self, id: Uuid, status: &str) -> AppResult<TaskWithProject> {
        let mut task = self.require_task(id).await?;

        if !TaskStatus::can_transition(&task.status, status) {
            return Err(ProjectsError::InvalidTransition {
                document: "task",
                from: task.status,
                to: status.to_string(),
            }
            .into());
        }

        task.progress_percent = TaskStatus::implied_progress(status, task.progress_percent);
        task.status = status.to_string();
        task.updated_at = Utc::now();

        let task = self.tasks.update(&task).await?;
        let project_progress = self.refresh_project_progress(task.project_id).await?;
        Ok(TaskWithProject { task, project_progress_percent: project_progress })
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let task = self.require_task(id).await?;

        // Subtasks would be orphaned; the caller must deal with them first.
        if self.tasks.count_subtasks(id).await? > 0 {
            return Err(ProjectsError::TaskHasSubtasks(task.task_code).into());
        }

        self.tasks.delete(id).await?;
        self.refresh_project_progress(task.project_id).await?;
        Ok(())
    }

    /// Averages task progress back onto the parent project.
    async fn refresh_project_progress(&self, project_id: Uuid) -> AppResult<i32> {
        let project = self.require_project(project_id).await?;
        let snapshot = self.tasks.progress_snapshot(project_id).await?;
        let progress = roll_up_progress(&snapshot, project.progress_percent);

        if progress != project.progress_percent {
            self.projects.update_progress(project_id, progress).await?;
        }

        Ok(progress)
    }

    async fn assert_no_cycle(&self, id: Uuid, parent_id: Uuid) -> AppResult<()> {
        let mut cursor = Some(parent_id);
        let mut hops = 0;

        while let Some(current) = cursor {
            if current == id {
                let task = self.require_task(id).await?;
                let parent = self.require_task(parent_id).await?;
                return Err(ProjectsError::CircularTaskHierarchy(
                    task.task_code,
                    parent.task_code,
                )
                .into());
            }

            hops += 1;
            if hops > 64 {
                break;
            }

            cursor = self.tasks.find_by_id(current).await?.and_then(|t| t.parent_task_id);
        }

        Ok(())
    }

    async fn require_task(&self, id: Uuid) -> AppResult<Task> {
        self.tasks
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Task {} not found", id)))
    }

    async fn require_project(&self, id: Uuid) -> AppResult<Project> {
        self.projects
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Project {} not found", id)))
    }
}

pub struct TimeEntryUseCases<E: TimeEntryRepository, T: TaskRepository, P: ProjectRepository> {
    entries: E,
    tasks: T,
    projects: P,
}

impl<E: TimeEntryRepository, T: TaskRepository, P: ProjectRepository> TimeEntryUseCases<E, T, P> {
    pub fn new(entries: E, tasks: T, projects: P) -> Self {
        Self { entries, tasks, projects }
    }

    pub async fn create(
        &self,
        req: CreateTimeEntryRequest,
        user: &CurrentUser,
    ) -> AppResult<TimeEntry> {
        assert_hours(req.hours)?;

        let task = self
            .tasks
            .find_by_id(req.task_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Task {} not found", req.task_id)))?;

        let project = self
            .projects
            .find_by_id(task.project_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Project not found".to_string()))?;

        if !ProjectStatus::accepts_work(&project.status) {
            return Err(
                ProjectsError::ProjectClosed(project.project_code, project.status).into()
            );
        }

        let entry = TimeEntry {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            task_id: req.task_id,
            employee_id: req.employee_id,
            entry_date: req.entry_date,
            hours: req.hours,
            description: req.description,
            is_billable: req.is_billable.unwrap_or(true),
            created_at: Utc::now(),
        };

        let entry = self.entries.create(&entry).await?;
        // Keep the task's actual_hours in step with its ledger of entries.
        self.tasks.sync_actual_hours(req.task_id).await?;
        Ok(entry)
    }

    pub async fn get(&self, id: Uuid) -> AppResult<TimeEntry> {
        self.entries
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Time entry {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &TimeEntryFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<TimeEntry>, i64)> {
        self.entries.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateTimeEntryRequest) -> AppResult<TimeEntry> {
        let mut entry = self.get(id).await?;

        if let Some(v) = req.entry_date {
            entry.entry_date = v;
        }
        if let Some(v) = req.hours {
            assert_hours(v)?;
            entry.hours = v;
        }
        if req.description.is_some() {
            entry.description = req.description;
        }
        if let Some(v) = req.is_billable {
            entry.is_billable = v;
        }

        let entry = self.entries.update(&entry).await?;
        self.tasks.sync_actual_hours(entry.task_id).await?;
        Ok(entry)
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let entry = self.get(id).await?;
        self.entries.delete(id).await?;
        self.tasks.sync_actual_hours(entry.task_id).await?;
        Ok(())
    }
}

fn assert_hours(hours: Decimal) -> AppResult<()> {
    if hours <= Decimal::ZERO {
        return Err(ProjectsError::NonPositiveHours.into());
    }
    if hours > Decimal::from(MAX_HOURS_PER_ENTRY) {
        return Err(ProjectsError::HoursExceedDay.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn hours_must_fit_inside_a_day() {
        assert!(assert_hours(dec!(7.5)).is_ok());
        assert!(assert_hours(dec!(24)).is_ok());
        assert!(assert_hours(dec!(24.5)).is_err());
        assert!(assert_hours(dec!(0)).is_err());
        assert!(assert_hours(dec!(-1)).is_err());
    }
}
