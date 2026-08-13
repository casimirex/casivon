use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::settings::application::dto::UpdateOrganizationRequest;
use crate::modules::settings::domain::entities::{FxRate, OrganizationSettings};

#[async_trait]
pub trait OrganizationRepository: Send + Sync {
    /// The one row. Created by migration, so this never returns nothing on a
    /// migrated database.
    async fn get(&self) -> AppResult<OrganizationSettings>;

    /// Applies only the fields the request carries, leaving the rest alone.
    async fn update(&self, req: &UpdateOrganizationRequest) -> AppResult<OrganizationSettings>;

    /// Whether any financial document has already been raised.
    ///
    /// Every exchange rate is expressed against the base currency, and every
    /// stored base amount was computed with one. Changing the base currency
    /// would therefore invalidate all of them at once — relabelling $1,000 of
    /// receivables as €1,000 and leaving every rate meaning the wrong thing —
    /// which is why it is refused once there is anything to invalidate.
    async fn has_financial_documents(&self) -> AppResult<bool>;
}

#[async_trait]
pub trait FxRateRepository: Send + Sync {
    /// Adds a rate, or corrects the one already on file for that currency and
    /// date. Correcting is an update rather than a second row, so a lookup
    /// never has to break a tie.
    async fn upsert(
        &self,
        currency: &str,
        effective_from: NaiveDate,
        rate: Decimal,
    ) -> AppResult<FxRate>;

    /// Newest first. Filtered to one currency when given.
    async fn list(&self, currency: Option<&str>) -> AppResult<Vec<FxRate>>;

    async fn delete(&self, id: Uuid) -> AppResult<()>;

    /// The rate in force on `on`: the most recent row on or before that date.
    async fn rate_on(&self, currency: &str, on: NaiveDate) -> AppResult<Option<Decimal>>;

    /// Every currency that has at least one rate on file — what a currency
    /// picker can legitimately offer, alongside the base currency itself.
    async fn currencies(&self) -> AppResult<Vec<String>>;

    /// Whether anything already refers to this currency. A rate cannot be
    /// removed while documents depend on it to be restated.
    async fn currency_is_in_use(&self, currency: &str) -> AppResult<bool>;
}
