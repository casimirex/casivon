//! Uploading a receipt, and who is allowed to look at it.
//!
//! The read rule is inherited rather than invented: a receipt is readable by
//! exactly whoever may read the expense claim it is attached to. Most of what is
//! checked here is that inheritance holding — including from the other side,
//! where attaching somebody else's upload to your own claim would otherwise turn
//! the rule inside out.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// A minimal but genuine PNG header, followed by filler. Enough for the byte
/// sniffer, which is all the application looks at.
fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(b"IHDR and the rest of a small receipt photo");
    bytes
}

fn pdf() -> Vec<u8> {
    b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec()
}

/// A login with an employee record linked to it — the same fixture the HR
/// scoping tests use, because this is the same rule.
struct Staff {
    token: String,
    employee: String,
}

async fn staff(app: &TestApp, email: &str, first: &str) -> Staff {
    let registered = app.register(email, "supersecret1", first, "Person").await;
    let user_id = registered.field("user.id");

    let employee = app
        .create(
            "/hr/employees",
            json!({
                "first_name": first, "last_name": "Person", "email": email,
                "hire_date": "2024-01-15", "user_id": user_id
            }),
        )
        .await;

    Staff { token: registered.field("access_token"), employee }
}

/// An expense claim with `receipt` attached to its only line.
fn claim_with(employee: &str, receipt: &str) -> serde_json::Value {
    json!({
        "employee_id": employee,
        "description": "Client visit",
        "lines": [{
            "expense_date": "2026-03-02", "category": "travel",
            "description": "Taxi", "amount": 42.50,
            "receipt_attachment_id": receipt
        }]
    })
}

#[sqlx::test]
async fn a_receipt_survives_the_round_trip(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;

    let uploaded = app.upload_as(&lisa.token, "Taxi receipt.png", &png()).await;
    assert_eq!(uploaded.status, 201, "{}", uploaded.body);
    assert_eq!(uploaded.field("file_name"), "Taxi receipt.png");
    // The claimed part header said image/png and so do the bytes, but it is the
    // bytes that were consulted - see `a_renamed_file_is_refused`.
    assert_eq!(uploaded.field("content_type"), "image/png");

    let id = uploaded.id();

    // The bytes really reached the store, under a generated key.
    assert_eq!(app.files.len(), 1);

    let claim = app.post_as(&lisa.token, "/hr/expense-reports", claim_with(&lisa.employee, &id)).await;
    assert!(claim.status.is_success(), "{}", claim.body);
    assert_eq!(claim.data()["lines"][0]["receipt_attachment_id"], id.as_str());

    let link = app.get_as(&lisa.token, &format!("/files/{id}")).await;
    assert!(link.status.is_success(), "{}", link.body);
    assert_eq!(link.field("file_name"), "Taxi receipt.png");
    assert_eq!(link.field("is_image"), "true");
    // Named for the download rather than for the uuid it is stored under.
    assert!(link.field("url").contains("Taxi%20receipt.png") || link.field("url").contains("Taxi receipt.png"), "{}", link.field("url"));
}

#[sqlx::test]
async fn a_colleagues_receipt_is_not_found(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;
    let bart = staff(&app, "bart@erp.test", "Bart").await;

    let receipt = app.upload_as(&bart.token, "hotel.pdf", &pdf()).await.id();
    app.post_as(&bart.token, "/hr/expense-reports", claim_with(&bart.employee, &receipt)).await;

    // 404 and not 403: a forbidden would confirm the id names a real file,
    // which is what somebody guessing ids wants to learn.
    let peeked = app.get_as(&lisa.token, &format!("/files/{receipt}")).await;
    assert_eq!(peeked.status, 404, "{}", peeked.body);

    // A id that names nothing answers identically.
    let invented = app
        .get_as(&lisa.token, "/files/00000000-0000-0000-0000-000000000000")
        .await;
    assert_eq!(invented.status, 404);
    assert_eq!(peeked.error_message(), invented.error_message());
}

