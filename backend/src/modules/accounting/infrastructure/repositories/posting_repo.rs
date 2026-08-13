use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::accounting::domain::posting::{
    PostingAccounts, EXPENSE_REFERENCE, INVENTORY_OPENING_KEY, INVOICE_REFERENCE, PAYMENT_REFERENCE,
    CREDIT_NOTE_REFERENCE, RECEIPT_REFERENCE, RETURN_REFERENCE, VENDOR_PAYMENT_REFERENCE,
};
use crate::modules::accounting::domain::repositories::{PostingRepository, StockOnHand};
use crate::shared::posting::{
    PostableCreditNote, PostableExpenseReport, PostableInvoice, PostablePayment,
    PostableReceipt, PostableReturn,
};

/// The posting mapping lives on the `organization_settings` singleton, next to
/// the base currency the posting rules need in the same breath. See
/// `014_gl_posting.sql`.
#[derive(Clone)]
pub struct PgPostingRepository {
    pool: PgPool,
}

impl PgPostingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Expense reports in one of `statuses` missing the entry `suffix` names.
    ///
    /// Matched on the posting key rather than on `reference_id`, because a
    /// report posts twice over its life and both entries carry the same
    /// reference — only the key tells the approval from the reimbursement.
    async fn unposted_expenses(
        &self,
        statuses: &[&str],
        suffix: &str,
    ) -> AppResult<Vec<PostableExpenseReport>> {
        let rows = sqlx::query_as::<_, UnpostedExpenseRow>(
            r#"
            SELECT r.id, r.org_id, r.report_number, r.base_total_amount,
                   COALESCE(r.approved_at::date, r.updated_at::date) AS on_date,
                   r.approved_by
            FROM expense_reports r
            WHERE r.status = ANY($1)
              AND r.approved_by IS NOT NULL
              AND NOT EXISTS (
                  SELECT 1 FROM general_ledger_entries g
                  WHERE g.posting_key = $2 || r.id::text || $3
              )
            ORDER BY r.report_number
            "#,
        )
        .bind(statuses)
        .bind(format!("{EXPENSE_REFERENCE}:"))
        .bind(format!(":{suffix}"))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| PostableExpenseReport {
                id: row.id,
                org_id: row.org_id,
                number: row.report_number,
                on: row.on_date,
                base_total: row.base_total_amount,
                // The approver, for the same reason the live path uses the
                // actor: the ledger's `created_by` references `users`, and
                // `employee_id` is not a user id.
                created_by: row.approved_by,
            })
            .collect())
    }
}

/// Shared by both payment queries — the two differ only in which table and
/// which reference type they look at.
fn payment_from(row: UnpostedPaymentRow) -> PostablePayment {
    PostablePayment {
        id: row.id,
        org_id: row.org_id,
        document_number: row.invoice_number,
        payment_date: row.payment_date,
        base_amount: row.base_amount,
        fx_gain_loss: row.fx_gain_loss,
        created_by: row.created_by,
    }
}

/// Invoice statuses that mean the revenue is earned and the money is owed.
///
/// A draft has not been issued and a cancelled invoice has been withdrawn, so
/// neither is owed to the ledger. `paid` and `overdue` are: an invoice does not
/// stop having been issued because it was later settled or went past its date.
const ISSUED_STATUSES: [&str; 3] = ["sent", "paid", "overdue"];

#[derive(sqlx::FromRow)]
struct UnpostedInvoiceRow {
    id: Uuid,
    org_id: Option<Uuid>,
    invoice_number: String,
    issue_date: NaiveDate,
    currency: String,
    fx_rate: Decimal,
    base_total: Decimal,
    tax_amount: Decimal,
    created_by: Uuid,
}

#[derive(sqlx::FromRow)]
struct UnpostedReceiptRow {
    id: Uuid,
    org_id: Option<Uuid>,
    receipt_number: String,
    receipt_date: NaiveDate,
    fx_rate: Decimal,
    stocked_net: Decimal,
    expensed_net: Decimal,
    tax: Decimal,
    created_by: Uuid,
}

#[derive(sqlx::FromRow)]
struct UnpostedCreditNoteRow {
    id: Uuid,
    org_id: Option<Uuid>,
    credit_note_number: String,
    issue_date: NaiveDate,
    fx_rate: Decimal,
    base_total: Decimal,
    tax_amount: Decimal,
    created_by: Uuid,
}

