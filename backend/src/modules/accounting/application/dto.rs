use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::modules::accounting::domain::entities::*;
use crate::modules::accounting::domain::posting::PostingAccounts;
use crate::modules::accounting::domain::repositories::{AccountBalance, StockOnHand};

// ----------------------------------------------------------------- accounts

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateAccountRequest {
    #[validate(length(min = 1, max = 50, message = "Account code is required"))]
    pub account_code: String,
    #[validate(length(min = 1, max = 255, message = "Account name is required"))]
    pub account_name: String,
    #[validate(custom = "validate_account_type")]
    pub account_type: String,
    pub parent_id: Option<Uuid>,
    pub is_bank_account: Option<bool>,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    pub opening_balance: Option<Decimal>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateAccountRequest {
    #[validate(length(min = 1, max = 255))]
    pub account_name: Option<String>,
    #[validate(custom = "validate_account_type")]
    pub account_type: Option<String>,
    pub parent_id: Option<Uuid>,
    pub is_bank_account: Option<bool>,
    pub is_active: Option<bool>,
}

fn validate_account_type(value: &str) -> Result<(), validator::ValidationError> {
    one_of(value, &AccountType::ALL, "account_type")
}

// ------------------------------------------------------------------- ledger

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateLedgerEntryRequest {
    pub entry_date: NaiveDate,
    #[validate(length(min = 1, max = 1000, message = "Description is required"))]
    pub description: String,
    pub debit_account_id: Uuid,
    pub credit_account_id: Uuid,
    pub amount: Decimal,
    #[validate(length(min = 3, max = 3, message = "Currency must be a 3-letter code"))]
    pub currency: Option<String>,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
}

// ------------------------------------------------------------ bank accounts

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateBankAccountRequest {
    /// The GL account this bank account settles against.
    pub account_id: Uuid,
    #[validate(length(min = 1, max = 255, message = "Bank name is required"))]
    pub bank_name: String,
    #[validate(length(min = 1, max = 100, message = "Account number is required"))]
    pub account_number: String,
    #[validate(length(max = 50))]
    pub iban: Option<String>,
    #[validate(length(max = 20))]
    pub swift: Option<String>,
    #[validate(length(max = 100))]
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateBankAccountRequest {
    #[validate(length(min = 1, max = 255))]
    pub bank_name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub account_number: Option<String>,
    pub iban: Option<String>,
    pub swift: Option<String>,
    pub branch: Option<String>,
    pub is_active: Option<bool>,
}

// --------------------------------------------------------------- tax rates

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateTaxRateRequest {
    #[validate(length(min = 1, max = 100, message = "Tax rate name is required"))]
    pub name: String,
    /// A whole percentage: 20 means 20%, matching `tax_rate` on document lines.
    pub rate: Decimal,
    #[validate(length(min = 1, max = 50, message = "Tax type is required"))]
    pub tax_type: String,
    #[validate(length(max = 100))]
    pub country: Option<String>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct UpdateTaxRateRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub rate: Option<Decimal>,
    pub tax_type: Option<String>,
    pub country: Option<String>,
    pub is_active: Option<bool>,
}

// ----------------------------------------------------------------- reports

/// Optional reporting window. Omit both for everything on record.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportPeriodQuery {
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

/// An account node with its children, for rendering the chart as a tree.
#[derive(Debug, Serialize, ToSchema)]
pub struct AccountNode {
    #[serde(flatten)]
    pub account: Account,
    /// Sub-accounts, nested to whatever depth the chart uses.
    ///
    /// `no_recursion` stops the schema generator descending forever: the tree
    /// is self-referencing, and without this it expands the type into itself
    /// until the stack runs out.
    #[schema(no_recursion)]
    pub children: Vec<AccountNode>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrialBalanceReport {
    pub rows: Vec<AccountBalance>,
    pub total_debits: Decimal,
    pub total_credits: Decimal,
    /// True when debits equal credits, which they always should.
    pub is_balanced: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProfitAndLossReport {
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub revenue: Vec<AccountBalance>,
    pub expenses: Vec<AccountBalance>,
    pub total_revenue: Decimal,
    pub total_expenses: Decimal,
    pub net_profit: Decimal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BalanceSheetReport {
    pub as_of: Option<NaiveDate>,
    pub assets: Vec<AccountBalance>,
    pub liabilities: Vec<AccountBalance>,
    pub equity: Vec<AccountBalance>,
    pub total_assets: Decimal,
    pub total_liabilities: Decimal,
    pub total_equity: Decimal,
    /// Retained profit for the period, folded into equity for the balance check.
    pub retained_earnings: Decimal,
    pub is_balanced: bool,
}

/// Replaces the whole posting mapping. A role sent as `null` is unmapped, which
/// switches automatic posting off until it is chosen again.
#[derive(Debug, Default, Deserialize, Validate, ToSchema)]
pub struct UpdatePostingAccountsRequest {
    pub ar_account_id: Option<Uuid>,
    pub bank_account_id: Option<Uuid>,
    pub sales_revenue_account_id: Option<Uuid>,
    pub tax_payable_account_id: Option<Uuid>,
    pub fx_gain_loss_account_id: Option<Uuid>,
    pub accounts_payable_account_id: Option<Uuid>,
    pub cost_of_sales_account_id: Option<Uuid>,
    pub purchase_tax_account_id: Option<Uuid>,
    pub employee_payable_account_id: Option<Uuid>,
    pub employee_expense_account_id: Option<Uuid>,
    pub inventory_account_id: Option<Uuid>,
    pub inventory_adjustment_account_id: Option<Uuid>,
}

/// The mapping plus whether it is enough to post with.
#[derive(Debug, Serialize, ToSchema)]
pub struct PostingConfiguration {
    pub accounts: PostingAccounts,
    /// True when every *required* role is filled. While false, sales documents
    /// post nothing at all — which is how an installation that never configured
    /// posting keeps behaving exactly as it did before.
    pub posting_enabled: bool,
    /// The required roles still to be chosen, ready to show on screen.
    pub missing_roles: Vec<String>,
    /// True once the inventory pair is mapped as well: stock is an asset on the
    /// balance sheet and becomes a cost when it leaves. False means goods are a
    /// cost the day they arrive, which is where this application started.
    pub perpetual_inventory: bool,
}

/// What switching to perpetual costing would put on the balance sheet.
///
/// A preview rather than an action, because the figure deserves a look before it
/// is posted: see the caveat on `assumes_everything_was_received`.
#[derive(Debug, Serialize, ToSchema)]
pub struct InventoryOpeningReport {
    /// False while the inventory accounts are unmapped, in which case there is
    /// nothing to open and posting would have nowhere to go.
    pub perpetual_inventory: bool,
    /// True once the opening entry exists. Posting again writes nothing.
    pub already_posted: bool,
    pub total_value: Decimal,
    pub lines: Vec<StockOnHand>,
    /// The honest caveat, carried to the screen rather than buried in a comment.
    ///
    /// The entry credits Cost of sales because goods received under periodic
    /// costing were expensed there on arrival, so this reverses an
    /// over-expensing. Stock that arrived through a hand-made movement was never
    /// posted at all, and the credit for that part has nothing behind it.
    pub assumes_everything_was_received: &'static str,
}

/// A document the ledger is owed an entry for.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnpostedDocument {
    /// Matches the entry's `reference_type`: `sales_invoice`, `sales_payment`,
    /// `goods_receipt`, `vendor_payment` or `expense_report`.
    pub kind: String,
    pub id: Uuid,
    /// The invoice number, for both kinds — it is what identifies either one on
    /// screen.
    pub reference: String,
    pub date: NaiveDate,
    pub base_amount: Decimal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnpostedReport {
    pub posting_enabled: bool,
    /// Empty is the healthy state. Anything here is either older than automatic
    /// posting or was interrupted between the document write and its entries.
    pub documents: Vec<UnpostedDocument>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PostingRunReport {
    pub invoices_posted: usize,
    pub payments_posted: usize,
    pub receipts_posted: usize,
    pub vendor_payments_posted: usize,
    /// Approvals and reimbursements together — a report posts twice over its
    /// life, and either half can be the one outstanding.
    pub expense_reports_posted: usize,
}

fn one_of(
    value: &str,
    allowed: &[&str],
    code: &'static str,
) -> Result<(), validator::ValidationError> {
    if allowed.contains(&value) {
        return Ok(());
    }
    let mut err = validator::ValidationError::new(code);
    err.message = Some(format!("Must be one of: {}", allowed.join(", ")).into());
    Err(err)
}
