use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::accounting::domain::entities::{AccountType, GeneralLedgerEntry};
use crate::modules::accounting::domain::posting::{
    credit_note_entries, expense_approval_entries, expense_reimbursement_entries,
    inventory_opening_entries,
    invoice_entries, movement_entries,
    payment_entries, receipt_entries, return_entries, reversal_entries, vendor_payment_entries,
    AccountMapping,
    InventoryMapping, PlannedEntry, PostingAccounts,
};
use crate::modules::accounting::domain::repositories::{LedgerRepository, PostingRow};
use crate::modules::accounting::infrastructure::repositories::ledger_repo::PgLedgerRepository;
use crate::shared::posting::{
    DocumentPoster, PostableCreditNote, PostableExpenseReport, PostableInvoice,
    PostableMovement, PostableOpening, PostablePayment, PostableReceipt, PostableReturn,
};

/// Posts sales, purchase and expense documents to the general ledger.
///
/// Reads the account mapping per posting rather than caching it: an admin can
/// change which account revenue lands in, and a cached mapping would keep
/// posting to the old one until the process restarted. It is one indexed row
/// plus the accounts it names.
#[derive(Clone)]
pub struct PgDocumentPoster {
    pool: PgPool,
}

impl PgDocumentPoster {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The mapping, the base currency, and the type of each mapped account —
    /// or `None` when posting is not configured.
    async fn context(&self) -> AppResult<Option<PostingContext>> {
        let row = sqlx::query_as::<_, ContextRow>(
            r#"
            SELECT default_currency, ar_account_id, bank_account_id, sales_revenue_account_id,
                   tax_payable_account_id, fx_gain_loss_account_id,
                   accounts_payable_account_id, cost_of_sales_account_id,
                   purchase_tax_account_id, employee_payable_account_id,
                   employee_expense_account_id, inventory_account_id,
                   inventory_adjustment_account_id
            FROM organization_settings WHERE singleton
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let base_currency = row.default_currency;
        let accounts = PostingAccounts {
            ar_account_id: row.ar_account_id,
            bank_account_id: row.bank_account_id,
            sales_revenue_account_id: row.sales_revenue_account_id,
            tax_payable_account_id: row.tax_payable_account_id,
            fx_gain_loss_account_id: row.fx_gain_loss_account_id,
            accounts_payable_account_id: row.accounts_payable_account_id,
            cost_of_sales_account_id: row.cost_of_sales_account_id,
            purchase_tax_account_id: row.purchase_tax_account_id,
            employee_payable_account_id: row.employee_payable_account_id,
            employee_expense_account_id: row.employee_expense_account_id,
            inventory_account_id: row.inventory_account_id,
            inventory_adjustment_account_id: row.inventory_adjustment_account_id,
        };

        // Optional, and separately so: an installation that has not opted into
        // perpetual costing still posts everything else.
        let inventory = InventoryMapping::complete(&accounts);

        let Some(mapping) = AccountMapping::complete(&accounts) else {
            // Posting is off until every role is filled. This is the ordinary
            // state of an installation that has never configured it, not an
            // error — see `014_gl_posting.sql`.
            return Ok(None);
        };

        // Every mapped account, not just the ones this particular posting will
        // touch: the types are looked up once and the context serves all events.
        let mut ids = vec![
            mapping.ar,
            mapping.bank,
            mapping.sales_revenue,
            mapping.tax_payable,
            mapping.fx_gain_loss,
            mapping.accounts_payable,
            mapping.cost_of_sales,
            mapping.purchase_tax,
            mapping.employee_payable,
            mapping.employee_expense,
        ];
        if let Some(inventory) = inventory {
            ids.push(inventory.inventory);
            ids.push(inventory.inventory_adjustment);
        }

        let types: HashMap<Uuid, String> =
            sqlx::query_as::<_, (Uuid, String)>("SELECT id, account_type FROM accounts WHERE id = ANY($1)")
                .bind(&ids[..])
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .collect();

        // The foreign keys are ON DELETE RESTRICT, so a mapped account cannot be
        // deleted out from under this. If one is missing anyway, refusing is the
        // only safe move: posting round a hole would put the books out by
        // whatever that account was meant to carry.
        for id in ids {
            if !types.contains_key(&id) {
                tracing::error!(
                    account_id = %id,
                    "account is mapped for automatic posting but does not exist"
                );
                return Err(AppError::Internal);
            }
        }

        Ok(Some(PostingContext { mapping, inventory, types, base_currency }))
    }

    /// Plans entries against the current mapping and writes them, idempotently.
    ///
    /// `plan` receives the mapping and produces the entries. Structured this way
    /// so the mapping is read once per posting rather than once to plan and
    /// again to write, and so "posting is off" is handled in exactly one place
    /// for all four events.
    ///
    /// Reversals re-derive the original entries from the document rather than
    /// reading them back: the entries are a pure function of the document, so
    /// deriving them again cannot disagree with what was written, and it works
    /// just as well for a document that was never posted in the first place.
    async fn post_with<F>(&self, plan: F) -> AppResult<()>
    where
        F: FnOnce(&AccountMapping, Option<InventoryMapping>) -> Vec<PlannedEntry>,
    {
        let Some(context) = self.context().await? else {
            return Ok(());
        };

        let planned = plan(&context.mapping, context.inventory);
        if planned.is_empty() {
            return Ok(());
        }

        let now = Utc::now();
        let rows: Vec<PostingRow> = planned
            .into_iter()
            .map(|entry| context.row(entry, now))
            .collect::<AppResult<Vec<_>>>()?;

        PgLedgerRepository::new(self.pool.clone()).post(&rows).await?;
        Ok(())
    }
}

struct PostingContext {
    mapping: AccountMapping,
    /// `None` until the installation opts into perpetual costing.
    inventory: Option<InventoryMapping>,
    types: HashMap<Uuid, String>,
    base_currency: String,
}

impl PostingContext {
    fn row(&self, planned: PlannedEntry, now: chrono::DateTime<Utc>) -> AppResult<PostingRow> {
        let account_type = |id: Uuid| -> AppResult<&str> {
            self.types.get(&id).map(String::as_str).ok_or_else(|| {
                tracing::error!(account_id = %id, "posting referenced an account not in the mapping");
                AppError::Internal
            })
        };

        // The same balance rule manual entries go through: an entry moves a
        // balance the same way whoever created it.
        let debit_delta =
            AccountType::balance_delta(account_type(planned.debit_account_id)?, planned.amount, Decimal::ZERO);
        let credit_delta =
            AccountType::balance_delta(account_type(planned.credit_account_id)?, Decimal::ZERO, planned.amount);

        Ok(PostingRow {
            entry: GeneralLedgerEntry {
                id: Uuid::new_v4(),
                org_id: planned.org_id,
                entry_date: planned.entry_date,
                reference_type: Some(planned.reference_type.to_string()),
                reference_id: Some(planned.reference_id),
                description: planned.description,
                debit_account_id: planned.debit_account_id,
                credit_account_id: planned.credit_account_id,
                amount: planned.amount,
                // Automatic postings are always in the base currency: the
                // mapped accounts are base-currency accounts, and an entry has
                // to agree with the accounts it touches.
                currency: self.base_currency.clone(),
                fx_rate: Decimal::ONE,
                base_amount: planned.amount,
                posting_key: Some(planned.posting_key),
                created_by: planned.created_by,
                created_at: now,
            },
            debit_delta,
            credit_delta,
        })
    }
}

/// The mapping and the base currency in one read: the posting rules need both,
/// and they live on the same singleton row.
#[derive(sqlx::FromRow)]
struct ContextRow {
    default_currency: String,
    ar_account_id: Option<Uuid>,
    bank_account_id: Option<Uuid>,
    sales_revenue_account_id: Option<Uuid>,
    tax_payable_account_id: Option<Uuid>,
    fx_gain_loss_account_id: Option<Uuid>,
    accounts_payable_account_id: Option<Uuid>,
    cost_of_sales_account_id: Option<Uuid>,
    purchase_tax_account_id: Option<Uuid>,
    employee_payable_account_id: Option<Uuid>,
    employee_expense_account_id: Option<Uuid>,
    inventory_account_id: Option<Uuid>,
    inventory_adjustment_account_id: Option<Uuid>,
}

/// Reversals are dated when they happen, not when the original was posted:
/// cancelling an invoice in April must not reach back and change March.
fn today() -> NaiveDate {
    Utc::now().date_naive()
}

#[async_trait]
impl DocumentPoster for PgDocumentPoster {
    async fn invoice_issued(&self, invoice: &PostableInvoice) -> AppResult<()> {
        self.post_with(|m, _| invoice_entries(invoice, m)).await
    }

