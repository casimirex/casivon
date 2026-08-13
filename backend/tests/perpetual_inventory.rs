//! Stock as an asset, and its cost recognised when the goods leave.
//!
//! Before this, receiving goods debited Cost of sales the day they arrived: buy
//! £10,000 of stock and sell none of it, and the P&L showed a £10,000 expense
//! while the balance sheet showed nothing. These tests pin the new arithmetic,
//! and — just as important — that an installation which has not mapped the two
//! inventory accounts is left behaving exactly as it did.

mod common;

use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;

/// Role order matches `POSTING_ROLES` on the backend. The last two are the
/// opt-in pair.
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

const AP: usize = 5;
const COST: usize = 6;
const INVENTORY: usize = 10;
const ADJUSTMENT: usize = 11;

/// Creates the twelve accounts and maps as many as `perpetual` calls for.
async fn configure(app: &TestApp, perpetual: bool) -> Vec<String> {
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

    let mut body = json!({
        "ar_account_id": ids[0], "bank_account_id": ids[1],
        "sales_revenue_account_id": ids[2], "tax_payable_account_id": ids[3],
        "fx_gain_loss_account_id": ids[4], "accounts_payable_account_id": ids[5],
        "cost_of_sales_account_id": ids[6], "purchase_tax_account_id": ids[7],
        "employee_payable_account_id": ids[8], "employee_expense_account_id": ids[9]
    });
    if perpetual {
        body["inventory_account_id"] = json!(ids[INVENTORY]);
        body["inventory_adjustment_account_id"] = json!(ids[ADJUSTMENT]);
    }

    let response = app.put("/accounting/posting-accounts", body).await;
    assert!(response.status.is_success(), "mapping failed: {}", response.body);
    // The ten core roles are enough to post with either way — the whole point of
    // keeping the inventory pair optional.
    assert_eq!(response.field("posting_enabled"), "true");
    assert_eq!(response.field("perpetual_inventory"), perpetual.to_string());

    ids
}

async fn entries(app: &TestApp) -> Vec<Value> {
    app.get("/accounting/ledger-entries?per_page=100").await.rows().clone()
}

/// Net movement on an account across the whole ledger, debit-positive.
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

async fn money(app: &TestApp, path: &str, field: &str) -> f64 {
    app.get(path).await.field(field).parse().unwrap()
}

/// A confirmed order for a stocked product. Returns (po_id, po_line_id).
async fn order_for(
    app: &TestApp,
    product: &str,
    quantity: i32,
    unit_price: f64,
    currency: Option<&str>,
) -> (String, String) {
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme Supplies" })).await;

    let mut body = json!({
        "vendor_id": vendor,
        "order_date": "2026-03-01",
        "lines": [{
            "product_id": product, "description": "Widget",
            "quantity": quantity, "unit_price": unit_price, "tax_rate": 0
        }]
    });
    if let Some(currency) = currency {
        body["currency"] = json!(currency);
    }

    let created = app.post("/purchasing/purchase-orders", body).await;
    assert!(created.status.is_success(), "{}", created.body);
    let po = created.id();
    let line = created.data()["lines"][0]["id"].as_str().unwrap().to_string();

    app.advance(&format!("/purchasing/purchase-orders/{po}/status"), &["sent", "confirmed"]).await;
    (po, line)
}

async fn receive(app: &TestApp, po: &str, line: &str, quantity: i32, warehouse: &str) {
    let response = app
        .post(
            "/purchasing/goods-receipts",
            json!({
                "po_id": po, "warehouse_id": warehouse, "receipt_date": "2026-03-05",
                "lines": [{ "po_line_id": line, "quantity_received": quantity }]
            }),
        )
        .await;
    assert!(response.status.is_success(), "receipt failed: {}", response.body);
}

async fn move_stock(
    app: &TestApp,
    product: &str,
    warehouse: &str,
    kind: &str,
    quantity: i32,
) -> Value {
    let response = app
        .post(
            "/inventory/movements",
            json!({
                "product_id": product, "warehouse_id": warehouse,
                "movement_type": kind, "quantity": quantity
            }),
        )
        .await;
    assert!(response.status.is_success(), "movement failed: {}", response.body);
    response.data().clone()
}

/// A product with no standing cost, so every figure in these tests comes from
/// what was actually paid rather than from a number typed into the form.
async fn uncosted_product(app: &TestApp, sku: &str) -> String {
    app.create("/inventory/products", json!({ "sku": sku, "name": "Widget", "sale_price": 19.99 }))
        .await
}

#[sqlx::test]
async fn receiving_goods_capitalises_them_instead_of_expensing_them(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;

    // The assertion that would have failed before this change.
    assert_eq!(net(&app, &ids[INVENTORY]).await, 400.00);
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
    assert_eq!(net(&app, &ids[AP]).await, -400.00);

    // And the cost the goods were received at is now what a unit is worth.
    let stock = app.get(&format!("/inventory/products/{product}")).await;
    assert_eq!(stock.field("average_cost"), "4.0000");
}

