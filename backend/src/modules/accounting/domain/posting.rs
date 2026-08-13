use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::modules::accounting::domain::entities::AccountType;
use crate::shared::money::to_base;
use crate::shared::posting::{
    PostableCreditNote, PostableExpenseReport, PostableInvoice, PostableMovement, PostablePayment,
    PostableReceipt, PostableReturn,
};

pub const INVOICE_REFERENCE: &str = "sales_invoice";
pub const PAYMENT_REFERENCE: &str = "sales_payment";
pub const RECEIPT_REFERENCE: &str = "goods_receipt";
pub const VENDOR_PAYMENT_REFERENCE: &str = "vendor_payment";
pub const EXPENSE_REFERENCE: &str = "expense_report";
pub const MOVEMENT_REFERENCE: &str = "stock_movement";
pub const RETURN_REFERENCE: &str = "purchase_return";
pub const CREDIT_NOTE_REFERENCE: &str = "sales_credit_note";
pub const OPENING_REFERENCE: &str = "inventory_opening";

/// Documents that post their own stock leg, so the movement they create must
/// not post a second one.
///
/// A list rather than a single check because it has already grown twice: it was
/// "is this from a goods receipt", then a purchase return, and now a credit note
/// bringing goods back. Anything added here has to post its own inventory entry,
/// or stock will move with nothing in the ledger to show it.
const SELF_POSTING_DOCUMENTS: [&str; 3] =
    [RECEIPT_REFERENCE, RETURN_REFERENCE, CREDIT_NOTE_REFERENCE];

/// A fixed key, because opening the Inventory account is a once-ever event for
/// an installation rather than one per document. The unique index on
/// `posting_key` is then the whole of the "only once" guarantee — no check in
/// application code can be raced the way that one cannot.
pub const INVENTORY_OPENING_KEY: &str = "inventory_opening";

/// Which account is chosen for each posting role, as configured.
///
/// Every field is optional because a half-configured installation is the normal
/// starting state, not an error. Stored on the `organization_settings` singleton
/// alongside the base currency, which the posting rules need in the same breath.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PostingAccounts {
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

/// The role each mapped account has to play, and the account type it must be.
///
/// One list, used by the validation, the "what is still missing" report and the
/// settings screen alike — so a role cannot be added in one place and forgotten
/// in another.
pub struct PostingRole {
    pub name: &'static str,
    pub account_type: &'static str,
    /// Whether the core posting rules need it.
    ///
    /// The inventory pair is optional, and that is what stops this change
    /// breaking every existing installation: adding two *required* roles would
    /// switch off sales, purchase and expense posting everywhere until an admin
    /// went and mapped them, which is a far worse failure than the periodic
    /// costing this replaces. Leave them empty and nothing changes.
    pub required: bool,
}

const fn role(name: &'static str, account_type: &'static str) -> PostingRole {
    PostingRole { name, account_type, required: true }
}

const fn optional(name: &'static str, account_type: &'static str) -> PostingRole {
    PostingRole { name, account_type, required: false }
}

pub const POSTING_ROLES: [PostingRole; 12] = [
    role("Accounts receivable", AccountType::ASSET),
    role("Bank", AccountType::ASSET),
    role("Sales revenue", AccountType::REVENUE),
    role("Tax payable", AccountType::LIABILITY),
    role("Foreign exchange gain/loss", AccountType::REVENUE),
    role("Accounts payable", AccountType::LIABILITY),
    role("Cost of sales", AccountType::EXPENSE),
    role("Purchase tax", AccountType::ASSET),
    role("Employee payable", AccountType::LIABILITY),
    role("Employee expense", AccountType::EXPENSE),
    // Choosing both switches perpetual costing on. Until then goods are a cost
    // when they arrive, which is where this application started.
    optional("Inventory", AccountType::ASSET),
    optional("Inventory adjustment", AccountType::EXPENSE),
];

impl PostingAccounts {
    /// The twelve in the same order as [`POSTING_ROLES`].
    pub fn in_role_order(&self) -> [Option<Uuid>; 12] {
        [
            self.ar_account_id,
            self.bank_account_id,
            self.sales_revenue_account_id,
            self.tax_payable_account_id,
            self.fx_gain_loss_account_id,
            self.accounts_payable_account_id,
            self.cost_of_sales_account_id,
            self.purchase_tax_account_id,
            self.employee_payable_account_id,
            self.employee_expense_account_id,
            self.inventory_account_id,
            self.inventory_adjustment_account_id,
        ]
    }

    /// The **required** roles still to be filled. Empty means posting is on.
    ///
    /// Deliberately not the optional pair: an installation with the ten core
    /// roles chosen is fully configured, and reporting it as incomplete because
    /// it has not opted into perpetual costing would be wrong.
    pub fn missing_roles(&self) -> Vec<&'static str> {
        POSTING_ROLES
            .iter()
            .zip(self.in_role_order())
            .filter(|(role, chosen)| role.required && chosen.is_none())
            .map(|(role, _)| role.name)
            .collect()
    }

    pub fn is_complete(&self) -> bool {
        self.missing_roles().is_empty()
    }

    /// Whether stock is an asset on this installation's balance sheet.
    pub fn is_perpetual(&self) -> bool {
        InventoryMapping::complete(self).is_some()
    }
}

/// A mapping with every role filled.
///
/// Only ever constructed complete, which is what makes "posting is off until the
/// accounts are chosen" a property of the type rather than a check somebody has
/// to remember at each call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountMapping {
    pub ar: Uuid,
    pub bank: Uuid,
    pub sales_revenue: Uuid,
    pub tax_payable: Uuid,
    pub fx_gain_loss: Uuid,
    pub accounts_payable: Uuid,
    pub cost_of_sales: Uuid,
    pub purchase_tax: Uuid,
    pub employee_payable: Uuid,
    pub employee_expense: Uuid,
}

impl AccountMapping {
    pub fn complete(accounts: &PostingAccounts) -> Option<Self> {
        Some(Self {
            ar: accounts.ar_account_id?,
            bank: accounts.bank_account_id?,
            sales_revenue: accounts.sales_revenue_account_id?,
            tax_payable: accounts.tax_payable_account_id?,
            fx_gain_loss: accounts.fx_gain_loss_account_id?,
            accounts_payable: accounts.accounts_payable_account_id?,
            cost_of_sales: accounts.cost_of_sales_account_id?,
            purchase_tax: accounts.purchase_tax_account_id?,
            employee_payable: accounts.employee_payable_account_id?,
            employee_expense: accounts.employee_expense_account_id?,
        })
    }
}

/// The two accounts perpetual costing needs, kept apart from [`AccountMapping`].
///
/// Separate rather than folded in, because `AccountMapping` is all-or-nothing:
/// adding these to it would mean an installation that has not opted into
/// perpetual costing posts *nothing at all*. Its own mapping means inventory
/// posting switches on independently, and everything else carries on either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryMapping {
    pub inventory: Uuid,
    pub inventory_adjustment: Uuid,
}

impl InventoryMapping {
    pub fn complete(accounts: &PostingAccounts) -> Option<Self> {
        Some(Self {
            inventory: accounts.inventory_account_id?,
            inventory_adjustment: accounts.inventory_adjustment_account_id?,
        })
    }
}

/// A journal entry the poster intends to write.
///
/// Amounts are already in the base currency: automatic postings never carry a
/// foreign amount, because the mapped accounts are base-currency accounts and an
/// entry has to agree with the accounts it touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedEntry {
    pub posting_key: String,
    pub reference_type: &'static str,
    pub reference_id: Uuid,
    pub entry_date: NaiveDate,
    pub description: String,
    pub debit_account_id: Uuid,
    pub credit_account_id: Uuid,
    pub amount: Decimal,
    pub org_id: Option<Uuid>,
    pub created_by: Uuid,
}

