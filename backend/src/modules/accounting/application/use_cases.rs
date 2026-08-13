use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::modules::accounting::application::dto::*;
use crate::modules::accounting::domain::entities::*;
use crate::modules::accounting::domain::errors::AccountingError;
use crate::modules::accounting::domain::posting::{
    PostingAccounts, PostingRole, EXPENSE_REFERENCE, INVOICE_REFERENCE, PAYMENT_REFERENCE,
    POSTING_ROLES,
    CREDIT_NOTE_REFERENCE, RECEIPT_REFERENCE, RETURN_REFERENCE, VENDOR_PAYMENT_REFERENCE,
};
use crate::modules::accounting::domain::repositories::*;
use crate::shared::auth::CurrentUser;
use crate::shared::currency::{CurrencyResolver, DocumentCurrency};
use crate::shared::posting::{
    DocumentPoster, PostableCreditNote, PostableExpenseReport, PostableInvoice,
    PostableOpening, PostablePayment, PostableReceipt, PostableReturn,
};
use crate::shared::money::{round_money, to_base};
use crate::shared::pagination::PaginationParams;

pub struct AccountUseCases<A: AccountRepository> {
    accounts: A,    fx: Arc<dyn CurrencyResolver>,
}

impl<A: AccountRepository> AccountUseCases<A> {
    pub fn new(accounts: A, fx: Arc<dyn CurrencyResolver>) -> Self {
        Self { accounts, fx }
    }

    /// The currency a document is raised in, together with the rate frozen onto
    /// it. Read at the point of use rather than cached, so a change under
    /// Settings applies to the next document raised.
    ///
    /// `on` is the document's own date: the rate that applied when it was
    /// raised is the rate it keeps.
    async fn currency(
        &self,
        requested: Option<String>,
        on: NaiveDate,
    ) -> AppResult<DocumentCurrency> {
        self.fx.resolve(requested.as_deref(), on).await
    }

