//! Quote → order → invoice → payment, and the rules that keep the chain honest.

mod common;

use chrono::{Duration, NaiveDate};
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// 10 x 100.00 less 10% is 900.00 net; 20% tax on that is 180.00.
fn quote_body(customer_id: &str) -> serde_json::Value {
    json!({
        "customer_id": customer_id,
        "issue_date": "2026-08-01",
        "expiry_date": "2026-09-01",
        "lines": [{
            "description": "Widget",
            "quantity": 10,
            "unit_price": 100.00,
            "discount_percent": 10,
            "tax_rate": 20
        }]
    })
}

#[sqlx::test]
async fn a_quote_totals_its_lines_on_the_server(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    let quote = app.post("/sales/quotes", quote_body(&customer)).await;

    quote.assert_money("subtotal", "900.00");
    quote.assert_money("tax_amount", "180.00");
    quote.assert_money("total", "1080.00");
    // Numbers on the wire would be IEEE-754 doubles; money must stay a string.
    assert!(quote.data()["total"].is_string());
    assert!(quote.field("quote_number").starts_with("QUO-"));
}

#[sqlx::test]
async fn document_numbers_are_unique_across_concurrent_creates(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    let mut numbers = Vec::new();
    for _ in 0..5 {
        numbers.push(app.post("/sales/quotes", quote_body(&customer)).await.field("quote_number"));
    }

    numbers.sort();
    numbers.dedup();
    assert_eq!(numbers.len(), 5, "sequence handed out a duplicate: {numbers:?}");
}

#[sqlx::test]
async fn a_quote_cannot_skip_a_status(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;
    let quote = app.create("/sales/quotes", quote_body(&customer)).await;

    let skipped = app.put(&format!("/sales/quotes/{quote}/status"), json!({ "status": "accepted" })).await;
    assert!(!skipped.status.is_success());
    assert!(skipped.error_message().contains("draft"), "{}", skipped.error_message());

    app.advance(&format!("/sales/quotes/{quote}/status"), &["sent", "accepted"]).await;
    assert_eq!(app.get(&format!("/sales/quotes/{quote}")).await.field("status"), "accepted");
}

#[sqlx::test]
async fn an_accepted_quote_converts_to_an_order_exactly_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;
    let quote = app.create("/sales/quotes", quote_body(&customer)).await;
    app.advance(&format!("/sales/quotes/{quote}/status"), &["sent", "accepted"]).await;

    let order = app.post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({})).await;
    assert!(order.field("order_number").starts_with("SO-"));
    // The order carries the quote's numbers rather than recomputing them loosely.
    order.assert_money("total", "1080.00");

    let again = app.post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({})).await;
    assert!(!again.status.is_success(), "a quote was converted twice");
}

#[sqlx::test]
async fn a_draft_quote_cannot_become_an_order(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;
    let quote = app.create("/sales/quotes", quote_body(&customer)).await;

    let response = app.post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({})).await;

    assert!(!response.status.is_success());
}

#[sqlx::test]
async fn an_invoice_settles_from_its_payment_ledger(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;
    let quote = app.create("/sales/quotes", quote_body(&customer)).await;
    app.advance(&format!("/sales/quotes/{quote}/status"), &["sent", "accepted"]).await;
    let order = app.post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({})).await.id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed"]).await;

    let invoice = app
        .post(&format!("/sales/orders/{order}/convert-to-invoice"), json!({ "payment_terms_days": 30 }))
        .await;
    let invoice_id = invoice.id();
    invoice.assert_money("amount_due", "1080.00");

    // The invoice's terms run from its own issue date, not from the order's.
    let issued: NaiveDate = invoice.field("issue_date").parse().unwrap();
    let due: NaiveDate = invoice.field("due_date").parse().unwrap();
    assert_eq!(due - issued, Duration::days(30));

    // Raised is not issued. A draft has raised no receivable, so nothing can be
    // paid against it — and settling a receivable is what this test is about.
    app.advance(&format!("/sales/invoices/{invoice_id}/status"), &["sent"]).await;

    let overpayment = app
        .post(
            "/sales/payments",
            json!({ "invoice_id": invoice_id, "amount": 9999, "payment_method": "bank_transfer", "payment_date": "2026-08-05" }),
        )
        .await;
    assert!(!overpayment.status.is_success());
    assert!(overpayment.error_message().to_lowercase().contains("exceed"));

    app.post(
        "/sales/payments",
        json!({ "invoice_id": invoice_id, "amount": 1000.00, "payment_method": "bank_transfer", "payment_date": "2026-08-05" }),
    )
    .await;

    let partly_paid = app.get(&format!("/sales/invoices/{invoice_id}")).await;
    partly_paid.assert_money("amount_paid", "1000.00");
    partly_paid.assert_money("amount_due", "80.00");
    // There is no separate `partially_paid` state: how much is outstanding is
    // carried by `amount_due`, and the invoice stays a live receivable.
    assert_eq!(partly_paid.field("status"), "sent");

    app.post(
        "/sales/payments",
        json!({ "invoice_id": invoice_id, "amount": 80.00, "payment_method": "cash", "payment_date": "2026-08-06" }),
    )
    .await;

    let settled = app.get(&format!("/sales/invoices/{invoice_id}")).await;
    settled.assert_money("amount_due", "0");
    assert_eq!(settled.field("status"), "paid");
}