impl PlannedEntry {
    /// Reversing an entry is the same entry with the sides swapped. Amounts stay
    /// positive; the ledger has no notion of a negative posting, and a negative
    /// amount would break every balance rule that assumes otherwise.
    fn reversed(mut self, description: String) -> Self {
        std::mem::swap(&mut self.debit_account_id, &mut self.credit_account_id);
        self.posting_key = format!("{}:reversal", self.posting_key);
        self.description = description;
        self
    }
}

/// Entries for an invoice being issued.
///
/// Two legs, because `general_ledger_entries` holds exactly one debit and one
/// credit account and the posting is three-sided: receivable up, revenue up,
/// tax owed up.
///
/// The revenue figure is the **remainder** — total less tax — rather than the
/// subtotal restated on its own. Restating subtotal and tax independently
/// rounds each to cents separately, and the two can land a cent away from the
/// restated total, which would post an invoice whose legs did not add up to the
/// receivable it created. Deriving one from the other makes that impossible.
pub fn invoice_entries(
    invoice: &PostableInvoice,
    mapping: &AccountMapping,
) -> Vec<PlannedEntry> {
    let base_tax = to_base(invoice.tax_amount, invoice.fx_rate);
    let base_revenue = invoice.base_total - base_tax;

    let entry = |suffix: &str, credit: Uuid, amount: Decimal, what: &str| PlannedEntry {
        posting_key: format!("{}:{}:{}", INVOICE_REFERENCE, invoice.id, suffix),
        reference_type: INVOICE_REFERENCE,
        reference_id: invoice.id,
        entry_date: invoice.issue_date,
        description: format!("Invoice {} — {}", invoice.number, what),
        debit_account_id: mapping.ar,
        credit_account_id: credit,
        amount,
        org_id: invoice.org_id,
        created_by: invoice.created_by,
    };

    // A zero leg is not written at all: an entry moving nothing is noise in the
    // ledger, and a zero-rated invoice is ordinary rather than exceptional.
    [
        (base_revenue != Decimal::ZERO)
            .then(|| entry("revenue", mapping.sales_revenue, base_revenue, "revenue")),
        (base_tax != Decimal::ZERO)
            .then(|| entry("tax", mapping.tax_payable, base_tax, "tax")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Entries for a credit note: the mirror of [`invoice_entries`].
///
/// Revenue is derived as the remainder — `base_total − base_tax` — for exactly
/// the reason issuing does it that way: restating the subtotal on its own can
/// land a cent from the restated total, and the two legs would then fail to add
/// up to the receivable they are relieving.
///
/// When goods came back a **second, independent pair** is written, putting the
/// stock back on the balance sheet and taking the cost back off the profit and
/// loss. It exists only under perpetual costing; with the inventory accounts
/// unmapped the cost was taken when the goods were *bought*, and there is nothing
/// here to reverse.
///
/// The two pairs are independent on purpose: what a customer is credited and what
/// the goods cost are unrelated numbers, and a credit note with no goods movement
/// is entirely ordinary.
pub fn credit_note_entries(
    note: &PostableCreditNote,
    mapping: &AccountMapping,
    inventory: Option<InventoryMapping>,
) -> Vec<PlannedEntry> {
    let entry = |suffix: &str, debit: Uuid, credit: Uuid, amount: Decimal, what: &str| PlannedEntry {
        posting_key: format!("{}:{}:{}", CREDIT_NOTE_REFERENCE, note.id, suffix),
        reference_type: CREDIT_NOTE_REFERENCE,
        reference_id: note.id,
        entry_date: note.issue_date,
        description: format!("Credit note {} — {}", note.number, what),
        debit_account_id: debit,
        credit_account_id: credit,
        amount,
        org_id: note.org_id,
        created_by: note.created_by,
    };

    let base_tax = to_base(note.tax_amount, note.fx_rate);
    let base_revenue = note.base_total - base_tax;

    let stock = inventory.filter(|_| note.returned_cost != Decimal::ZERO).map(|inventory| {
        entry(
            "stock",
            inventory.inventory,
            mapping.cost_of_sales,
            note.returned_cost,
            "goods returned",
        )
    });

    [
        (base_revenue != Decimal::ZERO)
            .then(|| entry("revenue", mapping.sales_revenue, mapping.ar, base_revenue, "revenue")),
        (base_tax != Decimal::ZERO)
            .then(|| entry("tax", mapping.tax_payable, mapping.ar, base_tax, "tax")),
        stock,
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Entries for money arriving against an invoice.
///
/// The settlement leg moves what the money was actually worth. When the rate has
/// moved since the invoice was raised, that is not what the receivable carries,
/// so a second leg moves the difference to the FX account and leaves the
/// receivable clearing at the rate it was raised at.
///
/// EUR 1,000 invoiced at 1.10 and settled at 1.15: bank +1150, receivable −1150
/// then +50, FX gain 50. The receivable nets to −1100, exactly what issuing put
/// there.
pub fn payment_entries(
    payment: &PostablePayment,
    mapping: &AccountMapping,
) -> Vec<PlannedEntry> {
    settlement_entries(payment, mapping, PAYMENT_REFERENCE, mapping.ar, MoneyFlow::In)
}

/// Entries for money leaving against a purchase order — the mirror of
/// [`payment_entries`], with payables as the control account.
///
/// EUR 1,000 ordered at 1.10 (payable 1,100) and paid at 1.05 (bank out 1,050)
/// leaves 50 still credited to payables, and that 50 is a gain: the debt cost
/// less than it was booked at.
pub fn vendor_payment_entries(
    payment: &PostablePayment,
    mapping: &AccountMapping,
) -> Vec<PlannedEntry> {
    settlement_entries(
        payment,
        mapping,
        VENDOR_PAYMENT_REFERENCE,
        mapping.accounts_payable,
        MoneyFlow::Out,
    )
}

/// Which way the money moved, which is the only thing that differs between
/// settling a receivable and settling a payable.
#[derive(Clone, Copy)]
enum MoneyFlow {
    In,
    Out,
}

/// Settling a debt in either direction.
///
/// The settlement leg moves what the money was actually worth. When the rate has
/// moved since the document was raised, that is not what the control account
/// carries, so a second leg moves the difference to the FX account and leaves
/// the control account clearing at the rate the document was raised at.
///
/// The FX leg is **the same in both directions**: a gain debits the control
/// account and credits the FX account, a loss does the reverse. That holds
/// whichever side the control account sits on, because the leg exists to pull it
/// back to what the document booked, and both a receivable that came in high and
/// a payable that went out low need the same nudge.
fn settlement_entries(
    payment: &PostablePayment,
    mapping: &AccountMapping,
    reference_type: &'static str,
    control: Uuid,
    flow: MoneyFlow,
) -> Vec<PlannedEntry> {
    let base = |suffix: &str, debit: Uuid, credit: Uuid, amount: Decimal, what: &str| {
        PlannedEntry {
            posting_key: format!("{}:{}:{}", reference_type, payment.id, suffix),
            reference_type,
            reference_id: payment.id,
            entry_date: payment.payment_date,
            description: format!("Payment for {} — {}", payment.document_number, what),
            debit_account_id: debit,
            credit_account_id: credit,
            amount,
            org_id: payment.org_id,
            created_by: payment.created_by,
        }
    };

    let settlement = (payment.base_amount != Decimal::ZERO).then(|| {
        let (debit, credit) = match flow {
            MoneyFlow::In => (mapping.bank, control),
            MoneyFlow::Out => (control, mapping.bank),
        };
        base("settlement", debit, credit, payment.base_amount, "settlement")
    });

    let fx = match payment.fx_gain_loss {
        gain if gain > Decimal::ZERO => {
            Some(base("fx", control, mapping.fx_gain_loss, gain, "exchange gain"))
        }
        loss if loss < Decimal::ZERO => {
            Some(base("fx", mapping.fx_gain_loss, control, -loss, "exchange loss"))
        }
        _ => None,
    };

    [settlement, fx].into_iter().flatten().collect()
}

/// Entries for goods arriving against a purchase order.
///
/// Where the goods land depends on whether the inventory accounts are mapped:
///
/// - **Mapped (perpetual).** Stocked lines debit Inventory — an asset. They
///   become a cost when they leave, which `movement_entries` handles.
/// - **Unmapped (periodic).** Everything debits Cost of sales the day it
///   arrives, which is what this application did before perpetual costing and
///   what every installation keeps doing until it opts in.
///
/// Either way the non-stocked part of a delivery — freight, services, free-text
/// lines — is a cost immediately: there is no asset to carry.
///
/// Tax is a separate leg rather than folded into cost, because input tax is
/// usually recoverable: burying it would overstate the cost *and* lose the
/// reclaim.
pub fn receipt_entries(
    receipt: &PostableReceipt,
    mapping: &AccountMapping,
    inventory: Option<InventoryMapping>,
) -> Vec<PlannedEntry> {
    let entry = |suffix: &str, debit: Uuid, amount: Decimal, what: &str| PlannedEntry {
        posting_key: format!("{}:{}:{}", RECEIPT_REFERENCE, receipt.id, suffix),
        reference_type: RECEIPT_REFERENCE,
        reference_id: receipt.id,
        entry_date: receipt.receipt_date,
        description: format!("Goods receipt {} — {}", receipt.number, what),
        debit_account_id: debit,
        credit_account_id: mapping.accounts_payable,
        amount,
        org_id: receipt.org_id,
        created_by: receipt.created_by,
    };

    // Restated independently, unlike an invoice's legs: there is no stored base
    // total for a receipt to reconcile against, so the payable is simply what
    // the legs come to.
    let base_stocked = to_base(receipt.stocked_net, receipt.fx_rate);
    let base_expensed = to_base(receipt.expensed_net, receipt.fx_rate);
    let base_tax = to_base(receipt.tax, receipt.fx_rate);

    // Under periodic costing the two halves of the delivery are indistinguishable
    // — both are a cost on arrival — so they are added together rather than
    // written as two identical legs to the same account.
    let (stock_account, stocked, expensed) = match inventory {
        Some(inventory) => (inventory.inventory, base_stocked, base_expensed),
        None => (mapping.cost_of_sales, Decimal::ZERO, base_stocked + base_expensed),
    };

    [
        (stocked != Decimal::ZERO).then(|| entry("stock", stock_account, stocked, "stock")),
        (expensed != Decimal::ZERO)
            .then(|| entry("cost", mapping.cost_of_sales, expensed, "cost")),
        (base_tax != Decimal::ZERO)
            .then(|| entry("tax", mapping.purchase_tax, base_tax, "tax")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Entries for goods going back to a supplier.
///
/// The exact mirror of [`receipt_entries`] with the sides swapped: the payable
/// is debited with what the supplier credits, and whatever the receipt
/// capitalised or expensed is given back.
///
/// The two agreeing is not a coincidence to be checked but a consequence of how
/// a return is valued — at the purchase order's own line price, which is what
/// the receipt brought the goods in at. Relieving stock at the *current* average
/// instead would leave a difference needing a variance account.
///
/// The tax leg goes back too. Leaving it would reclaim input tax on goods that
/// were sent back, which is the kind of thing an inspection notices.
pub fn return_entries(
    ret: &PostableReturn,
    mapping: &AccountMapping,
    inventory: Option<InventoryMapping>,
) -> Vec<PlannedEntry> {
    let entry = |suffix: &str, credit: Uuid, amount: Decimal, what: &str| PlannedEntry {
        posting_key: format!("{}:{}:{}", RETURN_REFERENCE, ret.id, suffix),
        reference_type: RETURN_REFERENCE,
        reference_id: ret.id,
        entry_date: ret.return_date,
        description: format!("Purchase return {} — {}", ret.number, what),
        debit_account_id: mapping.accounts_payable,
        credit_account_id: credit,
        amount,
        org_id: ret.org_id,
        created_by: ret.created_by,
    };

    let base_stocked = to_base(ret.stocked_net, ret.fx_rate);
    let base_expensed = to_base(ret.expensed_net, ret.fx_rate);
    let base_tax = to_base(ret.tax, ret.fx_rate);

    let (stock_account, stocked, expensed) = match inventory {
        Some(inventory) => (inventory.inventory, base_stocked, base_expensed),
        None => (mapping.cost_of_sales, Decimal::ZERO, base_stocked + base_expensed),
    };

    [
        (stocked != Decimal::ZERO).then(|| entry("stock", stock_account, stocked, "stock")),
        (expensed != Decimal::ZERO)
            .then(|| entry("cost", mapping.cost_of_sales, expensed, "cost")),
        (base_tax != Decimal::ZERO)
            .then(|| entry("tax", mapping.purchase_tax, base_tax, "tax")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Entries for stock moving, under perpetual costing.
///
/// This is where the cost of a sale reaches the profit and loss. Which accounts
/// it touches depends on *why* the stock moved:
///
/// | Movement | Caused by | Posts |
/// |---|---|---|
/// | `in` | a goods receipt | nothing — the receipt already debited Inventory |
/// | `out` | a purchase return | nothing — the return already credited Inventory |
/// | `in` | a sales credit note | nothing — the credit note already debited Inventory |
/// | `in` | a cancelled sales invoice | Dr Inventory / Cr Cost of sales |
/// | `in` | somebody by hand | Dr Inventory / Cr Inventory adjustment |
/// | `out` | anything | Dr Cost of sales / Cr Inventory |
/// | `transfer` | — | nothing; the value did not change, only its shelf |
/// | `adjustment` | — | Inventory against the adjustment account, by sign |
///
/// The `reference_type` is what separates "a document already accounted for
/// this" from "somebody moved stock by hand", and it is the whole of what stops
/// a goods receipt being counted twice.
///
/// A sales invoice is deliberately **not** in `SELF_POSTING_DOCUMENTS`: the cost
/// of a sale belongs to the movement rather than to the document, which is the
/// decision this whole design rests on. So invoicing posts its revenue and the
/// movement posts its cost, in both directions.
///
/// An outward movement is Cost of sales while a negative adjustment is not: a
/// sale and a stock-take shortfall are both stock leaving, but only one of them
/// earned revenue, and burying shrinkage in cost of sales hides it from the
/// person whose job it is to notice.
pub fn movement_entries(
    movement: &PostableMovement,
    mapping: &AccountMapping,
    inventory: InventoryMapping,
) -> Vec<PlannedEntry> {
    // A transfer moves stock between warehouses; the company owns exactly what
    // it owned before, at exactly the value it had.
    if movement.movement_type == "transfer" {
        return Vec::new();
    }

    // Already posted by the document that caused it — see
    // `SELF_POSTING_DOCUMENTS`. This is the whole of what stops a delivery or a
    // return being counted twice.
    if movement
        .reference_type
        .as_deref()
        .is_some_and(|reference| SELF_POSTING_DOCUMENTS.contains(&reference))
    {
        return Vec::new();
    }

    // A zero-valued movement is ordinary — an uncosted product, or a quantity
    // that nets to nothing — and an entry moving nothing is noise in the ledger.
    if movement.value == Decimal::ZERO || movement.quantity_delta == 0 {
        return Vec::new();
    }

    let inward = movement.quantity_delta > 0;
    let from_invoice = movement.reference_type.as_deref() == Some(INVOICE_REFERENCE);

    let (debit, credit, what) = match (inward, movement.movement_type.as_str()) {
        (false, "out") => (mapping.cost_of_sales, inventory.inventory, "cost of sale"),
        // Goods coming back because the sale was cancelled. Without this the
        // credit would land in Inventory adjustment, booking a cancelled sale as
        // shrinkage and leaving the cost of sale it reverses standing.
        (true, _) if from_invoice => {
            (inventory.inventory, mapping.cost_of_sales, "sale cancelled")
        }
        (true, _) => (inventory.inventory, inventory.inventory_adjustment, "stock in"),
        (false, _) => (inventory.inventory_adjustment, inventory.inventory, "stock out"),
    };

    vec![PlannedEntry {
        posting_key: format!("{}:{}", MOVEMENT_REFERENCE, movement.id),
        reference_type: MOVEMENT_REFERENCE,
        reference_id: movement.id,
        entry_date: movement.entry_date,
        description: format!("{} — {}", movement.description, what),
        debit_account_id: debit,
        credit_account_id: credit,
        amount: movement.value,
        org_id: movement.org_id,
        created_by: movement.created_by,
    }]
}

/// The one-time entry that puts existing stock on the balance sheet.
///
/// Dr Inventory / Cr Cost of sales. Crediting cost of sales rather than an
/// equity opening account is the accounting point of the whole exercise: under
/// periodic costing these goods were **already expensed there** when they
/// arrived, and they are still on the shelf, so this reverses an over-expensing
/// rather than inventing a balance.
///
/// It is imperfect in one direction, and the preview says so before anything is
/// posted: stock that arrived through a hand-made movement was never posted at
/// all, so the credit for that part has no matching debit behind it.
pub fn inventory_opening_entries(
    value: Decimal,
    on: NaiveDate,
    org_id: Option<Uuid>,
    created_by: Uuid,
    mapping: &AccountMapping,
    inventory: InventoryMapping,
) -> Vec<PlannedEntry> {
    if value == Decimal::ZERO {
        return Vec::new();
    }

    vec![PlannedEntry {
        posting_key: INVENTORY_OPENING_KEY.to_string(),
        reference_type: OPENING_REFERENCE,
        // No document to point at — this opens an account, it does not record a
        // transaction with anybody.
        reference_id: Uuid::nil(),
        entry_date: on,
        description: "Opening inventory — stock on hand when perpetual costing began".to_string(),
        debit_account_id: inventory.inventory,
        credit_account_id: mapping.cost_of_sales,
        amount: value,
        org_id,
        created_by,
    }]
}

/// An approved expense report: the cost is incurred, and the employee is owed.
pub fn expense_approval_entries(
    report: &PostableExpenseReport,
    mapping: &AccountMapping,
) -> Vec<PlannedEntry> {
    expense_entry(
        report,
        "expense",
        mapping.employee_expense,
        mapping.employee_payable,
        "approved",
    )
}

/// The employee has been paid back, clearing what they were owed.
pub fn expense_reimbursement_entries(
    report: &PostableExpenseReport,
    mapping: &AccountMapping,
) -> Vec<PlannedEntry> {
    expense_entry(
        report,
        "reimbursement",
        mapping.employee_payable,
        mapping.bank,
        "reimbursed",
    )
}

fn expense_entry(
    report: &PostableExpenseReport,
    suffix: &str,
    debit: Uuid,
    credit: Uuid,
    what: &str,
) -> Vec<PlannedEntry> {
    if report.base_total == Decimal::ZERO {
        return Vec::new();
    }

    vec![PlannedEntry {
        posting_key: format!("{}:{}:{}", EXPENSE_REFERENCE, report.id, suffix),
        reference_type: EXPENSE_REFERENCE,
        reference_id: report.id,
        entry_date: report.on,
        description: format!("Expense report {} — {}", report.number, what),
        debit_account_id: debit,
        credit_account_id: credit,
        amount: report.base_total,
        org_id: report.org_id,
        created_by: report.created_by,
    }]
}

/// The mirror of what issuing posted, dated the day the reversal happens.
///
/// Dated `on` rather than the original entry's date: cancelling an invoice in
/// April must not reach back and change what March reported.
pub fn reversal_entries(
    original: Vec<PlannedEntry>,
    on: NaiveDate,
    reason: &str,
) -> Vec<PlannedEntry> {
    original
        .into_iter()
        .map(|entry| {
            let description = format!("{} (reversed — {})", entry.description, reason);
            let mut reversed = entry.reversed(description);
            reversed.entry_date = on;
            reversed
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn mapping() -> AccountMapping {
        AccountMapping {
            ar: Uuid::from_u128(1),
            bank: Uuid::from_u128(2),
            sales_revenue: Uuid::from_u128(3),
            tax_payable: Uuid::from_u128(4),
            fx_gain_loss: Uuid::from_u128(5),
            accounts_payable: Uuid::from_u128(6),
            cost_of_sales: Uuid::from_u128(7),
            purchase_tax: Uuid::from_u128(8),
            employee_payable: Uuid::from_u128(9),
            employee_expense: Uuid::from_u128(10),
        }
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, d).unwrap()
    }

    fn invoice(base_total: Decimal, tax_amount: Decimal, fx_rate: Decimal) -> PostableInvoice {
        PostableInvoice {
            id: Uuid::from_u128(100),
            org_id: None,
            number: "INV-1".into(),
            issue_date: day(1),
            currency: "USD".into(),
            fx_rate,
            base_total,
            tax_amount,
            created_by: Uuid::from_u128(9),
        }
    }

    fn credit_note(
        base_total: Decimal,
        tax_amount: Decimal,
        returned_cost: Decimal,
        fx_rate: Decimal,
    ) -> PostableCreditNote {
        PostableCreditNote {
            id: Uuid::from_u128(700),
            org_id: None,
            number: "CN-1".into(),
            issue_date: day(9),
            fx_rate,
            base_total,
            tax_amount,
            returned_cost,
            created_by: Uuid::from_u128(9),
        }
    }

    fn payment(base_amount: Decimal, fx_gain_loss: Decimal) -> PostablePayment {
        PostablePayment {
            id: Uuid::from_u128(200),
            org_id: None,
            document_number: "INV-1".into(),
            payment_date: day(10),
            base_amount,
            fx_gain_loss,
            created_by: Uuid::from_u128(9),
        }
    }

    /// What every posting has to satisfy: summed across every account it
    /// touches, the movement is zero. True by construction while entries are
    /// two-legged — which is exactly why it is worth asserting, so that a future
    /// change producing a lopsided leg fails here rather than in the trial
    /// balance months later.
    fn is_balanced(entries: &[PlannedEntry]) -> bool {
        let accounts: std::collections::BTreeSet<Uuid> = entries
            .iter()
            .flat_map(|e| [e.debit_account_id, e.credit_account_id])
            .collect();

        accounts.iter().map(|account| net(entries, *account)).sum::<Decimal>() == Decimal::ZERO
    }

    /// Net movement on one account across a set of entries.
    fn net(entries: &[PlannedEntry], account: Uuid) -> Decimal {
        entries
            .iter()
            .map(|e| {
                let mut delta = Decimal::ZERO;
                if e.debit_account_id == account {
                    delta += e.amount;
                }
                if e.credit_account_id == account {
                    delta -= e.amount;
                }
                delta
            })
            .sum()
    }

    #[test]
    fn an_invoice_debits_receivables_and_credits_revenue_and_tax() {
        let m = mapping();
        let entries = invoice_entries(&invoice(dec!(1200.00), dec!(200.00), Decimal::ONE), &m);

        assert_eq!(entries.len(), 2);
        assert!(is_balanced(&entries));
        // The customer owes the whole invoice.
        assert_eq!(net(&entries, m.ar), dec!(1200.00));
        assert_eq!(net(&entries, m.sales_revenue), dec!(-1000.00));
        assert_eq!(net(&entries, m.tax_payable), dec!(-200.00));
    }

    #[test]
    fn the_legs_always_add_up_to_the_receivable() {
        // The case the remainder exists for. Subtotal 33.33 and tax 6.67 at
        // 1.085 restate to 36.16 and 7.24, summing to 43.40 — while the total
        // 40.00 restates to 43.40 as well here, the derivation guarantees it
        // rather than leaving it to luck.
        let m = mapping();
        for (total, tax, rate) in [
            (dec!(43.40), dec!(6.67), dec!(1.085)),
            (dec!(108.51), dec!(16.67), dec!(1.0851)),
            (dec!(1.09), dec!(0.17), dec!(1.0899)),
        ] {
            let entries = invoice_entries(&invoice(total, tax, rate), &m);
            assert_eq!(
                net(&entries, m.ar),
                total,
                "legs did not add up to the receivable for {total} / {tax} @ {rate}"
            );
        }
    }

    #[test]
    fn a_zero_rated_invoice_posts_one_leg() {
        let m = mapping();
        let entries = invoice_entries(&invoice(dec!(500.00), Decimal::ZERO, Decimal::ONE), &m);

        // No tax leg: an entry moving nothing is noise in the ledger.
        assert_eq!(entries.len(), 1);
        assert_eq!(net(&entries, m.ar), dec!(500.00));
        assert_eq!(net(&entries, m.tax_payable), Decimal::ZERO);
    }

    #[test]
    fn a_foreign_invoice_posts_the_restated_tax() {
        let m = mapping();
        // EUR 1,000 + EUR 200 tax at 1.10 => 1,320 base, of which 220 is tax.
        let entries = invoice_entries(&invoice(dec!(1320.00), dec!(200.00), dec!(1.10)), &m);

        assert_eq!(net(&entries, m.ar), dec!(1320.00));
        assert_eq!(net(&entries, m.tax_payable), dec!(-220.00));
        assert_eq!(net(&entries, m.sales_revenue), dec!(-1100.00));
    }

    #[test]
    fn a_payment_at_the_invoice_rate_just_clears_the_receivable() {
        let m = mapping();
        let entries = payment_entries(&payment(dec!(600.00), Decimal::ZERO), &m);

        assert_eq!(entries.len(), 1);
        assert_eq!(net(&entries, m.bank), dec!(600.00));
        assert_eq!(net(&entries, m.ar), dec!(-600.00));
    }

    #[test]
    fn a_gain_leaves_the_receivable_clearing_at_the_invoice_rate() {
        let m = mapping();
        // EUR 1,000 invoiced at 1.10 (receivable 1,100), settled at 1.15.
        let entries = payment_entries(&payment(dec!(1150.00), dec!(50.00)), &m);

        assert_eq!(net(&entries, m.bank), dec!(1150.00));
        // The receivable clears at what issuing put there, not at what arrived.
        assert_eq!(net(&entries, m.ar), dec!(-1100.00));
        assert_eq!(net(&entries, m.fx_gain_loss), dec!(-50.00));
    }

    #[test]
    fn a_loss_clears_the_same_receivable_the_other_way() {
        let m = mapping();
        // EUR 500 invoiced at 1.20 (receivable 600), settled at 1.10.
        let entries = payment_entries(&payment(dec!(550.00), dec!(-50.00)), &m);

        assert_eq!(net(&entries, m.bank), dec!(550.00));
        assert_eq!(net(&entries, m.ar), dec!(-600.00));
        // A loss is a debit to the FX account: it reduces profit.
        assert_eq!(net(&entries, m.fx_gain_loss), dec!(50.00));
    }

    #[test]
    fn reversing_undoes_every_account_it_touched() {
        let m = mapping();
        let original = invoice_entries(&invoice(dec!(1200.00), dec!(200.00), Decimal::ONE), &m);
        let reversal = reversal_entries(original.clone(), day(20), "cancelled");

        for account in [m.ar, m.sales_revenue, m.tax_payable] {
            assert_eq!(
                net(&original, account) + net(&reversal, account),
                Decimal::ZERO,
                "account did not return to where it started"
            );
        }
    }

    #[test]
    fn a_reversal_is_dated_when_it_happened_not_when_the_original_was() {
        let m = mapping();
        let original = invoice_entries(&invoice(dec!(100.00), Decimal::ZERO, Decimal::ONE), &m);
        let reversal = reversal_entries(original, day(20), "cancelled");

        // Cancelling in April must not reach back and change what March reported.
        assert!(reversal.iter().all(|e| e.entry_date == day(20)));
    }

    #[test]
    fn a_reversal_cannot_collide_with_what_it_reverses() {
        let m = mapping();
        let original = invoice_entries(&invoice(dec!(1200.00), dec!(200.00), Decimal::ONE), &m);
        let reversal = reversal_entries(original.clone(), day(20), "cancelled");

        let mut keys: Vec<&str> =
            original.iter().chain(&reversal).map(|e| e.posting_key.as_str()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        // The unique index is what actually enforces this; the keys have to be
        // distinct for it to let a legitimate reversal through at all.
        assert_eq!(keys.len(), before, "a reversal key collided with its original");
    }

    /// A delivery of stocked goods only, which is the ordinary case.
    fn receipt(net: Decimal, tax: Decimal, fx_rate: Decimal) -> PostableReceipt {
        mixed_receipt(net, Decimal::ZERO, tax, fx_rate)
    }

    /// A delivery carrying both stock and something that is a cost on arrival —
    /// freight, or a service line.
    fn mixed_receipt(
        stocked_net: Decimal,
        expensed_net: Decimal,
        tax: Decimal,
        fx_rate: Decimal,
    ) -> PostableReceipt {
        PostableReceipt {
            id: Uuid::from_u128(300),
            org_id: None,
            number: "GR-1".into(),
            receipt_date: day(5),
            fx_rate,
            stocked_net,
            expensed_net,
            tax,
            created_by: Uuid::from_u128(9),
        }
    }

    fn inventory() -> InventoryMapping {
        InventoryMapping {
            inventory: Uuid::from_u128(11),
            inventory_adjustment: Uuid::from_u128(12),
        }
    }

    fn movement(kind: &str, delta: i32, value: Decimal, reference: Option<&str>) -> PostableMovement {
        PostableMovement {
            id: Uuid::from_u128(500),
            org_id: None,
            movement_type: kind.into(),
            quantity_delta: delta,
            value,
            entry_date: day(5),
            reference_type: reference.map(str::to_string),
            description: "SKU-1 — Widget".into(),
            created_by: Uuid::from_u128(9),
        }
    }

    fn expense(base_total: Decimal) -> PostableExpenseReport {
        PostableExpenseReport {
            id: Uuid::from_u128(400),
            org_id: None,
            number: "EXP-1".into(),
            on: day(5),
            base_total,
            created_by: Uuid::from_u128(9),
        }
    }

    #[test]
    fn a_receipt_incurs_the_cost_and_owes_the_supplier() {
        let m = mapping();
        let entries = receipt_entries(&receipt(dec!(1000.00), dec!(200.00), Decimal::ONE), &m, None);

        assert_eq!(entries.len(), 2);
        assert!(is_balanced(&entries));
        // No inventory mapping, so this installation is on periodic costing and
        // the cost hits the P&L on arrival.
        assert_eq!(net(&entries, m.cost_of_sales), dec!(1000.00));
        // Input tax is recoverable, so it is an asset rather than part of cost.
        assert_eq!(net(&entries, m.purchase_tax), dec!(200.00));
        assert_eq!(net(&entries, m.accounts_payable), dec!(-1200.00));
    }

    #[test]
    fn a_receipt_is_restated_at_the_orders_rate() {
        let m = mapping();
        // EUR 500 of goods on an order struck at 1.10.
        let entries = receipt_entries(&receipt(dec!(500.00), Decimal::ZERO, dec!(1.10)), &m, None);

        assert_eq!(net(&entries, m.cost_of_sales), dec!(550.00));
        assert_eq!(net(&entries, m.accounts_payable), dec!(-550.00));
    }

    #[test]
    fn an_untaxed_receipt_posts_one_leg() {
        let m = mapping();
        let entries = receipt_entries(&receipt(dec!(300.00), Decimal::ZERO, Decimal::ONE), &m, None);

        assert_eq!(entries.len(), 1);
        assert_eq!(net(&entries, m.purchase_tax), Decimal::ZERO);
    }

    // ---- crediting a customer ---------------------------------------------

    #[test]
    fn a_credit_note_takes_back_the_revenue_and_the_receivable() {
        let m = mapping();
        let entries =
            credit_note_entries(&credit_note(dec!(48.00), dec!(8.00), Decimal::ZERO, Decimal::ONE), &m, None);

        assert_eq!(entries.len(), 2);
        assert!(is_balanced(&entries));
        assert_eq!(net(&entries, m.sales_revenue), dec!(40.00));
        assert_eq!(net(&entries, m.tax_payable), dec!(8.00));
        assert_eq!(net(&entries, m.ar), dec!(-48.00));
    }

    /// The property the whole document rests on: crediting an invoice in full
    /// leaves every account exactly where it started.
    #[test]
    fn a_full_credit_undoes_the_invoice_account_for_account() {
        let m = mapping();
        let issued = invoice_entries(&invoice(dec!(1080.00), dec!(80.00), Decimal::ONE), &m);
        let credited =
            credit_note_entries(&credit_note(dec!(1080.00), dec!(80.00), Decimal::ZERO, Decimal::ONE), &m, None);

        for account in [m.ar, m.sales_revenue, m.tax_payable] {
            assert_eq!(
                net(&issued, account),
                -net(&credited, account),
                "account did not net to zero across the invoice and its credit"
            );
        }
    }

    /// Revenue is the remainder, not a restated subtotal — the same reason
    /// issuing derives it that way. A subtotal restated on its own can land a
    /// cent from the total and leave the legs failing to add up to the
    /// receivable they are relieving.
    #[test]
    fn the_revenue_leg_is_whatever_is_left_after_tax() {
        let m = mapping();
        let entries =
            credit_note_entries(&credit_note(dec!(100.01), dec!(16.67), Decimal::ZERO, dec!(1.10)), &m, None);

        assert!(is_balanced(&entries));
        assert_eq!(
            net(&entries, m.sales_revenue) + net(&entries, m.tax_payable),
            -net(&entries, m.ar)
        );
    }

    #[test]
    fn goods_coming_back_put_the_stock_on_and_the_cost_off() {
        let m = mapping();
        let i = inventory();
        let entries =
            credit_note_entries(&credit_note(dec!(48.00), dec!(8.00), dec!(16.00), Decimal::ONE), &m, Some(i));

        assert_eq!(entries.len(), 3);
        assert!(is_balanced(&entries));
        assert_eq!(net(&entries, i.inventory), dec!(16.00));
        assert_eq!(net(&entries, m.cost_of_sales), dec!(-16.00));
        // The money legs are untouched by it: what a customer is credited and
        // what the goods cost are unrelated numbers.
        assert_eq!(net(&entries, m.ar), dec!(-48.00));
    }

    #[test]
    fn a_credit_with_no_goods_writes_no_stock_leg() {
        let m = mapping();
        let entries =
            credit_note_entries(&credit_note(dec!(48.00), dec!(8.00), Decimal::ZERO, Decimal::ONE), &m, Some(inventory()));

        assert_eq!(entries.len(), 2);
        assert_eq!(net(&entries, inventory().inventory), Decimal::ZERO);
    }

    /// Under periodic costing the cost was taken when the goods were *bought*,
    /// so there is nothing here to reverse.
    #[test]
    fn returned_goods_post_no_stock_leg_without_an_inventory_mapping() {
        let m = mapping();
        let entries =
            credit_note_entries(&credit_note(dec!(48.00), dec!(8.00), dec!(16.00), Decimal::ONE), &m, None);

        assert_eq!(entries.len(), 2);
        assert_eq!(net(&entries, m.cost_of_sales), Decimal::ZERO);
    }

    #[test]
    fn the_movement_a_credit_note_creates_posts_nothing() {
        let m = mapping();
        let entries = movement_entries(
            &movement("in", 2, dec!(16.00), Some(CREDIT_NOTE_REFERENCE)),
            &m,
            inventory(),
        );

        assert!(entries.is_empty());
    }

    // ---- perpetual costing ------------------------------------------------

    /// The change this whole feature exists for: goods arriving become an asset
    /// rather than an expense.
    #[test]
    fn with_inventory_mapped_a_receipt_capitalises_the_goods() {
        let m = mapping();
        let i = inventory();
        let entries =
            receipt_entries(&receipt(dec!(1000.00), dec!(200.00), Decimal::ONE), &m, Some(i));

        assert!(is_balanced(&entries));
        assert_eq!(net(&entries, i.inventory), dec!(1000.00));
        // The assertion that would have failed before: nothing reaches the P&L
        // until the goods leave.
        assert_eq!(net(&entries, m.cost_of_sales), Decimal::ZERO);
        assert_eq!(net(&entries, m.accounts_payable), dec!(-1200.00));
    }

    /// Freight and services on the same delivery are a cost on arrival either
    /// way — there is no asset to carry.
    #[test]
    fn only_the_stocked_part_of_a_delivery_is_capitalised() {
        let m = mapping();
        let i = inventory();
        let entries = receipt_entries(
            &mixed_receipt(dec!(800.00), dec!(200.00), Decimal::ZERO, Decimal::ONE),
            &m,
            Some(i),
        );

        assert!(is_balanced(&entries));
        assert_eq!(net(&entries, i.inventory), dec!(800.00));
        assert_eq!(net(&entries, m.cost_of_sales), dec!(200.00));
        assert_eq!(net(&entries, m.accounts_payable), dec!(-1000.00));
    }

    /// Under periodic costing the two halves are indistinguishable, so they are
    /// one leg rather than two to the same account.
    #[test]
    fn without_inventory_mapped_both_halves_are_one_cost() {
        let m = mapping();
        let entries = receipt_entries(
            &mixed_receipt(dec!(800.00), dec!(200.00), Decimal::ZERO, Decimal::ONE),
            &m,
            None,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(net(&entries, m.cost_of_sales), dec!(1000.00));
    }

    // ---- returning goods --------------------------------------------------

    fn purchase_return(
        stocked_net: Decimal,
        expensed_net: Decimal,
        tax: Decimal,
        fx_rate: Decimal,
    ) -> PostableReturn {
        PostableReturn {
            id: Uuid::from_u128(600),
            org_id: None,
            number: "PR-1".into(),
            return_date: day(9),
            fx_rate,
            stocked_net,
            expensed_net,
            tax,
            created_by: Uuid::from_u128(9),
        }
    }

    #[test]
    fn a_return_gives_back_the_stock_and_reduces_the_payable() {
        let m = mapping();
        let i = inventory();
        let entries =
            return_entries(&purchase_return(dec!(55.00), Decimal::ZERO, dec!(11.00), Decimal::ONE), &m, Some(i));

        assert!(is_balanced(&entries));
        assert_eq!(net(&entries, i.inventory), dec!(-55.00));
        // Input tax goes back with the goods — leaving it would reclaim tax on
        // something that was sent back.
        assert_eq!(net(&entries, m.purchase_tax), dec!(-11.00));
        assert_eq!(net(&entries, m.accounts_payable), dec!(66.00));
    }

    /// The property the whole valuation rests on: a return is worth exactly what
    /// the receipt brought it in at, so the two are mirror images and no
    /// variance account is needed.
    #[test]
    fn a_return_is_the_exact_mirror_of_the_receipt_that_brought_it_in() {
        let m = mapping();
        let i = inventory();
        let received =
            receipt_entries(&mixed_receipt(dec!(800.00), dec!(200.00), dec!(50.00), dec!(1.10)), &m, Some(i));
        let returned =
            return_entries(&purchase_return(dec!(800.00), dec!(200.00), dec!(50.00), dec!(1.10)), &m, Some(i));

        for account in [i.inventory, m.cost_of_sales, m.purchase_tax, m.accounts_payable] {
            assert_eq!(
                net(&received, account),
                -net(&returned, account),
                "account did not net to zero across receipt and return"
            );
        }
    }

    #[test]
    fn a_return_under_periodic_costing_credits_the_cost() {
        let m = mapping();
        let entries =
            return_entries(&purchase_return(dec!(55.00), Decimal::ZERO, Decimal::ZERO, Decimal::ONE), &m, None);

        assert_eq!(entries.len(), 1);
        assert_eq!(net(&entries, m.cost_of_sales), dec!(-55.00));
        assert_eq!(net(&entries, m.accounts_payable), dec!(55.00));
    }

    #[test]
    fn a_return_is_restated_at_the_orders_rate() {
        let m = mapping();
        let i = inventory();
        // EUR 500 of goods on an order struck at 1.10.
        let entries =
            return_entries(&purchase_return(dec!(500.00), Decimal::ZERO, Decimal::ZERO, dec!(1.10)), &m, Some(i));

        assert_eq!(net(&entries, i.inventory), dec!(-550.00));
    }

    /// The return posts its own inventory leg, so the movement it creates must
    /// not post a second one.
    #[test]
    fn the_movement_a_return_creates_posts_nothing() {
        let m = mapping();
        let entries =
            movement_entries(&movement("out", -10, dec!(55.00), Some(RETURN_REFERENCE)), &m, inventory());

        assert!(entries.is_empty());
    }

    #[test]
    fn a_return_and_a_receipt_cannot_share_a_posting_key() {
        let m = mapping();
        let i = inventory();
        let mut keys: Vec<String> =
            receipt_entries(&receipt(dec!(100.00), dec!(20.00), Decimal::ONE), &m, Some(i))
                .into_iter()
                .chain(return_entries(
                    &purchase_return(dec!(100.00), Decimal::ZERO, dec!(20.00), Decimal::ONE),
                    &m,
                    Some(i),
                ))
                .map(|e| e.posting_key)
                .collect();

        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before, "a return key collided with a receipt key");
    }

    #[test]
    fn stock_going_out_is_the_cost_of_the_sale() {
        let m = mapping();
        let i = inventory();
        let entries = movement_entries(&movement("out", -60, dec!(270.00), None), &m, i);

        assert_eq!(entries.len(), 1);
        assert!(is_balanced(&entries));
        assert_eq!(net(&entries, m.cost_of_sales), dec!(270.00));
        assert_eq!(net(&entries, i.inventory), dec!(-270.00));
    }

    /// The receipt already debited Inventory. Posting here as well would count
    /// the same delivery twice.
    #[test]
    fn stock_arriving_from_a_receipt_posts_nothing() {
        let m = mapping();
        let entries =
            movement_entries(&movement("in", 100, dec!(400.00), Some(RECEIPT_REFERENCE)), &m, inventory());

        assert!(entries.is_empty());
    }

    #[test]
    fn stock_arriving_by_hand_finds_its_other_leg_in_the_adjustment_account() {
        let m = mapping();
        let i = inventory();
        let entries = movement_entries(&movement("in", 20, dec!(80.00), None), &m, i);

        assert_eq!(net(&entries, i.inventory), dec!(80.00));
        assert_eq!(net(&entries, i.inventory_adjustment), dec!(-80.00));
    }

    /// Shrinkage is not a sale. Burying it in cost of sales would hide it from
    /// the person whose job it is to notice.
    #[test]
    fn a_shortfall_is_an_adjustment_rather_than_a_cost_of_sale() {
        let m = mapping();
        let i = inventory();
        let entries = movement_entries(&movement("adjustment", -4, dec!(18.00), None), &m, i);

        assert_eq!(net(&entries, i.inventory_adjustment), dec!(18.00));
        assert_eq!(net(&entries, i.inventory), dec!(-18.00));
        assert_eq!(net(&entries, m.cost_of_sales), Decimal::ZERO);
    }

    /// A cancelled sale putting goods back is not shrinkage. Crediting Inventory
    /// adjustment would book it as such *and* leave the cost of sale it reverses
    /// standing in the P&L.
    #[test]
    fn goods_back_from_a_cancelled_invoice_relieve_the_cost_of_sale() {
        let m = mapping();
        let i = inventory();
        let entries =
            movement_entries(&movement("in", 10, dec!(45.00), Some(INVOICE_REFERENCE)), &m, i);

        assert_eq!(entries.len(), 1);
        assert!(is_balanced(&entries));
        assert_eq!(net(&entries, i.inventory), dec!(45.00));
        assert_eq!(net(&entries, m.cost_of_sales), dec!(-45.00));
        assert_eq!(net(&entries, i.inventory_adjustment), Decimal::ZERO);
    }

    /// Going the other way it is an ordinary sale: the reference changes nothing,
    /// because the cost of a sale is the cost of a sale.
    #[test]
    fn stock_leaving_for_an_invoice_is_still_a_cost_of_sale() {
        let m = mapping();
        let i = inventory();
        let entries =
            movement_entries(&movement("out", -10, dec!(45.00), Some(INVOICE_REFERENCE)), &m, i);

        assert_eq!(net(&entries, m.cost_of_sales), dec!(45.00));
        assert_eq!(net(&entries, i.inventory), dec!(-45.00));
    }

    #[test]
    fn a_transfer_posts_nothing_because_nothing_changed_hands() {
        let m = mapping();
        let entries =
            movement_entries(&movement("transfer", -10, dec!(45.00), None), &m, inventory());

        assert!(entries.is_empty());
    }

    #[test]
    fn a_movement_of_uncosted_stock_writes_no_entry() {
        let m = mapping();
        let entries = movement_entries(&movement("out", -10, Decimal::ZERO, None), &m, inventory());

        assert!(entries.is_empty());
    }

    /// One key per movement, so a retry cannot double-post it.
    #[test]
    fn a_movement_keys_on_its_own_id() {
        let m = mapping();
        let entries = movement_entries(&movement("out", -60, dec!(270.00), None), &m, inventory());

        assert_eq!(entries[0].posting_key, format!("{MOVEMENT_REFERENCE}:{}", Uuid::from_u128(500)));
    }

    /// Adding the inventory pair must not make an otherwise-configured
    /// installation report itself as unconfigured.
    #[test]
    fn the_inventory_roles_are_not_required_for_posting() {
        let accounts = PostingAccounts {
            ar_account_id: Some(Uuid::from_u128(1)),
            bank_account_id: Some(Uuid::from_u128(2)),
            sales_revenue_account_id: Some(Uuid::from_u128(3)),
            tax_payable_account_id: Some(Uuid::from_u128(4)),
            fx_gain_loss_account_id: Some(Uuid::from_u128(5)),
            accounts_payable_account_id: Some(Uuid::from_u128(6)),
            cost_of_sales_account_id: Some(Uuid::from_u128(7)),
            purchase_tax_account_id: Some(Uuid::from_u128(8)),
            employee_payable_account_id: Some(Uuid::from_u128(9)),
            employee_expense_account_id: Some(Uuid::from_u128(10)),
            inventory_account_id: None,
            inventory_adjustment_account_id: None,
        };

        assert!(accounts.is_complete(), "{:?}", accounts.missing_roles());
        assert!(!accounts.is_perpetual());
        assert!(AccountMapping::complete(&accounts).is_some());
        assert!(InventoryMapping::complete(&accounts).is_none());
    }

    /// Half a mapping is no mapping: posting one leg of a stock movement and
    /// leaving the other unplaced would put the books out by its value.
    #[test]
    fn one_inventory_account_alone_does_not_switch_costing_on() {
        let accounts = PostingAccounts {
            inventory_account_id: Some(Uuid::from_u128(11)),
            ..Default::default()
        };

        assert!(!accounts.is_perpetual());
    }

    #[test]
    fn paying_a_vendor_clears_the_payable() {
        let m = mapping();
        let entries = vendor_payment_entries(&payment(dec!(600.00), Decimal::ZERO), &m);

        assert_eq!(entries.len(), 1);
        // Money out, debt discharged — the opposite of a customer paying us.
        assert_eq!(net(&entries, m.bank), dec!(-600.00));
        assert_eq!(net(&entries, m.accounts_payable), dec!(600.00));
    }

    #[test]
    fn a_payable_settled_cheaply_is_a_gain_and_still_clears_in_full() {
        let m = mapping();
        // EUR 1,000 ordered at 1.10 (payable 1,100), paid at 1.05 => 1,050 out.
        let entries = vendor_payment_entries(&payment(dec!(1050.00), dec!(50.00)), &m);

        assert_eq!(net(&entries, m.bank), dec!(-1050.00));
        // The debt clears at what the order booked, not at what was paid.
        assert_eq!(net(&entries, m.accounts_payable), dec!(1100.00));
        assert_eq!(net(&entries, m.fx_gain_loss), dec!(-50.00), "a gain credits the FX account");
    }

    #[test]
    fn a_payable_settled_dearly_is_a_loss_and_still_clears_in_full() {
        let m = mapping();
        // Ordered at 1.10 (payable 1,100), paid at 1.15 => 1,150 out.
        let entries = vendor_payment_entries(&payment(dec!(1150.00), dec!(-50.00)), &m);

        assert_eq!(net(&entries, m.bank), dec!(-1150.00));
        assert_eq!(net(&entries, m.accounts_payable), dec!(1100.00));
        assert_eq!(net(&entries, m.fx_gain_loss), dec!(50.00), "a loss debits the FX account");
    }

    #[test]
    fn the_fx_leg_is_the_same_shape_for_a_receivable_and_a_payable() {
        let m = mapping();
        let gain = payment(dec!(1000.00), dec!(50.00));

        // Whichever side the control account sits on, a gain debits it and
        // credits the FX account. The one rule is why both directions can share
        // an implementation at all.
        let sales = payment_entries(&gain, &m);
        let purchase = vendor_payment_entries(&gain, &m);

        let fx_of = |entries: &[PlannedEntry]| {
            entries.iter().find(|e| e.posting_key.ends_with(":fx")).cloned().unwrap()
        };
        assert_eq!(fx_of(&sales).debit_account_id, m.ar);
        assert_eq!(fx_of(&purchase).debit_account_id, m.accounts_payable);
        assert_eq!(fx_of(&sales).credit_account_id, m.fx_gain_loss);
        assert_eq!(fx_of(&purchase).credit_account_id, m.fx_gain_loss);
    }

    #[test]
    fn an_expense_is_owed_on_approval_and_cleared_on_reimbursement() {
        let m = mapping();
        let report = expense(dec!(87.50));

        let approval = expense_approval_entries(&report, &m);
        assert_eq!(net(&approval, m.employee_expense), dec!(87.50));
        assert_eq!(net(&approval, m.employee_payable), dec!(-87.50));

        let reimbursement = expense_reimbursement_entries(&report, &m);
        assert_eq!(net(&reimbursement, m.bank), dec!(-87.50));

        // Across both, the employee is owed nothing and the cost has landed.
        let both: Vec<_> = approval.into_iter().chain(reimbursement).collect();
        assert_eq!(net(&both, m.employee_payable), Decimal::ZERO);
        assert_eq!(net(&both, m.employee_expense), dec!(87.50));
    }

    #[test]
    fn every_event_keys_its_entries_distinctly() {
        let m = mapping();
        let mut keys: Vec<String> = invoice_entries(&invoice(dec!(120.00), dec!(20.00), Decimal::ONE), &m)
            .into_iter()
            .chain(payment_entries(&payment(dec!(100.00), dec!(5.00)), &m))
            .chain(vendor_payment_entries(&payment(dec!(100.00), dec!(5.00)), &m))
            .chain(receipt_entries(&receipt(dec!(100.00), dec!(20.00), Decimal::ONE), &m, None))
            .chain(expense_approval_entries(&expense(dec!(50.00)), &m))
            .chain(expense_reimbursement_entries(&expense(dec!(50.00)), &m))
            .map(|e| e.posting_key)
            .collect();

        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        // Sales and vendor payments share an id in this test, so a key that did
        // not name its own event would collide here rather than in production.
        assert_eq!(keys.len(), before, "two different events produced the same posting key");
    }

    #[test]
    fn a_mapping_is_only_complete_when_every_role_is_filled() {
        let mut accounts = PostingAccounts::default();
        // Every *required* role, which is not every role: the inventory pair is
        // opt-in, so an installation is not reported as unconfigured for having
        // left it alone.
        let required = POSTING_ROLES.iter().filter(|role| role.required).count();
        assert_eq!(accounts.missing_roles().len(), required);
        assert_eq!(required, POSTING_ROLES.len() - 2);
        assert!(AccountMapping::complete(&accounts).is_none());

        // Exactly the five the sales cycle needs — the state an installation
        // configured before the purchase and expense cycles existed is in.
        accounts.ar_account_id = Some(Uuid::from_u128(1));
        accounts.bank_account_id = Some(Uuid::from_u128(2));
        accounts.sales_revenue_account_id = Some(Uuid::from_u128(3));
        accounts.tax_payable_account_id = Some(Uuid::from_u128(4));
        accounts.fx_gain_loss_account_id = Some(Uuid::from_u128(5));

        // Posting is off for *everything*, sales included. A partial mapping
        // would post lopsided entries, which is worse than posting nothing —
        // and this is the upgrade case worth being loud about.
        assert!(!accounts.is_complete());
        assert_eq!(
            accounts.missing_roles(),
            vec![
                "Accounts payable",
                "Cost of sales",
                "Purchase tax",
                "Employee payable",
                "Employee expense",
            ]
        );
        assert!(AccountMapping::complete(&accounts).is_none());

        accounts.accounts_payable_account_id = Some(Uuid::from_u128(6));
        accounts.cost_of_sales_account_id = Some(Uuid::from_u128(7));
        accounts.purchase_tax_account_id = Some(Uuid::from_u128(8));
        accounts.employee_payable_account_id = Some(Uuid::from_u128(9));
        accounts.employee_expense_account_id = Some(Uuid::from_u128(10));

        assert!(accounts.is_complete());
        assert_eq!(AccountMapping::complete(&accounts), Some(mapping()));
    }

    #[test]
    fn posting_keys_name_the_event_that_caused_them() {
        let m = mapping();
        let entries = invoice_entries(&invoice(dec!(1200.00), dec!(200.00), Decimal::ONE), &m);

        assert_eq!(entries[0].posting_key, format!("sales_invoice:{}:revenue", Uuid::from_u128(100)));
        assert_eq!(entries[1].posting_key, format!("sales_invoice:{}:tax", Uuid::from_u128(100)));
        assert!(entries.iter().all(|e| e.reference_type == INVOICE_REFERENCE));
    }
}
