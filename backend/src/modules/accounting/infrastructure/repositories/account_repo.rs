use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::accounting::domain::entities::Account;
use crate::modules::accounting::domain::repositories::{AccountFilters, AccountRepository};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 4] = ["account_code", "account_name", "created_at", "current_balance"];

#[derive(Clone)]
pub struct PgAccountRepository {
    pool: PgPool,
}

impl PgAccountRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a AccountFilters) {
    if let Some(account_type) = &filters.account_type {
        builder.push(" AND account_type = ").push_bind(account_type);
    }
    if let Some(parent_id) = filters.parent_id {
        builder.push(" AND parent_id = ").push_bind(parent_id);
    }
    if let Some(is_active) = filters.is_active {
        builder.push(" AND is_active = ").push_bind(is_active);
    }
    if let Some(is_bank_account) = filters.is_bank_account {
        builder.push(" AND is_bank_account = ").push_bind(is_bank_account);
    }
    if let Some(search) = &filters.search {
        let pattern = format!("%{}%", search);
        builder
            .push(" AND (account_code ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR account_name ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

#[async_trait]
impl AccountRepository for PgAccountRepository {
    async fn create(&self, account: &Account) -> AppResult<Account> {
        Ok(sqlx::query_as::<_, Account>(
            r#"
            INSERT INTO accounts
                (id, org_id, account_code, account_name, account_type, parent_id,
                 is_bank_account, currency, opening_balance, current_balance, is_active,
                 created_at, updated_at, fx_rate, base_opening_balance, base_current_balance)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            RETURNING *
            "#,
        )
        .bind(account.id)
        .bind(account.org_id)
        .bind(&account.account_code)
        .bind(&account.account_name)
        .bind(&account.account_type)
        .bind(account.parent_id)
        .bind(account.is_bank_account)
        .bind(&account.currency)
        .bind(account.opening_balance)
        .bind(account.current_balance)
        .bind(account.is_active)
        .bind(account.created_at)
        .bind(account.updated_at)
        .bind(account.fx_rate)
        .bind(account.base_opening_balance)
        .bind(account.base_current_balance)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Account>> {
        Ok(sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn find_by_code(&self, code: &str) -> AppResult<Option<Account>> {
        Ok(sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE account_code = $1")
            .bind(code)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update(&self, account: &Account) -> AppResult<Account> {
        Ok(sqlx::query_as::<_, Account>(
            r#"
            UPDATE accounts SET
                account_name = $2, account_type = $3, parent_id = $4, is_bank_account = $5,
                is_active = $6, updated_at = $7
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(account.id)
        .bind(&account.account_name)
        .bind(&account.account_type)
        .bind(account.parent_id)
        .bind(account.is_bank_account)
        .bind(account.is_active)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?)
    }

    async fn delete(&self, id: Uuid) -> AppResult<()> {
        sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &AccountFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Account>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM accounts WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "account_code")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<Account>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM accounts WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn list_all(&self) -> AppResult<Vec<Account>> {
        Ok(sqlx::query_as::<_, Account>("SELECT * FROM accounts ORDER BY account_code")
            .fetch_all(&self.pool)
            .await?)
    }

    async fn count_children(&self, id: Uuid) -> AppResult<i64> {
        Ok(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM accounts WHERE parent_id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    async fn count_entries(&self, id: Uuid) -> AppResult<i64> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM general_ledger_entries
            WHERE debit_account_id = $1 OR credit_account_id = $1
            "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn adjust_balance(&self, id: Uuid, delta: Decimal) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE accounts
            SET current_balance = COALESCE(current_balance, 0) + $2, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(delta)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn recalculate_balances(&self) -> AppResult<u64> {
        // Rebuilds every balance from opening balance + the ledger, applying the
        // debit/credit sign rule per account type in SQL.
        //
        // Both balances are rebuilt in one pass, from the same rows: the
        // transaction balance from `amount` and the base balance from
        // `base_amount`. Recomputing one without the other is what would let
        // them drift apart, and this is the procedure that exists to stop drift.
        let result = sqlx::query(
            r#"
            UPDATE accounts a
            SET current_balance = COALESCE(a.opening_balance, 0) + COALESCE(movement.delta, 0),
                base_current_balance =
                    COALESCE(a.base_opening_balance, 0) + COALESCE(movement.base_delta, 0),
                updated_at = NOW()
            FROM (
                SELECT
                    acc.id,
                    CASE WHEN acc.account_type IN ('asset', 'expense')
                         THEN COALESCE(debits.total, 0) - COALESCE(credits.total, 0)
                         ELSE COALESCE(credits.total, 0) - COALESCE(debits.total, 0)
                    END AS delta,
                    CASE WHEN acc.account_type IN ('asset', 'expense')
                         THEN COALESCE(debits.base_total, 0) - COALESCE(credits.base_total, 0)
                         ELSE COALESCE(credits.base_total, 0) - COALESCE(debits.base_total, 0)
                    END AS base_delta
                FROM accounts acc
                LEFT JOIN (
                    SELECT debit_account_id AS id, SUM(amount) AS total,
                           SUM(base_amount) AS base_total
                    FROM general_ledger_entries GROUP BY debit_account_id
                ) debits ON debits.id = acc.id
                LEFT JOIN (
                    SELECT credit_account_id AS id, SUM(amount) AS total,
                           SUM(base_amount) AS base_total
                    FROM general_ledger_entries GROUP BY credit_account_id
                ) credits ON credits.id = acc.id
            ) AS movement
            WHERE a.id = movement.id
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
