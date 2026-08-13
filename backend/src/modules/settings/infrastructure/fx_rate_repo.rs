use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::settings::domain::entities::FxRate;
use crate::modules::settings::domain::repositories::FxRateRepository;

#[derive(Clone)]
pub struct PgFxRateRepository {
    pool: PgPool,
}

impl PgFxRateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Every table that carries a currency code, for the in-use check. Kept next to
/// the query it feeds so that adding a currency-bearing table and forgetting to
/// protect its rate is a one-line omission in an obvious place.
const CURRENCY_BEARING_TABLES: [&str; 9] = [
    "opportunities",
    "quotes",
    "sales_orders",
    "invoices",
    "payments",
    "purchase_orders",
    "general_ledger_entries",
    "expense_reports",
    "projects",
];

#[async_trait]
impl FxRateRepository for PgFxRateRepository {
    async fn upsert(
        &self,
        currency: &str,
        effective_from: NaiveDate,
        rate: Decimal,
    ) -> AppResult<FxRate> {
        let row = sqlx::query_as::<_, FxRate>(
            r#"
            INSERT INTO fx_rates (currency, effective_from, rate)
            VALUES ($1, $2, $3)
            ON CONFLICT (currency, effective_from)
                DO UPDATE SET rate = EXCLUDED.rate, updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(currency.to_uppercase())
        .bind(effective_from)
        .bind(rate)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    async fn list(&self, currency: Option<&str>) -> AppResult<Vec<FxRate>> {
        let rows = sqlx::query_as::<_, FxRate>(
            r#"
            SELECT * FROM fx_rates
            WHERE ($1::CHAR(3) IS NULL OR currency = $1)
            ORDER BY currency, effective_from DESC
            "#,
        )
        .bind(currency.map(str::to_uppercase))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM fx_rates WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("Exchange rate not found".into()));
        }

        Ok(())
    }

    async fn rate_on(&self, currency: &str, on: NaiveDate) -> AppResult<Option<Decimal>> {
        // The most recent rate on or before the date: a rate stays in force
        // until superseded, and one entered later must not reach back.
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

    async fn currencies(&self) -> AppResult<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT currency FROM fx_rates ORDER BY currency",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    async fn currency_is_in_use(&self, currency: &str) -> AppResult<bool> {
        let clauses = CURRENCY_BEARING_TABLES
            .iter()
            .map(|table| format!("EXISTS (SELECT 1 FROM {table} WHERE currency = $1)"))
            .collect::<Vec<_>>()
            .join(" OR ");

        // Table names come from the constant above, never from input.
        let in_use = sqlx::query_scalar::<_, bool>(&format!("SELECT {clauses}"))
            .bind(currency.to_uppercase())
            .fetch_one(&self.pool)
            .await?;

        Ok(in_use)
    }
}
