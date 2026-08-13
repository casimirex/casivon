//! Billing an order in instalments.
//!
//! An order could be invoiced exactly once, for the whole of it, while
//! purchasing had handled the mirror case since it was written. The cost was not
//! merely a missing feature: a draft invoice is editable, so an operator short of
//! stock would trim the invoice to what the shelf held and issue that — leaving
//! the order terminally `delivered` for ten units with six shipped, the other
//! four unbillable because the order had had its one invoice, and unrecorded
//! because nothing tracked how much of a line had been fulfilled.

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

async fn net(app: &TestApp, account: &str) -> f64 {
    app.get("/accounting/ledger-entries?per_page=100")
        .await
        .rows()
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

async fn receive(app: &TestApp, warehouse: &str, product: &str, quantity: i32) {
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme" })).await;
    let created = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor, "order_date": "2026-02-01",
                "lines": [{ "product_id": product, "description": "x",
                            "quantity": quantity, "unit_price": 4.00, "tax_rate": 0 }]
            }),
        )
        .await;
    let po = created.id();
    let po_line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();
    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;
    let receipt = app
        .post(
            "/purchasing/goods-receipts",
            json!({
                "po_id": po, "warehouse_id": warehouse, "receipt_date": "2026-02-05",
                "lines": [{ "po_line_id": po_line, "quantity_received": quantity }]
            }),
        )
        .await;
    assert!(receipt.status.is_success(), "{}", receipt.body);
}

async fn stocked_product(app: &TestApp, warehouse: &str, sku: &str, quantity: i32) -> String {
    let product = app
        .create("/inventory/products", json!({ "sku": sku, "name": sku, "sale_price": 20.00 }))
        .await;
    if quantity > 0 {
        receive(app, warehouse, &product, quantity).await;
    }
    product
}

/// An order for `quantity`, pushed to `processing`. Returns (order, line).
async fn order_for(app: &TestApp, product: &str, quantity: i32) -> (String, String) {
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
    let line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed", "processing"]).await;
    (order, line)
}

async fn convert(app: &TestApp, order: &str, body: Value) -> common::TestResponse {
    app.post(&format!("/sales/orders/{order}/convert-to-invoice"), body).await
}

async fn instalment(app: &TestApp, order: &str, line: &str, quantity: i32) -> common::TestResponse {
    convert(
        app,
        order,
        json!({ "payment_terms_days": 30,
                "lines": [{ "order_line_id": line, "quantity": quantity }] }),
    )
    .await
}

async fn issue(app: &TestApp, invoice: &str) -> common::TestResponse {
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await
}

async fn order_status(app: &TestApp, order: &str) -> String {
    app.get(&format!("/sales/orders/{order}")).await.field("status")
}

/// (invoiced, outstanding) for the order's first line.
async fn coverage(app: &TestApp, order: &str) -> (i64, i64) {
    let detail = app.get(&format!("/sales/orders/{order}")).await;
    let line = &detail.data()["lines"][0];
    (line["invoiced_quantity"].as_i64().unwrap(), line["outstanding"].as_i64().unwrap())
}

// ------------------------------------------------------------ the whole story

#[sqlx::test]
async fn an_order_short_of_stock_is_billed_for_what_shipped(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 6).await;
    dispatch_from(&app, &warehouse).await;

    let (order, line) = order_for(&app, &product, 10).await;
    assert_eq!(coverage(&app, &order).await, (0, 10));

    let invoice = instalment(&app, &order, &line, 6).await;
    assert!(invoice.status.is_success(), "{}", invoice.body);
    invoice.assert_money("total", "120.00");
    assert!(issue(&app, &invoice.id()).await.status.is_success());

    // The order says what it is: part shipped, four still owed. It used to be
    // markable `delivered` for ten with six billed and four lost.
    assert_eq!(order_status(&app, &order).await, "partially_shipped");
    assert_eq!(coverage(&app, &order).await, (6, 4));
    assert_eq!(level(&app, &warehouse, &product).await, (0, 0, 0));
    assert_eq!(net(&app, &REVENUE_ACCOUNT(&app).await).await, -120.00);
}

