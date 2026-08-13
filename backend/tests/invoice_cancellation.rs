//! Cancelling an invoice undoes issuing it — no more, and no less.
//!
//! It used to do more and less at once. Cancelling a **draft** ran the whole
//! unwind: it posted a reversal of a posting that never happened and brought
//! stock in that never went out, inventing goods and blending them into the
//! moving average. Cancelling an **issued** invoice unwound correctly but left
//! its order permanently unbillable — any invoice at all blocked a second one —
//! and gave the goods back to the shelf without giving the order its hold back.

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
const REVENUE: usize = 2;
const COST: usize = 6;
const INVENTORY: usize = 10;

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

/// A confirmed order sitting in `processing`, holding its goods.
async fn confirmed_order(app: &TestApp, product: &str, quantity: i32) -> String {
    let customer = app.customer().await;
    let created = app
        .post(
            "/sales/orders",
            json!({
                "customer_id": customer, "order_date": "2026-03-01",
                "lines": [{ "product_id": product, "description": "Widget",
                            "quantity": quantity, "unit_price": 20.00, "tax_rate": 0 }]
            }),
        )
        .await;
    assert!(created.status.is_success(), "{}", created.body);
    let order = created.id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed", "processing"]).await;
    order
}

async fn invoice_for(app: &TestApp, order: &str) -> common::TestResponse {
    app.post(
        &format!("/sales/orders/{order}/convert-to-invoice"),
        json!({ "issue_date": "2026-03-02", "payment_terms_days": 30 }),
    )
    .await
}

async fn set_invoice(app: &TestApp, invoice: &str, status: &str) -> common::TestResponse {
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": status })).await
}

async fn set_order(app: &TestApp, order: &str, status: &str) -> common::TestResponse {
    app.put(&format!("/sales/orders/{order}/status"), json!({ "status": status })).await
}

/// Raises an invoice from the order and issues it.
async fn issued_invoice(app: &TestApp, order: &str) -> String {
    let raised = invoice_for(app, order).await;
    assert!(raised.status.is_success(), "{}", raised.body);
    let invoice = raised.id();
    let issued = set_invoice(app, &invoice, "sent").await;
    assert!(issued.status.is_success(), "{}", issued.body);
    invoice
}

// ------------------------------------------------ cancelling a draft is inert

#[sqlx::test]
async fn cancelling_a_draft_moves_no_stock(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let raised = invoice_for(&app, &order).await;
    assert!(raised.status.is_success(), "{}", raised.body);

    let cancelled = set_invoice(&app, &raised.id(), "cancelled").await;
    assert!(cancelled.status.is_success(), "{}", cancelled.body);

    // A draft shipped nothing, so there is nothing to bring back. This used to
    // read 110 — ten units conjured onto the shelf out of a document that never
    // moved anything.
    assert_eq!(level(&app, &warehouse, &product).await.0, 100);
}

#[sqlx::test]
async fn cancelling_a_draft_posts_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let raised = invoice_for(&app, &order).await;
    let before = entries(&app).await.len();

    assert!(set_invoice(&app, &raised.id(), "cancelled").await.status.is_success());

    // A draft recognises no revenue and creates no receivable, so reversing it
    // could only ever have taken the books somewhere they had never been.
    assert_eq!(entries(&app).await.len(), before, "a draft cancellation posted something");
    assert_eq!(net(&app, &ids[REVENUE]).await, 0.00);
    assert_eq!(net(&app, &ids[RECEIVABLE]).await, 0.00);
    // Only the goods receipt, untouched.
    assert_eq!(net(&app, &ids[INVENTORY]).await, 400.00);
}

#[sqlx::test]
async fn cancelling_a_draft_leaves_the_hold_alone(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let raised = invoice_for(&app, &order).await;
    assert!(set_invoice(&app, &raised.id(), "cancelled").await.status.is_success());

    // Issuing never released anything, so there is nothing to give back — the
    // order's ten are still held exactly once.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));
}

// ------------------------------------------------------------- re-invoicing

#[sqlx::test]
async fn cancelling_frees_the_order_to_be_invoiced_again(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let first = issued_invoice(&app, &order).await;
    assert!(set_invoice(&app, &first, "cancelled").await.status.is_success());

    // The defect this file exists for: the order was billable exactly once, ever,
    // and with nothing issued it could not be shipped or delivered either.
    let second = invoice_for(&app, &order).await;
    assert!(second.status.is_success(), "{}", second.body);
    assert_ne!(second.field("invoice_number"), "", "the replacement has its own number");
    assert_ne!(second.id(), first);
}

#[sqlx::test]
async fn the_replacement_invoice_ships_the_goods_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let first = issued_invoice(&app, &order).await;
    assert_eq!(level(&app, &warehouse, &product).await.0, 90);

    assert!(set_invoice(&app, &first, "cancelled").await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await.0, 100);

    let second = issued_invoice(&app, &order).await;
    assert_ne!(second, first);

    // Out, back, and out again — the shelf and the books both land where one
    // shipment leaves them, not two.
    assert_eq!(level(&app, &warehouse, &product).await, (90, 0, 90));
    assert_eq!(net(&app, &ids[REVENUE]).await, -200.00);
    assert_eq!(net(&app, &ids[COST]).await, 40.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 360.00);
}

