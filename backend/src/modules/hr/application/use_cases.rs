use std::sync::Arc;

use chrono::{Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::hr::application::dto::*;
use crate::modules::hr::domain::entities::*;
use crate::modules::hr::domain::errors::HrError;
use crate::modules::hr::domain::repositories::*;
use crate::modules::files::domain::repositories::AttachmentRepository;
use crate::shared::auth::CurrentUser;
use crate::shared::currency::{CurrencyResolver, DocumentCurrency};
use crate::shared::posting::{DocumentPoster, PostableExpenseReport};
use crate::shared::money::round_money;
use crate::shared::pagination::PaginationParams;

const DEFAULT_ENTITLEMENT: i32 = 25;

/// Who may see everybody's records. Salaries, approvals and other people's
/// claims are all management data.
///
/// Lives here rather than in the handler now that it decides what a query
/// returns and not merely who may call it.
pub const HR_ROLES: [&str; 2] = ["hr", "manager"];

/// What HR records the caller may touch.
///
/// Resolved once per request from the login, and then consulted rather than
/// re-derived — every endpoint asks the same question, and asking it in one
/// place is what stops a new endpoint quietly forgetting to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HrScope {
    /// An `hr` or `manager` role: everyone's records.
    All,
    /// The employee this login is linked to, and nobody else.
    Own(Uuid),
    /// Signed in, no HR role, and no employee record linked to this login.
    /// Ordinary for an administrator or a contractor account.
    Nothing,
}

impl HrScope {
    /// Public because a receipt is governed by the same rule as the claim it is
    /// attached to, and the files module has to be able to ask.
    pub async fn resolve<E: EmployeeRepository>(
        employees: &E,
        user: &CurrentUser,
    ) -> AppResult<HrScope> {
        if user.require_any_role(&HR_ROLES).is_ok() {
            return Ok(HrScope::All);
        }

        // `find_by_user_id` has existed since the HR module was written and was
        // called from nowhere until now. See `017_link_employees_to_users.sql`.
        Ok(match employees.find_by_user_id(user.id).await? {
            Some(employee) => HrScope::Own(employee.id),
            None => HrScope::Nothing,
        })
    }

    /// Whether a record belonging to `employee_id` is theirs to see.
    pub fn allows(self, employee_id: Uuid) -> bool {
        match self {
            HrScope::All => true,
            HrScope::Own(mine) => mine == employee_id,
            HrScope::Nothing => false,
        }
    }
}

/// Refuses a claim raised in somebody else's name.
///
/// Refused rather than quietly rewritten to the caller's own id: silently
/// correcting a payload tells the client its request succeeded as sent, and
/// hides the bug that produced it. HR may file on anyone's behalf, which is what
/// the role is for.
async fn assert_may_file_for<E: EmployeeRepository>(
    employees: &E,
    user: &CurrentUser,
    employee_id: Uuid,
) -> AppResult<()> {
    match HrScope::resolve(employees, user).await? {
        HrScope::All => Ok(()),
        HrScope::Own(mine) if mine == employee_id => Ok(()),
        HrScope::Own(_) => Err(HrError::NotYoursToFile.into()),
        HrScope::Nothing => Err(HrError::NoEmployeeRecord.into()),
    }
}

pub struct EmployeeUseCases<E: EmployeeRepository, L: LeaveRequestRepository> {
    employees: E,
    leave: L,    fx: Arc<dyn CurrencyResolver>,
}

