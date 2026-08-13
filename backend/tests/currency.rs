//! The base currency, and what every document defaults to.
//!
//! These tests cover the single-currency path: the organisation's own currency,
//! which needs no exchange rate and is what a document gets when it asks for
//! nothing. Trading in a second currency is covered in `multi_currency.rs`.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

fn quote(customer: &str, currency: Option<&str>) -> serde_json::Value {
    let mut body = json!({
        "customer_id": customer,
        "issue_date": "2026-08-01",
        "expiry_date": "2026-09-01",
        "lines": [{ "description": "Widget", "quantity": 1, "unit_price": 100.00 }]
    });
    if let Some(currency) = currency {
        body["currency"] = json!(currency);
    }
    body
}

#[sqlx::test]
async fn a_document_takes_the_organisations_currency(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    assert_eq!(app.get("/settings/organization").await.field("default_currency"), "USD");
    assert_eq!(app.post("/sales/quotes", quote(&customer, None)).await.field("currency"), "USD");
}

#[sqlx::test]
async fn changing_the_organisation_currency_changes_what_new_documents_carry(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    // Nothing has been raised yet, so the switch is allowed.
    app.put("/settings/organization", json!({ "default_currency": "EUR" })).await;

    // Read at the point of use, not cached at start-up.
    assert_eq!(app.post("/sales/quotes", quote(&customer, None)).await.field("currency"), "EUR");
    assert_eq!(
        app.create("/projects", json!({ "name": "Rollout", "budget": 1000 })).await.is_empty(),
        false
    );
}

#[sqlx::test]
async fn a_currency_with_no_rate_is_refused_rather_than_assumed_to_be_parity(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    // No EUR rate has been entered, so there is no way to say what a euro quote
    // is worth. Booking it at 1:1 would overstate the sale by whatever the real
    // rate is, silently and permanently.
    let response = app.post("/sales/quotes", quote(&customer, Some("EUR"))).await;

    assert_eq!(response.status, 422);
    let message = response.error_message();
    assert!(message.contains("USD") && message.contains("EUR"), "{message}");
    assert!(message.contains("Exchange Rates"), "{message}");
}

#[sqlx::test]
async fn the_base_currency_is_accepted_in_any_case(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    assert_eq!(app.post("/sales/quotes", quote(&customer, Some("usd"))).await.field("currency"), "USD");

    // A blank code is a malformed field rather than "use the default", and the
    // DTO's length rule catches it before the currency logic is reached. The
    // frontend drops empty optional fields, so this is the shape of a
    // hand-written request.
    let blank = app.post("/sales/quotes", quote(&customer, Some(""))).await;
    assert_eq!(blank.status, 422);
    assert!(blank.error_message().contains("3-letter"), "{}", blank.error_message());
}

#[sqlx::test]
async fn every_module_that_stamps_a_currency_uses_the_same_one(pool: PgPool) {
    let app = TestApp::new(pool).await;
    app.put("/settings/organization", json!({ "default_currency": "GBP" })).await;

    let customer = app.customer().await;
    let vendor = app.create("/purchasing/vendors", json!({ "name": "Acme" })).await;
    let account = app
        .create(
            "/accounting/accounts",
            json!({ "account_code": "1000", "account_name": "Cash", "account_type": "asset" }),
        )
        .await;

    // One assertion per module that stamps a currency onto something.
    assert_eq!(app.post("/sales/quotes", quote(&customer, None)).await.field("currency"), "GBP");
    assert_eq!(
        app.get(&format!("/purchasing/vendors/{vendor}")).await.field("currency"),
        "GBP"
    );
    assert_eq!(
        app.get(&format!("/accounting/accounts/{account}")).await.field("currency"),
        "GBP"
    );
    assert_eq!(
        app.post(
            "/crm/opportunities",
            json!({ "title": "Deal", "company_id": customer, "value": 100, "stage": "proposal" })
        )
        .await
        .field("currency"),
        "GBP"
    );
    assert_eq!(
        app.post("/projects", json!({ "name": "Rollout", "budget": 1000 })).await.field("currency"),
        "GBP"
    );
    assert_eq!(
        app.post(
            "/hr/employees",
            json!({
                "first_name": "Lisa", "last_name": "Simpson", "email": "lisa@erp.test",
                "hire_date": "2024-01-15"
            })
        )
        .await
        .field("currency"),
        "GBP"
    );
}

#[sqlx::test]
async fn the_currency_cannot_be_changed_once_money_has_been_recorded(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;
    app.post("/sales/quotes", quote(&customer, None)).await;

    let response = app.put("/settings/organization", json!({ "default_currency": "EUR" })).await;

    assert_eq!(response.status, 422);
    let message = response.error_message();
    // Every rate on file is quoted against the base currency, and every stored
    // base amount was computed with one. Changing it invalidates all of them.
    assert!(message.contains("exchange rate") && message.contains("base amount"), "{message}");
    assert_eq!(app.get("/settings/organization").await.field("default_currency"), "USD");
}

#[sqlx::test]
async fn other_settings_still_save_once_documents_exist(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;
    app.post("/sales/quotes", quote(&customer, None)).await;

    // The guard is about the currency, not about the whole form.
    let response = app.put("/settings/organization", json!({ "name": "Globex" })).await;
    assert!(response.status.is_success(), "{}", response.body);
    assert_eq!(response.field("name"), "Globex");

    // Re-sending the currency it already has is not a change.
    assert!(app
        .put("/settings/organization", json!({ "name": "Globex", "default_currency": "USD" }))
        .await
        .status
        .is_success());
}