#[derive(sqlx::FromRow)]
struct UnpostedReturnRow {
    id: Uuid,
    org_id: Option<Uuid>,
    return_number: String,
    return_date: NaiveDate,
    fx_rate: Decimal,
    stocked_net: Decimal,
    expensed_net: Decimal,
    tax: Decimal,
    created_by: Uuid,
}

#[derive(sqlx::FromRow)]
struct UnpostedExpenseRow {
    id: Uuid,
    org_id: Option<Uuid>,
    report_number: String,
    base_total_amount: Decimal,
    on_date: NaiveDate,
    approved_by: Uuid,
}

#[derive(sqlx::FromRow)]
struct UnpostedPaymentRow {
    id: Uuid,
    org_id: Option<Uuid>,
    invoice_number: String,
    payment_date: NaiveDate,
    base_amount: Decimal,
    fx_gain_loss: Decimal,
    created_by: Uuid,
}

#[async_trait]
impl PostingRepository for PgPostingRepository {
    async fn get_accounts(&self) -> AppResult<PostingAccounts> {
        let accounts = sqlx::query_as::<_, PostingAccounts>(
            r#"
            SELECT ar_account_id, bank_account_id, sales_revenue_account_id,
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

        Ok(accounts)
    }

    async fn update_accounts(&self, accounts: &PostingAccounts) -> AppResult<PostingAccounts> {
        // Every column is written, including the ones being cleared: this
        // replaces the mapping rather than merging into it, so unmapping a role
        // is expressed by sending it as null.
        let updated = sqlx::query_as::<_, PostingAccounts>(
            r#"
            UPDATE organization_settings SET
                ar_account_id               = $1,
                bank_account_id             = $2,
                sales_revenue_account_id    = $3,
                tax_payable_account_id      = $4,
                fx_gain_loss_account_id     = $5,
                accounts_payable_account_id = $6,
                cost_of_sales_account_id    = $7,
                purchase_tax_account_id     = $8,
                employee_payable_account_id = $9,
                employee_expense_account_id = $10,
                inventory_account_id            = $11,
                inventory_adjustment_account_id = $12
            WHERE singleton
            RETURNING ar_account_id, bank_account_id, sales_revenue_account_id,
                      tax_payable_account_id, fx_gain_loss_account_id,
                      accounts_payable_account_id, cost_of_sales_account_id,
                      purchase_tax_account_id, employee_payable_account_id,
                      employee_expense_account_id, inventory_account_id,
                      inventory_adjustment_account_id
            "#,
        )
        .bind(accounts.ar_account_id)
        .bind(accounts.bank_account_id)
        .bind(accounts.sales_revenue_account_id)
        .bind(accounts.tax_payable_account_id)
        .bind(accounts.fx_gain_loss_account_id)
        .bind(accounts.accounts_payable_account_id)
        .bind(accounts.cost_of_sales_account_id)
        .bind(accounts.purchase_tax_account_id)
        .bind(accounts.employee_payable_account_id)
        .bind(accounts.employee_expense_account_id)
        .bind(accounts.inventory_account_id)
        .bind(accounts.inventory_adjustment_account_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(updated)
    }

    async fn unposted_invoices(&self) -> AppResult<Vec<PostableInvoice>> {
        let rows = sqlx::query_as::<_, UnpostedInvoiceRow>(
            r#"
            SELECT i.id, i.org_id, i.invoice_number, i.issue_date, i.currency, i.fx_rate,
                   COALESCE(i.base_total, 0)  AS base_total,
                   COALESCE(i.tax_amount, 0)  AS tax_amount,
                   i.created_by
            FROM invoices i
            WHERE i.status = ANY($1)
              AND NOT EXISTS (
                  SELECT 1 FROM general_ledger_entries g
                  WHERE g.reference_type = $2 AND g.reference_id = i.id
              )
            ORDER BY i.issue_date, i.invoice_number
            "#,
        )
        .bind(&ISSUED_STATUSES[..])
        .bind(INVOICE_REFERENCE)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| PostableInvoice {
                id: row.id,
                org_id: row.org_id,
                number: row.invoice_number,
                issue_date: row.issue_date,
                currency: row.currency,
                fx_rate: row.fx_rate,
                base_total: row.base_total,
                tax_amount: row.tax_amount,
                created_by: row.created_by,
            })
            .collect())
    }