impl<E: EmployeeRepository, L: LeaveRequestRepository> EmployeeUseCases<E, L> {
    pub fn new(employees: E, leave: L, fx: Arc<dyn CurrencyResolver>) -> Self {
        Self { employees, leave, fx }
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

    pub async fn create(
        &self,
        req: CreateEmployeeRequest,
        user: &CurrentUser,
    ) -> AppResult<Employee> {
        let employee_number = match req.employee_number {
            Some(number) => {
                if self.employees.find_by_number(&number).await?.is_some() {
                    return Err(HrError::DuplicateEmployeeNumber(number).into());
                }
                number
            }
            None => self.employees.next_number().await?,
        };

        if let Some(manager_id) = req.manager_id {
            if self.employees.find_by_id(manager_id).await?.is_none() {
                return Err(AppError::NotFound(format!("Manager {} not found", manager_id)));
            }
        }

        let now = Utc::now();

        // Resolved against today rather than the hire date: a salary is a
        // current fact, and someone hired in 2011 would otherwise need a 2011
        // rate on file before their record could be created at all.
        let currency = self.currency(req.currency.clone(), now.date_naive()).await?;

        let employee = Employee {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            user_id: req.user_id,
            employee_number,
            first_name: req.first_name,
            last_name: req.last_name,
            email: req.email,
            phone: req.phone,
            hire_date: req.hire_date,
            termination_date: None,
            department: req.department,
            job_title: req.job_title,
            manager_id: req.manager_id,
            salary: req.salary,
            base_salary: currency.to_base_opt(req.salary),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            status: EmployeeStatus::ACTIVE.to_string(),
            annual_leave_entitlement: req.annual_leave_entitlement.unwrap_or(DEFAULT_ENTITLEMENT),
            created_at: now,
            updated_at: now,
        };

        self.employees.create(&employee).await
    }

    /// Own-or-HR.
    ///
    /// Gating this to HR alone — as the previous change did while closing the
    /// salary leak — also stopped an employee reading their *own* profile and
    /// leave balance, which is ordinary self-service and worked before.
    pub async fn get(&self, id: Uuid, user: &CurrentUser) -> AppResult<EmployeeDetail> {
        let missing = || AppError::NotFound(format!("Employee {} not found", id));

        if !HrScope::resolve(&self.employees, user).await?.allows(id) {
            return Err(missing());
        }

        let employee = self.employees.find_by_id(id).await?.ok_or_else(missing)?;
        let leave_balance = self.leave_balance(&employee).await?;
        Ok(EmployeeDetail { employee, leave_balance })
    }

    pub async fn list(
        &self,
        filters: &EmployeeFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Employee>, i64)> {
        self.employees.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateEmployeeRequest) -> AppResult<Employee> {
        let mut employee = self.require_employee(id).await?;

        if let Some(v) = req.first_name {
            employee.first_name = v;
        }
        if let Some(v) = req.last_name {
            employee.last_name = v;
        }
        if let Some(v) = req.email {
            employee.email = v;
        }
        if req.phone.is_some() {
            employee.phone = req.phone;
        }
        if req.department.is_some() {
            employee.department = req.department;
        }
        if req.job_title.is_some() {
            employee.job_title = req.job_title;
        }
        if let Some(manager_id) = req.manager_id {
            if manager_id == id {
                return Err(HrError::SelfManaged.into());
            }
            if self.employees.find_by_id(manager_id).await?.is_none() {
                return Err(AppError::NotFound(format!("Manager {} not found", manager_id)));
            }
            employee.manager_id = Some(manager_id);
        }
        if req.salary.is_some() {
            employee.salary = req.salary;
        }
        if req.termination_date.is_some() {
            employee.termination_date = req.termination_date;
            // Recording a leaving date terminates the record unless told otherwise.
            employee.status = EmployeeStatus::TERMINATED.to_string();
        }
        if let Some(v) = req.status {
            employee.status = v;
        }
        if let Some(v) = req.annual_leave_entitlement {
            employee.annual_leave_entitlement = v;
        }
        employee.updated_at = Utc::now();

        self.employees.update(&employee).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.require_employee(id).await?;
        self.employees.delete(id).await
    }

    pub async fn leave_balance_for(&self, id: Uuid, user: &CurrentUser) -> AppResult<LeaveBalance> {
        // Own-or-HR, like the profile: knowing how much leave you have left is
        // the most routine self-service question there is.
        let missing = || AppError::NotFound(format!("Employee {} not found", id));

        if !HrScope::resolve(&self.employees, user).await?.allows(id) {
            return Err(missing());
        }

        let employee = self.employees.find_by_id(id).await?.ok_or_else(missing)?;
        self.leave_balance(&employee).await
    }

    async fn leave_balance(&self, employee: &Employee) -> AppResult<LeaveBalance> {
        let year = Utc::now().year();
        let taken = self.leave.approved_days_in_year(employee.id, year).await?;

        Ok(LeaveBalance {
            year,
            entitlement: employee.annual_leave_entitlement,
            taken,
            remaining: employee.annual_leave_entitlement - taken,
        })
    }

    async fn require_employee(&self, id: Uuid) -> AppResult<Employee> {
        self.employees
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Employee {} not found", id)))
    }
}

pub struct LeaveUseCases<L: LeaveRequestRepository, E: EmployeeRepository> {
    leave: L,
    employees: E,
}

impl<L: LeaveRequestRepository, E: EmployeeRepository> LeaveUseCases<L, E> {
    pub fn new(leave: L, employees: E) -> Self {
        Self { leave, employees }
    }

