//! A draft invoice cannot take money.
//!
//! It could, and two things followed. The receivable it relieved had never been
//! raised, so the ledger showed cash in against a negative asset and no revenue
//! at all. And `settle_invoice` wrote the derived status straight through
//! `update_settlement`, bypassing `set_status` — so a single unit paid against a
//! draft moved it to `sent` without shipping its goods or posting its revenue,
//! and a full payment moved it to `paid`, which is terminal: it could then never
//! be issued and never be cancelled.
//!
//! The second of those defeated the order lifecycle guard outright. That guard
//! asks whether an order has an *issued* invoice before letting it claim its
//! goods have gone; a part-paid draft answered yes, having issued nothing.

mod common;

use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;

const ROLES: [(&str, &str, &str); 12] = [
    ("1100", "Accounts receivable", "asset"),
    ("1000", "Bank", "asset"),
    ("4000", "Sales revenue", "revenue"),
    ("2100", "Tax payable", "liability"),
    ("4900", "Foreign exchange gain/loss", "revenue"),
    ("2000", "Accounts payable", "liability"),
    ("5000", "Cost of sales", "expense"),
    ("1300", "Purchase tax", "asset"),
    ("2200", "Employee payable", "liability"),
    ("5100", "Employee expense", "expense"),
    ("1400", "Inventory", "asset"),
    ("5200", "Inventory adjustment", "expense"),
];

const RECEIVABLE: usize = 0;
const BANK: usize = 1;
const REVENUE: usize = 2;

async fn configure(app: &TestApp) -> Vec<String> {
    let mut ids = Vec::new();
    for (code, name, account_type) in ROLES {
        ids.push(
            app.create(
                "/accounting/accounts",
                json!({ "account_code": code, "account_name": name, "account_type": account_type }),
            )
            .await,
        );
    }

    let response = app
        .put(
            "/accounting/posting-accounts",
            json!({
                "ar_account_id": ids[0], "bank_account_id": ids[1],
                "sales_revenue_account_id": ids[2], "tax_payable_account_id": ids[3],
                "fx_gain_loss_account_id": ids[4], "accounts_payable_account_id": ids[5],
                "cost_of_sales_account_id": ids[6], "purchase_tax_account_id": ids[7],
                "employee_payable_account_id": ids[8], "employee_expense_account_id": ids[9],
                "inventory_account_id": ids[10], "inventory_adjustment_account_id": ids[11]
            }),
        )
        .await;
    assert!(response.status.is_success(), "{}", response.body);
    ids
}

async fn dispatch_from(app: &TestApp, warehouse: &str) {
    let response = app
        .put("/settings/organization", json!({ "default_dispatch_warehouse_id": warehouse }))
        .await;
    assert!(response.status.is_success(), "{}", response.body);
}

async fn entries(app: &TestApp) -> Vec<Value> {
    app.get("/accounting/ledger-entries?per_page=100").await.rows().clone()
}

async fn net(app: &TestApp, account: &str) -> f64 {
    entries(app)
        .await
        .iter()
        .map(|entry| {
            let amount: f64 = entry["amount"].as_str().unwrap().parse().unwrap();
            match (entry["debit_account_id"].as_str(), entry["credit_account_id"].as_str()) {
                (Some(d), _) if d == account => amount,
                (_, Some(c)) if c == account => -amount,
                _ => 0.0,
            }
        })
        .sum()
}

/// (quantity, reserved, available) for a product in a warehouse.
async fn level(app: &TestApp, warehouse: &str, product: &str) -> (i64, i64, i64) {
    let rows = app.get(&format!("/inventory/warehouses/{warehouse}/stock")).await;
    let row = rows
        .rows()
        .iter()
        .find(|row| row["product_id"] == product)
        .unwrap_or_else(|| panic!("no stock row: {}", rows.body))
        .clone();

    (
        row["quantity"].as_i64().unwrap(),
        row["reserved_quantity"].as_i64().unwrap(),
        row["available"].as_i64().unwrap(),
    )
}