    async fn unposted_receipts(&self) -> AppResult<Vec<PostableReceipt>> {
        // Valued exactly as the live posting values it: rounded per line, net
        // first and then tax on the rounded net, which is what
        // `shared::money::calculate_line` does. Summing unrounded and rounding
        // once at the end would let a repaired receipt differ from a posted one
        // by a cent, and the whole point of the repair is that it produces the
        // same entries.
        let rows = sqlx::query_as::<_, UnpostedReceiptRow>(
            r#"
            SELECT r.id, r.org_id, r.receipt_number, r.receipt_date, r.created_by,
                   po.fx_rate,
                   -- Split the same way the live posting splits it: a line
                   -- naming a stocked product becomes an asset, everything else
                   -- (freight, services, free text) is a cost on arrival.
                   COALESCE(SUM(ROUND(rl.quantity_received * pol.unit_price, 2))
                       FILTER (WHERE p.id IS NOT NULL AND p.product_type <> 'service'), 0)
                       AS stocked_net,
                   COALESCE(SUM(ROUND(rl.quantity_received * pol.unit_price, 2))
                       FILTER (WHERE p.id IS NULL OR p.product_type = 'service'), 0)
                       AS expensed_net,
                   COALESCE(SUM(ROUND(
                       ROUND(rl.quantity_received * pol.unit_price, 2) * pol.tax_rate / 100, 2
                   )), 0) AS tax
            FROM goods_receipts r
            JOIN purchase_orders po ON po.id = r.po_id
            JOIN goods_receipt_lines rl ON rl.receipt_id = r.id
            JOIN purchase_order_lines pol ON pol.id = rl.po_line_id
            LEFT JOIN products p ON p.id = rl.product_id
            WHERE NOT EXISTS (
                SELECT 1 FROM general_ledger_entries g
                WHERE g.reference_type = $1 AND g.reference_id = r.id
            )
            GROUP BY r.id, po.fx_rate
            ORDER BY r.receipt_date, r.receipt_number
            "#,
        )
        .bind(RECEIPT_REFERENCE)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| PostableReceipt {
                id: row.id,
                org_id: row.org_id,
                number: row.receipt_number,
                receipt_date: row.receipt_date,
                fx_rate: row.fx_rate,
                stocked_net: row.stocked_net,
                expensed_net: row.expensed_net,
                tax: row.tax,
                created_by: row.created_by,
            })
            .collect())
    }

    async fn unposted_vendor_payments(&self) -> AppResult<Vec<PostablePayment>> {
        let rows = sqlx::query_as::<_, UnpostedPaymentRow>(
            r#"
            SELECT p.id, p.org_id, po.po_number AS invoice_number, p.payment_date,
                   p.base_amount, p.fx_gain_loss, p.created_by
            FROM vendor_payments p
            JOIN purchase_orders po ON po.id = p.po_id
            WHERE NOT EXISTS (
                SELECT 1 FROM general_ledger_entries g
                WHERE g.reference_type = $1 AND g.reference_id = p.id
            )
            ORDER BY p.payment_date, po.po_number
            "#,
        )
        .bind(VENDOR_PAYMENT_REFERENCE)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(payment_from).collect())
    }

    async fn unposted_expense_approvals(&self) -> AppResult<Vec<PostableExpenseReport>> {
        self.unposted_expenses(&["approved", "reimbursed"], "expense").await
    }

    async fn unposted_expense_reimbursements(&self) -> AppResult<Vec<PostableExpenseReport>> {
        self.unposted_expenses(&["reimbursed"], "reimbursement").await
    }

    async fn unposted_payments(&self) -> AppResult<Vec<PostablePayment>> {
        let rows = sqlx::query_as::<_, UnpostedPaymentRow>(
            r#"
            SELECT p.id, p.org_id, i.invoice_number, p.payment_date,
                   p.base_amount, p.fx_gain_loss, p.created_by
            FROM payments p
            JOIN invoices i ON i.id = p.invoice_id
            WHERE NOT EXISTS (
                SELECT 1 FROM general_ledger_entries g
                WHERE g.reference_type = $1 AND g.reference_id = p.id
            )
            ORDER BY p.payment_date, i.invoice_number
            "#,
        )
        .bind(PAYMENT_REFERENCE)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(payment_from).collect())
    }

    async fn unposted_credit_notes(&self) -> AppResult<Vec<PostableCreditNote>> {
        // `returned_cost` is deliberately zero here: the stock came back when the
        // note was raised, and the movement carries the cost it came back at.
        // Re-deriving it now would use today's average, which is a different
        // number — so a repaired note posts the money legs and leaves the stock
        // leg to the movement that already recorded it.
        let rows = sqlx::query_as::<_, UnpostedCreditNoteRow>(
            r#"
            SELECT n.id, n.org_id, n.credit_note_number, n.issue_date, n.fx_rate,
                   n.base_total, n.tax_amount, n.created_by
            FROM credit_notes n
            WHERE NOT EXISTS (
                SELECT 1 FROM general_ledger_entries g
                WHERE g.reference_type = $1 AND g.reference_id = n.id
            )
            ORDER BY n.issue_date, n.credit_note_number
            "#,
        )
        .bind(CREDIT_NOTE_REFERENCE)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| PostableCreditNote {
                id: row.id,
                org_id: row.org_id,
                number: row.credit_note_number,
                issue_date: row.issue_date,
                fx_rate: row.fx_rate,
                base_total: row.base_total,
                tax_amount: row.tax_amount,
                returned_cost: Decimal::ZERO,
                created_by: row.created_by,
            })
            .collect())
    }

    async fn unposted_returns(&self) -> AppResult<Vec<PostableReturn>> {
        // Valued exactly as the live posting values it, and split the same way:
        // a line naming a stocked product came off the Inventory asset,
        // everything else off cost.
        let rows = sqlx::query_as::<_, UnpostedReturnRow>(
            r#"
            SELECT r.id, r.org_id, r.return_number, r.return_date, r.created_by,
                   po.fx_rate,
                   COALESCE(SUM(ROUND(rl.quantity_returned * pol.unit_price, 2))
                       FILTER (WHERE p.id IS NOT NULL AND p.product_type <> 'service'), 0)
                       AS stocked_net,
                   COALESCE(SUM(ROUND(rl.quantity_returned * pol.unit_price, 2))
                       FILTER (WHERE p.id IS NULL OR p.product_type = 'service'), 0)
                       AS expensed_net,
                   COALESCE(SUM(ROUND(
                       ROUND(rl.quantity_returned * pol.unit_price, 2) * pol.tax_rate / 100, 2
                   )), 0) AS tax
            FROM purchase_returns r
            JOIN purchase_orders po ON po.id = r.po_id
            JOIN purchase_return_lines rl ON rl.return_id = r.id
            JOIN purchase_order_lines pol ON pol.id = rl.po_line_id
            LEFT JOIN products p ON p.id = rl.product_id
            WHERE NOT EXISTS (
                SELECT 1 FROM general_ledger_entries g
                WHERE g.reference_type = $1 AND g.reference_id = r.id
            )
            GROUP BY r.id, po.fx_rate
            ORDER BY r.return_date, r.return_number
            "#,
        )
        .bind(RETURN_REFERENCE)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| PostableReturn {
                id: row.id,
                org_id: row.org_id,
                number: row.return_number,
                return_date: row.return_date,
                fx_rate: row.fx_rate,
                stocked_net: row.stocked_net,
                expensed_net: row.expensed_net,
                tax: row.tax,
                created_by: row.created_by,
            })
            .collect())
    }

    async fn stock_on_hand(&self) -> AppResult<Vec<StockOnHand>> {
        Ok(sqlx::query_as::<_, StockOnHand>(
            r#"
            SELECT p.id AS product_id, p.sku, p.name,
                   SUM(sl.quantity)::BIGINT AS quantity,
                   p.average_cost,
                   ROUND(SUM(sl.quantity) * COALESCE(p.average_cost, 0), 2) AS value
            FROM products p
            JOIN stock_levels sl ON sl.product_id = p.id
            GROUP BY p.id, p.sku, p.name, p.average_cost
            -- Nothing on the shelf is nothing to open with, and a negative
            -- balance is a data problem to fix rather than an asset to record.
            HAVING SUM(sl.quantity) > 0
            ORDER BY p.sku
            "#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    async fn inventory_opening_posted(&self) -> AppResult<bool> {
        // The posting key, not the reference type: the key is what the unique
        // index enforces, so asking about it is asking the same question the
        // database would answer on insert.
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM general_ledger_entries WHERE posting_key = $1)",
        )
        .bind(INVENTORY_OPENING_KEY)
        .fetch_one(&self.pool)
        .await?)
    }
}
