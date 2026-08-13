use async_trait::async_trait;
use utoipa::{IntoParams, ToSchema};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::accounting::domain::entities::*;
use crate::modules::accounting::domain::posting::PostingAccounts;
use crate::shared::pagination::PaginationParams;
use crate::shared::posting::{
    PostableCreditNote, PostableExpenseReport, PostableInvoice, PostablePayment,
    PostableReceipt, PostableReturn,
};

/// One product's stock, valued at its moving average.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow, ToSchema)]
pub struct StockOnHand {
    pub product_id: Uuid,
    pub sku: String,
    pub name: String,
    pub quantity: i64,
    pub average_cost: Option<Decimal>,
    /// `quantity × average_cost`, rounded to cents.
    pub value: Decimal,
}

#[async_trait]
pub trait PostingRepository: Send + Sync {
    /// The configured mapping. Roles that have not been chosen come back `None`.
    async fn get_accounts(&self) -> AppResult<PostingAccounts>;

    /// Replaces the whole mapping.
    ///
    /// Wholesale rather than field by field because the five are one setting:
    /// posting is on when all of them are chosen and off otherwise, so a partial
    /// update has no meaning the caller could have intended.
    async fn update_accounts(&self, accounts: &PostingAccounts) -> AppResult<PostingAccounts>;

    /// Invoices that have been issued but have no entries against them.
    ///
    /// Two populations end up here, and they are handled the same way: documents
    /// raised before automatic posting existed, and documents whose posting did
    /// not complete because the process died between writing the invoice and
    /// writing its entries. Both are simply "owed to the ledger".
    ///
    /// These queries read the sales tables from the accounting module. The
    /// alternative — sales reporting on its own unposted state — would put
    /// knowledge of what the ledger expects into the module that should not have
    /// it, so the coupling is here, where the question is actually asked.
    async fn unposted_invoices(&self) -> AppResult<Vec<PostableInvoice>>;

    /// Credit notes with no entries against them.
    async fn unposted_credit_notes(&self) -> AppResult<Vec<PostableCreditNote>>;

    /// Returns with no entries against them.
    ///
    /// Same two populations as every other unposted query: raised before posting
    /// was configured, or a posting that did not complete.
    async fn unposted_returns(&self) -> AppResult<Vec<PostableReturn>>;

    /// Stock on hand, valued, product by product.
    ///
    /// Only used to open the Inventory account when an installation switches to
    /// perpetual costing: everything on the shelves was expensed on arrival, so
    /// there is an asset to establish and an over-expensing to reverse.
    async fn stock_on_hand(&self) -> AppResult<Vec<StockOnHand>>;

    /// Whether the opening entry has already been made.
    async fn inventory_opening_posted(&self) -> AppResult<bool>;

    /// Payments with no entries against them.
    async fn unposted_payments(&self) -> AppResult<Vec<PostablePayment>>;

    /// Goods receipts with no entries against them.
    async fn unposted_receipts(&self) -> AppResult<Vec<PostableReceipt>>;

    /// Vendor payments with no entries against them.
    async fn unposted_vendor_payments(&self) -> AppResult<Vec<PostablePayment>>;

    /// Expense reports approved but not yet posted as a cost.
    async fn unposted_expense_approvals(&self) -> AppResult<Vec<PostableExpenseReport>>;

    /// Expense reports reimbursed but with no reimbursement entry.
    ///
    /// Separate from approvals because a report posts twice over its life, and
    /// one of the two can be missing without the other — a report approved
    /// before posting existed and reimbursed after it owes both, but only the
    /// approval is missing for one approved after and reimbursed during an
    /// outage.
    async fn unposted_expense_reimbursements(&self) -> AppResult<Vec<PostableExpenseReport>>;
}