    pub async fn create(
        &self,
        req: CreateLeaveRequest,
        user: &CurrentUser,
    ) -> AppResult<LeaveRequest> {
        if !LeaveType::is_valid(&req.leave_type) {
            return Err(HrError::UnknownLeaveType(req.leave_type).into());
        }
        if req.end_date < req.start_date {
            return Err(HrError::EndBeforeStart.into());
        }

        assert_may_file_for(&self.employees, user, req.employee_id).await?;

        let employee = self
            .employees
            .find_by_id(req.employee_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Employee {} not found", req.employee_id)))?;

        if employee.status == EmployeeStatus::TERMINATED {
            return Err(HrError::EmployeeTerminated(employee.employee_number).into());
        }

        // Booking the same days twice would silently double-spend the entitlement.
        let overlapping = self
            .leave
            .find_overlapping(req.employee_id, req.start_date, req.end_date, None)
            .await?;
        if !overlapping.is_empty() {
            return Err(HrError::OverlappingLeave(
                req.start_date.to_string(),
                req.end_date.to_string(),
            )
            .into());
        }

        // Default to working days: nobody expects weekends to burn holiday.
        let days_requested = req
            .days_requested
            .unwrap_or_else(|| working_days(req.start_date, req.end_date).max(1));

        if LeaveType::counts_against_entitlement(&req.leave_type) {
            let taken = self
                .leave
                .approved_days_in_year(employee.id, req.start_date.year())
                .await?;
            let remaining = employee.annual_leave_entitlement - taken;

            if days_requested > remaining {
                return Err(HrError::InsufficientLeaveBalance {
                    employee: employee.employee_number,
                    remaining,
                    requested: days_requested,
                }
                .into());
            }
        }

        let now = Utc::now();
        let request = LeaveRequest {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            employee_id: req.employee_id,
            leave_type: req.leave_type,
            start_date: req.start_date,
            end_date: req.end_date,
            days_requested,
            reason: req.reason,
            status: LeaveStatus::PENDING.to_string(),
            approved_by: None,
            approved_at: None,
            created_at: now,
            updated_at: now,
        };

        self.leave.create(&request).await
    }

    pub async fn get(&self, id: Uuid, user: &CurrentUser) -> AppResult<LeaveRequest> {
        let missing = || AppError::NotFound(format!("Leave request {} not found", id));
        let request = self.leave.find_by_id(id).await?.ok_or_else(missing)?;

        // Somebody else's request answers *not found* rather than *forbidden*:
        // a 403 confirms the record exists, which is what a probe is after.
        if !HrScope::resolve(&self.employees, user).await?.allows(request.employee_id) {
            return Err(missing());
        }

        Ok(request)
    }

    pub async fn list(
        &self,
        filters: &LeaveFilters,
        params: &PaginationParams,
        user: &CurrentUser,
    ) -> AppResult<(Vec<LeaveRequest>, i64)> {
        match HrScope::resolve(&self.employees, user).await? {
            HrScope::All => self.leave.list(filters, params).await,
            HrScope::Own(employee_id) => {
                // Overriding rather than adding a condition: the filter already
                // exists, so scoping is a matter of deciding its value instead
                // of letting the caller.
                let mut scoped = filters.clone();
                scoped.employee_id = Some(employee_id);
                self.leave.list(&scoped, params).await
            }
            HrScope::Nothing => Ok((Vec::new(), 0)),
        }
    }

    /// Approve or reject. Approving re-checks the balance, since other requests
    /// may have been approved since this one was raised.
    pub async fn decide(
        &self,
        id: Uuid,
        status: &str,
        approver: &CurrentUser,
    ) -> AppResult<LeaveRequest> {
        // The approver is HR by the time this is reached, so `get` sees
        // everything — the scope check is a no-op here rather than a second
        // gate on top of the handler's.
        let mut request = self.get(id, approver).await?;

        if !LeaveStatus::can_transition(&request.status, status) {
            return Err(HrError::InvalidTransition {
                document: "leave request",
                from: request.status,
                to: status.to_string(),
            }
            .into());
        }

        if status == LeaveStatus::APPROVED
            && LeaveType::counts_against_entitlement(&request.leave_type)
        {
            let employee = self
                .employees
                .find_by_id(request.employee_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Employee not found".to_string()))?;

            let taken = self
                .leave
                .approved_days_in_year(employee.id, request.start_date.year())
                .await?;
            let remaining = employee.annual_leave_entitlement - taken;

            if request.days_requested > remaining {
                return Err(HrError::InsufficientLeaveBalance {
                    employee: employee.employee_number,
                    remaining,
                    requested: request.days_requested,
                }
                .into());
            }
        }

        request.status = status.to_string();
        request.approved_by = Some(approver.id);
        request.approved_at = Some(Utc::now());
        request.updated_at = Utc::now();

        self.leave.update(&request).await
    }

    /// Only an undecided request can be withdrawn.
    pub async fn delete(&self, id: Uuid, user: &CurrentUser) -> AppResult<()> {
        // `get` carries the ownership check, so withdrawing somebody else's
        // request is a 404 before the state machine is ever consulted.
        let request = self.get(id, user).await?;

        if request.status != LeaveStatus::PENDING {
            return Err(HrError::InvalidTransition {
                document: "leave request",
                from: request.status,
                to: "deleted".to_string(),
            }
            .into());
        }

        self.leave.delete(id).await
    }
}

pub struct ExpenseUseCases<X: ExpenseReportRepository, E: EmployeeRepository> {
    reports: X,
    employees: E,
    fx: Arc<dyn CurrencyResolver>,
    /// Where approving and reimbursing a report reach the books.
    poster: Arc<dyn DocumentPoster>,
    /// Consulted only to check that a receipt being attached is the caller's own
    /// upload. See [`Self::assert_receipts_are_the_callers`].
    attachments: Arc<dyn AttachmentRepository>,
}

impl<X: ExpenseReportRepository, E: EmployeeRepository> ExpenseUseCases<X, E> {
    pub fn new(
        reports: X,
        employees: E,
        fx: Arc<dyn CurrencyResolver>,
        poster: Arc<dyn DocumentPoster>,
        attachments: Arc<dyn AttachmentRepository>,
    ) -> Self {
        Self { reports, employees, fx, poster, attachments }
    }

    /// Refuses an attachment the caller did not upload.
    ///
    /// Without this the read rule could be turned inside out: a receipt is
    /// readable by whoever may read the claim it hangs off, so attaching a
    /// guessed id to *your own* claim would make somebody else's file readable
    /// to you. The upload endpoint is open to everyone, so the id space is the
    /// only thing between a stranger and a colleague's receipt — and ids are not
    /// a secret worth relying on.
    ///
    /// A missing id and somebody else's id are refused identically, for the same
    /// reason reads answer 404: distinguishing them would confirm which ids
    /// exist.
    async fn assert_receipts_are_the_callers(
        &self,
        lines: &[ExpenseLineRequest],
        user: &CurrentUser,
    ) -> AppResult<()> {
        let ids: Vec<Uuid> = lines.iter().filter_map(|line| line.receipt_attachment_id).collect();
        if ids.is_empty() {
            return Ok(());
        }

        if !self.attachments.ids_not_uploaded_by(&ids, user.id).await?.is_empty() {
            return Err(HrError::ReceiptNotYours.into());
        }

        Ok(())
    }

    /// What the ledger needs from a report, dated the day the decision was made.
    ///
    /// A claim has no document date of its own — what matters to the books is
    /// when it was accepted, not when the taxi was taken.
    /// `actor` rather than anything on the report: the entry is attributed to
    /// whoever made the decision, and `employee_id` is an employee id, not a
    /// user id — the ledger's `created_by` references `users`.
    fn postable(report: &ExpenseReport, on: NaiveDate, actor: &CurrentUser) -> PostableExpenseReport {
        PostableExpenseReport {
            id: report.id,
            org_id: report.org_id,
            number: report.report_number.clone(),
            on,
            base_total: report.base_total_amount,
            created_by: actor.id,
        }
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

    pub async fn create(
        &self,
        req: CreateExpenseReportRequest,
        user: &CurrentUser,
    ) -> AppResult<ExpenseReportDetail> {
        if req.lines.is_empty() {
            return Err(HrError::EmptyExpenseReport.into());
        }
        if req.lines.iter().any(|l| l.amount <= Decimal::ZERO) {
            return Err(HrError::NonPositiveExpense.into());
        }

        assert_may_file_for(&self.employees, user, req.employee_id).await?;
        self.assert_receipts_are_the_callers(&req.lines, user).await?;

        if self.employees.find_by_id(req.employee_id).await?.is_none() {
            return Err(AppError::NotFound(format!("Employee {} not found", req.employee_id)));
        }

        let report_id = Uuid::new_v4();
        let total = total_of(&req.lines);
        let now = Utc::now();
        let currency = self.currency(req.currency.clone(), now.date_naive()).await?;

        let report = ExpenseReport {
            id: report_id,
            org_id: user.org_id,
            employee_id: req.employee_id,
            report_number: self.reports.next_number().await?,
            description: req.description,
            total_amount: total,
            base_total_amount: currency.to_base(total),
            fx_rate: currency.fx_rate,
            currency: currency.code.clone(),
            status: ExpenseStatus::DRAFT.to_string(),
            submitted_at: None,
            approved_by: None,
            approved_at: None,
            created_at: now,
            updated_at: now,
        };

        let lines = build_lines(report_id, &req.lines, &currency);
        let report = self.reports.create(&report, &lines).await?;
        let lines = self.reports.find_lines(report.id).await?;
        Ok(ExpenseReportDetail { report, lines })
    }

    pub async fn get(&self, id: Uuid, user: &CurrentUser) -> AppResult<ExpenseReportDetail> {
        let report = self.require_report(id).await?;

        // Not found rather than forbidden, so this cannot be used to discover
        // that a colleague filed a claim.
        if !HrScope::resolve(&self.employees, user).await?.allows(report.employee_id) {
            return Err(AppError::NotFound(format!("Expense report {} not found", id)));
        }

        let lines = self.reports.find_lines(id).await?;
        Ok(ExpenseReportDetail { report, lines })
    }

    pub async fn list(
        &self,
        filters: &ExpenseFilters,
        params: &PaginationParams,
        user: &CurrentUser,
    ) -> AppResult<(Vec<ExpenseReport>, i64)> {
        match HrScope::resolve(&self.employees, user).await? {
            HrScope::All => self.reports.list(filters, params).await,
            HrScope::Own(employee_id) => {
                let mut scoped = filters.clone();
                scoped.employee_id = Some(employee_id);
                self.reports.list(&scoped, params).await
            }
            HrScope::Nothing => Ok((Vec::new(), 0)),
        }
    }

    pub async fn update(
        &self,
        id: Uuid,
        req: UpdateExpenseReportRequest,
        user: &CurrentUser,
    ) -> AppResult<ExpenseReportDetail> {
        // Editing a colleague's draft was possible until now. Through `get`, so
        // it is a 404 rather than a confirmation the claim exists.
        let mut report = self.get(id, user).await?.report;

        if !ExpenseStatus::is_editable(&report.status) {
            return Err(HrError::ExpenseNotEditable(report.status).into());
        }

        if req.description.is_some() {
            report.description = req.description;
        }
        report.updated_at = Utc::now();

        // Restated at the rate the report already carries: editing a draft's
        // lines must not also revalue it at a rate that has moved since.
        let currency =
            DocumentCurrency { code: report.currency.clone(), fx_rate: report.fx_rate };

        let new_lines = match &req.lines {
            Some(requested) => {
                if requested.is_empty() {
                    return Err(HrError::EmptyExpenseReport.into());
                }
                if requested.iter().any(|l| l.amount <= Decimal::ZERO) {
                    return Err(HrError::NonPositiveExpense.into());
                }
                self.assert_receipts_are_the_callers(requested, user).await?;
                report.total_amount = total_of(requested);
                Some(build_lines(report.id, requested, &currency))
            }
            None => None,
        };

        report.base_total_amount = currency.to_base(report.total_amount);

        let report = self.reports.update(&report, new_lines.as_deref()).await?;
        let lines = self.reports.find_lines(report.id).await?;
        Ok(ExpenseReportDetail { report, lines })
    }

    pub async fn set_status(
        &self,
        id: Uuid,
        status: &str,
        actor: &CurrentUser,
    ) -> AppResult<ExpenseReport> {
        let mut report = self.require_report(id).await?;

        if !ExpenseStatus::can_transition(&report.status, status) {
            return Err(HrError::InvalidTransition {
                document: "expense report",
                from: report.status,
                to: status.to_string(),
            }
            .into());
        }

        match status {
            ExpenseStatus::SUBMITTED => report.submitted_at = Some(Utc::now()),
            s if ExpenseStatus::requires_approver(s) => {
                report.approved_by = Some(actor.id);
                report.approved_at = Some(Utc::now());
            }
            ExpenseStatus::DRAFT => {
                // Reworking a rejected report clears the previous decision.
                report.submitted_at = None;
                report.approved_by = None;
                report.approved_at = None;
            }
            _ => {}
        }

        report.status = status.to_string();
        report.updated_at = Utc::now();

        let updated = self.reports.update(&report, None).await?;

        // Approval is what commits the business to the cost and to owing the
        // employee; reimbursement is what settles it. Neither the draft nor the
        // submitted state is a commitment, and a rejection never was one.
        //
        // No reversal path exists to build: `can_transition` allows no route out
        // of `approved` except `reimbursed`, and `delete` refuses anything past
        // draft — so an approved report can neither be un-approved nor removed.
        let today = Utc::now().date_naive();
        match status {
            ExpenseStatus::APPROVED => {
                self.poster.expense_approved(&Self::postable(&updated, today, actor)).await?
            }
            ExpenseStatus::REIMBURSED => {
                self.poster.expense_reimbursed(&Self::postable(&updated, today, actor)).await?
            }
            _ => {}
        }

        Ok(updated)
    }

    pub async fn delete(&self, id: Uuid, user: &CurrentUser) -> AppResult<()> {
        // Through `get`, so removing a colleague's claim is a 404 before the
        // draft-only rule is even consulted.
        let report = self.get(id, user).await?.report;

        if report.status != ExpenseStatus::DRAFT {
            return Err(HrError::ExpenseNotEditable(report.status).into());
        }

        self.reports.delete(id).await
    }

    async fn require_report(&self, id: Uuid) -> AppResult<ExpenseReport> {
        self.reports
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Expense report {} not found", id)))
    }
}

fn total_of(lines: &[ExpenseLineRequest]) -> Decimal {
    round_money(lines.iter().map(|l| l.amount).sum())
}

/// Lines inherit the report's currency, so they inherit its rate: restating a
/// line at anything else would leave the lines disagreeing with the total they
/// add up to.
fn build_lines(
    report_id: Uuid,
    requested: &[ExpenseLineRequest],
    currency: &DocumentCurrency,
) -> Vec<ExpenseLine> {
    requested
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let amount = round_money(line.amount);
            ExpenseLine {
                id: Uuid::new_v4(),
                expense_report_id: report_id,
                expense_date: line.expense_date,
                category: line.category.clone(),
                description: line.description.clone(),
                amount,
                base_amount: Some(currency.to_base(amount)),
                receipt_url: line.receipt_url.clone(),
                receipt_attachment_id: line.receipt_attachment_id,
                sort_order: index as i32,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn line(amount: Decimal) -> ExpenseLineRequest {
        ExpenseLineRequest {
            expense_date: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            category: "travel".to_string(),
            description: "Taxi".to_string(),
            amount,
            receipt_url: None,
            receipt_attachment_id: None,
        }
    }

    #[test]
    fn report_total_is_the_sum_of_its_lines() {
        let total = total_of(&[line(dec!(12.50)), line(dec!(7.25)), line(dec!(0.30))]);
        assert_eq!(total, dec!(20.05));
    }

    fn base() -> DocumentCurrency {
        DocumentCurrency::base("USD")
    }

    #[test]
    fn lines_are_numbered_in_submission_order() {
        let report_id = Uuid::new_v4();
        let lines = build_lines(report_id, &[line(dec!(10)), line(dec!(20))], &base());

        assert_eq!(lines[0].sort_order, 0);
        assert_eq!(lines[1].sort_order, 1);
        assert!(lines.iter().all(|l| l.expense_report_id == report_id));
    }

    #[test]
    fn line_amounts_are_rounded_to_cents() {
        let lines = build_lines(Uuid::new_v4(), &[line(dec!(9.999))], &base());
        assert_eq!(lines[0].amount, dec!(10.00));
    }

    #[test]
    fn lines_are_restated_at_the_reports_rate() {
        let eur = DocumentCurrency { code: "EUR".into(), fx_rate: dec!(1.10) };
        let lines = build_lines(Uuid::new_v4(), &[line(dec!(50.00))], &eur);

        // The claim stays EUR 50; what the company is out is USD 55.
        assert_eq!(lines[0].amount, dec!(50.00));
        assert_eq!(lines[0].base_amount, Some(dec!(55.00)));
    }
}
