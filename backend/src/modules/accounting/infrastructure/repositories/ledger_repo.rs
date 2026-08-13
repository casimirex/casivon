use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::accounting::domain::entities::GeneralLedgerEntry;
use crate::modules::accounting::domain::repositories::{
    AccountBalance, LedgerFilters, LedgerRepository, PostingRow,
};
use crate::shared::pagination::PaginationParams;

const SORTABLE: [&str; 3] = ["entry_date", "created_at", "amount"];

#[derive(Clone)]
pub struct PgLedgerRepository {
    pool: PgPool,
}

impl PgLedgerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a LedgerFilters) {
    if let Some(account_id) = filters.account_id {
        // An account's ledger shows both the entries it was debited and credited.
        builder
            .push(" AND (debit_account_id = ")
            .push_bind(account_id)
            .push(" OR credit_account_id = ")
            .push_bind(account_id)
            .push(")");
    }
    if let Some(reference_type) = &filters.reference_type {
        builder.push(" AND reference_type = ").push_bind(reference_type);
    }
    if let Some(reference_id) = filters.reference_id {
        builder.push(" AND reference_id = ").push_bind(reference_id);
    }
    if let Some(from) = filters.date_from {
        builder.push(" AND entry_date >= ").push_bind(from);
    }
    if let Some(to) = filters.date_to {
        builder.push(" AND entry_date <= ").push_bind(to);
    }
    if let Some(search) = &filters.search {
        builder.push(" AND description ILIKE ").push_bind(format!("%{}%", search));
    }
}

#[async_trait]
impl LedgerRepository for PgLedgerRepository {
    async fn create(
        &self,
        entry: &GeneralLedgerEntry,
        debit_delta: Decimal,
        credit_delta: Decimal,
        base_debit_delta: Decimal,
        base_credit_delta: Decimal,
    ) -> AppResult<GeneralLedgerEntry> {
        let mut tx = self.pool.begin().await?;

        let created = sqlx::query_as::<_, GeneralLedgerEntry>(
            r#"
            INSERT INTO general_ledger_entries
                (id, org_id, entry_date, reference_type, reference_id, description,
                 debit_account_id, credit_account_id, amount, currency, created_by, created_at,
                 fx_rate, base_amount)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            RETURNING *
            "#,
        )
        .bind(entry.id)
        .bind(entry.org_id)
        .bind(entry.entry_date)
        .bind(&entry.reference_type)
        .bind(entry.reference_id)
        .bind(&entry.description)
        .bind(entry.debit_account_id)
        .bind(entry.credit_account_id)
        .bind(entry.amount)
        .bind(&entry.currency)
        .bind(entry.created_by)
        .bind(entry.created_at)
        .bind(entry.fx_rate)
        .bind(entry.base_amount)
        .fetch_one(&mut *tx)
        .await?;

        // Both sides move inside the same transaction as the entry itself; a
        // half-posted journal would corrupt the ledger permanently.
        adjust(&mut tx, entry.debit_account_id, debit_delta, base_debit_delta).await?;
        adjust(&mut tx, entry.credit_account_id, credit_delta, base_credit_delta).await?;

        tx.commit().await?;
        Ok(created)
    }

