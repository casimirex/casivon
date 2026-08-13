use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::error::AppResult;
use crate::modules::search::domain::repositories::{SearchHit, SearchKind, SearchRepository};

#[derive(Clone)]
pub struct PgSearchRepository {
    pool: PgPool,
}

impl PgSearchRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// What one kind contributes to the query.
///
/// `columns` are matched with `ILIKE`, the same mechanism every module's own
/// list filter uses — global search finds a record on the same terms the list
/// screen for it would, which is the point.
struct Branch {
    table: &'static str,
    /// SQL producing the label. Concatenations are cast because `||` over
    /// `varchar` yields `text` in some arms and not others, and a `UNION` needs
    /// every branch to agree on its column types.
    title: &'static str,
    subtitle: &'static str,
    columns: &'static [&'static str],
    /// Newest first within a kind, since recency is the best proxy for
    /// relevance without a ranking function.
    order_by: &'static str,
}

fn branch(kind: SearchKind) -> Branch {
    match kind {
        SearchKind::Contact => Branch {
            table: "contacts",
            title: "(first_name || ' ' || last_name)::text",
            subtitle: "email::text",
            columns: &["first_name", "last_name", "email"],
            order_by: "created_at",
        },
        SearchKind::Company => Branch {
            table: "companies",
            title: "name::text",
            subtitle: "email::text",
            columns: &["name", "email"],
            order_by: "created_at",
        },
        SearchKind::Opportunity => Branch {
            table: "opportunities",
            title: "title::text",
            subtitle: "stage::text",
            columns: &["title"],
            order_by: "created_at",
        },
        SearchKind::Quote => Branch {
            table: "quotes",
            title: "quote_number::text",
            subtitle: "status::text",
            columns: &["quote_number"],
            order_by: "issue_date",
        },
        SearchKind::Order => Branch {
            table: "sales_orders",
            title: "order_number::text",
            subtitle: "status::text",
            columns: &["order_number"],
            order_by: "order_date",
        },
        SearchKind::Invoice => Branch {
            table: "invoices",
            title: "invoice_number::text",
            subtitle: "status::text",
            columns: &["invoice_number"],
            order_by: "issue_date",
        },
        SearchKind::Product => Branch {
            table: "products",
            title: "name::text",
            subtitle: "sku::text",
            columns: &["sku", "name", "barcode"],
            order_by: "created_at",
        },
        SearchKind::Warehouse => Branch {
            table: "warehouses",
            title: "name::text",
            subtitle: "code::text",
            columns: &["code", "name"],
            order_by: "created_at",
        },
        SearchKind::Vendor => Branch {
            table: "vendors",
            title: "name::text",
            subtitle: "email::text",
            columns: &["name", "email"],
            order_by: "created_at",
        },
        SearchKind::PurchaseOrder => Branch {
            table: "purchase_orders",
            title: "po_number::text",
            subtitle: "status::text",
            columns: &["po_number"],
            order_by: "order_date",
        },
        SearchKind::Project => Branch {
            table: "projects",
            title: "name::text",
            subtitle: "project_code::text",
            columns: &["name", "project_code"],
            order_by: "created_at",
        },
        SearchKind::Task => Branch {
            table: "tasks",
            title: "title::text",
            subtitle: "task_code::text",
            columns: &["title", "task_code"],
            order_by: "created_at",
        },
        SearchKind::Account => Branch {
            table: "accounts",
            title: "account_name::text",
            subtitle: "account_code::text",
            columns: &["account_code", "account_name"],
            order_by: "created_at",
        },
        SearchKind::LedgerEntry => Branch {
            table: "general_ledger_entries",
            title: "description::text",
            subtitle: "entry_date::text",
            columns: &["description"],
            order_by: "entry_date",
        },
        SearchKind::Employee => Branch {
            table: "employees",
            title: "(first_name || ' ' || last_name)::text",
            subtitle: "employee_number::text",
            columns: &["first_name", "last_name", "email", "employee_number"],
            order_by: "created_at",
        },
    }
}

#[async_trait]
impl SearchRepository for PgSearchRepository {
    async fn search(
        &self,
        term: &str,
        kinds: &[SearchKind],
        per_kind: i64,
    ) -> AppResult<Vec<SearchHit>> {
        if kinds.is_empty() {
            return Ok(Vec::new());
        }

        let pattern = format!("%{term}%");
        let mut query: QueryBuilder<Postgres> = QueryBuilder::new("");

        // One statement rather than a query per kind: fifteen round trips to
        // fill a dropdown that redraws on every keystroke is the difference
        // between search feeling instant and feeling broken.
        for (index, kind) in kinds.iter().enumerate() {
            let branch = branch(*kind);

            if index > 0 {
                query.push(" UNION ALL ");
            }

            // Each branch is wrapped and limited on its own, so one noisy kind
            // cannot fill the whole result and hide the others.
            query.push("(SELECT ");
            query.push_bind(kind.as_str());
            query.push(format!(
                "::text AS kind, id, {} AS title, {} AS subtitle FROM {} WHERE (",
                branch.title, branch.subtitle, branch.table
            ));

            for (n, column) in branch.columns.iter().enumerate() {
                if n > 0 {
                    query.push(" OR ");
                }
                // Column and table names come from the `branch` table above,
                // never from input; only the pattern is bound.
                query.push(format!("{column} ILIKE "));
                query.push_bind(pattern.clone());
            }

            query.push(format!(") ORDER BY {} DESC LIMIT ", branch.order_by));
            query.push_bind(per_kind);
            query.push(")");
        }

        let hits = query.build_query_as::<SearchHit>().fetch_all(&self.pool).await?;
        Ok(hits)
    }
}