async fn stocked_product(app: &TestApp, warehouse: &str, quantity: i32) -> String {
    let product = app
        .create("/inventory/products", json!({ "sku": "SKU-1", "name": "Widget", "sale_price": 20.00 }))
        .await;
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme" })).await;

    let created = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor, "order_date": "2026-02-01",
                "lines": [{ "product_id": product, "description": "Widget",
                            "quantity": quantity, "unit_price": 4.00, "tax_rate": 0 }]
            }),
        )
        .await;
    let po = created.id();
    let po_line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();
    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;
    app.post(
        "/purchasing/goods-receipts",
        json!({
            "po_id": po, "warehouse_id": warehouse, "receipt_date": "2026-02-05",
            "lines": [{ "po_line_id": po_line, "quantity_received": quantity }]
        }),
    )
    .await;

    product
}

/// A draft invoice for 200.00, raised directly against a customer.
async fn draft_invoice(app: &TestApp) -> String {
    let customer = app.customer().await;
    let created = app
        .post(
            "/sales/invoices",
            json!({
                "customer_id": customer, "issue_date": "2026-03-01", "due_date": "2026-03-31",
                "lines": [{ "description": "Consulting", "quantity": 1,
                            "unit_price": 200.00, "tax_rate": 0 }]
            }),
        )
        .await;
    assert!(created.status.is_success(), "{}", created.body);
    created.id()
}

async fn set_invoice(app: &TestApp, invoice: &str, status: &str) -> common::TestResponse {
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": status })).await
}

async fn pay(app: &TestApp, invoice: &str, amount: f64) -> common::TestResponse {
    app.post(
        "/sales/payments",
        json!({ "invoice_id": invoice, "amount": amount,
                "payment_method": "bank_transfer", "payment_date": "2026-03-05" }),
    )
    .await
}

#[sqlx::test]
async fn a_draft_invoice_refuses_payment(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;

    let invoice = draft_invoice(&app).await;
    let refused = pay(&app, &invoice, 200.00).await;

    assert_eq!(refused.status, 409, "{}", refused.body);
    // Says what to do about it, rather than "cannot take further payments" —
    // which reads oddly to a document that has taken none.
    assert!(refused.error_message().contains("issue it"), "{}", refused.error_message());
}

#[sqlx::test]
async fn the_refused_payment_posts_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app).await;

    let invoice = draft_invoice(&app).await;
    assert!(!pay(&app, &invoice, 200.00).await.status.is_success());

    // The whole ledger, not just one account: a draft has recognised no revenue
    // and raised no receivable, so relieving one could only ever have taken the
    // books somewhere they had never been.
    assert!(entries(&app).await.is_empty(), "a refused payment posted something");
    assert_eq!(net(&app, &ids[BANK]).await, 0.00);
    assert_eq!(net(&app, &ids[RECEIVABLE]).await, 0.00);
    assert_eq!(net(&app, &ids[REVENUE]).await, 0.00);
}

#[sqlx::test]
async fn the_invoice_stays_a_draft_owing_the_whole_amount(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;

    let invoice = draft_invoice(&app).await;
    assert!(!pay(&app, &invoice, 200.00).await.status.is_success());

    // It used to land on `paid` — terminal, so it could never afterwards be
    // issued or cancelled, and its revenue was unrecognisable for good.
    let after = app.get(&format!("/sales/invoices/{invoice}")).await;
    assert_eq!(after.field("status"), "draft");
    after.assert_money("amount_paid", "0.00");
    after.assert_money("amount_due", "200.00");
}

/// The refusal is about the invoice's state, not about the payment.
#[sqlx::test]
async fn the_same_payment_succeeds_once_it_is_issued(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app).await;

    let invoice = draft_invoice(&app).await;
    assert!(!pay(&app, &invoice, 200.00).await.status.is_success());

    assert!(set_invoice(&app, &invoice, "sent").await.status.is_success());
    assert!(pay(&app, &invoice, 200.00).await.status.is_success());

    let after = app.get(&format!("/sales/invoices/{invoice}")).await;
    assert_eq!(after.field("status"), "paid");
    after.assert_money("amount_due", "0.00");
    // Revenue recognised by the issue, the receivable relieved by the money.
    assert_eq!(net(&app, &ids[REVENUE]).await, -200.00);
    assert_eq!(net(&app, &ids[RECEIVABLE]).await, 0.00);
    assert_eq!(net(&app, &ids[BANK]).await, 200.00);
}