/// Named so the account lookup reads once; the ledger helper wants an id.
#[allow(non_snake_case)]
async fn REVENUE_ACCOUNT(app: &TestApp) -> String {
    app.get("/accounting/accounts?per_page=50")
        .await
        .rows()
        .iter()
        .find(|row| row["account_code"] == "4000")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[sqlx::test]
async fn shipping_is_refused_while_anything_is_outstanding(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 6).await;
    dispatch_from(&app, &warehouse).await;

    let (order, line) = order_for(&app, &product, 10).await;
    let invoice = instalment(&app, &order, &line, 6).await.id();
    assert!(issue(&app, &invoice).await.status.is_success());

    let refused =
        app.put(&format!("/sales/orders/{order}/status"), json!({ "status": "shipped" })).await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(
        refused.error_message().contains("4 unit(s) are still to be invoiced"),
        "{}",
        refused.error_message()
    );
    assert_eq!(order_status(&app, &order).await, "partially_shipped");
}

#[sqlx::test]
async fn billing_the_rest_closes_the_order(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 6).await;
    dispatch_from(&app, &warehouse).await;

    let (order, line) = order_for(&app, &product, 10).await;
    let first = instalment(&app, &order, &line, 6).await.id();
    assert!(issue(&app, &first).await.status.is_success());

    // The rest arrives and is billed.
    receive(&app, &warehouse, &product, 4).await;
    let second = instalment(&app, &order, &line, 4).await;
    assert!(second.status.is_success(), "{}", second.body);
    second.assert_money("total", "80.00");
    assert!(issue(&app, &second.id()).await.status.is_success());

    assert_eq!(coverage(&app, &order).await, (10, 0));
    assert!(app
        .put(&format!("/sales/orders/{order}/status"), json!({ "status": "shipped" }))
        .await
        .status
        .is_success());
    assert!(app
        .put(&format!("/sales/orders/{order}/status"), json!({ "status": "delivered" }))
        .await
        .status
        .is_success());

    // Two instalments, one order's worth of revenue and one order's worth of
    // goods.
    assert_eq!(net(&app, &REVENUE_ACCOUNT(&app).await).await, -200.00);
    assert_eq!(level(&app, &warehouse, &product).await, (0, 0, 0));
}

// -------------------------------------------------------------- the refusals

#[sqlx::test]
async fn a_line_cannot_be_invoiced_past_what_it_ordered(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;

    let (order, line) = order_for(&app, &product, 10).await;
    assert!(instalment(&app, &order, &line, 6).await.status.is_success());

    let refused = instalment(&app, &order, &line, 5).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(refused.error_message().contains("Widget"), "{}", refused.error_message());
    assert!(refused.error_message().contains("4 left"), "{}", refused.error_message());

    // Nothing was written by the refusal.
    assert_eq!(coverage(&app, &order).await, (6, 4));
    assert_eq!(app.get("/sales/invoices").await.rows().len(), 1);
}

/// A line named twice in one request is measured against its outstanding once —
/// the care `record_receipt` already takes over a repeated PO line.
#[sqlx::test]
async fn a_line_named_twice_is_counted_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;

    let (order, line) = order_for(&app, &product, 10).await;

    let refused = convert(
        &app,
        &order,
        json!({ "lines": [
            { "order_line_id": line, "quantity": 6 },
            { "order_line_id": line, "quantity": 6 }
        ] }),
    )
    .await;
    assert_eq!(refused.status, 422, "{}", refused.body);

    // Two that do fit together are allowed, and bill their sum.
    let allowed = convert(
        &app,
        &order,
        json!({ "lines": [
            { "order_line_id": line, "quantity": 6 },
            { "order_line_id": line, "quantity": 4 }
        ] }),
    )
    .await;
    assert!(allowed.status.is_success(), "{}", allowed.body);
    allowed.assert_money("total", "200.00");
    assert_eq!(coverage(&app, &order).await, (10, 0));
}

#[sqlx::test]
async fn omitting_the_lines_bills_everything_outstanding(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;

    let (order, line) = order_for(&app, &product, 10).await;
    assert!(instalment(&app, &order, &line, 6).await.status.is_success());

    // The one-click conversion, unchanged in shape, now means "the rest".
    let rest = convert(&app, &order, json!({ "payment_terms_days": 30 })).await;
    assert!(rest.status.is_success(), "{}", rest.body);
    rest.assert_money("total", "80.00");
    assert_eq!(coverage(&app, &order).await, (10, 0));

    // And with nothing left, there is nothing to raise.
    let refused = convert(&app, &order, json!({})).await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(
        refused.error_message().contains("nothing left to bill"),
        "{}",
        refused.error_message()
    );
}

