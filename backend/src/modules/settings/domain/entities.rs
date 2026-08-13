use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

/// The company this installation belongs to — the details that appear on a
/// quote or an invoice.
///
/// Exactly one row exists. See `011_organization_settings.sql` for why this is
/// a singleton rather than a tenant table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct OrganizationSettings {
    pub id: Uuid,
    pub name: String,
    pub legal_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub tax_number: Option<String>,
    pub address_line1: Option<String>,
    pub address_line2: Option<String>,
    pub city: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    pub default_currency: String,
    /// Which account plays which role when a sales document posts itself.
    ///
    /// All five together are what switches automatic posting on: an
    /// installation with any of them unset posts nothing and behaves exactly as
    /// it did before posting existed. See `014_gl_posting.sql` and
    /// [`crate::modules::accounting::domain::posting::AccountMapping`].
    pub ar_account_id: Option<Uuid>,
    pub bank_account_id: Option<Uuid>,
    pub sales_revenue_account_id: Option<Uuid>,
    pub tax_payable_account_id: Option<Uuid>,
    pub fx_gain_loss_account_id: Option<Uuid>,
    /// Where goods ship from when an invoice is issued.
    ///
    /// `None` means invoicing moves no stock at all, which is how this
    /// application behaved before automatic dispatch existed. Setting it also
    /// turns on a refusal: an invoice the shelf cannot cover stops being
    /// issuable. See `022_default_dispatch_warehouse.sql`.
    pub default_dispatch_warehouse_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One exchange rate, in force from `effective_from` until a later row for the
/// same currency supersedes it.
///
/// `rate` is units of the organisation's base currency per one unit of
/// `currency`, so restating an amount is a multiplication. See
/// `013_multi_currency.sql`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct FxRate {
    pub id: Uuid,
    pub currency: String,
    pub effective_from: NaiveDate,
    pub rate: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
