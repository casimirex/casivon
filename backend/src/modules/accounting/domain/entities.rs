use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Account {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub account_code: String,
    pub account_name: String,
    pub account_type: String, // asset, liability, equity, revenue, expense
    pub parent_id: Option<Uuid>,
    pub is_bank_account: bool,
    pub currency: String,
    pub opening_balance: Option<Decimal>,
    pub current_balance: Option<Decimal>,
    pub fx_rate: Decimal,
    /// The balances restated in the base currency, so that a trial balance over
    /// accounts denominated in different currencies adds up to something.
    pub base_opening_balance: Option<Decimal>,
    pub base_current_balance: Option<Decimal>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Every entry is one balanced pair: `amount` is debited from one account and
/// credited to another, so the ledger can never go out of balance by construction.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct GeneralLedgerEntry {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub entry_date: NaiveDate,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub description: String,
    pub debit_account_id: Uuid,
    pub credit_account_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub fx_rate: Decimal,
    /// `amount` restated in the base currency. The trial balance and every
    /// account balance are sums over this column.
    pub base_amount: Decimal,
    /// Names the business event that posted this entry, e.g.
    /// `sales_invoice:{uuid}:revenue`. `None` means a person wrote it in the
    /// manual journal form — which is also what decides whether it may be
    /// deleted directly or has to be reversed. Unique, so an event cannot post
    /// twice; see `014_gl_posting.sql`.
    pub posting_key: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct BankAccount {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub account_id: Uuid,
    pub bank_name: String,
    pub account_number: String,
    pub iban: Option<String>,
    pub swift: Option<String>,
    pub branch: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TaxRate {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub name: String,
    /// A whole percentage: 20.00 means 20%.
    pub rate: Decimal,
    pub tax_type: String,
    pub country: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

pub struct AccountType;

impl AccountType {
    pub const ASSET: &'static str = "asset";
    pub const LIABILITY: &'static str = "liability";
    pub const EQUITY: &'static str = "equity";
    pub const REVENUE: &'static str = "revenue";
    pub const EXPENSE: &'static str = "expense";

    pub const ALL: [&'static str; 5] =
        [Self::ASSET, Self::LIABILITY, Self::EQUITY, Self::REVENUE, Self::EXPENSE];

    pub fn is_valid(value: &str) -> bool {
        Self::ALL.contains(&value)
    }

    /// Assets and expenses increase on the debit side; everything else increases
    /// on the credit side. This single rule drives every balance calculation.
    pub fn is_debit_normal(account_type: &str) -> bool {
        matches!(account_type, Self::ASSET | Self::EXPENSE)
    }

    /// Signed change to an account's balance given what it was debited/credited.
    pub fn balance_delta(account_type: &str, debited: Decimal, credited: Decimal) -> Decimal {
        if Self::is_debit_normal(account_type) {
            debited - credited
        } else {
            credited - debited
        }
    }

    /// Revenue and expense accounts appear on the P&L; the rest on the balance sheet.
    pub fn is_profit_and_loss(account_type: &str) -> bool {
        matches!(account_type, Self::REVENUE | Self::EXPENSE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn asset_and_expense_are_debit_normal() {
        assert!(AccountType::is_debit_normal(AccountType::ASSET));
        assert!(AccountType::is_debit_normal(AccountType::EXPENSE));
        assert!(!AccountType::is_debit_normal(AccountType::LIABILITY));
        assert!(!AccountType::is_debit_normal(AccountType::REVENUE));
        assert!(!AccountType::is_debit_normal(AccountType::EQUITY));
    }

    #[test]
    fn debiting_an_asset_increases_it() {
        assert_eq!(
            AccountType::balance_delta(AccountType::ASSET, dec!(100), dec!(0)),
            dec!(100)
        );
        assert_eq!(
            AccountType::balance_delta(AccountType::ASSET, dec!(0), dec!(40)),
            dec!(-40)
        );
    }

    #[test]
    fn crediting_revenue_increases_it() {
        assert_eq!(
            AccountType::balance_delta(AccountType::REVENUE, dec!(0), dec!(250)),
            dec!(250)
        );
        assert_eq!(
            AccountType::balance_delta(AccountType::LIABILITY, dec!(75), dec!(0)),
            dec!(-75)
        );
    }

    #[test]
    fn profit_and_loss_accounts_are_revenue_and_expense() {
        assert!(AccountType::is_profit_and_loss(AccountType::REVENUE));
        assert!(AccountType::is_profit_and_loss(AccountType::EXPENSE));
        assert!(!AccountType::is_profit_and_loss(AccountType::ASSET));
    }
}
