use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

// ------------------------------------------------------------------ contacts

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateContactRequest {
    #[validate(length(min = 1, max = 100, message = "First name is required"))]
    pub first_name: String,
    #[validate(length(min = 1, max = 100, message = "Last name is required"))]
    pub last_name: String,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    #[validate(length(max = 50))]
    pub phone: Option<String>,
    #[validate(length(max = 50))]
    pub mobile: Option<String>,
    pub address: Option<String>,
    #[validate(length(max = 100))]
    pub city: Option<String>,
    #[validate(length(max = 100))]
    pub country: Option<String>,
    #[validate(length(max = 100))]
    pub job_title: Option<String>,
    pub company_id: Option<Uuid>,
    #[validate(custom = "validate_contact_status")]
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
    pub assigned_to: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateContactRequest {
    #[validate(length(min = 1, max = 100))]
    pub first_name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub last_name: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    #[validate(length(max = 50))]
    pub phone: Option<String>,
    #[validate(length(max = 50))]
    pub mobile: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub job_title: Option<String>,
    pub company_id: Option<Uuid>,
    #[validate(custom = "validate_contact_status")]
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
    pub assigned_to: Option<Uuid>,
}

pub const CONTACT_STATUSES: [&str; 4] = ["lead", "prospect", "customer", "supplier"];

fn validate_contact_status(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &CONTACT_STATUSES, "status")
}

// ----------------------------------------------------------------- companies

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateCompanyRequest {
    #[validate(length(min = 1, max = 255, message = "Company name is required"))]
    pub name: String,
    #[validate(length(max = 255))]
    pub legal_name: Option<String>,
    #[validate(length(max = 100))]
    pub tax_id: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    #[validate(length(max = 50))]
    pub phone: Option<String>,
    #[validate(url(message = "Website must be a valid URL"))]
    pub website: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub industry: Option<String>,
    #[validate(custom = "validate_company_type")]
    pub company_type: String,
    pub assigned_to: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateCompanyRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    pub phone: Option<String>,
    #[validate(url(message = "Website must be a valid URL"))]
    pub website: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub industry: Option<String>,
    #[validate(custom = "validate_company_type")]
    pub company_type: Option<String>,
    pub status: Option<String>,
    pub assigned_to: Option<Uuid>,
}

pub const COMPANY_TYPES: [&str; 4] = ["customer", "supplier", "prospect", "partner"];

fn validate_company_type(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &COMPANY_TYPES, "company_type")
}

// ------------------------------------------------------------- opportunities

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateOpportunityRequest {
    #[validate(length(min = 1, max = 255, message = "Title is required"))]
    pub title: String,
    pub company_id: Uuid,
    pub contact_id: Option<Uuid>,
    pub value: Option<Decimal>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    #[validate(range(min = 0, max = 100, message = "Probability must be between 0 and 100"))]
    pub probability: Option<i32>,
    pub expected_close_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub source: Option<String>,
    #[validate(custom = "validate_stage")]
    pub stage: Option<String>,
    pub assigned_to: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateOpportunityRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: Option<String>,
    pub company_id: Option<Uuid>,
    pub contact_id: Option<Uuid>,
    pub value: Option<Decimal>,
    #[validate(range(min = 0, max = 100, message = "Probability must be between 0 and 100"))]
    pub probability: Option<i32>,
    pub expected_close_date: Option<NaiveDate>,
    pub description: Option<String>,
    pub source: Option<String>,
    #[validate(custom = "validate_stage")]
    pub stage: Option<String>,
    pub assigned_to: Option<Uuid>,
}

pub const OPPORTUNITY_STAGES: [&str; 6] = [
    "prospecting",
    "qualification",
    "proposal",
    "negotiation",
    "closed_won",
    "closed_lost",
];

fn validate_stage(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &OPPORTUNITY_STAGES, "stage")
}

// ---------------------------------------------------------------- activities

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateActivityRequest {
    #[validate(custom = "validate_activity_type")]
    pub activity_type: String,
    #[validate(length(min = 1, max = 255, message = "Subject is required"))]
    pub subject: String,
    pub description: Option<String>,
    pub related_to_type: Option<String>,
    pub related_to_id: Option<Uuid>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub assigned_to: Option<Uuid>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateActivityRequest {
    #[validate(length(min = 1, max = 255))]
    pub subject: Option<String>,
    pub description: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    #[validate(custom = "validate_activity_status")]
    pub status: Option<String>,
    pub assigned_to: Option<Uuid>,
}

pub const ACTIVITY_TYPES: [&str; 5] = ["call", "meeting", "email", "note", "task"];
pub const ACTIVITY_STATUSES: [&str; 3] = ["scheduled", "completed", "cancelled"];

fn validate_activity_type(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &ACTIVITY_TYPES, "activity_type")
}

fn validate_activity_status(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &ACTIVITY_STATUSES, "status")
}

/// Shared "must be one of" check — keeps the allowed sets in one place per field
/// instead of scattering them through the use cases.
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

// ----------------------------------------------------------------- responses

#[derive(Debug, Serialize, ToSchema)]
pub struct PipelineStage {
    pub stage: String,
    pub count: i64,
    pub value: Decimal,
}