    pub async fn create(&self, req: CreateAccountRequest, user: &CurrentUser) -> AppResult<Account> {
        if !AccountType::is_valid(&req.account_type) {
            return Err(AccountingError::UnknownAccountType(req.account_type).into());
        }

        if self.accounts.find_by_code(&req.account_code).await?.is_some() {
            return Err(AccountingError::DuplicateAccountCode(req.account_code).into());
        }

        if let Some(parent_id) = req.parent_id {
            if self.accounts.find_by_id(parent_id).await?.is_none() {
                return Err(AppError::NotFound(format!("Parent account {} not found", parent_id)));
            }
        }

        let opening = req.opening_balance.unwrap_or(Decimal::ZERO);
        let now = Utc::now();

        // An account has no date of its own, so its opening balance is restated
        // at the rate in force when the account was opened, and keeps it.
        let currency = self.currency(req.currency.clone(), now.date_naive()).await?;
        let base_opening = currency.to_base(opening);

        let account = Account {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            account_code: req.account_code,
            account_name: req.account_name,
            account_type: req.account_type,
            parent_id: req.parent_id,
            is_bank_account: req.is_bank_account.unwrap_or(false),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            opening_balance: Some(opening),
            // A new account starts at its opening balance; postings move it from there.
            current_balance: Some(opening),
            base_opening_balance: Some(base_opening),
            base_current_balance: Some(base_opening),
            is_active: true,
            created_at: now,
            updated_at: now,
        };

        self.accounts.create(&account).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Account> {
        self.accounts
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Account {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &AccountFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<Account>, i64)> {
        self.accounts.list(filters, params).await
    }

    /// The chart of accounts as a tree, roots first.
    pub async fn tree(&self) -> AppResult<Vec<AccountNode>> {
        let accounts = self.accounts.list_all().await?;
        Ok(build_tree(accounts, None))
    }

    pub async fn update(&self, id: Uuid, req: UpdateAccountRequest) -> AppResult<Account> {
        let mut account = self.get(id).await?;

        if let Some(v) = req.account_name {
            account.account_name = v;
        }
        if let Some(v) = req.account_type {
            if !AccountType::is_valid(&v) {
                return Err(AccountingError::UnknownAccountType(v).into());
            }
            account.account_type = v;
        }
        if let Some(parent_id) = req.parent_id {
            if parent_id == id {
                return Err(AccountingError::SelfParent.into());
            }
            self.assert_no_cycle(id, parent_id).await?;
            account.parent_id = Some(parent_id);
        }
        if let Some(v) = req.is_bank_account {
            account.is_bank_account = v;
        }
        if let Some(v) = req.is_active {
            account.is_active = v;
        }
        account.updated_at = Utc::now();

        self.accounts.update(&account).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let account = self.get(id).await?;

        // Deleting a posted-to account would silently unbalance the ledger.
        if self.accounts.count_entries(id).await? > 0 {
            return Err(AccountingError::AccountHasEntries(account.account_code).into());
        }
        if self.accounts.count_children(id).await? > 0 {
            return Err(AccountingError::AccountHasChildren(account.account_code).into());
        }

        self.accounts.delete(id).await
    }

    pub async fn recalculate_balances(&self) -> AppResult<u64> {
        self.accounts.recalculate_balances().await
    }

    /// Walks up from `parent_id`; if we meet `id` again the move would loop.
    async fn assert_no_cycle(&self, id: Uuid, parent_id: Uuid) -> AppResult<()> {
        let mut cursor = Some(parent_id);
        let mut hops = 0;

        while let Some(current) = cursor {
            if current == id {
                let account = self.get(id).await?;
                let parent = self.get(parent_id).await?;
                return Err(AccountingError::CircularHierarchy(
                    account.account_code,
                    parent.account_code,
                )
                .into());
            }

            // Belt and braces: stop even if the stored data is already cyclic.
            hops += 1;
            if hops > 64 {
                break;
            }

            cursor = self.accounts.find_by_id(current).await?.and_then(|a| a.parent_id);
        }

        Ok(())
    }
}

/// Groups a flat account list into parent/child nodes.
fn build_tree(accounts: Vec<Account>, parent_id: Option<Uuid>) -> Vec<AccountNode> {
    let (children, rest): (Vec<Account>, Vec<Account>) =
        accounts.into_iter().partition(|a| a.parent_id == parent_id);

    children
        .into_iter()
        .map(|account| {
            let id = account.id;
            AccountNode { account, children: build_tree(rest.clone(), Some(id)) }
        })
        .collect()
}

pub struct LedgerUseCases<L: LedgerRepository, A: AccountRepository> {
    ledger: L,
    accounts: A,    fx: Arc<dyn CurrencyResolver>,
}

impl<L: LedgerRepository, A: AccountRepository> LedgerUseCases<L, A> {
    pub fn new(ledger: L, accounts: A, fx: Arc<dyn CurrencyResolver>) -> Self {
        Self { ledger, accounts, fx }
    }

    /// The currency a document is raised in, together with the rate frozen onto
    /// it. Read at the point of use rather than cached, so a change under
    /// Settings applies to the next document raised.
    ///
    /// `on` is the document's own date: the rate that applied when it was
    /// raised is the rate it keeps.
    async fn currency(
        &self,
        requested: Option<String>,
        on: NaiveDate,
    ) -> AppResult<DocumentCurrency> {
        self.fx.resolve(requested.as_deref(), on).await
    }

    /// Posts one balanced journal entry.
    pub async fn create(
        &self,
        req: CreateLedgerEntryRequest,
        user: &CurrentUser,
    ) -> AppResult<GeneralLedgerEntry> {
        if req.debit_account_id == req.credit_account_id {
            return Err(AccountingError::SameDebitAndCreditAccount.into());
        }
        if req.amount <= Decimal::ZERO {
            return Err(AccountingError::NonPositiveAmount.into());
        }

        let debit_account = self.require_account(req.debit_account_id).await?;
        let credit_account = self.require_account(req.credit_account_id).await?;

        for account in [&debit_account, &credit_account] {
            if !account.is_active {
                return Err(
                    AccountingError::InactiveAccount(account.account_code.clone()).into()
                );
            }
        }

        let amount = round_money(req.amount);
        let currency = self.currency(req.currency.clone(), req.entry_date).await?;

        // An entry moves two account balances, and each account holds its balance
        // in its own currency. Posting a EUR amount into a USD account would add
        // two numbers that are not the same kind of thing, so the entry has to
        // agree with both sides rather than being converted into them — which
        // account's rate would it even use?
        for account in [&debit_account, &credit_account] {
            if account.currency != currency.code {
                return Err(AppError::Validation(format!(
                    "currency: this entry is in {}, but account {} is denominated in {}. A \
                     journal entry has to be posted in the currency of the accounts it touches.",
                    currency.code, account.account_code, account.currency
                )));
            }
        }

        let entry = GeneralLedgerEntry {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            entry_date: req.entry_date,
            reference_type: req.reference_type,
            reference_id: req.reference_id,
            description: req.description,
            debit_account_id: req.debit_account_id,
            credit_account_id: req.credit_account_id,
            amount,
            base_amount: currency.to_base(amount),
            fx_rate: currency.fx_rate,
            currency: currency.code,
            // A person is writing this in the journal form. Only automatic
            // postings carry a key, and that absence is what lets this entry be
            // deleted directly rather than having to be reversed.
            posting_key: None,
            created_by: user.id,
            created_at: Utc::now(),
        };

        // Each side moves by its own normal-balance rule.
        let debit_delta =
            AccountType::balance_delta(&debit_account.account_type, amount, Decimal::ZERO);
        let credit_delta =
            AccountType::balance_delta(&credit_account.account_type, Decimal::ZERO, amount);

        self.ledger
            .create(
                &entry,
                debit_delta,
                credit_delta,
                to_base(debit_delta, entry.fx_rate),
                to_base(credit_delta, entry.fx_rate),
            )
            .await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<GeneralLedgerEntry> {
        self.ledger
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Ledger entry {} not found", id)))
    }

    pub async fn list(
        &self,
        filters: &LedgerFilters,
        params: &PaginationParams,
    ) -> AppResult<(Vec<GeneralLedgerEntry>, i64)> {
        self.ledger.list(filters, params).await
    }

    /// Removes an entry and unwinds the balances it moved.
    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let entry = self.get(id).await?;

        // An entry a document posted belongs to that document. Deleting it here
        // would leave the invoice believing it is posted while the books say
        // otherwise, and the posting key would keep a repost from ever putting
        // it back. Cancelling the document is the way to undo it, and that posts
        // a mirror instead of erasing anything.
        if entry.posting_key.is_some() {
            let what = entry
                .reference_type
                .as_deref()
                .map(|reference| reference.replace('_', " "))
                .unwrap_or_else(|| "the document that posted it".to_string());
            return Err(AccountingError::PostedEntryNotDeletable(what).into());
        }
        let debit_account = self.require_account(entry.debit_account_id).await?;
        let credit_account = self.require_account(entry.credit_account_id).await?;

        // The negation of what posting the entry applied.
        let debit_delta =
            -AccountType::balance_delta(&debit_account.account_type, entry.amount, Decimal::ZERO);
        let credit_delta =
            -AccountType::balance_delta(&credit_account.account_type, Decimal::ZERO, entry.amount);

        // Unwound at the entry's own frozen rate, so removing a posting restores
        // the balance to exactly what it was before — not to what today's rate
        // would make of it.
        self.ledger
            .delete(
                &entry,
                debit_delta,
                credit_delta,
                to_base(debit_delta, entry.fx_rate),
                to_base(credit_delta, entry.fx_rate),
            )
            .await
    }

    pub async fn trial_balance(&self, query: &ReportPeriodQuery) -> AppResult<TrialBalanceReport> {
        validate_period(query)?;
        let rows = self.ledger.balances(query.date_from, query.date_to, None).await?;

        let total_debits = round_money(rows.iter().map(|r| r.total_debits).sum());
        let total_credits = round_money(rows.iter().map(|r| r.total_credits).sum());

        Ok(TrialBalanceReport {
            rows,
            total_debits,
            total_credits,
            is_balanced: total_debits == total_credits,
        })
    }

    pub async fn profit_and_loss(
        &self,
        query: &ReportPeriodQuery,
    ) -> AppResult<ProfitAndLossReport> {
        validate_period(query)?;
        let rows = self
            .ledger
            .balances(
                query.date_from,
                query.date_to,
                Some(&[AccountType::REVENUE, AccountType::EXPENSE]),
            )
            .await?;

        let (revenue, expenses): (Vec<_>, Vec<_>) =
            rows.into_iter().partition(|r| r.account_type == AccountType::REVENUE);

        let total_revenue = round_money(revenue.iter().map(|r| r.balance).sum());
        let total_expenses = round_money(expenses.iter().map(|r| r.balance).sum());

        Ok(ProfitAndLossReport {
            date_from: query.date_from,
            date_to: query.date_to,
            revenue,
            expenses,
            total_revenue,
            total_expenses,
            net_profit: round_money(total_revenue - total_expenses),
        })
    }

    pub async fn balance_sheet(&self, query: &ReportPeriodQuery) -> AppResult<BalanceSheetReport> {
        validate_period(query)?;

        // A balance sheet is cumulative: everything up to `date_to`.
        let rows = self
            .ledger
            .balances(
                None,
                query.date_to,
                Some(&[AccountType::ASSET, AccountType::LIABILITY, AccountType::EQUITY]),
            )
            .await?;

        let mut assets = Vec::new();
        let mut liabilities = Vec::new();
        let mut equity = Vec::new();
        for row in rows {
            match row.account_type.as_str() {
                AccountType::ASSET => assets.push(row),
                AccountType::LIABILITY => liabilities.push(row),
                _ => equity.push(row),
            }
        }

        // Profit for the period is not posted to equity until year end, so fold
        // it in here or the sheet will not balance.
        let pnl = self
            .ledger
            .balances(None, query.date_to, Some(&[AccountType::REVENUE, AccountType::EXPENSE]))
            .await?;
        let retained_earnings = round_money(
            pnl.iter()
                .map(|r| {
                    if r.account_type == AccountType::REVENUE {
                        r.balance
                    } else {
                        -r.balance
                    }
                })
                .sum(),
        );

        let total_assets = round_money(assets.iter().map(|r| r.balance).sum());
        let total_liabilities = round_money(liabilities.iter().map(|r| r.balance).sum());
        let total_equity = round_money(equity.iter().map(|r| r.balance).sum());

        Ok(BalanceSheetReport {
            as_of: query.date_to,
            assets,
            liabilities,
            equity,
            total_assets,
            total_liabilities,
            total_equity,
            retained_earnings,
            is_balanced: total_assets == round_money(
                total_liabilities + total_equity + retained_earnings,
            ),
        })
    }

    async fn require_account(&self, id: Uuid) -> AppResult<Account> {
        self.accounts
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Account {} not found", id)))
    }
}

fn validate_period(query: &ReportPeriodQuery) -> AppResult<()> {
    if let (Some(from), Some(to)) = (query.date_from, query.date_to) {
        if from > to {
            return Err(AccountingError::InvalidPeriod.into());
        }
    }
    Ok(())
}

pub struct BankAccountUseCases<B: BankAccountRepository, A: AccountRepository> {
    bank_accounts: B,
    accounts: A,
}

impl<B: BankAccountRepository, A: AccountRepository> BankAccountUseCases<B, A> {
    pub fn new(bank_accounts: B, accounts: A) -> Self {
        Self { bank_accounts, accounts }
    }

    pub async fn create(
        &self,
        req: CreateBankAccountRequest,
        user: &CurrentUser,
    ) -> AppResult<BankAccount> {
        if self.accounts.find_by_id(req.account_id).await?.is_none() {
            return Err(AppError::NotFound(format!(
                "Ledger account {} not found",
                req.account_id
            )));
        }

        let now = Utc::now();
        let bank_account = BankAccount {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            account_id: req.account_id,
            bank_name: req.bank_name,
            account_number: req.account_number,
            iban: req.iban,
            swift: req.swift,
            branch: req.branch,
            is_active: true,
            created_at: now,
            updated_at: now,
        };

        self.bank_accounts.create(&bank_account).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<BankAccount> {
        self.bank_accounts
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Bank account {} not found", id)))
    }

    pub async fn list(
        &self,
        params: &PaginationParams,
    ) -> AppResult<(Vec<BankAccount>, i64)> {
        self.bank_accounts.list(params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateBankAccountRequest) -> AppResult<BankAccount> {
        let mut bank_account = self.get(id).await?;

        if let Some(v) = req.bank_name {
            bank_account.bank_name = v;
        }
        if let Some(v) = req.account_number {
            bank_account.account_number = v;
        }
        if req.iban.is_some() {
            bank_account.iban = req.iban;
        }
        if req.swift.is_some() {
            bank_account.swift = req.swift;
        }
        if req.branch.is_some() {
            bank_account.branch = req.branch;
        }
        if let Some(v) = req.is_active {
            bank_account.is_active = v;
        }
        bank_account.updated_at = Utc::now();

        self.bank_accounts.update(&bank_account).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.bank_accounts.delete(id).await
    }
}

pub struct TaxRateUseCases<T: TaxRateRepository> {
    tax_rates: T,
}

impl<T: TaxRateRepository> TaxRateUseCases<T> {
    pub fn new(tax_rates: T) -> Self {
        Self { tax_rates }
    }

    pub async fn create(&self, req: CreateTaxRateRequest, user: &CurrentUser) -> AppResult<TaxRate> {
        assert_rate_in_range(req.rate)?;

        let tax_rate = TaxRate {
            id: Uuid::new_v4(),
            org_id: user.org_id,
            name: req.name,
            rate: req.rate,
            tax_type: req.tax_type,
            country: req.country,
            is_active: true,
            created_at: Utc::now(),
        };

        self.tax_rates.create(&tax_rate).await
    }

    pub async fn get(&self, id: Uuid) -> AppResult<TaxRate> {
        self.tax_rates
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Tax rate {} not found", id)))
    }

    pub async fn list(&self, params: &PaginationParams) -> AppResult<(Vec<TaxRate>, i64)> {
        self.tax_rates.list(params).await
    }

    pub async fn update(&self, id: Uuid, req: UpdateTaxRateRequest) -> AppResult<TaxRate> {
        let mut tax_rate = self.get(id).await?;

        if let Some(v) = req.name {
            tax_rate.name = v;
        }
        if let Some(v) = req.rate {
            assert_rate_in_range(v)?;
            tax_rate.rate = v;
        }
        if let Some(v) = req.tax_type {
            tax_rate.tax_type = v;
        }
        if req.country.is_some() {
            tax_rate.country = req.country;
        }
        if let Some(v) = req.is_active {
            tax_rate.is_active = v;
        }

        self.tax_rates.update(&tax_rate).await
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        self.get(id).await?;
        self.tax_rates.delete(id).await
    }
}

/// Tax rates are whole percentages here and on every document line: 20 means
/// 20%. The upper bound is what stops a fraction being typed by mistake — 0.2
/// is accepted as a rate, but it means a fifth of a percent, so the bound
/// cannot catch that. The field description carries its weight instead.
fn assert_rate_in_range(rate: Decimal) -> AppResult<()> {
    if rate < Decimal::ZERO || rate > Decimal::from(100) {
        return Err(AccountingError::TaxRateOutOfRange.into());
    }
    Ok(())
}

// --------------------------------------------------------- posting mapping

pub struct PostingUseCases<P: PostingRepository, A: AccountRepository> {
    mapping: P,
    accounts: A,
    fx: Arc<dyn CurrencyResolver>,
    poster: Arc<dyn DocumentPoster>,
}

impl<P: PostingRepository, A: AccountRepository> PostingUseCases<P, A> {
    pub fn new(
        mapping: P,
        accounts: A,
        fx: Arc<dyn CurrencyResolver>,
        poster: Arc<dyn DocumentPoster>,
    ) -> Self {
        Self { mapping, accounts, fx, poster }
    }

    pub async fn get(&self) -> AppResult<PostingConfiguration> {
        Ok(configuration(self.mapping.get_accounts().await?))
    }

    /// What the ledger is owed: documents that have been issued or settled but
    /// have no entries against them.
    pub async fn unposted(&self) -> AppResult<UnpostedReport> {
        let outstanding = self.outstanding().await?;

        let payment_rows = |kind: &'static str, payments: &[PostablePayment]| {
            payments
                .iter()
                .map(|payment| UnpostedDocument {
                    kind: kind.to_string(),
                    id: payment.id,
                    reference: payment.document_number.clone(),
                    date: payment.payment_date,
                    base_amount: payment.base_amount,
                })
                .collect::<Vec<_>>()
        };

        let expense_rows = |kind: &'static str, reports: &[PostableExpenseReport]| {
            reports
                .iter()
                .map(|report| UnpostedDocument {
                    kind: kind.to_string(),
                    id: report.id,
                    reference: report.number.clone(),
                    date: report.on,
                    base_amount: report.base_total,
                })
                .collect::<Vec<_>>()
        };

        Ok(UnpostedReport {
            posting_enabled: self.mapping.get_accounts().await?.is_complete(),
            documents: outstanding
                .invoices
                .iter()
                .map(|invoice| UnpostedDocument {
                    kind: INVOICE_REFERENCE.to_string(),
                    id: invoice.id,
                    reference: invoice.number.clone(),
                    date: invoice.issue_date,
                    base_amount: invoice.base_total,
                })
                .chain(outstanding.receipts.iter().map(|receipt| UnpostedDocument {
                    kind: RECEIPT_REFERENCE.to_string(),
                    id: receipt.id,
                    reference: receipt.number.clone(),
                    date: receipt.receipt_date,
                    base_amount: to_base(
                        receipt.stocked_net + receipt.expensed_net + receipt.tax,
                        receipt.fx_rate,
                    ),
                }))
                .chain(outstanding.returns.iter().map(|ret| UnpostedDocument {
                    kind: RETURN_REFERENCE.to_string(),
                    id: ret.id,
                    reference: ret.number.clone(),
                    date: ret.return_date,
                    base_amount: to_base(
                        ret.stocked_net + ret.expensed_net + ret.tax,
                        ret.fx_rate,
                    ),
                }))
                .chain(outstanding.credit_notes.iter().map(|note| UnpostedDocument {
                    kind: CREDIT_NOTE_REFERENCE.to_string(),
                    id: note.id,
                    reference: note.number.clone(),
                    date: note.issue_date,
                    base_amount: note.base_total,
                }))
                .chain(payment_rows(PAYMENT_REFERENCE, &outstanding.payments))
                .chain(payment_rows(VENDOR_PAYMENT_REFERENCE, &outstanding.vendor_payments))
                .chain(expense_rows(EXPENSE_REFERENCE, &outstanding.expense_approvals))
                .chain(expense_rows(
                    EXPENSE_REFERENCE,
                    &outstanding.expense_reimbursements,
                ))
                .collect(),
        })
    }

    /// Everything the ledger is owed, in one read.
    async fn outstanding(&self) -> AppResult<Outstanding> {
        Ok(Outstanding {
            invoices: self.mapping.unposted_invoices().await?,
            receipts: self.mapping.unposted_receipts().await?,
            returns: self.mapping.unposted_returns().await?,
            credit_notes: self.mapping.unposted_credit_notes().await?,
            payments: self.mapping.unposted_payments().await?,
            vendor_payments: self.mapping.unposted_vendor_payments().await?,
            expense_approvals: self.mapping.unposted_expense_approvals().await?,
            expense_reimbursements: self.mapping.unposted_expense_reimbursements().await?,
        })
    }

    /// What switching to perpetual costing would put on the balance sheet.
    ///
    /// A preview, because the figure is worth a look first — it assumes the
    /// stock on the shelves got there through goods receipts that were posted,
    /// which is true of anything received through a purchase order and not true
    /// of stock typed in as an adjustment.
    pub async fn inventory_opening(&self) -> AppResult<InventoryOpeningReport> {
        let accounts = self.mapping.get_accounts().await?;
        let lines = self.mapping.stock_on_hand().await?;

        Ok(InventoryOpeningReport {
            perpetual_inventory: accounts.is_perpetual(),
            already_posted: self.mapping.inventory_opening_posted().await?,
            total_value: round_money(lines.iter().map(|line| line.value).sum()),
            lines,
            assumes_everything_was_received:
                "Credits Cost of sales, where goods received under periodic costing were \
                 already expensed. Stock that arrived as a hand-made adjustment was never \
                 posted, so that part of the credit has nothing behind it.",
        })
    }

    /// Posts the opening entry. Writes nothing the second time it is called.
    pub async fn post_inventory_opening(
        &self,
        user: &CurrentUser,
    ) -> AppResult<InventoryOpeningReport> {
        let report = self.inventory_opening().await?;

        if !report.perpetual_inventory {
            return Err(AppError::Validation(
                "Perpetual inventory is not configured. Choose the Inventory and Inventory \
                 adjustment accounts before opening the balance."
                    .into(),
            ));
        }

        self.poster
            .inventory_opened(&PostableOpening {
                org_id: user.org_id,
                value: report.total_value,
                // Dated today rather than backdated to when the stock arrived:
                // the entry records the day the books changed method, and
                // backdating it would restate a closed period.
                on: Utc::now().date_naive(),
                created_by: user.id,
            })
            .await?;

        // Re-read so the caller sees `already_posted` as it now stands rather
        // than as it was before the write.
        self.inventory_opening().await
    }

    /// Posts everything outstanding. Safe to run repeatedly: each entry carries
    /// a posting key, so a document already posted by a concurrent run is
    /// skipped by the database rather than doubled.
    ///
    /// Does nothing at all while the mapping is incomplete — there would be
    /// nowhere to post to, and reporting success would be a lie.
    pub async fn post_unposted(&self) -> AppResult<PostingRunReport> {
        if !self.mapping.get_accounts().await?.is_complete() {
            return Err(AppError::Validation(
                "Automatic posting is not configured. Choose the posting accounts before \
                 posting outstanding documents."
                    .into(),
            ));
        }

        let outstanding = self.outstanding().await?;

        // What creates a balance is posted before what clears it: a payment
        // credits the receivable its invoice created, and an expense
        // reimbursement clears what its approval owed. Posting them the other
        // way round would leave a control account transiently negative for
        // anyone reading the ledger mid-run.
        for invoice in &outstanding.invoices {
            self.poster.invoice_issued(invoice).await?;
        }
        for receipt in &outstanding.receipts {
            self.poster.goods_received(receipt).await?;
        }
        // After the receipts, so a return never lands before the delivery it
        // reverses — the same ordering rule payments follow.
        for ret in &outstanding.returns {
            self.poster.goods_returned(ret).await?;
        }
        // After the invoices they credit, so a credit never lands before the
        // receivable it relieves.
        for note in &outstanding.credit_notes {
            self.poster.credit_note_issued(note).await?;
        }
        for report in &outstanding.expense_approvals {
            self.poster.expense_approved(report).await?;
        }

        for payment in &outstanding.payments {
            self.poster.payment_received(payment).await?;
        }
        for payment in &outstanding.vendor_payments {
            self.poster.vendor_payment_made(payment).await?;
        }
        for report in &outstanding.expense_reimbursements {
            self.poster.expense_reimbursed(report).await?;
        }

        Ok(PostingRunReport {
            invoices_posted: outstanding.invoices.len(),
            payments_posted: outstanding.payments.len(),
            receipts_posted: outstanding.receipts.len(),
            vendor_payments_posted: outstanding.vendor_payments.len(),
            expense_reports_posted: outstanding.expense_approvals.len()
                + outstanding.expense_reimbursements.len(),
        })
    }

    /// Replaces the mapping, refusing anything that would post nonsense.
    ///
    /// Validated here rather than by a database constraint because every rule
    /// needs to read the referenced account — its type, whether it is active,
    /// what currency it is in — which a CHECK cannot do.
    pub async fn update(
        &self,
        req: UpdatePostingAccountsRequest,
    ) -> AppResult<PostingConfiguration> {
        let requested = PostingAccounts {
            ar_account_id: req.ar_account_id,
            bank_account_id: req.bank_account_id,
            sales_revenue_account_id: req.sales_revenue_account_id,
            tax_payable_account_id: req.tax_payable_account_id,
            fx_gain_loss_account_id: req.fx_gain_loss_account_id,
            accounts_payable_account_id: req.accounts_payable_account_id,
            cost_of_sales_account_id: req.cost_of_sales_account_id,
            purchase_tax_account_id: req.purchase_tax_account_id,
            employee_payable_account_id: req.employee_payable_account_id,
            employee_expense_account_id: req.employee_expense_account_id,
            inventory_account_id: req.inventory_account_id,
            inventory_adjustment_account_id: req.inventory_adjustment_account_id,
        };

        let base = self.fx.base_code().await?;

        for (PostingRole { name: role, account_type: expected_type, .. }, chosen) in
            POSTING_ROLES.iter().zip(requested.in_role_order())
        {
            // Unmapping a role is allowed — it just switches posting off. Only
            // the roles actually being set are checked.
            let Some(id) = chosen else {
                continue;
            };

            let account = self
                .accounts
                .find_by_id(id)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("Account {id} not found")))?;

            if !account.is_active {
                return Err(AccountingError::InactiveAccount(account.account_code).into());
            }

            if account.account_type != *expected_type {
                return Err(AccountingError::WrongAccountTypeForRole {
                    role,
                    expected: expected_type,
                    code: account.account_code,
                    actual: account.account_type,
                }
                .into());
            }

            // Automatic postings are made in the base currency, and an entry has
            // to agree with the accounts it touches — the rule journal entries
            // already follow. A foreign-currency account mapped here would make
            // every automatic posting fail at the moment an invoice was sent,
            // which is far too late to find out.
            if account.currency != base {
                return Err(AccountingError::PostingAccountNotInBaseCurrency {
                    role,
                    code: account.account_code,
                    currency: account.currency,
                    base,
                }
                .into());
            }
        }

        Ok(configuration(self.mapping.update_accounts(&requested).await?))
    }
}

