use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::error::AppResult;
use crate::shared::currency::CurrencyResolver;

/// Reads the organisation's base currency and the rates entered against it.
///
/// Queried per use rather than cached at start-up: an admin can change either
/// under Settings, and a cached value would keep stamping a stale rate onto new
/// documents until the process restarted. Both are single indexed lookups.
#[derive(Clone)]
pub struct PgCurrencyResolver {
    pool: PgPool,
}

impl PgCurrencyResolver {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CurrencyResolver for PgCurrencyResolver {
    async fn base_code(&self) -> AppResult<String> {
        let code = sqlx::query_scalar::<_, String>(
            "SELECT default_currency FROM organization_settings WHERE singleton",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(code)
    }

    async fn rate_on(&self, currency: &str, on: NaiveDate) -> AppResult<Option<Decimal>> {
        let rate = sqlx::query_scalar::<_, Decimal>(
            r#"
            SELECT rate FROM fx_rates
            WHERE currency = $1 AND effective_from <= $2
            ORDER BY effective_from DESC
            LIMIT 1
            "#,
        )
        .bind(currency.to_uppercase())
        .bind(on)
        .fetch_optional(&self.pool)
        .await?;

        Ok(rate)
    }
}