    async fn post(&self, rows: &[PostingRow]) -> AppResult<u64> {
        let mut tx = self.pool.begin().await?;
        let mut written = 0;

        for row in rows {
            let entry = &row.entry;

            // ON CONFLICT DO NOTHING against the unique index on `posting_key`.
            // The database decides who was first, inside the insert itself, so
            // two concurrent attempts at the same event cannot both proceed.
            let inserted = sqlx::query(
                r#"
                INSERT INTO general_ledger_entries
                    (id, org_id, entry_date, reference_type, reference_id, description,
                     debit_account_id, credit_account_id, amount, currency, created_by,
                     created_at, fx_rate, base_amount, posting_key)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                ON CONFLICT (posting_key) DO NOTHING
                "#,
            )
            .bind(entry.id)
            .bind(entry.org_id)
            .bind(entry.entry_date)
            .bind(&entry.reference_type)
            .bind(entry.reference_id)
            .bind(&entry.description)
            .bind(entry.debit_account_id)
            .bind(entry.credit_account_id)
            .bind(entry.amount)
            .bind(&entry.currency)
            .bind(entry.created_by)
            .bind(entry.created_at)
            .bind(entry.fx_rate)
            .bind(entry.base_amount)
            .bind(&entry.posting_key)
            .execute(&mut *tx)
            .await?
            .rows_affected();

            // Only a row that was genuinely inserted moves a balance. Adjusting
            // on a skipped row is exactly how a retry would double a balance
            // while the ledger itself still looked correct.
            if inserted == 1 {
                adjust(&mut tx, entry.debit_account_id, row.debit_delta, row.debit_delta).await?;
                adjust(&mut tx, entry.credit_account_id, row.credit_delta, row.credit_delta)
                    .await?;
                written += 1;
            }
        }

        tx.commit().await?;
        Ok(written)
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<GeneralLedgerEntry>> {
        Ok(sqlx::query_as::<_, GeneralLedgerEntry>(
            "SELECT * FROM general_ledger_entries WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn delete(
        &self,
        entry: &GeneralLedgerEntry,
        debit_delta: Decimal,
        credit_delta: Decimal,
        base_debit_delta: Decimal,
        base_credit_delta: Decimal,
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("DELETE FROM general_ledger_entries WHERE id = $1")
            .bind(entry.id)
            .execute(&mut *tx)
            .await?;

        adjust(&mut tx, entry.debit_account_id, debit_delta, base_debit_delta).await?;
        adjust(&mut tx, entry.credit_account_id, credit_delta, base_credit_delta).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn list(
        &self,
        filters: &LedgerFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<GeneralLedgerEntry>, i64)> {
        let mut query: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT * FROM general_ledger_entries WHERE 1 = 1");
        push_filters(&mut query, filters);
        query.push(format!(" {} ", params.order_by(&SORTABLE, "entry_date")));
        query.push(" LIMIT ").push_bind(params.per_page());
        query.push(" OFFSET ").push_bind(params.offset());
        let rows = query.build_query_as::<GeneralLedgerEntry>().fetch_all(&self.pool).await?;

        let mut count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*) FROM general_ledger_entries WHERE 1 = 1");
        push_filters(&mut count, filters);
        let total: i64 = count.build_query_scalar().fetch_one(&self.pool).await?;

        Ok((rows, total))
    }

    async fn balances(
        &self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        account_types: Option<&[&str]>,
    ) -> AppResult<Vec<AccountBalance>> {
        // Sums each account's debit and credit legs over the window, then applies
        // the normal-balance sign rule so `balance` reads naturally per type.
        let types: Option<Vec<String>> =
            account_types.map(|t| t.iter().map(|s| s.to_string()).collect());

        let rows = sqlx::query(
            r#"
            SELECT
                a.id,
                a.account_code,
                a.account_name,
                a.account_type,
                COALESCE(d.total, 0) AS total_debits,
                COALESCE(c.total, 0) AS total_credits,
                CASE WHEN a.account_type IN ('asset', 'expense')
                     THEN COALESCE(d.total, 0) - COALESCE(c.total, 0)
                     ELSE COALESCE(c.total, 0) - COALESCE(d.total, 0)
                END AS balance
            FROM accounts a
            LEFT JOIN (
                SELECT debit_account_id AS id, SUM(base_amount) AS total
                FROM general_ledger_entries
                WHERE ($1::date IS NULL OR entry_date >= $1)
                  AND ($2::date IS NULL OR entry_date <= $2)
                GROUP BY debit_account_id
            ) d ON d.id = a.id
            LEFT JOIN (
                SELECT credit_account_id AS id, SUM(base_amount) AS total
                FROM general_ledger_entries
                WHERE ($1::date IS NULL OR entry_date >= $1)
                  AND ($2::date IS NULL OR entry_date <= $2)
                GROUP BY credit_account_id
            ) c ON c.id = a.id
            WHERE ($3::text[] IS NULL OR a.account_type = ANY($3))
              AND (d.total IS NOT NULL OR c.total IS NOT NULL)
            ORDER BY a.account_code
            "#,
        )
        .bind(from)
        .bind(to)
        .bind(types)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| AccountBalance {
                account_id: row.get("id"),
                account_code: row.get("account_code"),
                account_name: row.get("account_name"),
                account_type: row.get("account_type"),
                total_debits: row.get("total_debits"),
                total_credits: row.get("total_credits"),
                balance: row.get("balance"),
            })
            .collect())
    }
}

/// Moves one account's balance, in its own currency and in base, together.
///
/// The two must move in the same statement: an account whose transaction
/// balance and base balance were written by separate queries could be left
/// disagreeing by a failure between them.
async fn adjust(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    account_id: Uuid,
    delta: Decimal,
    base_delta: Decimal,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE accounts
        SET current_balance = COALESCE(current_balance, 0) + $2,
            base_current_balance = COALESCE(base_current_balance, 0) + $3,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(account_id)
    .bind(delta)
    .bind(base_delta)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