    async fn invoice_cancelled(&self, invoice: &PostableInvoice) -> AppResult<()> {
        self.post_with(|m, _| {
            reversal_entries(invoice_entries(invoice, m), today(), "invoice cancelled")
        })
        .await
    }

    async fn credit_note_issued(&self, note: &PostableCreditNote) -> AppResult<()> {
        self.post_with(|m, inventory| credit_note_entries(note, m, inventory)).await
    }

    async fn payment_received(&self, payment: &PostablePayment) -> AppResult<()> {
        self.post_with(|m, _| payment_entries(payment, m)).await
    }

    async fn payment_reversed(&self, payment: &PostablePayment) -> AppResult<()> {
        self.post_with(|m, _| {
            reversal_entries(payment_entries(payment, m), today(), "payment reversed")
        })
        .await
    }

    async fn goods_received(&self, receipt: &PostableReceipt) -> AppResult<()> {
        self.post_with(|m, inventory| receipt_entries(receipt, m, inventory)).await
    }

    async fn goods_returned(&self, ret: &PostableReturn) -> AppResult<()> {
        self.post_with(|m, inventory| return_entries(ret, m, inventory)).await
    }

    async fn vendor_payment_made(&self, payment: &PostablePayment) -> AppResult<()> {
        self.post_with(|m, _| vendor_payment_entries(payment, m)).await
    }

