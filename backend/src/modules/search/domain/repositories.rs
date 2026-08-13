use async_trait::async_trait;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::AppResult;

/// One thing the search found.
///
/// Carries `kind` and `id` but no URL. Route shapes are the frontend's
/// business, and a backend that emitted `/sales/invoices/{id}` would have to be
/// redeployed to rename a route.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct SearchHit {
    /// `invoice`, `company`, `product` … matching [`SearchKind::as_str`].
    pub kind: String,
    pub id: Uuid,
    /// What identifies the record — a document number, a name.
    pub title: String,
    /// Secondary context: a status, an email, a code.
    pub subtitle: Option<String>,
}

/// Everything the search can return, and what a caller must be to see it.
///
/// One list, so adding a searchable thing means adding a row here and a branch
/// in the SQL — not remembering a permission check in a second place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Contact,
    Company,
    Opportunity,
    Quote,
    Order,
    Invoice,
    Product,
    Warehouse,
    Vendor,
    PurchaseOrder,
    Project,
    Task,
    Account,
    LedgerEntry,
    Employee,
}

impl SearchKind {
    pub const ALL: [SearchKind; 15] = [
        SearchKind::Contact,
        SearchKind::Company,
        SearchKind::Opportunity,
        SearchKind::Quote,
        SearchKind::Order,
        SearchKind::Invoice,
        SearchKind::Product,
        SearchKind::Warehouse,
        SearchKind::Vendor,
        SearchKind::PurchaseOrder,
        SearchKind::Project,
        SearchKind::Task,
        SearchKind::Account,
        SearchKind::LedgerEntry,
        SearchKind::Employee,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SearchKind::Contact => "contact",
            SearchKind::Company => "company",
            SearchKind::Opportunity => "opportunity",
            SearchKind::Quote => "quote",
            SearchKind::Order => "order",
            SearchKind::Invoice => "invoice",
            SearchKind::Product => "product",
            SearchKind::Warehouse => "warehouse",
            SearchKind::Vendor => "vendor",
            SearchKind::PurchaseOrder => "purchase_order",
            SearchKind::Project => "project",
            SearchKind::Task => "task",
            SearchKind::Account => "account",
            SearchKind::LedgerEntry => "ledger_entry",
            SearchKind::Employee => "employee",
        }
    }

    /// The roles that may see this kind, or `None` when any signed-in user may.
    ///
    /// Mirrors the gates on the modules themselves: accounting data is for
    /// accountants, employee records are for HR. A kind the caller fails this
    /// check for is never added to the query, rather than filtered out of the
    /// results afterwards.
    pub fn required_roles(self) -> Option<&'static [&'static str]> {
        match self {
            SearchKind::Account | SearchKind::LedgerEntry => {
                Some(&["accountant", "manager"])
            }
            SearchKind::Employee => Some(&["hr", "manager"]),
            _ => None,
        }
    }
}

#[async_trait]
pub trait SearchRepository: Send + Sync {
    /// Searches `kinds` for `term`, at most `per_kind` hits from each.
    async fn search(
        &self,
        term: &str,
        kinds: &[SearchKind],
        per_kind: i64,
    ) -> AppResult<Vec<SearchHit>>;
}