#[sqlx::test]
async fn attaching_someone_elses_upload_is_refused(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;
    let bart = staff(&app, "bart@erp.test", "Bart").await;

    let his = app.upload_as(&bart.token, "hotel.pdf", &pdf()).await.id();

    // The attack this closes: attaching a guessed id to a claim Lisa is
    // entitled to read would make Bart's file readable to her, because reading
    // walks from the file to the claim it hangs off.
    let filed = app
        .post_as(&lisa.token, "/hr/expense-reports", claim_with(&lisa.employee, &his))
        .await;
    assert_eq!(filed.status, 403, "{}", filed.body);
    assert!(filed.error_message().contains("not uploaded by you"), "{}", filed.error_message());

    // And it is still not readable.
    assert_eq!(app.get_as(&lisa.token, &format!("/files/{his}")).await.status, 404);

    // The same check on the edit path, not only on create.
    let hers = app.upload_as(&lisa.token, "taxi.png", &png()).await.id();
    let claim = app
        .post_as(&lisa.token, "/hr/expense-reports", claim_with(&lisa.employee, &hers))
        .await
        .id();

    let edited = app
        .put_as(
            &lisa.token,
            &format!("/hr/expense-reports/{claim}"),
            json!({ "lines": [{
                "expense_date": "2026-03-02", "category": "travel",
                "description": "Taxi", "amount": 42.50,
                "receipt_attachment_id": his
            }] }),
        )
        .await;
    assert_eq!(edited.status, 403, "{}", edited.body);
}

#[sqlx::test]
async fn an_unattached_upload_belongs_to_whoever_uploaded_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;
    let bart = staff(&app, "bart@erp.test", "Bart").await;

    // The window between choosing a file and saving the form: nothing points at
    // this file yet, so there is no claim to inherit a rule from.
    let orphan = app.upload_as(&lisa.token, "taxi.png", &png()).await.id();

    assert!(app.get_as(&lisa.token, &format!("/files/{orphan}")).await.status.is_success());
    assert_eq!(app.get_as(&bart.token, &format!("/files/{orphan}")).await.status, 404);

    // Not even HR, until it is attached to something they have business seeing.
    assert_eq!(app.get(&format!("/files/{orphan}")).await.status, 404);
}

#[sqlx::test]
async fn a_renamed_file_is_refused_on_its_bytes(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // Named .png, and the multipart part claims image/png too. Neither counts.
    let refused = app.upload("definitely-a-receipt.png", b"<html><script>alert(1)</script>").await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(refused.error_message().contains("does not look like"), "{}", refused.error_message());

    // Nothing was written on the way to refusing.
    assert_eq!(app.files.len(), 0);

    // An empty file is refused too, rather than stored as a zero-byte receipt.
    assert_eq!(app.upload("blank.png", b"").await.status, 422);
    assert_eq!(app.files.len(), 0);
}

#[sqlx::test]
async fn an_oversized_upload_is_refused_rather_than_truncated(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // Just over the 10 MB limit, with a valid PNG header so that size is the
    // only thing wrong with it.
    let mut huge = png();
    huge.resize(11 * 1024 * 1024, 0);

    let refused = app.upload("enormous.png", &huge).await;
    assert_eq!(refused.status, 413, "{}", refused.body);
    assert_eq!(app.files.len(), 0);

    // A file inside the limit still goes through, so the cap is a cap and not a
    // wall.
    let mut large = png();
    large.resize(9 * 1024 * 1024, 0);
    assert_eq!(app.upload("big.png", &large).await.status, 201);
}

#[sqlx::test]
async fn hr_can_read_any_employees_receipt(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let lisa = staff(&app, "lisa@erp.test", "Lisa").await;

    let receipt = app.upload_as(&lisa.token, "taxi.png", &png()).await.id();
    app.post_as(&lisa.token, "/hr/expense-reports", claim_with(&lisa.employee, &receipt)).await;

    // The admin is HR-equivalent here, and approving a claim you cannot see the
    // evidence for is the workflow this feature exists to fix.
    let seen = app.get(&format!("/files/{receipt}")).await;
    assert!(seen.status.is_success(), "{}", seen.body);
    assert_eq!(seen.field("file_name"), "taxi.png");
}

#[sqlx::test]
async fn without_storage_configured_the_upload_says_so(pool: PgPool) {
    let app = TestApp::without_storage(pool).await;

    let refused = app.upload("taxi.png", &png()).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    // Names the setting, rather than reporting success and dropping the file —
    // which is the difference between this and the logging email sender.
    assert!(refused.error_message().contains("S3_ENDPOINT"), "{}", refused.error_message());

    // Nothing else about the application is affected.
    assert!(app.get("/hr/employees").await.status.is_success());
}
