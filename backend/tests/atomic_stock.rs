//! A document's stock moves all at once, or not at all.
//!
//! Shipping an invoice released the order's hold, then moved each line in its
//! own transaction. Three things followed. A refusal left the hold released, so
//! an order that still expected its goods protected none of them. A line the
//! shelf could not cover left the *earlier* lines already gone — costed, against
//! a document that stayed a draft. And retrying moved those lines a second time,
//! because movements have no equivalent of the ledger's `posting_key`.
//!
//! Purchase returns had the same shape, moving stock out line by line.

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

async fn average_cost(app: &TestApp, product: &str) -> String {
    app.get(&format!("/inventory/products/{product}")).await.field("average_cost")
}

/// Receives `quantity` of a new product at 4.00 each, returning its id.
async fn stocked_product(app: &TestApp, warehouse: &str, sku: &str, quantity: i32) -> String {
    let product = app
        .create("/inventory/products", json!({ "sku": sku, "name": sku, "sale_price": 20.00 }))
        .await;
    receive(app, warehouse, &product, quantity, 4.00).await;
    product
}

/// A goods receipt for one product, through its own purchase order.
async fn receive(app: &TestApp, warehouse: &str, product: &str, quantity: i32, price: f64) {
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme" })).await;
    let created = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor, "order_date": "2026-02-01",
                "lines": [{ "product_id": product, "description": "x",
                            "quantity": quantity, "unit_price": price, "tax_rate": 0 }]
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

async fn confirmed_order(app: &TestApp, product: &str, quantity: i32) -> String {
    let customer = app.customer().await;
    let created = app
        .post(
            "/sales/orders",
            json!({
                "customer_id": customer, "order_date": "2026-03-01",
                "lines": [{ "product_id": product, "description": "x",
                            "quantity": quantity, "unit_price": 20.00, "tax_rate": 0 }]
            }),
        )
        .await;
    assert!(created.status.is_success(), "{}", created.body);
    let order = created.id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed", "processing"]).await;
    order
}

/// A draft invoice raised straight against a customer, with whatever lines.
async fn draft_invoice(app: &TestApp, lines: Value) -> String {
    let customer = app.customer().await;
    let created = app
        .post(
            "/sales/invoices",
            json!({
                "customer_id": customer, "issue_date": "2026-03-01",
                "due_date": "2026-03-31", "lines": lines
            }),
        )
        .await;
    assert!(created.status.is_success(), "{}", created.body);
    created.id()
}

async fn issue(app: &TestApp, invoice: &str) -> common::TestResponse {
    app.put(&format!("/sales/invoices/{invoice}/status"), json!({ "status": "sent" })).await
}

fn line(product: &str, quantity: i32) -> Value {
    json!({ "product_id": product, "description": "x",
            "quantity": quantity, "unit_price": 20.00, "tax_rate": 0 })
}

// ------------------------------------------------------- the refused shipment

#[sqlx::test]
async fn a_refused_shipment_leaves_the_order_still_holding_its_goods(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 6).await;
    dispatch_from(&app, &warehouse).await;

    let order = confirmed_order(&app, &product, 10).await;
    assert_eq!(level(&app, &warehouse, &product).await, (6, 6, 0));

    let invoice = app
        .post(
            &format!("/sales/orders/{order}/convert-to-invoice"),
            json!({ "payment_terms_days": 30 }),
        )
        .await
        .id();

    let refused = issue(&app, &invoice).await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(refused.error_message().contains("are available"), "{}", refused.error_message());

    // The release and the movements are one transaction now, so a refusal rolls
    // the release back with them. This used to read (6, 0, 6): the order still
    // expected ten and protected none of them.
    assert_eq!(level(&app, &warehouse, &product).await, (6, 6, 0));
}

#[sqlx::test]
async fn a_line_the_shelf_cannot_cover_moves_none_of_the_others(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let plenty = stocked_product(&app, &warehouse, "PLENTY", 50).await;
    let scarce = stocked_product(&app, &warehouse, "SCARCE", 1).await;
    dispatch_from(&app, &warehouse).await;

    let invoice = draft_invoice(&app, json!([line(&plenty, 5), line(&scarce, 5)])).await;
    let refused = issue(&app, &invoice).await;
    assert_eq!(refused.status, 409, "{}", refused.body);

    // PLENTY used to drop to 45 — five units gone from the shelf for a document
    // that was never issued.
    assert_eq!(level(&app, &warehouse, &plenty).await.0, 50);
    assert_eq!(level(&app, &warehouse, &scarce).await.0, 1);
}

