use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Contact {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub first_name: String,
    pub last_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub mobile: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub job_title: Option<String>,
    pub company_id: Option<Uuid>,
    pub status: String, // lead, prospect, customer, supplier
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Company {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub name: String,
    pub legal_name: Option<String>,
    pub tax_id: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub address: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub industry: Option<String>,
    pub company_type: String, // customer, supplier, prospect, partner
    pub status: String,
    pub assigned_to: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Opportunity {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub title: String,
    pub company_id: Uuid,
    pub contact_id: Option<Uuid>,
    pub stage: String, // prospecting, qualification, proposal, negotiation, closed_won, closed_lost
    pub value: Option<rust_decimal::Decimal>,
    pub currency: String,
    pub fx_rate: rust_decimal::Decimal,
    /// `value` restated in the base currency. Pipeline by stage sums this, so
    /// that deals in different currencies can be added at all.
    pub base_value: Option<rust_decimal::Decimal>,
    pub probability: Option<i32>,
    pub expected_close_date: Option<chrono::NaiveDate>,
    pub description: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub source: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Activity {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub activity_type: String, // call, meeting, email, note, task
    pub subject: String,
    pub description: Option<String>,
    pub related_to_type: Option<String>, // contact, company, opportunity, project
    pub related_to_id: Option<Uuid>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: String, // scheduled, completed, cancelled
    pub assigned_to: Option<Uuid>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