/// One automatic entry, ready to write: the row itself plus how each side's
/// balance moves.
///
/// The deltas are computed by the caller with
/// [`AccountType::balance_delta`](crate::modules::accounting::domain::entities::AccountType::balance_delta),
/// the same rule manual entries go through — a posted entry moves a balance the
/// same way whoever created it. Automatic entries are always in the base
/// currency, so the base delta equals the delta and is not carried separately.
#[derive(Debug, Clone)]
pub struct PostingRow {
    pub entry: GeneralLedgerEntry,
    pub debit_delta: Decimal,
    pub credit_delta: Decimal,
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AccountFilters {
    pub account_type: Option<String>,
    pub parent_id: Option<Uuid>,
    pub is_active: Option<bool>,
    pub is_bank_account: Option<bool>,
    pub search: Option<String>,
}

/// One row of the trial balance / report queries.
#[derive(Debug, Clone, serde::Serialize, ToSchema)]
pub struct AccountBalance {
    pub account_id: Uuid,
    pub account_code: String,
    pub account_name: String,
    pub account_type: String,
    pub total_debits: Decimal,
    pub total_credits: Decimal,
    pub balance: Decimal,
}

#[async_trait]
pub trait AccountRepository: Send + Sync {
    async fn create(&self, account: &Account) -> AppResult<Account>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Account>>;
    async fn find_by_code(&self, code: &str) -> AppResult<Option<Account>>;
    async fn update(&self, account: &Account) -> AppResult<Account>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(
        &self,
        filters: &AccountFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Account>, i64)>;
    /// Every account, unpaginated — the chart is a tree the client assembles.
    async fn list_all(&self) -> AppResult<Vec<Account>>;
    async fn count_children(&self, id: Uuid) -> AppResult<i64>;
    async fn count_entries(&self, id: Uuid) -> AppResult<i64>;
    /// Applies a signed delta to `current_balance`.
    async fn adjust_balance(&self, id: Uuid, delta: Decimal) -> AppResult<()>;
    /// Recomputes `current_balance` for every account from the ledger.
    async fn recalculate_balances(&self) -> AppResult<u64>;
}

#[derive(Debug, Default, Clone, serde::Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LedgerFilters {
    pub account_id: Option<Uuid>,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub search: Option<String>,
}

#[async_trait]
pub trait LedgerRepository: Send + Sync {
    /// Writes the entry and moves both account balances in one transaction.
    async fn create(
        &self,
        entry: &GeneralLedgerEntry,
        debit_delta: Decimal,
        credit_delta: Decimal,
        base_debit_delta: Decimal,
        base_credit_delta: Decimal,
    ) -> AppResult<GeneralLedgerEntry>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<GeneralLedgerEntry>>;
    /// Writes automatic entries, skipping any whose posting key is already on
    /// file, and returns how many were actually new.
    ///
    /// The skip is what makes posting safe to retry: the unique index on
    /// `posting_key` decides, inside the same statement that inserts, so two
    /// concurrent attempts at the same event cannot both conclude they are
    /// first. Balances move only for rows that were genuinely inserted —
    /// adjusting on a skipped row is precisely how a retry would double a
    /// balance while the ledger still looked correct.
    async fn post(&self, rows: &[PostingRow]) -> AppResult<u64>;

    /// Reverses an entry by unwinding both balances, then deleting it.
    async fn delete(
        &self,
        entry: &GeneralLedgerEntry,
        debit_delta: Decimal,
        credit_delta: Decimal,
        base_debit_delta: Decimal,
        base_credit_delta: Decimal,
    ) -> AppResult<()>;
    async fn list(
        &self,
        filters: &LedgerFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<GeneralLedgerEntry>, i64)>;
    /// Debit and credit totals per account within a date window.
    async fn balances(
        &self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        account_types: Option<&[&str]>,
    ) -> AppResult<Vec<AccountBalance>>;
}

#[async_trait]
pub trait BankAccountRepository: Send + Sync {
    async fn create(&self, bank_account: &BankAccount) -> AppResult<BankAccount>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<BankAccount>>;
    async fn update(&self, bank_account: &BankAccount) -> AppResult<BankAccount>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(&self, params: &PaginationParams) -> AppResult<(Vec<BankAccount>, i64)>;
}

#[async_trait]
pub trait TaxRateRepository: Send + Sync {
    async fn create(&self, tax_rate: &TaxRate) -> AppResult<TaxRate>;
    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<TaxRate>>;
    async fn update(&self, tax_rate: &TaxRate) -> AppResult<TaxRate>;
    async fn delete(&self, id: Uuid) -> AppResult<()>;
    async fn list(&self, params: &PaginationParams) -> AppResult<(Vec<TaxRate>, i64)>;
}