/// The defect that made this worth fixing rather than tidying: a part-payment
/// used to promote a draft to `sent` through the settlement write, which skips
/// shipping and posting entirely — and an order whose invoice reads `sent` may
/// claim its goods have gone.
#[sqlx::test]
async fn a_part_payment_cannot_issue_an_invoice_behind_the_lifecycle_guard(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let customer = app.customer().await;
    let order = app
        .post(
            "/sales/orders",
            json!({
                "customer_id": customer, "order_date": "2026-03-01",
                "lines": [{ "product_id": product, "description": "Widget",
                            "quantity": 10, "unit_price": 20.00, "tax_rate": 0 }]
            }),
        )
        .await
        .id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed", "processing"]).await;

    let invoice = app
        .post(
            &format!("/sales/orders/{order}/convert-to-invoice"),
            json!({ "payment_terms_days": 30 }),
        )
        .await
        .id();

    // One unit against two hundred used to be enough.
    let refused = pay(&app, &invoice, 1.00).await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert_eq!(app.get(&format!("/sales/invoices/{invoice}")).await.field("status"), "draft");

    // Nothing shipped, so the order still cannot say its goods have gone —
    // which is the state the lifecycle guard exists to keep unreachable.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));
    let shipped = app.put(&format!("/sales/orders/{order}/status"), json!({ "status": "shipped" })).await;
    assert_eq!(shipped.status, 409, "{}", shipped.body);
}

#[sqlx::test]
async fn cancelled_and_paid_invoices_refuse_payment_as_before(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;

    let cancelled = draft_invoice(&app).await;
    assert!(set_invoice(&app, &cancelled, "cancelled").await.status.is_success());
    let refused = pay(&app, &cancelled, 50.00).await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(
        refused.error_message().contains("cannot take further payments"),
        "{}",
        refused.error_message()
    );

    let paid = draft_invoice(&app).await;
    assert!(set_invoice(&app, &paid, "sent").await.status.is_success());
    assert!(pay(&app, &paid, 200.00).await.status.is_success());
    assert_eq!(pay(&app, &paid, 10.00).await.status, 409);
}

/// The settlement arm that has to keep working: reversing a payment makes a paid
/// invoice a live receivable again, rather than leaving it marked paid while
/// owing the full amount.
#[sqlx::test]
async fn reversing_a_payment_still_reopens_the_receivable(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;

    // Its own invoice, due well after any plausible run date: the derivation
    // answers `overdue` rather than `sent` once the due date has passed, and it
    // is the `sent` branch this test is about.
    let customer = app.customer().await;
    let invoice = app
        .post(
            "/sales/invoices",
            json!({
                "customer_id": customer, "issue_date": "2026-03-01", "due_date": "2099-12-31",
                "lines": [{ "description": "Consulting", "quantity": 1,
                            "unit_price": 200.00, "tax_rate": 0 }]
            }),
        )
        .await
        .id();
    assert!(set_invoice(&app, &invoice, "sent").await.status.is_success());
    let payment = pay(&app, &invoice, 200.00).await.id();
    assert_eq!(app.get(&format!("/sales/invoices/{invoice}")).await.field("status"), "paid");

    app.delete(&format!("/sales/payments/{payment}")).await;

    let after = app.get(&format!("/sales/invoices/{invoice}")).await;
    assert_eq!(after.field("status"), "sent");
    after.assert_money("amount_due", "200.00");
    after.assert_money("amount_paid", "0.00");
}

/// The other document that settles an invoice, unaffected: it refuses drafts on
/// its own account and reaches `paid` through the same derivation.
#[sqlx::test]
async fn crediting_still_settles_an_issued_invoice(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;

    let invoice = draft_invoice(&app).await;
    let refused = app
        .post(
            "/sales/credit-notes",
            json!({ "invoice_id": invoice, "note_date": "2026-03-05",
                    "lines": [{ "description": "Consulting", "quantity": 1, "unit_price": 200.00 }] }),
        )
        .await;
    assert!(!refused.status.is_success(), "a draft was credited");

    assert!(set_invoice(&app, &invoice, "sent").await.status.is_success());
    let line = app.get(&format!("/sales/invoices/{invoice}")).await.data()["lines"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let credited = app
        .post(
            "/sales/credit-notes",
            json!({ "invoice_id": invoice, "note_date": "2026-03-05",
                    "lines": [{ "invoice_line_id": line, "description": "Consulting",
                                "quantity": 1, "unit_price": 200.00, "tax_rate": 0 }] }),
        )
        .await;
    assert!(credited.status.is_success(), "{}", credited.body);

    let after = app.get(&format!("/sales/invoices/{invoice}")).await;
    assert_eq!(after.field("status"), "paid");
    after.assert_money("amount_due", "0.00");
}