#[sqlx::test]
async fn a_refused_shipment_posts_nothing_and_leaves_a_draft(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let plenty = stocked_product(&app, &warehouse, "PLENTY", 50).await;
    let scarce = stocked_product(&app, &warehouse, "SCARCE", 1).await;
    dispatch_from(&app, &warehouse).await;

    // Only the two goods receipts.
    let before = entries(&app).await.len();
    let inventory_before = net(&app, &ids[INVENTORY]).await;

    let invoice = draft_invoice(&app, json!([line(&plenty, 5), line(&scarce, 5)])).await;
    assert!(!issue(&app, &invoice).await.status.is_success());

    assert_eq!(entries(&app).await.len(), before, "a refused shipment posted something");
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, inventory_before);
    assert_eq!(app.get(&format!("/sales/invoices/{invoice}")).await.field("status"), "draft");
}

#[sqlx::test]
async fn retrying_after_a_refusal_ships_each_line_exactly_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let plenty = stocked_product(&app, &warehouse, "PLENTY", 50).await;
    let scarce = stocked_product(&app, &warehouse, "SCARCE", 1).await;
    dispatch_from(&app, &warehouse).await;

    let invoice = draft_invoice(&app, json!([line(&plenty, 5), line(&scarce, 5)])).await;
    assert!(!issue(&app, &invoice).await.status.is_success());

    // Enough SCARCE to cover it, and try again.
    receive(&app, &warehouse, &scarce, 20, 4.00).await;
    assert!(issue(&app, &invoice).await.status.is_success());

    // PLENTY used to end at 40: shipped once by the failed attempt and once by
    // the retry, for an invoice naming five.
    assert_eq!(level(&app, &warehouse, &plenty).await.0, 45);
    assert_eq!(level(&app, &warehouse, &scarce).await.0, 16);
    // Ten units at 4.00, costed once.
    assert_eq!(net(&app, &ids[COST]).await, 40.00);
}

/// Two lines of the same product, each fitting on its own and not together. The
/// use case cannot see this without moving stock; the repository can, because by
/// the second line the first has already taken its units inside the transaction.
#[sqlx::test]
async fn two_lines_of_one_product_are_measured_together(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 6).await;
    dispatch_from(&app, &warehouse).await;

    let invoice = draft_invoice(&app, json!([line(&product, 5), line(&product, 5)])).await;
    let refused = issue(&app, &invoice).await;

    assert_eq!(refused.status, 409, "{}", refused.body);
    assert_eq!(level(&app, &warehouse, &product).await.0, 6);
}

// ---------------------------------------------------- the other three callers

/// Returns refuse *before* they write anything: every line's availability is
/// checked up front (`purchasing/use_cases.rs:684`), so the document is never
/// created and no stock moves. That guard predates this change — the plural call
/// hardens the loop behind it against a database error partway through, which is
/// the same protection goods receipts and credit notes get.
#[sqlx::test]
async fn a_purchase_return_the_shelf_cannot_cover_returns_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme" })).await;
    let plenty = app
        .create("/inventory/products", json!({ "sku": "PLENTY", "name": "P", "sale_price": 20.0 }))
        .await;
    let scarce = app
        .create("/inventory/products", json!({ "sku": "SCARCE", "name": "S", "sale_price": 20.0 }))
        .await;

    let created = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor, "order_date": "2026-02-01",
                "lines": [
                    { "product_id": plenty, "description": "P", "quantity": 10, "unit_price": 4.00, "tax_rate": 0 },
                    { "product_id": scarce, "description": "S", "quantity": 10, "unit_price": 4.00, "tax_rate": 0 }
                ]
            }),
        )
        .await;
    let po = created.id();
    let plenty_line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();
    let scarce_line = created.data()["lines"][1]["id"].as_str().unwrap().to_string();
    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;
    app.post(
        "/purchasing/goods-receipts",
        json!({
            "po_id": po, "warehouse_id": warehouse, "receipt_date": "2026-02-05",
            "lines": [
                { "po_line_id": plenty_line, "quantity_received": 10 },
                { "po_line_id": scarce_line, "quantity_received": 10 }
            ]
        }),
    )
    .await;

    // Most of the scarce ones are sold before anyone tries to send them back.
    let sold = draft_invoice(&app, json!([line(&scarce, 9)])).await;
    dispatch_from(&app, &warehouse).await;
    assert!(issue(&app, &sold).await.status.is_success());
    assert_eq!(level(&app, &warehouse, &scarce).await.0, 1);

    let refused = app
        .post(
            "/purchasing/purchase-returns",
            json!({
                "po_id": po, "warehouse_id": warehouse, "return_date": "2026-03-01",
                "lines": [
                    { "po_line_id": plenty_line, "quantity_returned": 10 },
                    { "po_line_id": scarce_line, "quantity_returned": 10 }
                ]
            }),
        )
        .await;
    assert_eq!(refused.status, 409, "{}", refused.body);

    // Neither line went back, and no document was written.
    assert_eq!(level(&app, &warehouse, &plenty).await.0, 10);
    assert_eq!(level(&app, &warehouse, &scarce).await.0, 1);
    assert!(app.get("/purchasing/purchase-returns").await.rows().is_empty());
}

