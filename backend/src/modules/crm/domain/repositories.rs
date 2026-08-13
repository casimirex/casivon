use async_trait::async_trait;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::crm::domain::entities::*;
use crate::shared::pagination::PaginationParams;

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ContactFilters {
    pub company_id: Option<Uuid>,
    pub status: Option<String>,
    pub assigned_to: Option<Uuid>,
    /// Matches first name, last name or email.
    pub search: Option<String>,
}

#[async_trait]
pub trait ContactRepository: Send + Sync {
    async fn create(&self, contact: &Contact) -> AppResult<Contact>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Contact>>;
    async fn update(&self, contact: &Contact) -> AppResult<Contact>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &ContactFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Contact>, i64)>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CompanyFilters {
    pub company_type: Option<String>,
    pub status: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub search: Option<String>,
}

#[async_trait]
pub trait CompanyRepository: Send + Sync {
    async fn create(&self, company: &Company) -> AppResult<Company>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Company>>;
    async fn update(&self, company: &Company) -> AppResult<Company>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &CompanyFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Company>, i64)>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OpportunityFilters {
    pub stage: Option<String>,
    pub company_id: Option<Uuid>,
    pub assigned_to: Option<Uuid>,
    pub search: Option<String>,
}

#[async_trait]
pub trait OpportunityRepository: Send + Sync {
    async fn create(&self, opp: &Opportunity) -> AppResult<Opportunity>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Opportunity>>;
    async fn update(&self, opp: &Opportunity) -> AppResult<Opportunity>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &OpportunityFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Opportunity>, i64)>;
    /// Open pipeline value grouped by stage, for the CRM dashboard.
    async fn pipeline_by_stage(&self) -> AppResult<Vec<(String, i64, rust_decimal::Decimal)>>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ActivityFilters {
    pub related_to_type: Option<String>,
    pub related_to_id: Option<Uuid>,
    pub status: Option<String>,
    pub assigned_to: Option<Uuid>,
}

#[async_trait]
pub trait ActivityRepository: Send + Sync {
    async fn create(&self, activity: &Activity) -> AppResult<Activity>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Activity>>;
    async fn update(&self, activity: &Activity) -> AppResult<Activity>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &ActivityFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Activity>, i64)>;
}