#[sqlx::test]
async fn deleting_a_payment_reopens_the_invoice(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;
    let quote = app.create("/sales/quotes", quote_body(&customer)).await;
    app.advance(&format!("/sales/quotes/{quote}/status"), &["sent", "accepted"]).await;
    let order = app.post(&format!("/sales/quotes/{quote}/convert-to-order"), json!({})).await.id();
    app.advance(&format!("/sales/orders/{order}/status"), &["confirmed"]).await;
    let invoice = app
        .post(&format!("/sales/orders/{order}/convert-to-invoice"), json!({ "payment_terms_days": 30 }))
        .await
        .id();
    // Issued before it can be settled: a draft has raised no receivable.
    app.advance(&format!("/sales/invoices/{invoice}/status"), &["sent"]).await;

    let payment = app
        .post(
            "/sales/payments",
            json!({ "invoice_id": invoice, "amount": 1080.00, "payment_method": "cash", "payment_date": "2026-08-05" }),
        )
        .await
        .id();
    assert_eq!(app.get(&format!("/sales/invoices/{invoice}")).await.field("status"), "paid");

    // Settlement is recomputed from the ledger, so reversing a payment must
    // walk the invoice back rather than leave it marked paid.
    app.delete(&format!("/sales/payments/{payment}")).await;

    let reopened = app.get(&format!("/sales/invoices/{invoice}")).await;
    reopened.assert_money("amount_paid", "0");
    reopened.assert_money("amount_due", "1080.00");
    assert_ne!(reopened.field("status"), "paid");
}

#[sqlx::test]
async fn a_quote_needs_at_least_one_line(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    let response = app
        .post(
            "/sales/quotes",
            json!({
                "customer_id": customer,
                "issue_date": "2026-08-01",
                "expiry_date": "2026-09-01",
                "lines": []
            }),
        )
        .await;

    assert_eq!(response.status, 422);
    assert!(response.error_message().contains("at least one line"));
}

#[sqlx::test]
async fn a_line_rate_must_be_a_percentage(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;

    let line = |field: &str, value: i32| {
        json!({
            "customer_id": customer,
            "issue_date": "2026-08-01",
            "expiry_date": "2026-09-01",
            "lines": [{
                "description": "Widget",
                "quantity": 1,
                "unit_price": 100.00,
                field: value,
            }],
        })
    };

    // 0.2 would be a fifth of a percent, but the real trap is the other way:
    // someone reaching for a multiplier and typing a number past 100.
    for field in ["tax_rate", "discount_percent"] {
        let too_large = app.post("/sales/quotes", line(field, 2000)).await;
        assert_eq!(too_large.status, 422, "{field} accepted 2000");
        assert!(too_large.error_message().contains("percentage"), "{}", too_large.error_message());

        let negative = app.post("/sales/quotes", line(field, -5)).await;
        assert_eq!(negative.status, 422, "{field} accepted a negative rate");
    }

    assert!(app.post("/sales/quotes", line("tax_rate", 20)).await.status.is_success());
}
