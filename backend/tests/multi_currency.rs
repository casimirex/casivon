//! Trading in more than one currency.
//!
//! The rule the whole feature rests on: a document keeps the amount the
//! customer agreed, plus the rate it was raised at, plus that amount restated
//! in the organisation's own currency. Reports add the restated column, so a
//! euro sale and a dollar sale can appear in one total without either of them
//! being altered.
//!
//! `currency.rs` covers the single-currency path these build on.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// Adds a rate, as an admin would under Settings → Exchange Rates.
async fn rate(app: &TestApp, currency: &str, from: &str, rate: f64) {
    let response = app
        .put(
            "/settings/fx-rates",
            json!({ "currency": currency, "effective_from": from, "rate": rate }),
        )
        .await;
    assert!(response.status.is_success(), "setting {currency} rate failed: {}", response.body);
}

fn quote(customer: &str, currency: &str, issue_date: &str, unit_price: f64) -> serde_json::Value {
    json!({
        "customer_id": customer,
        "currency": currency,
        "issue_date": issue_date,
        "expiry_date": "2026-12-31",
        "lines": [{ "description": "Widget", "quantity": 1, "unit_price": unit_price }]
    })
}

#[sqlx::test]
async fn a_rate_is_what_makes_a_foreign_document_possible(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    // Refused while nothing says what a euro is worth.
    assert_eq!(app.post("/sales/quotes", quote(&customer, "EUR", "2026-08-01", 100.0)).await.status, 422);

    rate(&app, "EUR", "2026-01-01", 1.10).await;

    let raised = app.post("/sales/quotes", quote(&customer, "EUR", "2026-08-01", 100.0)).await;
    assert!(raised.status.is_success(), "{}", raised.body);
    assert_eq!(raised.field("currency"), "EUR");

    // The customer's number is untouched; the business's number is derived.
    raised.assert_money("total", "100.00");
    raised.assert_money("base_total", "110.00");
    raised.assert_money("fx_rate", "1.10");
}

#[sqlx::test]
async fn the_rate_in_force_on_the_document_date_is_the_one_used(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    rate(&app, "EUR", "2026-01-01", 1.10).await;
    rate(&app, "EUR", "2026-06-01", 1.20).await;

    // Raised between the two: the earlier rate is still the one in force. A
    // document must not be restated at a rate that did not exist yet.
    let march = app.post("/sales/quotes", quote(&customer, "EUR", "2026-03-15", 100.0)).await;
    march.assert_money("base_total", "110.00");

    let july = app.post("/sales/quotes", quote(&customer, "EUR", "2026-07-15", 100.0)).await;
    july.assert_money("base_total", "120.00");

    // And a document predating every rate cannot be restated at all, rather
    // than borrowing the nearest one from the future.
    assert_eq!(app.post("/sales/quotes", quote(&customer, "EUR", "2025-12-31", 100.0)).await.status, 422);
}

/// Builds a EUR invoice for `unit_price`, issued on `issue_date`.
async fn eur_invoice(app: &TestApp, customer: &str, issue_date: &str, unit_price: f64) -> String {
    let quote_id = app.create("/sales/quotes", quote(customer, "EUR", issue_date, unit_price)).await;
    app.advance(&format!("/sales/quotes/{quote_id}/status"), &["sent", "accepted"]).await;

    let order = app
        .post(
            &format!("/sales/quotes/{quote_id}/convert-to-order"),
            json!({ "order_date": issue_date }),
        )
        .await
        .id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed"]).await;

    let invoice = app
        .post(
            &format!("/sales/orders/{order}/convert-to-invoice"),
            json!({ "issue_date": issue_date, "payment_terms_days": 30 }),
        )
        .await
        .id();

    // Issued, not merely raised: a draft has no receivable to settle, so it
    // takes no payment — and settling one is what every caller of this helper
    // is about.
    app.advance(&format!("/sales/invoices/{invoice}/status"), &["sent"]).await;
    invoice
}