/// An order may be billed as many times as it takes; what it may not do is bill
/// the same units twice. A live invoice covering the whole order therefore
/// leaves nothing outstanding, and a second one has nothing to bill.
#[sqlx::test]
async fn a_fully_invoiced_order_has_nothing_left_to_bill(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;

    // A draft still counts: it has not been withdrawn, so its units are spoken
    // for and a second invoice would bill them twice.
    let first = invoice_for(&app, &order).await;
    assert!(first.status.is_success(), "{}", first.body);
    let refused = invoice_for(&app, &order).await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(
        refused.error_message().contains("nothing left to bill"),
        "{}",
        refused.error_message()
    );

    // And so does an issued one.
    assert!(set_invoice(&app, &first.id(), "sent").await.status.is_success());
    assert_eq!(invoice_for(&app, &order).await.status, 409);

    // Cancelling it gives the units back, and the order is billable again —
    // which is what `cancelling_frees_the_order_to_be_invoiced_again` pins.
    assert!(set_invoice(&app, &first.id(), "cancelled").await.status.is_success());
    assert!(invoice_for(&app, &order).await.status.is_success());
}

/// The lookup behind all of this returns *every* invoice for the order, so the
/// lifecycle guard has to pick out the issued one rather than whichever row came
/// back first.
#[sqlx::test]
async fn shipping_reads_the_live_invoice_not_a_stale_one(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let first = issued_invoice(&app, &order).await;
    assert!(set_invoice(&app, &first, "cancelled").await.status.is_success());

    // Nothing issued stands, so the goods have not left.
    let refused = set_order(&app, &order, "shipped").await;
    assert_eq!(refused.status, 409, "{}", refused.body);

    // Issue a replacement and the older cancelled row must not get in its way.
    issued_invoice(&app, &order).await;
    assert!(set_order(&app, &order, "shipped").await.status.is_success());
    assert!(set_order(&app, &order, "delivered").await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await, (90, 0, 90));
}

// --------------------------------------------------------------- the re-hold

#[sqlx::test]
async fn cancelling_gives_the_order_its_hold_back(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let invoice = issued_invoice(&app, &order).await;
    // Issuing released the hold and shipped the goods.
    assert_eq!(level(&app, &warehouse, &product).await, (90, 0, 90));

    assert!(set_invoice(&app, &invoice, "cancelled").await.status.is_success());

    // Cancelling undoes issuing on both sides: the goods are back and so is the
    // hold. Without this the order still expects ten and protects none of them.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));
}

#[sqlx::test]
async fn the_hold_comes_back_only_as_far_as_the_shelf_allows(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let invoice = issued_invoice(&app, &order).await;

    // Somebody else takes almost everything while the invoice stands: 90 on the
    // shelf, 86 sold off, 4 left.
    let other = confirmed_order(&app, &product, 86).await;
    issued_invoice(&app, &other).await;
    assert_eq!(level(&app, &warehouse, &product).await, (4, 0, 4));

    assert!(set_invoice(&app, &invoice, "cancelled").await.status.is_success());

    // Fourteen back on the shelf, but only what is available is held — the same
    // rule confirming an order short of stock follows.
    assert_eq!(level(&app, &warehouse, &product).await, (14, 10, 4));
}

#[sqlx::test]
async fn a_cancelled_order_gets_no_hold_back(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let invoice = issued_invoice(&app, &order).await;
    assert!(set_order(&app, &order, "cancelled").await.status.is_success());

    assert!(set_invoice(&app, &invoice, "cancelled").await.status.is_success());

    // The goods come back to the shelf, but nothing holds them for a dead
    // order — that would strand stock against a document nobody will fulfil.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 0, 100));
}

// ------------------------------------------------------------- the refusal

#[sqlx::test]
async fn cancelling_is_refused_once_the_order_says_the_goods_went(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    let invoice = issued_invoice(&app, &order).await;
    assert!(set_order(&app, &order, "shipped").await.status.is_success());

    let refused = set_invoice(&app, &invoice, "cancelled").await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(refused.error_message().contains("credit note"), "{}", refused.error_message());

    // Refused before the write, so nothing moved and nothing was reversed.
    assert_eq!(app.get(&format!("/sales/invoices/{invoice}")).await.field("status"), "sent");
    assert_eq!(level(&app, &warehouse, &product).await, (90, 0, 90));

    // The same once it is delivered.
    assert!(set_order(&app, &order, "delivered").await.status.is_success());
    assert_eq!(set_invoice(&app, &invoice, "cancelled").await.status, 409);
}

/// The refusal exists to protect the shelf, so where invoicing ships nothing it
/// does not exist at all — the same opt-in as everything else in dispatch.
#[sqlx::test]
async fn without_a_dispatch_warehouse_cancelling_stays_unrestricted(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, 100).await;

    let order = confirmed_order(&app, &product, 10).await;
    let invoice = issued_invoice(&app, &order).await;
    app.advance(&format!("/sales/orders/{order}/status"), &["shipped", "delivered"]).await;
    assert_eq!(app.get(&format!("/sales/orders/{order}")).await.field("status"), "delivered");

    let cancelled = set_invoice(&app, &invoice, "cancelled").await;
    assert!(cancelled.status.is_success(), "{}", cancelled.body);

    // The books unwind as they always have, and the shelf never took part.
    assert_eq!(net(&app, &ids[REVENUE]).await, 0.00);
    assert_eq!(level(&app, &warehouse, &product).await, (100, 0, 100));
}