#[sqlx::test]
async fn a_second_delivery_at_a_new_price_moves_the_average(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (first, first_line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &first, &first_line, 100, &warehouse).await;

    let (second, second_line) = order_for(&app, &product, 50, 5.50, None).await;
    receive(&app, &second, &second_line, 50, &warehouse).await;

    // (100 × 4.00 + 50 × 5.50) / 150 = 4.50
    assert_eq!(
        app.get(&format!("/inventory/products/{product}")).await.field("average_cost"),
        "4.5000"
    );
    assert_eq!(net(&app, &ids[INVENTORY]).await, 675.00);
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
}

#[sqlx::test]
async fn stock_going_out_is_where_the_cost_lands(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (first, first_line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &first, &first_line, 100, &warehouse).await;
    let (second, second_line) = order_for(&app, &product, 50, 5.50, None).await;
    receive(&app, &second, &second_line, 50, &warehouse).await;

    move_stock(&app, &product, &warehouse, "out", 60).await;

    // 60 at the 4.50 average.
    assert_eq!(net(&app, &ids[COST]).await, 270.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 405.00);

    // Selling does not change what the remaining stock cost.
    assert_eq!(
        app.get(&format!("/inventory/products/{product}")).await.field("average_cost"),
        "4.5000"
    );
}

#[sqlx::test]
async fn the_reports_show_stock_as_an_asset_and_the_sale_as_a_cost(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;
    move_stock(&app, &product, &warehouse, "out", 60).await;

    let sheet = app.get("/accounting/reports/balance-sheet?date_from=2026-01-01&date_to=2026-12-31").await;
    let inventory = sheet.data()["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["account_name"] == "Inventory")
        .expect("no Inventory line on the balance sheet");
    assert_eq!(inventory["balance"].as_str().unwrap().parse::<f64>().unwrap(), 160.00);

    let pnl = app.get("/accounting/reports/profit-and-loss?date_from=2026-01-01&date_to=2026-12-31").await;
    let cost = pnl.data()["expenses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["account_name"] == "Cost of sales")
        .expect("no Cost of sales line in the P&L");
    assert_eq!(cost["balance"].as_str().unwrap().parse::<f64>().unwrap(), 240.00);
}

/// The invariant worth reaching for first when the books look wrong: the stock
/// report and the Inventory account are two views of one number.
#[sqlx::test]
async fn the_valuation_report_agrees_with_the_inventory_account(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (first, first_line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &first, &first_line, 100, &warehouse).await;
    let (second, second_line) = order_for(&app, &product, 50, 5.50, None).await;
    receive(&app, &second, &second_line, 50, &warehouse).await;
    move_stock(&app, &product, &warehouse, "out", 60).await;
    move_stock(&app, &product, &warehouse, "adjustment", -4).await;

    let valuation = money(&app, "/inventory/stock/valuation", "total_value").await;
    assert_eq!(valuation, net(&app, &ids[INVENTORY]).await, "valuation and ledger disagree");
    // 86 units left at 4.50.
    assert_eq!(valuation, 387.00);

    // And it reads like every other money figure in the API rather than
    // carrying the four decimal places the average is kept to.
    app.get("/inventory/stock/valuation").await.assert_money("total_value", "387.00");
    assert_eq!(
        app.get("/inventory/stock/valuation").await.field("total_value"),
        "387.00"
    );
}

#[sqlx::test]
async fn a_foreign_delivery_is_valued_at_the_base_rate(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let rate = app
        .put(
            "/settings/fx-rates",
            json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": 1.10 }),
        )
        .await;
    assert!(rate.status.is_success(), "{}", rate.body);

    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    // EUR 500 of goods on an order struck at 1.10.
    let (po, line) = order_for(&app, &product, 100, 5.00, Some("EUR")).await;
    receive(&app, &po, &line, 100, &warehouse).await;

    // Stock is worth what it cost in the base currency, not the foreign figure.
    assert_eq!(net(&app, &ids[INVENTORY]).await, 550.00);
    assert_eq!(
        app.get(&format!("/inventory/products/{product}")).await.field("average_cost"),
        "5.5000"
    );
    assert_eq!(money(&app, "/inventory/stock/valuation", "total_value").await, 550.00);
}

#[sqlx::test]
async fn a_transfer_moves_stock_without_touching_the_books(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let main = app.warehouse("MAIN", "Main").await;
    let annex = app.warehouse("ANNEX", "Annex").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &po, &line, 100, &main).await;
    let before = entries(&app).await.len();

    let response = app
        .post(
            "/inventory/movements",
            json!({
                "product_id": product, "warehouse_id": main, "to_warehouse_id": annex,
                "movement_type": "transfer", "quantity": 40
            }),
        )
        .await;
    assert!(response.status.is_success(), "{}", response.body);

    // The company owns what it owned, at what it was worth.
    assert_eq!(entries(&app).await.len(), before, "a transfer wrote a journal entry");
    assert_eq!(net(&app, &ids[INVENTORY]).await, 400.00);
    assert_eq!(money(&app, "/inventory/stock/valuation", "total_value").await, 400.00);
}

/// Shrinkage is not a sale, and hiding it in cost of sales would keep it from
/// the person whose job it is to notice.
#[sqlx::test]
async fn a_hand_made_adjustment_reaches_the_adjustment_account(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;
    move_stock(&app, &product, &warehouse, "adjustment", -4).await;

    assert_eq!(net(&app, &ids[ADJUSTMENT]).await, 16.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 384.00);
    assert_eq!(net(&app, &ids[COST]).await, 0.00);
}

/// The compatibility promise: leave the two accounts unmapped and nothing at all
/// changes.
#[sqlx::test]
async fn without_the_inventory_accounts_costing_stays_periodic(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, false).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;

    // Straight to the P&L on arrival, exactly as before.
    assert_eq!(net(&app, &ids[COST]).await, 400.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 0.00);

    // And stock leaving posts nothing, because the cost was already taken.
    let before = entries(&app).await.len();
    move_stock(&app, &product, &warehouse, "out", 60).await;
    assert_eq!(entries(&app).await.len(), before);
}