    async fn vendor_payment_reversed(&self, payment: &PostablePayment) -> AppResult<()> {
        self.post_with(|m, _| {
            reversal_entries(vendor_payment_entries(payment, m), today(), "payment reversed")
        })
        .await
    }

    async fn expense_approved(&self, report: &PostableExpenseReport) -> AppResult<()> {
        self.post_with(|m, _| expense_approval_entries(report, m)).await
    }

    async fn expense_reimbursed(&self, report: &PostableExpenseReport) -> AppResult<()> {
        self.post_with(|m, _| expense_reimbursement_entries(report, m)).await
    }

    async fn expense_reversed(&self, report: &PostableExpenseReport) -> AppResult<()> {
        self.post_with(|m, _| {
            reversal_entries(expense_approval_entries(report, m), today(), "report withdrawn")
        })
        .await
    }

    async fn stock_moved(&self, movement: &PostableMovement) -> AppResult<()> {
        // The only rule that needs the inventory mapping to exist at all. Without
        // it the installation is on periodic costing, where a movement is a
        // quantity and nothing more — the cost was already taken when the goods
        // arrived.
        self.post_with(|m, inventory| match inventory {
            Some(inventory) => movement_entries(movement, m, inventory),
            None => Vec::new(),
        })
        .await
    }

    async fn inventory_opened(&self, opening: &PostableOpening) -> AppResult<bool> {
        let Some(context) = self.context().await? else {
            return Ok(false);
        };
        let Some(inventory) = context.inventory else {
            return Ok(false);
        };

        let planned = inventory_opening_entries(
            opening.value,
            opening.on,
            opening.org_id,
            opening.created_by,
            &context.mapping,
            inventory,
        );
        if planned.is_empty() {
            return Ok(false);
        }

        let now = Utc::now();
        let rows: Vec<PostingRow> = planned
            .into_iter()
            .map(|entry| context.row(entry, now))
            .collect::<AppResult<Vec<_>>>()?;

        // `post()` is keyed and skips rows whose key already exists, so a second
        // call writes nothing and says so rather than doubling the balance.
        let written = PgLedgerRepository::new(self.pool.clone()).post(&rows).await?;
        Ok(written > 0)
    }
}
