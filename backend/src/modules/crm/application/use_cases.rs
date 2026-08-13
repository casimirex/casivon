use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::crm::application::dto::*;
use crate::modules::crm::domain::entities::*;
use crate::modules::crm::domain::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::currency::{CurrencyResolver, DocumentCurrency};
use crate::shared::pagination::PaginationParams;

pub struct ContactUseCases<R: ContactRepository> {
    repo: R,
}

impl<R: ContactRepository> ContactUseCases<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub async fn create(&self, req: CreateContactRequest, user: &CurrentUser) -> AppResult<Contact> {
        let now = Utc::now();
        let contact = Contact {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            first_name: req.first_name,
            last_name: req.last_name,
            email: req.email,
            phone: req.phone,
            mobile: req.mobile,
            address: req.address,
            city: req.city,
            country: req.country,
            job_title: req.job_title,
            company_id: req.company_id,
            status: req.status.unwrap_or_else(|| "lead".to_string()),
            tags: req.tags,
            notes: req.notes,
            // Unassigned contacts default to whoever created them.
            assigned_to: req.assigned_to.or(Some(user.id)),
            created_at: now,
            updated_at: now,
        };
        self.repo.create(&contact).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Contact> {
        self.repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Contact {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &ContactFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Contact>, i64)> {
        self.repo.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateContactRequest) -> AppResult<Contact> {
        let mut contact = self.get(id).await?;

        if let Some(v) = req.first_name {
            contact.first_name = v;
        }
        if let Some(v) = req.last_name {
            contact.last_name = v;
        }
        if req.email.is_some() {
            contact.email = req.email;
        }
        if req.phone.is_some() {
            contact.phone = req.phone;
        }
        if req.mobile.is_some() {
            contact.mobile = req.mobile;
        }
        if req.address.is_some() {
            contact.address = req.address;
        }
        if req.city.is_some() {
            contact.city = req.city;
        }
        if req.country.is_some() {
            contact.country = req.country;
        }
        if req.job_title.is_some() {
            contact.job_title = req.job_title;
        }
        if req.company_id.is_some() {
            contact.company_id = req.company_id;
        }
        if let Some(v) = req.status {
            contact.status = v;
        }
        if req.tags.is_some() {
            contact.tags = req.tags;
        }
        if req.notes.is_some() {
            contact.notes = req.notes;
        }
        if req.assigned_to.is_some() {
            contact.assigned_to = req.assigned_to;
        }
        contact.updated_at = Utc::now();

        self.repo.update(&contact).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.repo.delete(id).await
    }
}

pub struct CompanyUseCases<R: CompanyRepository> {
    repo: R,
}

impl<R: CompanyRepository> CompanyUseCases<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub async fn create(&self, req: CreateCompanyRequest, user: &CurrentUser) -> AppResult<Company> {
        let now = Utc::now();
        let company = Company {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            name: req.name,
            legal_name: req.legal_name,
            tax_id: req.tax_id,
            email: req.email,
            phone: req.phone,
            website: req.website,
            address: req.address,
            city: req.city,
            country: req.country,
            industry: req.industry,
            company_type: req.company_type,
            status: "active".to_string(),
            assigned_to: req.assigned_to.or(Some(user.id)),
            created_at: now,
            updated_at: now,
        };
        self.repo.create(&company).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Company> {
        self.repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Company {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &CompanyFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Company>, i64)> {
        self.repo.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateCompanyRequest) -> AppResult<Company> {
        let mut company = self.get(id).await?;

        if let Some(v) = req.name {
            company.name = v;
        }
        if req.legal_name.is_some() {
            company.legal_name = req.legal_name;
        }
        if req.tax_id.is_some() {
            company.tax_id = req.tax_id;
        }
        if req.email.is_some() {
            company.email = req.email;
        }
        if req.phone.is_some() {
            company.phone = req.phone;
        }
        if req.website.is_some() {
            company.website = req.website;
        }
        if req.address.is_some() {
            company.address = req.address;
        }
        if req.city.is_some() {
            company.city = req.city;
        }
        if req.country.is_some() {
            company.country = req.country;
        }
        if req.industry.is_some() {
            company.industry = req.industry;
        }
        if let Some(v) = req.company_type {
            company.company_type = v;
        }
        if let Some(v) = req.status {
            company.status = v;
        }
        if req.assigned_to.is_some() {
            company.assigned_to = req.assigned_to;
        }
        company.updated_at = Utc::now();

        self.repo.update(&company).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.repo.delete(id).await
    }
}

pub struct OpportunityUseCases<R: OpportunityRepository> {
    repo: R,    fx: Arc<dyn CurrencyResolver>,
}

impl<R: OpportunityRepository> OpportunityUseCases<R> {
    pub fn new(repo: R, fx: Arc<dyn CurrencyResolver>) -> Self {
        Self { repo, fx }
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
        req: CreateOpportunityRequest,
        user: &CurrentUser,
    ) -> AppResult<Opportunity> {
        let now = Utc::now();

        // An opportunity has no document date of its own — it is a forecast, not
        // a posting — so it is restated at the rate in force when it was raised
        // and keeps it, like every other document here.
        let currency = self.currency(req.currency.clone(), now.date_naive()).await?;

        let opp = Opportunity {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            title: req.title,
            company_id: req.company_id,
            contact_id: req.contact_id,
            stage: req.stage.unwrap_or_else(|| "prospecting".to_string()),
            value: req.value,
            base_value: currency.to_base_opt(req.value),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            probability: req.probability,
            expected_close_date: req.expected_close_date,
            description: req.description,
            assigned_to: req.assigned_to.or(Some(user.id)),
            source: req.source,
            created_at: now,
            updated_at: now,
        };
        self.repo.create(&opp).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Opportunity> {
        self.repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Opportunity {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &OpportunityFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Opportunity>, i64)> {
        self.repo.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateOpportunityRequest) -> AppResult<Opportunity> {
        let mut opp = self.get(id).await?;

        if let Some(v) = req.title {
            opp.title = v;
        }
        if let Some(v) = req.company_id {
            opp.company_id = v;
        }
        if req.contact_id.is_some() {
            opp.contact_id = req.contact_id;
        }
        if req.value.is_some() {
            opp.value = req.value;
        }
        if req.probability.is_some() {
            opp.probability = req.probability;
        }
        if req.expected_close_date.is_some() {
            opp.expected_close_date = req.expected_close_date;
        }
        if req.description.is_some() {
            opp.description = req.description;
        }
        if req.source.is_some() {
            opp.source = req.source;
        }
        if let Some(v) = req.stage {
            // Closing an opportunity pins its probability to the outcome.
            opp.probability = match v.as_str() {
                "closed_won" => Some(100),
                "closed_lost" => Some(0),
                _ => opp.probability,
            };
            opp.stage = v;
        }
        if req.assigned_to.is_some() {
            opp.assigned_to = req.assigned_to;
        }

        // Restated at the rate the opportunity already carries rather than
        // today's, so that revising a forecast's value does not silently also
        // revalue it at a different rate.
        opp.base_value = DocumentCurrency {
            code: opp.currency.clone(),
            fx_rate: opp.fx_rate,
        }
        .to_base_opt(opp.value);

        opp.updated_at = Utc::now();

        self.repo.update(&opp).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.repo.delete(id).await
    }

    pub async fn pipeline(&self) -> AppResult<Vec<PipelineStage>> {
        let rows = self.repo.pipeline_by_stage().await?;
        Ok(rows
            .into_iter()
            .map(|(stage, count, value)| PipelineStage { stage, count, value })
            .collect())
    }
}

pub struct ActivityUseCases<R: ActivityRepository> {
    repo: R,
}

impl<R: ActivityRepository> ActivityUseCases<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub async fn create(
        &self,
        req: CreateActivityRequest,
        user: &CurrentUser,
    ) -> AppResult<Activity> {
        let now = Utc::now();
        let activity = Activity {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            activity_type: req.activity_type,
            subject: req.subject,
            description: req.description,
            related_to_type: req.related_to_type,
            related_to_id: req.related_to_id,
            scheduled_at: req.scheduled_at,
            completed_at: None,
            status: "scheduled".to_string(),
            assigned_to: req.assigned_to.or(Some(user.id)),
            created_by: user.id,
            created_at: now,
            updated_at: now,
        };
        self.repo.create(&activity).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Activity> {
        self.repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Activity {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &ActivityFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Activity>, i64)> {
        self.repo.list(filters, params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateActivityRequest) -> AppResult<Activity> {
        let mut activity = self.get(id).await?;

        if let Some(v) = req.subject {
            activity.subject = v;
        }
        if req.description.is_some() {
            activity.description = req.description;
        }
        if req.scheduled_at.is_some() {
            activity.scheduled_at = req.scheduled_at;
        }
        if let Some(v) = req.status {
            // Completing an activity stamps the time; re-opening clears it.
            activity.completed_at = if v == "completed" { Some(Utc::now()) } else { None };
            activity.status = v;
        }
        if req.assigned_to.is_some() {
            activity.assigned_to = req.assigned_to;
        }
        activity.updated_at = Utc::now();

        self.repo.update(&activity).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.repo.delete(id).await
    }
}