#[sqlx::test]
async fn opening_the_inventory_account_reverses_the_old_over_expensing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, false).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    // Received while the books were still periodic: the whole 400 went to cost.
    let (po, line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;
    assert_eq!(net(&app, &ids[COST]).await, 400.00);

    // Now switch costing on.
    let mut body = json!({
        "ar_account_id": ids[0], "bank_account_id": ids[1],
        "sales_revenue_account_id": ids[2], "tax_payable_account_id": ids[3],
        "fx_gain_loss_account_id": ids[4], "accounts_payable_account_id": ids[5],
        "cost_of_sales_account_id": ids[6], "purchase_tax_account_id": ids[7],
        "employee_payable_account_id": ids[8], "employee_expense_account_id": ids[9]
    });
    body["inventory_account_id"] = json!(ids[INVENTORY]);
    body["inventory_adjustment_account_id"] = json!(ids[ADJUSTMENT]);
    app.put("/accounting/posting-accounts", body).await;

    let preview = app.get("/accounting/inventory-opening").await;
    assert_eq!(preview.field("perpetual_inventory"), "true");
    assert_eq!(preview.field("already_posted"), "false");
    preview.assert_money("total_value", "400.00");
    assert_eq!(preview.data()["lines"][0]["sku"], "SKU-1");

    let posted = app.post("/accounting/inventory-opening", json!({})).await;
    assert!(posted.status.is_success(), "{}", posted.body);
    assert_eq!(posted.field("already_posted"), "true");

    // The goods are an asset, and the cost they were wrongly charged to is
    // relieved — which is why this credits Cost of sales rather than equity.
    assert_eq!(net(&app, &ids[INVENTORY]).await, 400.00);
    assert_eq!(net(&app, &ids[COST]).await, 0.00);

    // And now selling them behaves like any other perpetual sale.
    move_stock(&app, &product, &warehouse, "out", 60).await;
    assert_eq!(net(&app, &ids[COST]).await, 240.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 160.00);
}

#[sqlx::test]
async fn opening_the_inventory_account_twice_writes_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;

    app.post("/accounting/inventory-opening", json!({})).await;
    let after_first = net(&app, &ids[INVENTORY]).await;

    app.post("/accounting/inventory-opening", json!({})).await;
    assert_eq!(net(&app, &ids[INVENTORY]).await, after_first, "the opening was posted twice");
}

/// The average belongs to the movements, not to the product form. Letting an
/// edit set it would pull the valuation report away from the Inventory account
/// with nothing on either side to show why.
#[sqlx::test]
async fn editing_a_product_cannot_rewrite_what_its_stock_cost(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let ids = configure(&app, true).await;
    let warehouse = app.warehouse("MAIN", "Main").await;
    let product = uncosted_product(&app, "SKU-1").await;

    let (po, line) = order_for(&app, &product, 100, 4.00, None).await;
    receive(&app, &po, &line, 100, &warehouse).await;

    // A standing cost of 99.00 is what the buyer *expects* to pay next time. It
    // has no bearing on what the stock on the shelf cost.
    let edited = app
        .put(&format!("/inventory/products/{product}"), json!({ "cost_price": 99.00 }))
        .await;
    assert!(edited.status.is_success(), "{}", edited.body);

    assert_eq!(edited.field("cost_price"), "99.00");
    assert_eq!(edited.field("average_cost"), "4.0000");
    assert_eq!(money(&app, "/inventory/stock/valuation", "total_value").await, 400.00);
    assert_eq!(net(&app, &ids[INVENTORY]).await, 400.00);
}

#[sqlx::test]
async fn opening_is_refused_while_costing_is_periodic(pool: PgPool) {
    let app = TestApp::new(pool).await;
    configure(&app, false).await;

    let refused = app.post("/accounting/inventory-opening", json!({})).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(refused.error_message().contains("not configured"), "{}", refused.error_message());
}