#[sqlx::test]
async fn settling_above_the_invoice_rate_realises_a_gain(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    rate(&app, "EUR", "2026-01-01", 1.10).await;
    let invoice = eur_invoice(&app, &customer, "2026-03-01", 1000.0).await;

    // The euro strengthens between issuing and being paid.
    rate(&app, "EUR", "2026-04-01", 1.15).await;

    let payment = app
        .post(
            "/sales/payments",
            json!({
                "invoice_id": invoice, "amount": 1000.00,
                "payment_method": "bank_transfer", "payment_date": "2026-04-10"
            }),
        )
        .await;
    assert!(payment.status.is_success(), "{}", payment.body);

    // The customer paid exactly what they owed, in euro.
    payment.assert_money("amount", "1000.00");
    // But those euro bought more dollars than the sale was booked at: 1150
    // against the 1100 of revenue recognised. That 50 is a real result.
    payment.assert_money("base_amount", "1150.00");
    payment.assert_money("fx_gain_loss", "50.00");

    let settled = app.get(&format!("/sales/invoices/{invoice}")).await;
    assert_eq!(settled.field("status"), "paid");
    settled.assert_money("amount_due", "0");
    // The receivable is cleared at the invoice's own rate, so paid and due still
    // reconcile against the invoice's base total. The extra 50 lives on the
    // payment, not smeared into the invoice.
    settled.assert_money("base_total", "1100.00");
    settled.assert_money("base_amount_paid", "1100.00");
    settled.assert_money("base_amount_due", "0");
}

#[sqlx::test]
async fn settling_below_the_invoice_rate_realises_a_loss(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    rate(&app, "EUR", "2026-01-01", 1.20).await;
    let invoice = eur_invoice(&app, &customer, "2026-03-01", 500.0).await;

    rate(&app, "EUR", "2026-04-01", 1.10).await;

    let payment = app
        .post(
            "/sales/payments",
            json!({
                "invoice_id": invoice, "amount": 500.00,
                "payment_method": "bank_transfer", "payment_date": "2026-04-10"
            }),
        )
        .await;

    // 550 received against 600 booked: a 50 loss, carried as a negative.
    payment.assert_money("base_amount", "550.00");
    payment.assert_money("fx_gain_loss", "-50.00");
}

#[sqlx::test]
async fn a_payment_in_the_base_currency_realises_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    let quote_id = app.create("/sales/quotes", quote(&customer, "USD", "2026-03-01", 400.0)).await;
    app.advance(&format!("/sales/quotes/{quote_id}/status"), &["sent", "accepted"]).await;
    let order = app.post(&format!("/sales/quotes/{quote_id}/convert-to-order"), json!({})).await.id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed"]).await;
    let invoice = app
        .post(&format!("/sales/orders/{order}/convert-to-invoice"), json!({}))
        .await
        .id();
    // Issued before it can be settled: a draft has raised no receivable.
    app.advance(&format!("/sales/invoices/{invoice}/status"), &["sent"]).await;

    let payment = app
        .post(
            "/sales/payments",
            json!({
                "invoice_id": invoice, "amount": 400.00,
                "payment_method": "cash", "payment_date": "2026-04-10"
            }),
        )
        .await;

    // The single-currency case has to stay exactly as boring as it was.
    payment.assert_money("base_amount", "400.00");
    payment.assert_money("fx_gain_loss", "0");
    payment.assert_money("fx_rate", "1");
}

#[sqlx::test]
async fn the_pipeline_adds_deals_across_currencies(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    rate(&app, "EUR", "2026-01-01", 1.10).await;

    for (currency, value) in [("USD", 1000.0), ("EUR", 1000.0)] {
        let created = app
            .post(
                "/crm/opportunities",
                json!({
                    "title": format!("{currency} deal"), "company_id": customer,
                    "value": value, "currency": currency, "stage": "proposal"
                }),
            )
            .await;
        assert!(created.status.is_success(), "{}", created.body);
    }

    let pipeline = app.get("/crm/opportunities/pipeline").await;
    let proposal = pipeline
        .data()
        .as_array()
        .expect("pipeline is a list")
        .iter()
        .find(|row| row["stage"] == "proposal")
        .expect("a proposal stage")
        .clone();

    // 1000 + 1000 would be 2000 and would mean nothing. Restated, the euro deal
    // is worth 1100, so the pipeline is 2100.
    assert_eq!(proposal["value"].as_str().unwrap().parse::<f64>().unwrap(), 2100.0);
}