#[sqlx::test]
async fn a_multi_line_goods_receipt_still_lands_whole(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme" })).await;
    let first = app
        .create("/inventory/products", json!({ "sku": "A", "name": "A", "sale_price": 20.0 }))
        .await;
    let second = app
        .create("/inventory/products", json!({ "sku": "B", "name": "B", "sale_price": 20.0 }))
        .await;

    let created = app
        .post(
            "/purchasing/purchase-orders",
            json!({
                "vendor_id": vendor, "order_date": "2026-02-01",
                "lines": [
                    { "product_id": first, "description": "A", "quantity": 10, "unit_price": 4.00, "tax_rate": 0 },
                    { "product_id": second, "description": "B", "quantity": 5, "unit_price": 6.00, "tax_rate": 0 }
                ]
            }),
        )
        .await;
    let po = created.id();
    let a_line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();
    let b_line = created.data()["lines"][1]["id"].as_str().unwrap().to_string();
    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;

    let receipt = app
        .post(
            "/purchasing/goods-receipts",
            json!({
                "po_id": po, "warehouse_id": warehouse, "receipt_date": "2026-02-05",
                "lines": [
                    { "po_line_id": a_line, "quantity_received": 10 },
                    { "po_line_id": b_line, "quantity_received": 5 }
                ]
            }),
        )
        .await;
    assert!(receipt.status.is_success(), "{}", receipt.body);

    // Both lines landed, each blended at its own price — the plural path did not
    // disturb what a delivery is worth.
    assert_eq!(level(&app, &warehouse, &first).await.0, 10);
    assert_eq!(level(&app, &warehouse, &second).await.0, 5);
    assert_eq!(average_cost(&app, &first).await, "4.0000");
    assert_eq!(average_cost(&app, &second).await, "6.0000");
}

#[sqlx::test]
async fn a_credit_note_still_returns_goods_at_the_average(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 100).await;
    dispatch_from(&app, &warehouse).await;

    let invoice = draft_invoice(&app, json!([line(&product, 10)])).await;
    assert!(issue(&app, &invoice).await.status.is_success());
    assert_eq!(level(&app, &warehouse, &product).await.0, 90);

    let invoice_line = app.get(&format!("/sales/invoices/{invoice}")).await.data()["lines"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let credited = app
        .post(
            "/sales/credit-notes",
            json!({
                "invoice_id": invoice, "note_date": "2026-03-05", "warehouse_id": warehouse,
                "lines": [{ "invoice_line_id": invoice_line, "description": "x",
                            "quantity": 4, "unit_price": 20.00, "tax_rate": 0 }]
            }),
        )
        .await;
    assert!(credited.status.is_success(), "{}", credited.body);

    assert_eq!(level(&app, &warehouse, &product).await.0, 94);
    assert_eq!(average_cost(&app, &product).await, "4.0000");
    // Six sold, four back: cost of sales keeps only the six.
    assert_eq!(net(&app, &ids[COST]).await, 24.00);
}

#[sqlx::test]
async fn the_single_movement_endpoint_is_unchanged(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 10).await;

    let moved = app
        .post(
            "/inventory/movements",
            json!({ "product_id": product, "warehouse_id": warehouse,
                    "movement_type": "out", "quantity": 4 }),
        )
        .await;
    assert!(moved.status.is_success(), "{}", moved.body);
    assert_eq!(level(&app, &warehouse, &product).await.0, 6);

    let refused = app
        .post(
            "/inventory/movements",
            json!({ "product_id": product, "warehouse_id": warehouse,
                    "movement_type": "out", "quantity": 99 }),
        )
        .await;
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(
        refused.error_message().contains("Only 6 unit(s) of 'SKU-1' are available in 'Main'"),
        "{}",
        refused.error_message()
    );
    assert_eq!(level(&app, &warehouse, &product).await.0, 6);
}

/// Two shipments of the last units, in flight at once.
///
/// Both pass any check made before the movement — that is the whole point of the
/// race. The level is locked inside the transaction, so the second waits for the
/// first to commit and then finds the shelf as the first left it.
#[sqlx::test]
async fn two_shipments_of_the_last_units_cannot_both_succeed(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = stocked_product(&app, &warehouse, "SKU-1", 6).await;
    dispatch_from(&app, &warehouse).await;

    let first = draft_invoice(&app, json!([line(&product, 5)])).await;
    let second = draft_invoice(&app, json!([line(&product, 5)])).await;

    let (a, b) = tokio::join!(issue(&app, &first), issue(&app, &second));

    let succeeded = [&a, &b].iter().filter(|r| r.status.is_success()).count();
    assert_eq!(succeeded, 1, "both shipped: {} / {}", a.body, b.body);

    // Five gone, one left — never minus four.
    assert_eq!(level(&app, &warehouse, &product).await.0, 1);
}
