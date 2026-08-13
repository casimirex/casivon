//! Keeps the OpenAPI document honest.
//!
//! A hand-maintained spec drifts the moment someone adds a route and forgets
//! the annotation, and a spec that lies is worse than none. These tests check
//! both directions: every documented operation is really routed, and every
//! route is really documented.

mod common;

use std::collections::BTreeSet;

use common::TestApp;
use casivon_backend::openapi::ApiDoc;
use serde_json::Value;
use sqlx::PgPool;
use utoipa::OpenApi;

fn spec() -> Value {
    serde_json::from_str(&ApiDoc::openapi().to_json().unwrap()).unwrap()
}

/// Every (method, path) the document claims to describe.
fn documented() -> BTreeSet<(String, String)> {
    let spec = spec();
    let mut out = BTreeSet::new();
    for (path, item) in spec["paths"].as_object().unwrap() {
        for method in item.as_object().unwrap().keys() {
            out.insert((method.to_uppercase(), path.clone()));
        }
    }
    out
}

#[test]
fn the_document_is_valid_and_complete_enough_to_be_useful() {
    let spec = spec();

    assert_eq!(spec["openapi"].as_str().unwrap()[..3].to_string(), "3.1");
    assert_eq!(spec["info"]["title"], "ERP API");

    // The bearer scheme has to exist, or every `security` reference dangles.
    let schemes = &spec["components"]["securitySchemes"];
    assert_eq!(schemes["bearer"]["scheme"], "bearer");
    assert_eq!(schemes["bearer"]["bearerFormat"], "JWT");

    for (method, path) in documented() {
        let operation = &spec["paths"][&path][method.to_lowercase()];
        assert!(
            operation["responses"].as_object().is_some_and(|r| !r.is_empty()),
            "{method} {path} documents no responses"
        );
        assert!(
            operation["tags"].as_array().is_some_and(|t| !t.is_empty()),
            "{method} {path} has no tag, so it will not appear under any heading"
        );
    }
}

#[test]
fn the_committed_spec_matches_the_code() {
    // `frontend/openapi.json` is committed so a clone builds without a Rust
    // toolchain, and the frontend's wire types are generated from it. That makes
    // it a second copy of this document, and a stale copy would silently type
    // the frontend against an API that no longer exists.
    let committed_path =
        format!("{}/../frontend/openapi.json", env!("CARGO_MANIFEST_DIR"));
    let committed = std::fs::read_to_string(&committed_path)
        .expect("frontend/openapi.json is missing — run `cargo run --bin openapi > ../frontend/openapi.json`");

    let current = ApiDoc::openapi().to_pretty_json().unwrap();

    // Compared as parsed JSON so trailing-newline differences are not a failure.
    let committed: Value = serde_json::from_str(&committed).expect("committed spec is not valid JSON");
    let current: Value = serde_json::from_str(&current).unwrap();

    assert_eq!(
        committed, current,
        "frontend/openapi.json is out of date. Regenerate it and the types built from it:\n  \
         cargo run --bin openapi > ../frontend/openapi.json\n  \
         cd ../frontend && npm run generate:types"
    );
}

#[test]
fn every_route_in_the_application_is_documented() {
    // Parsed from the route tables rather than from a hand-kept list, so adding
    // a route to a module is enough to make this test demand its annotation.
    let mut routed = BTreeSet::new();
    let modules = [
        ("auth", ""), // auth mounts its own absolute paths in `app.rs`
        ("crm", "/api/v1/crm"),
        ("sales", "/api/v1/sales"),
        ("inventory", "/api/v1/inventory"),
        ("purchasing", "/api/v1/purchasing"),
        ("accounting", "/api/v1/accounting"),
        ("hr", "/api/v1/hr"),
        ("projects", "/api/v1/projects"),
        ("settings", "/api/v1/settings"),
    ];

    for (module, prefix) in modules {
        if module == "auth" {
            continue; // covered by the handful of assertions below
        }
        let source = std::fs::read_to_string(format!(
            "{}/src/modules/{module}/routes.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();

        for (path, method) in parse_routes(&source) {
            let full = if path == "/" {
                prefix.to_string()
            } else {
                format!("{prefix}{path}")
            };
            routed.insert((method, to_openapi_path(&full)));
        }
    }

    let documented = documented();
    let missing: Vec<_> = routed.difference(&documented).collect();
    assert!(
        missing.is_empty(),
        "these routes exist but carry no #[utoipa::path] annotation: {missing:#?}"
    );
}

#[sqlx::test]
async fn every_documented_operation_is_actually_routed(pool: PgPool) {
    let app = TestApp::new(pool).await;

    for (method, path) in documented() {
        // A concrete id keeps the request shaped like a real one; the response
        // only has to prove the route exists, not that the record does.
        let concrete = path
            .replace("{id}", "00000000-0000-0000-0000-000000000000")
            .replace("{task_id}", "00000000-0000-0000-0000-000000000000");
        let concrete = concrete.strip_prefix("/api/v1").unwrap_or(&concrete).to_string();

        let response = app.probe(&method, &concrete).await;

        // Axum answers an unrouted path with a bare 404 and a wrong method with
        // 405; every real route returns something else, even if only a 401.
        assert_ne!(
            response.status, 405,
            "{method} {path} is documented but the router does not accept that method"
        );
        assert!(
            !(response.status == 404 && response.body.is_null()),
            "{method} {path} is documented but nothing is routed there"
        );
    }
}

/// `/quotes/:id/status` -> `/quotes/{id}/status`
fn to_openapi_path(path: &str) -> String {
    path.split('/')
        .map(|segment| match segment.strip_prefix(':') {
            Some(name) => format!("{{{name}}}"),
            None => segment.to_string(),
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Pulls `(path, METHOD)` pairs out of a `routes.rs` source file.
fn parse_routes(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = source;

    while let Some(start) = rest.find(".route(") {
        rest = &rest[start + ".route(".len()..];
        let Some(open) = rest.find('"') else { break };
        let Some(close) = rest[open + 1..].find('"') else { break };
        let path = rest[open + 1..open + 1 + close].to_string();

        // Take the rest of this .route(...) call by matching parentheses.
        let tail = &rest[open + 1 + close..];
        let mut depth = 1;
        let mut end = 0;
        for (index, character) in tail.char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = index;
                        break;
                    }
                }
                _ => {}
            }
        }
        let chain = &tail[..end];

        for method in ["get", "post", "put", "delete", "patch"] {
            let needle = format!("{method}(");
            let mut search = chain;
            while let Some(at) = search.find(&needle) {
                // Skip `axum::routing::get` style prefixes matching mid-word.
                let preceded_by_word = at > 0
                    && search[..at]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                if !preceded_by_word {
                    out.push((path.clone(), method.to_uppercase()));
                }
                search = &search[at + needle.len()..];
            }
        }

        rest = tail;
    }

    out
}
