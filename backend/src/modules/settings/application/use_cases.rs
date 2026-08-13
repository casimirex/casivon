use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::settings::application::dto::{
    AvailableCurrencies, UpdateOrganizationRequest, UpsertFxRateRequest,
};
use crate::modules::settings::domain::entities::{FxRate, OrganizationSettings};
use crate::modules::settings::domain::repositories::{FxRateRepository, OrganizationRepository};
use crate::shared::currency::normalise_requested;

pub struct SettingsUseCases<R: OrganizationRepository> {
    organizations: R,
}

impl<R: OrganizationRepository> SettingsUseCases<R> {
    pub fn new(organizations: R) -> Self {
        Self { organizations }
    }

    pub async fn get_organization(&self) -> AppResult<OrganizationSettings> {
        self.organizations.get().await
    }

    pub async fn update_organization(
        &self,
        req: UpdateOrganizationRequest,
    ) -> AppResult<OrganizationSettings> {
        if let Some(requested) = req.default_currency.as_deref() {
            let current = self.organizations.get().await?;
            let requested = requested.trim().to_uppercase();

            if requested != current.default_currency
                && self.organizations.has_financial_documents().await?
            {
                return Err(AppError::Validation(format!(
                    "default_currency: cannot change from {} to {} — every exchange rate on \
                     file is expressed against {}, and every base amount already stored was \
                     computed with one. Changing it would leave each of those meaning something \
                     different from what it says, without altering a single figure.",
                    current.default_currency, requested, current.default_currency
                )));
            }
        }

        self.organizations.update(&req).await
    }
}

pub struct FxRateUseCases<F: FxRateRepository, O: OrganizationRepository> {
    rates: F,
    organizations: O,
}

impl<F: FxRateRepository, O: OrganizationRepository> FxRateUseCases<F, O> {
    pub fn new(rates: F, organizations: O) -> Self {
        Self { rates, organizations }
    }

    async fn base_code(&self) -> AppResult<String> {
        Ok(self.organizations.get().await?.default_currency)
    }

    pub async fn list(&self, currency: Option<String>) -> AppResult<Vec<FxRate>> {
        let currency = normalise_requested(currency.as_deref())?;
        self.rates.list(currency.as_deref()).await
    }

    pub async fn upsert(&self, req: UpsertFxRateRequest) -> AppResult<FxRate> {
        let Some(currency) = normalise_requested(Some(&req.currency))? else {
            return Err(AppError::Validation("currency: required".into()));
        };

        let base = self.base_code().await?;
        if currency == base {
            // Storing this would be storing an editable 1. Somebody eventually
            // edits it to 0.98 and every amount in the system quietly rescales.
            return Err(AppError::Validation(format!(
                "currency: {base} is this organisation's base currency, so its rate is 1 by \
                 definition and is not stored. Add rates for the currencies you transact in \
                 alongside it."
            )));
        }

        if req.rate <= Decimal::ZERO {
            return Err(AppError::Validation(
                "rate: must be greater than zero — a rate is what an amount is multiplied by."
                    .into(),
            ));
        }

        self.rates.upsert(&currency, req.effective_from, req.rate).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let existing = self
            .rates
            .list(None)
            .await?
            .into_iter()
            .find(|rate| rate.id == id)
            .ok_or_else(|| AppError::NotFound("Exchange rate not found".into()))?;

        // Deleting the last rate for a currency that documents are denominated
        // in would leave those documents unable to be restated — their stored
        // base amounts survive, but nothing new could be raised and any
        // recalculation would fail. Refused while the currency is in use.
        let remaining = self
            .rates
            .list(Some(&existing.currency))
            .await?
            .into_iter()
            .filter(|rate| rate.id != id)
            .count();

        if remaining == 0 && self.rates.currency_is_in_use(&existing.currency).await? {
            return Err(AppError::Validation(format!(
                "This is the only exchange rate on file for {}, and documents are already \
                 denominated in it. Add a replacement rate before removing this one.",
                existing.currency
            )));
        }

        self.rates.delete(id).await
    }

    /// What a currency picker may offer: the base currency, which never needs a
    /// rate, plus everything that has one.
    pub async fn available(&self) -> AppResult<AvailableCurrencies> {
        let base = self.base_code().await?;

        let mut available = self.rates.currencies().await?;
        if !available.contains(&base) {
            available.push(base.clone());
        }
        available.sort();

        Ok(AvailableCurrencies { base, available })
    }
}
