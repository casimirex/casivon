use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::accounting::domain::entities::{BankAccount, TaxRate};
use crate::modules::accounting::domain::repositories::{
    BankAccountRepository, TaxRateRepository,
};
use crate::shared::pagination::PaginationParams;

#[derive(Clone)]
pub struct PgBankAccountRepository {
    pool: PgPool,
}

impl PgBankAccountRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BankAccountRepository for PgBankAccountRepository {
    async fn create(&self, bank_account: &BankAccount) -> AppResult<BankAccount> {
        Ok(sqlx::query_as::<_, BankAccount>(
            r#"
            INSERT INTO bank_accounts
                (id, org_id, account_id, bank_name, account_number, iban, swift, branch,
                 is_active, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            "#,
        )
        .bind(bank_account.id)
        .bind(bank_account.org_id)
        .bind(bank_account.account_id)
        .bind(&bank_account.bank_name)
        .bind(&bank_account.account_number)
        .bind(&bank_account.iban)
        .bind(&bank_account.swift)
        .bind(&bank_account.branch)
        .bind(bank_account.is_active)
        .bind(bank_account.created_at)
        .bind(bank_account.updated_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<BankAccount>> {
        Ok(sqlx::query_as::<_, BankAccount>("SELECT * FROM bank_accounts WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, bank_account: &BankAccount) -> AppResult<BankAccount> {
        Ok(sqlx::query_as::<_, BankAccount>(
            r#"
            UPDATE bank_accounts SET
                bank_name = $2, account_number = $3, iban = $4, swift = $5, branch = $6,
                is_active = $7, updated_at = $8
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(bank_account.id)
        .bind(&bank_account.bank_name)
        .bind(&bank_account.account_number)
        .bind(&bank_account.iban)
        .bind(&bank_account.swift)
        .bind(&bank_account.branch)
        .bind(bank_account.is_active)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM bank_accounts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(&self, params: &PaginationParams) -> AppResult<(Vec<BankAccount>, i64)> {
        let rows = sqlx::query_as::<_, BankAccount>(
            "SELECT * FROM bank_accounts ORDER BY bank_name LIMIT $1 OFFSET $2",
        )
        .bind(params.per_page())
        .bind(params.offset())
        .fetch_all(&self.pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bank_accounts")
            .fetch_one(&self.pool)
            .await?;

        Ok((rows, total))
    }
}

#[derive(Clone)]
pub struct PgTaxRateRepository {
    pool: PgPool,
}

impl PgTaxRateRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TaxRateRepository for PgTaxRateRepository {
    async fn create(&self, tax_rate: &TaxRate) -> AppResult<TaxRate> {
        Ok(sqlx::query_as::<_, TaxRate>(
            r#"
            INSERT INTO tax_rates (id, org_id, name, rate, tax_type, country, is_active, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(tax_rate.id)
        .bind(tax_rate.org_id)
        .bind(&tax_rate.name)
        .bind(tax_rate.rate)
        .bind(&tax_rate.tax_type)
        .bind(&tax_rate.country)
        .bind(tax_rate.is_active)
        .bind(tax_rate.created_at)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<TaxRate>> {
        Ok(sqlx::query_as::<_, TaxRate>("SELECT * FROM tax_rates WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, tax_rate: &TaxRate) -> AppResult<TaxRate> {
        Ok(sqlx::query_as::<_, TaxRate>(
            r#"
            UPDATE tax_rates SET name = $2, rate = $3, tax_type = $4, country = $5, is_active = $6
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(tax_rate.id)
        .bind(&tax_rate.name)
        .bind(tax_rate.rate)
        .bind(&tax_rate.tax_type)
        .bind(&tax_rate.country)
        .bind(tax_rate.is_active)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM tax_rates WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(&self, params: &PaginationParams) -> AppResult<(Vec<TaxRate>, i64)> {
        let rows = sqlx::query_as::<_, TaxRate>(
            "SELECT * FROM tax_rates ORDER BY name LIMIT $1 OFFSET $2",
        )
        .bind(params.per_page())
        .bind(params.offset())
        .fetch_all(&self.pool)
        .await?;

        let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tax_rates")
            .fetch_one(&self.pool)
            .await?;

        Ok((rows, total))
    }
}
