use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

/// A partial update: every field is optional, and an omitted field is left as
/// it was. Sending `""` clears an optional field.
#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
pub struct UpdateOrganizationRequest {
    #[validate(length(min = 1, max = 200, message = "Company name is required"))]
    pub name: Option<String>,
    #[validate(length(max = 200))]
    pub legal_name: Option<String>,
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,
    #[validate(length(max = 50))]
    pub phone: Option<String>,
    #[validate(url(message = "Website must be a valid URL"))]
    pub website: Option<String>,
    #[validate(length(max = 50))]
    pub tax_number: Option<String>,
    #[validate(length(max = 200))]
    pub address_line1: Option<String>,
    #[validate(length(max = 200))]
    pub address_line2: Option<String>,
    #[validate(length(max = 100))]
    pub city: Option<String>,
    #[validate(length(max = 20))]
    pub postal_code: Option<String>,
    #[validate(length(max = 100))]
    pub country: Option<String>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub default_currency: Option<String>,
    /// A warehouse id, or `""` to stop shipping automatically.
    ///
    /// A string rather than a `Uuid` so that clearing it is expressible — the
    /// same convention every other optional field on this form uses, where an
    /// omitted field keeps what is stored and an empty one clears it.
    #[validate(custom = "validate_optional_uuid")]
    pub default_dispatch_warehouse_id: Option<String>,
}

/// Accepts a uuid, or an empty string meaning "clear it".
fn validate_optional_uuid(value: &str) -> Result<(), validator::ValidationError> {
    if value.is_empty() || Uuid::parse_str(value).is_ok() {
        return Ok(());
    }
    let mut error = validator::ValidationError::new("uuid");
    error.message = Some("Must be a warehouse id, or empty to switch dispatch off".into());
    Err(error)
}

/// Adds a rate, or corrects the one already on file for that currency and date.
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpsertFxRateRequest {
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: String,

    /// The date the rate starts applying from. It stays in force until a later
    /// row supersedes it, so back-dating one is how a historical document
    /// becomes restatable.
    pub effective_from: NaiveDate,

    /// Units of base currency per one unit of `currency`.
    #[schema(value_type = String, example = "1.08")]
    pub rate: Decimal,
}

/// Narrows the rate list to one currency.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
pub struct FxRateFilter {
    pub currency: Option<String>,
}

/// What a currency picker may offer.
#[derive(Debug, Serialize, ToSchema)]
pub struct AvailableCurrencies {
    pub base: String,
    /// The base currency plus every currency with a rate on file, sorted. The
    /// base is always present and always needs no rate.
    pub available: Vec<String>,
}