#[sqlx::test]
async fn a_line_from_another_order_is_refused(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;

    let (first, _) = order_for(&app, &product, 10).await;
    let (_, other_line) = order_for(&app, &product, 10).await;

    let refused = instalment(&app, &first, &other_line, 1).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(
        refused.error_message().contains("does not belong to order"),
        "{}",
        refused.error_message()
    );
}

// --------------------------------------------- what gives the quantity back

#[sqlx::test]
async fn cancelling_an_instalment_puts_its_quantity_back(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;
    dispatch_from(&app, &warehouse).await;

    let (order, line) = order_for(&app, &product, 10).await;
    let invoice = instalment(&app, &order, &line, 6).await.id();
    assert!(issue(&app, &invoice).await.status.is_success());
    assert_eq!(coverage(&app, &order).await, (6, 4));

    let cancelled = app
        .put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "cancelled" }))
        .await;
    assert!(cancelled.status.is_success(), "{}", cancelled.body);

    // Derived, so nothing had to remember to decrement it.
    assert_eq!(coverage(&app, &order).await, (0, 10));
    // The order keeps saying `partially_shipped`: work was done on it and then
    // undone, and only shipping the rest moves it on.
    assert_eq!(order_status(&app, &order).await, "partially_shipped");
}

#[sqlx::test]
async fn deleting_a_draft_instalment_puts_its_quantity_back(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;

    let (order, line) = order_for(&app, &product, 10).await;
    let invoice = instalment(&app, &order, &line, 6).await.id();
    assert_eq!(coverage(&app, &order).await, (6, 4));

    app.delete(&format!("/sales/invoices/{invoice}")).await;
    assert_eq!(coverage(&app, &order).await, (0, 10));
}

#[sqlx::test]
async fn editing_a_draft_instalment_moves_what_is_outstanding(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;

    let (order, line) = order_for(&app, &product, 10).await;
    let raised = instalment(&app, &order, &line, 6).await;
    let invoice = raised.id();
    assert_eq!(coverage(&app, &order).await, (6, 4));

    // Editing replaces the line set, and a replaced line has no order line
    // behind it — so the whole order reads outstanding again. Recorded rather
    // than admired: hand-editing an instalment's lines is how the old workaround
    // went wrong, and it should push an operator back to raising a second
    // instalment instead.
    let edited = app
        .put(
            &format!("/sales/invoices/{invoice}"),
            json!({ "lines": [{ "product_id": product, "description": "Widget",
                                "quantity": 3, "unit_price": 20.00, "tax_rate": 0 }] }),
        )
        .await;
    assert!(edited.status.is_success(), "{}", edited.body);
    assert_eq!(coverage(&app, &order).await, (0, 10));
}

// ----------------------------------------------------------- the reservation

#[sqlx::test]
async fn shipping_part_of_an_order_releases_only_that_part(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;
    dispatch_from(&app, &warehouse).await;

    let (order, line) = order_for(&app, &product, 10).await;
    // The whole order is held, because the shelf can cover it.
    assert_eq!(level(&app, &warehouse, &product).await, (100, 10, 90));

    let invoice = instalment(&app, &order, &line, 6).await.id();
    assert!(issue(&app, &invoice).await.status.is_success());

    // Six gone, four still held for the rest of the order — not the whole hold
    // dropped, which is what releasing by the order would have done.
    assert_eq!(level(&app, &warehouse, &product).await, (94, 4, 90));
}

// ------------------------------------------------------------ still unchanged

#[sqlx::test]
async fn an_invoice_raised_without_an_order_is_unaffected(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;
    dispatch_from(&app, &warehouse).await;

    let customer = app.customer().await;
    let invoice = app
        .post(
            "/sales/invoices",
            json!({
                "customer_id": customer, "issue_date": "2026-03-01", "due_date": "2026-03-31",
                "lines": [{ "product_id": product, "description": "Widget",
                            "quantity": 5, "unit_price": 20.00, "tax_rate": 0 }]
            }),
        )
        .await;
    assert!(invoice.status.is_success(), "{}", invoice.body);
    assert!(issue(&app, &invoice.id()).await.status.is_success());

    // Ships as it always did, with no order line to point at and no hold to
    // release.
    assert_eq!(level(&app, &warehouse, &product).await, (95, 0, 95));
}