#[sqlx::test]
async fn a_journal_entry_must_agree_with_the_accounts_it_touches(pool: PgPool) {
    let app = TestApp::new(pool).await;
    rate(&app, "EUR", "2026-01-01", 1.10).await;

    let cash = app
        .create(
            "/accounting/accounts",
            json!({ "account_code": "1000", "account_name": "Cash", "account_type": "asset" }),
        )
        .await;
    let revenue = app
        .create(
            "/accounting/accounts",
            json!({ "account_code": "4000", "account_name": "Sales", "account_type": "revenue" }),
        )
        .await;

    // Both accounts are USD; posting EUR into them would add two different
    // kinds of number together in the balance columns.
    let mismatched = app
        .post(
            "/accounting/ledger-entries",
            json!({
                "entry_date": "2026-03-01", "description": "Sale", "currency": "EUR",
                "debit_account_id": cash, "credit_account_id": revenue, "amount": 100
            }),
        )
        .await;

    assert_eq!(mismatched.status, 422);
    let message = mismatched.error_message();
    assert!(message.contains("EUR") && message.contains("USD"), "{message}");

    // The same entry in the accounts' own currency posts fine.
    let matched = app
        .post(
            "/accounting/ledger-entries",
            json!({
                "entry_date": "2026-03-01", "description": "Sale", "currency": "USD",
                "debit_account_id": cash, "credit_account_id": revenue, "amount": 100
            }),
        )
        .await;
    assert!(matched.status.is_success(), "{}", matched.body);
    matched.assert_money("base_amount", "100.00");
}

#[sqlx::test]
async fn the_base_currency_is_not_given_a_rate(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let response = app
        .put(
            "/settings/fx-rates",
            json!({ "currency": "USD", "effective_from": "2026-01-01", "rate": 1.0 }),
        )
        .await;

    // A stored, editable 1 is a rate somebody can set to 0.98 by accident, and
    // every amount in the system rescales.
    assert_eq!(response.status, 422);
    assert!(response.error_message().contains("base currency"), "{}", response.error_message());
}

#[sqlx::test]
async fn a_rate_must_be_positive(pool: PgPool) {
    let app = TestApp::new(pool).await;

    for bad in [0.0, -1.5] {
        let response = app
            .put(
                "/settings/fx-rates",
                json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": bad }),
            )
            .await;
        assert_eq!(response.status, 422, "rate {bad} should be refused");
    }
}

#[sqlx::test]
async fn correcting_a_rate_replaces_it_rather_than_duplicating_it(pool: PgPool) {
    let app = TestApp::new(pool).await;

    rate(&app, "EUR", "2026-01-01", 1.10).await;
    rate(&app, "EUR", "2026-01-01", 1.12).await;

    let rates = app.get("/settings/fx-rates?currency=EUR").await;
    let rows = rates.data().as_array().unwrap();
    // One currency, one date, one rate — otherwise a lookup would have to break
    // a tie between two rows that both claim to be in force.
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["rate"].as_str().unwrap().parse::<f64>().unwrap(), 1.12);
}

#[sqlx::test]
async fn the_last_rate_for_a_currency_in_use_cannot_be_removed(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    rate(&app, "EUR", "2026-01-01", 1.10).await;
    app.post("/sales/quotes", quote(&customer, "EUR", "2026-08-01", 100.0)).await;

    let id = app.get("/settings/fx-rates?currency=EUR").await.data()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let refused = app.delete(&format!("/settings/fx-rates/{id}")).await;
    assert_eq!(refused.status, 422);
    assert!(refused.error_message().contains("EUR"), "{}", refused.error_message());

    // With a replacement on file it can go: the currency stays restatable.
    rate(&app, "EUR", "2026-02-01", 1.12).await;
    assert!(app.delete(&format!("/settings/fx-rates/{id}")).await.status.is_success());
}

#[sqlx::test]
async fn only_an_admin_can_set_a_rate(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let member = app.register("clerk@erp.test", "supersecret1", "Cy", "Clerk").await;
    let token = member.field("access_token");

    let response = app
        .put_as(
            &token,
            "/settings/fx-rates",
            json!({ "currency": "EUR", "effective_from": "2026-01-01", "rate": 1.1 }),
        )
        .await;
    assert_eq!(response.status, 403);

    // Reading them is not restricted: a document shows the rate it was raised
    // at, and that has to be explainable without admin rights.
    assert!(app.get_as(&token, "/settings/fx-rates").await.status.is_success());
}

#[sqlx::test]
async fn the_currency_list_offers_the_base_plus_whatever_has_a_rate(pool: PgPool) {
    let app = TestApp::new(pool).await;

    let before = app.get("/settings/currencies").await;
    assert_eq!(before.field("base"), "USD");
    assert_eq!(before.data()["available"].as_array().unwrap().len(), 1);

    rate(&app, "EUR", "2026-01-01", 1.10).await;
    rate(&app, "GBP", "2026-01-01", 1.27).await;

    let after = app.get("/settings/currencies").await;
    let available: Vec<String> = after.data()["available"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    // The base is always offered and never needs a rate of its own.
    assert_eq!(available, vec!["EUR", "GBP", "USD"]);
}