/// Everything the ledger is owed, gathered once so the report and the repair
/// run agree on what is outstanding.
struct Outstanding {
    invoices: Vec<PostableInvoice>,
    receipts: Vec<PostableReceipt>,
    returns: Vec<PostableReturn>,
    credit_notes: Vec<PostableCreditNote>,
    payments: Vec<PostablePayment>,
    vendor_payments: Vec<PostablePayment>,
    expense_approvals: Vec<PostableExpenseReport>,
    expense_reimbursements: Vec<PostableExpenseReport>,
}

fn configuration(accounts: PostingAccounts) -> PostingConfiguration {
    let missing = accounts.missing_roles();
    PostingConfiguration {
        posting_enabled: missing.is_empty(),
        missing_roles: missing.into_iter().map(str::to_string).collect(),
        perpetual_inventory: accounts.is_perpetual(),
        accounts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn account(code: &str, parent_id: Option<Uuid>) -> Account {
        Account {
            id: Uuid::new_v4(),
            org_id: None,
            account_code: code.to_string(),
            account_name: code.to_string(),
            account_type: AccountType::ASSET.to_string(),
            parent_id,
            is_bank_account: false,
            currency: "USD".to_string(),
            fx_rate: Decimal::ONE,
            opening_balance: Some(Decimal::ZERO),
            current_balance: Some(Decimal::ZERO),
            base_opening_balance: Some(Decimal::ZERO),
            base_current_balance: Some(Decimal::ZERO),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn tree_nests_children_under_their_parent() {
        let root = account("1000", None);
        let child = account("1100", Some(root.id));
        let grandchild = account("1110", Some(child.id));
        let other_root = account("2000", None);

        let tree = build_tree(
            vec![grandchild.clone(), root.clone(), other_root.clone(), child.clone()],
            None,
        );

        assert_eq!(tree.len(), 2, "two roots");
        let root_node = tree.iter().find(|n| n.account.account_code == "1000").unwrap();
        assert_eq!(root_node.children.len(), 1);
        assert_eq!(root_node.children[0].account.account_code, "1100");
        assert_eq!(root_node.children[0].children[0].account.account_code, "1110");
    }

    #[test]
    fn rate_is_a_whole_percentage() {
        assert!(assert_rate_in_range(dec!(20)).is_ok());
        assert!(assert_rate_in_range(dec!(0)).is_ok());
        assert!(assert_rate_in_range(dec!(100)).is_ok());
        assert!(assert_rate_in_range(dec!(17.5)).is_ok());
        // Beyond 100% is a typo, not a tax.
        assert!(assert_rate_in_range(dec!(2000)).is_err());
        assert!(assert_rate_in_range(dec!(-0.1)).is_err());
    }

    #[test]
    fn period_must_not_run_backwards() {
        let bad = ReportPeriodQuery {
            date_from: Some(chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()),
            date_to: Some(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
        };
        assert!(validate_period(&bad).is_err());

        let open_ended = ReportPeriodQuery { date_from: None, date_to: None };
        assert!(validate_period(&open_ended).is_ok());
    }
}
