//! One query across every module.
//!
//! Most of what matters here is what a given caller *cannot* find: search reads
//! fifteen tables, two of which hold data the rest of the business is not
//! entitled to, so the same term has to return different sets to different
//! people.

mod common;

use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

/// The hits a token gets for a term, as `(kind, title)` pairs.
async fn search_as(app: &TestApp, token: &str, term: &str) -> Vec<(String, String)> {
    let response = app.get_as(token, &format!("/search?q={term}")).await;
    assert!(response.status.is_success(), "{}", response.body);
    response.data()["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| {
            (hit["kind"].as_str().unwrap().to_string(), hit["title"].as_str().unwrap().to_string())
        })
        .collect()
}

/// The same, as the bootstrap admin.
async fn search(app: &TestApp, term: &str) -> Vec<(String, String)> {
    search_as(app, &app.admin_token, term).await
}

fn kinds(hits: &[(String, String)]) -> Vec<&str> {
    let mut k: Vec<&str> = hits.iter().map(|(kind, _)| kind.as_str()).collect();
    k.sort_unstable();
    k.dedup();
    k
}

/// A company, a product and a warehouse all sharing a term.
async fn seed_matching(app: &TestApp) {
    app.create(
        "/crm/companies",
        json!({ "name": "Northwind Trading", "company_type": "customer", "email": "ap@northwind.test" }),
    )
    .await;
    app.create(
        "/inventory/products",
        json!({ "sku": "NW-100", "name": "Northwind Widget", "sale_price": 10.00 }),
    )
    .await;
    app.create("/inventory/warehouses", json!({ "code": "NW1", "name": "Northwind Depot" })).await;
}

#[sqlx::test]
async fn one_term_finds_records_across_modules(pool: PgPool) {
    let app = TestApp::new(pool).await;
    seed_matching(&app).await;

    let hits = search(&app, "northwind").await;

    // The point of the feature: no need to know which screen to open first.
    assert_eq!(kinds(&hits), vec!["company", "product", "warehouse"], "{hits:?}");
    assert!(hits.iter().any(|(_, title)| title == "Northwind Trading"));
}

#[sqlx::test]
async fn matching_behaves_like_the_list_filters(pool: PgPool) {
    let app = TestApp::new(pool).await;
    seed_matching(&app).await;

    // Case-insensitive and mid-string, the same as every module's own `search`
    // filter — a term should not behave differently depending on where it was
    // typed.
    assert!(!search(&app, "NORTHWIND").await.is_empty());
    // Encoded, because a raw space is not a valid URI character.
    assert!(!search(&app, "thwind%20tra").await.is_empty());
    assert!(!search(&app, "Widget").await.is_empty());
}

#[sqlx::test]
async fn a_document_is_found_by_its_number(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let customer = app.customer().await;
    let quote = app
        .post(
            "/sales/quotes",
            json!({
                "customer_id": customer, "issue_date": "2026-03-01", "expiry_date": "2026-12-31",
                "lines": [{ "description": "Widget", "quantity": 1, "unit_price": 100.00 }]
            }),
        )
        .await;
    let number = quote.field("quote_number");

    let hits = search(&app, &number).await;
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0], ("quote".to_string(), number));
}

#[sqlx::test]
async fn a_term_too_short_to_mean_anything_searches_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    seed_matching(&app).await;

    // `ILIKE '%n%'` is a sequential scan over fifteen tables to return
    // everything containing one letter.
    assert!(search(&app, "n").await.is_empty());
    assert!(search(&app, "").await.is_empty());
    // Two is enough to be a search.
    assert!(!search(&app, "no").await.is_empty());
}

#[sqlx::test]
async fn no_match_is_an_empty_list_rather_than_an_error(pool: PgPool) {
    let app = TestApp::new(pool).await;
    seed_matching(&app).await;

    let response = app.get("/search?q=nothingmatchesthis").await;
    assert!(response.status.is_success());
    assert!(response.data()["hits"].as_array().unwrap().is_empty());
    // Echoed back so a client can discard a stale response.
    assert_eq!(response.field("query"), "nothingmatchesthis");
}

#[sqlx::test]
async fn one_noisy_kind_cannot_bury_the_others(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // Twenty products and one company, all matching.
    for n in 0..20 {
        app.create(
            "/inventory/products",
            json!({ "sku": format!("ACME-{n:03}"), "name": format!("Acme part {n}"), "sale_price": 1.00 }),
        )
        .await;
    }
    app.create("/crm/companies", json!({ "name": "Acme Corp", "company_type": "customer" })).await;

    let hits = search(&app, "acme").await;
    let products = hits.iter().filter(|(kind, _)| kind == "product").count();

    assert_eq!(products, 5, "the per-kind cap did not hold");
    assert!(
        hits.iter().any(|(kind, _)| kind == "company"),
        "the single company was buried by products"
    );
}

#[sqlx::test]
async fn the_books_and_the_staff_list_are_not_searchable_by_everyone(pool: PgPool) {
    let app = TestApp::new(pool).await;

    // Something to find in each restricted kind, plus one anybody may see.
    app.create(
        "/accounting/accounts",
        json!({ "account_code": "1000", "account_name": "Restricted Cash", "account_type": "asset" }),
    )
    .await;
    app.employee("restricted@erp.test").await;
    app.create("/crm/companies", json!({ "name": "Restricted Supplies", "company_type": "customer" }))
        .await;

    let clerk = app.register("clerk@erp.test", "supersecret1", "Cy", "Clerk").await;
    let clerk_hits = search_as(&app, &clerk.field("access_token"), "restricted").await;

    // The company, and nothing else. Accounting and HR data are absent rather
    // than redacted — the query never reads them.
    assert_eq!(kinds(&clerk_hits), vec!["company"], "{clerk_hits:?}");

    // The admin sees all three.
    let admin_hits = search(&app, "restricted").await;
    assert_eq!(kinds(&admin_hits), vec!["account", "company", "employee"], "{admin_hits:?}");
}

#[sqlx::test]
async fn each_role_finds_its_own_module(pool: PgPool) {
    let app = TestApp::new(pool).await;
    app.create(
        "/accounting/accounts",
        json!({ "account_code": "1000", "account_name": "Shared Cash", "account_type": "asset" }),
    )
    .await;
    app.employee("shared@erp.test").await;

    let accountant = app.register("acc@erp.test", "supersecret1", "Ann", "Counter").await;
    let hr = app.register("hr@erp.test", "supersecret1", "Hank", "Ruh").await;
    app.put(&format!("/users/{}/role", accountant.field("user.id")), json!({ "role": "accountant" })).await;
    app.put(&format!("/users/{}/role", hr.field("user.id")), json!({ "role": "hr" })).await;

    // Roles are on the token, so both need a fresh one.
    let acc_token = app
        .post_anon("/auth/login", json!({ "email": "acc@erp.test", "password": "supersecret1" }))
        .await
        .field("access_token");
    let hr_token = app
        .post_anon("/auth/login", json!({ "email": "hr@erp.test", "password": "supersecret1" }))
        .await
        .field("access_token");

    assert_eq!(kinds(&search_as(&app, &acc_token, "shared").await), vec!["account"]);
    assert_eq!(kinds(&search_as(&app, &hr_token, "shared").await), vec!["employee"]);
}
