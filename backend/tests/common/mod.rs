//! Shared harness for the integration tests.
//!
//! Every test gets a throwaway database from `#[sqlx::test]` with all migrations
//! already applied, mounts the *real* router onto it, and drives it in-process
//! through `tower::ServiceExt::oneshot` — no socket, no port, no fixed ordering
//! between tests. What is exercised is the same stack a client hits: extractors,
//! validation, auth middleware, use cases and SQL.

// Each integration test file is its own crate and uses a different slice of this
// module, so unused helpers here are expected rather than a smell.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use chrono::{DateTime, Utc};
use casivon_backend::{
    app::build_router,
    config::AppConfig,
    error::AppResult,
    modules::auth::domain::repositories::RevokedTokenStore,
    infrastructure::s3_storage::UnconfiguredObjectStore,
    modules::settings::infrastructure::currency_resolver::PgCurrencyResolver,
    shared::email::{EmailMessage, EmailSender},
    shared::storage::ObjectStore,
};
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

/// In-memory stand-in for the Redis denylist.
///
/// Keeps the suite hermetic: tests get a store of their own instead of sharing
/// one Redis across parallel runs. The Redis implementation is exercised
/// separately in `tests/redis_revocation.rs`.
#[derive(Default)]
pub struct InMemoryRevokedTokens {
    entries: Mutex<HashMap<Uuid, DateTime<Utc>>>,
}

#[async_trait]
impl RevokedTokenStore for InMemoryRevokedTokens {
    async fn revoke(&self, jti: Uuid, expires_at: DateTime<Utc>) -> AppResult<()> {
        self.entries.lock().unwrap().insert(jti, expires_at);
        Ok(())
    }

    async fn is_revoked(&self, jti: Uuid) -> AppResult<bool> {
        // Redis drops the key when it expires; do the same so a revoked token
        // is not reported as revoked forever.
        Ok(self.entries.lock().unwrap().get(&jti).is_some_and(|expiry| *expiry > Utc::now()))
    }
}

/// Captures mail instead of sending it, so tests can read the link a user would
/// have received rather than reaching into the database for the token.
#[derive(Default)]
pub struct RecordingEmailSender {
    sent: Mutex<Vec<EmailMessage>>,
}

impl RecordingEmailSender {
    pub fn sent(&self) -> Vec<EmailMessage> {
        self.sent.lock().unwrap().clone()
    }

    /// The most recent message to an address, if any.
    pub fn last_to(&self, address: &str) -> Option<EmailMessage> {
        self.sent().into_iter().rev().find(|message| message.to == address)
    }
}

#[async_trait]
impl EmailSender for RecordingEmailSender {
    async fn send(&self, message: EmailMessage) -> AppResult<()> {
        self.sent.lock().unwrap().push(message);
        Ok(())
    }
}

/// Keeps uploaded bytes in a map instead of a bucket.
///
/// Same reasoning as `InMemoryRevokedTokens`: the suite stays hermetic, so
/// `cargo test` needs no MinIO running and parallel tests cannot see each
/// other's files. The real client is exercised against a live MinIO by hand —
/// see the receipt upload section of the README.
#[derive(Default)]
pub struct InMemoryObjectStore {
    objects: Mutex<HashMap<String, Vec<u8>>>,
}

impl InMemoryObjectStore {
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.objects.lock().unwrap().get(key).cloned()
    }

    pub fn len(&self) -> usize {
        self.objects.lock().unwrap().len()
    }
}

#[async_trait]
impl ObjectStore for InMemoryObjectStore {
    async fn put(&self, key: &str, _content_type: &str, bytes: Vec<u8>) -> AppResult<()> {
        self.objects.lock().unwrap().insert(key.to_string(), bytes);
        Ok(())
    }

    async fn presigned_get(
        &self,
        key: &str,
        file_name: &str,
        ttl: std::time::Duration,
    ) -> AppResult<String> {
        // Shaped like the real thing — host, key, an expiry and a signature —
        // so a test can assert the key and the download name reached the store
        // without pretending to verify a signature nothing here produces.
        if self.objects.lock().unwrap().contains_key(key) {
            Ok(format!(
                "http://object-store.test/{key}?X-Amz-Expires={}\
                 &response-content-disposition=inline%3B%20filename%3D%22{file_name}%22\
                 &X-Amz-Signature=test",
                ttl.as_secs()
            ))
        } else {
            Err(casivon_backend::error::AppError::Internal)
        }
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }
}

pub struct TestApp {
    router: Router,
    /// The mail this app would have sent.
    pub email: Arc<RecordingEmailSender>,
    /// The files this app stored.
    pub files: Arc<InMemoryObjectStore>,
    /// Token of the bootstrap admin, used by the `*_as_admin` helpers.
    pub admin_token: String,
    pub admin_id: String,
}

/// A response decoded far enough to assert on.
pub struct TestResponse {
    pub status: StatusCode,
    pub body: Value,
}

impl TestResponse {
    /// The payload inside the `{ success, data }` envelope.
    ///
    /// Panics with the whole body when the request failed, so a broken test
    /// reports the server's complaint instead of "key `data` not found".
    pub fn data(&self) -> &Value {
        self.body.get("data").unwrap_or_else(|| {
            panic!("expected a data payload, got {} {}", self.status, self.body)
        })
    }

    /// The message from the `{ success: false, error }` envelope.
    pub fn error_message(&self) -> String {
        self.body
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("expected an error, got {} {}", self.status, self.body))
            .to_string()
    }

    pub fn pagination(&self) -> &Value {
        self.body
            .get("pagination")
            .unwrap_or_else(|| panic!("expected pagination, got {}", self.body))
    }

    /// `data` as an array — list endpoints only.
    pub fn rows(&self) -> &Vec<Value> {
        self.data()
            .as_array()
            .unwrap_or_else(|| panic!("expected a list, got {}", self.body))
    }

    /// Convenience for the common `data.<field>` string lookup.
    pub fn field(&self, path: &str) -> String {
        let value = self
            .data()
            .pointer(&format!("/{}", path.replace('.', "/")))
            .unwrap_or_else(|| panic!("no field `{}` in {}", path, self.body));
        match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }

    pub fn id(&self) -> String {
        self.field("id")
    }

    /// Asserts on a money field by value rather than by text.
    ///
    /// Money crosses the wire as a string, but the scale is not stable at zero:
    /// sqlx decodes a Postgres `numeric(15, 2)` zero as scale 0, so a settled
    /// invoice reports `"0"` where every other amount reads `"0.00"`. Comparing
    /// the parsed decimals keeps these tests about the arithmetic.
    pub fn assert_money(&self, path: &str, expected: &str) {
        let actual = self.field(path);
        let parsed: Decimal = actual
            .parse()
            .unwrap_or_else(|_| panic!("`{path}` is not a decimal string: {actual}"));
        let expected: Decimal = expected.parse().unwrap();
        assert_eq!(parsed, expected, "{path} was {actual}, expected {expected}");
    }
}

impl TestApp {
    /// Boots the application on `pool` and registers the bootstrap admin.
    pub async fn new(pool: PgPool) -> Self {
        Self::boot(pool, None).await
    }

    /// Boots as a deployment with no `S3_ENDPOINT` set, to check what an upload
    /// says when nobody configured anywhere to put it.
    pub async fn without_storage(pool: PgPool) -> Self {
        Self::boot(pool, Some(Arc::new(UnconfiguredObjectStore))).await
    }

    /// `storage` overrides what the router gets. `self.files` is always an
    /// in-memory store so assertions have something typed to look at; when an
    /// override is given it is simply never written to, which is exactly what
    /// the unconfigured case should look like.
    async fn boot(pool: PgPool, storage: Option<Arc<dyn ObjectStore>>) -> Self {
        let files = Arc::new(InMemoryObjectStore::default());
        let store: Arc<dyn ObjectStore> =
            storage.unwrap_or_else(|| files.clone() as Arc<dyn ObjectStore>);

        let config = AppConfig {
            database_url: String::new(), // unused: the pool is already open
            redis_url: String::new(),    // unused: no route touches Redis yet
            jwt_secret: "integration-test-secret".to_string(),
            jwt_access_expiry: 900,
            jwt_refresh_expiry: 604_800,
            app_base_url: "http://localhost:3000".to_string(),
            host: "127.0.0.1".to_string(),
            port: 0,
            // Mail is captured by `RecordingEmailSender` below, so no relay.
            smtp: None,
            // Files go to the store passed in above, not to a bucket.
            s3: None,
        };

        let email = Arc::new(RecordingEmailSender::default());
        let mut app = Self {
            router: build_router(
                pool.clone(),
                config,
                Arc::new(InMemoryRevokedTokens::default()),
                email.clone(),
                // The real reader: the organisation row is seeded by migration,
                // so tests exercise the same lookup production does.
                Arc::new(PgCurrencyResolver::new(pool)),
                store,
            ),
            email,
            files,
            admin_token: String::new(),
            admin_id: String::new(),
        };

        // The first account on a fresh database owns the instance, so this both
        // sets up the tests and asserts that bootstrap keeps working.
        let admin = app.register("admin@erp.test", "supersecret1", "Ada", "Admin").await;
        app.admin_token = admin.field("access_token");
        app.admin_id = admin.field("user.id");
        app
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> TestResponse {
        let mut builder = Request::builder().method(method).uri(format!("/api/v1{path}"));
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }

        let request = match body {
            Some(json) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        };

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();

        // A handler that panics or returns a non-JSON body would otherwise fail
        // with an opaque parse error; keep the raw text visible instead.
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                json!({ "raw": String::from_utf8_lossy(&bytes).to_string() })
            })
        };

        TestResponse { status, body }
    }

    // ---- unauthenticated -------------------------------------------------

    pub async fn register(&self, email: &str, password: &str, first: &str, last: &str) -> TestResponse {
        self.post_anon(
            "/auth/register",
            json!({
                "email": email,
                "password": password,
                "first_name": first,
                "last_name": last
            }),
        )
        .await
    }

    pub async fn post_anon(&self, path: &str, body: Value) -> TestResponse {
        self.send(Method::POST, path, None, Some(body)).await
    }

    pub async fn get_anon(&self, path: &str) -> TestResponse {
        self.send(Method::GET, path, None, None).await
    }

    /// Posts one file as a `multipart/form-data` body.
    ///
    /// Built by hand rather than with a helper crate: the multipart framing is
    /// part of what these tests are checking the handler against, and a
    /// generated body would hide a mistake in how the part is named.
    pub async fn upload_as(&self, token: &str, file_name: &str, bytes: &[u8]) -> TestResponse {
        const BOUNDARY: &str = "----erp-test-boundary";

        let mut body = Vec::new();
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n")
                .as_bytes(),
        );
        // Deliberately a lie in some tests: the handler decides the type from
        // the bytes, so what is claimed here should never matter.
        body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());

        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/files")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(Body::from(body))
            .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&bytes).to_string() }))
        };

        TestResponse { status, body }
    }

    /// The same, as the bootstrap admin.
    pub async fn upload(&self, file_name: &str, bytes: &[u8]) -> TestResponse {
        self.upload_as(&self.admin_token, file_name, bytes).await
    }

    // ---- as an arbitrary user -------------------------------------------

    pub async fn get_as(&self, token: &str, path: &str) -> TestResponse {
        self.send(Method::GET, path, Some(token), None).await
    }

    pub async fn post_as(&self, token: &str, path: &str, body: Value) -> TestResponse {
        self.send(Method::POST, path, Some(token), Some(body)).await
    }

    pub async fn put_as(&self, token: &str, path: &str, body: Value) -> TestResponse {
        self.send(Method::PUT, path, Some(token), Some(body)).await
    }

    pub async fn delete_as(&self, token: &str, path: &str) -> TestResponse {
        self.send(Method::DELETE, path, Some(token), None).await
    }

    // ---- as the bootstrap admin -----------------------------------------

    pub async fn get(&self, path: &str) -> TestResponse {
        self.get_as(&self.admin_token, path).await
    }

    pub async fn post(&self, path: &str, body: Value) -> TestResponse {
        self.post_as(&self.admin_token, path, body).await
    }

    pub async fn put(&self, path: &str, body: Value) -> TestResponse {
        self.put_as(&self.admin_token, path, body).await
    }

    pub async fn delete(&self, path: &str) -> TestResponse {
        self.send(Method::DELETE, path, Some(&self.admin_token), None).await
    }

    /// Sends a bare, unauthenticated request purely to see whether anything is
    /// routed at that method and path. Used by the OpenAPI drift test.
    pub async fn probe(&self, method: &str, path: &str) -> TestResponse {
        let method = Method::from_bytes(method.as_bytes()).expect("unknown HTTP method");
        self.send(method, path, None, Some(json!({}))).await
    }

    /// Creates a resource and returns its id, failing loudly if the create was
    /// rejected — most tests only care about the id of their fixtures.
    pub async fn create(&self, path: &str, body: Value) -> String {
        let response = self.post(path, body).await;
        assert!(
            response.status.is_success(),
            "POST {path} failed: {} {}",
            response.status,
            response.body
        );
        response.id()
    }

    // ---- fixtures --------------------------------------------------------
    //
    // Cross-module flows need the same handful of records (a customer to sell
    // to, a product to move, somewhere to keep it), so they live here rather
    // than being retyped in each file.

    pub async fn customer(&self) -> String {
        self.create(
            "/crm/companies",
            json!({ "name": "Globex Corp", "company_type": "customer", "email": "ap@globex.test" }),
        )
        .await
    }

    pub async fn warehouse(&self, code: &str, name: &str) -> String {
        self.create("/inventory/warehouses", json!({ "code": code, "name": name })).await
    }

    pub async fn product(&self, sku: &str, name: &str) -> String {
        self.create(
            "/inventory/products",
            json!({ "sku": sku, "name": name, "cost_price": 4.50, "sale_price": 19.99 }),
        )
        .await
    }

    pub async fn employee(&self, email: &str) -> String {
        self.create(
            "/hr/employees",
            json!({
                "first_name": "Lisa",
                "last_name": "Simpson",
                "email": email,
                "hire_date": "2024-01-15",
                "annual_leave_entitlement": 25
            }),
        )
        .await
    }

    /// Walks a document through a chain of status transitions, asserting each
    /// one is accepted.
    pub async fn advance(&self, path: &str, statuses: &[&str]) {
        for status in statuses {
            let response = self.put(path, json!({ "status": status })).await;
            assert!(
                response.status.is_success(),
                "PUT {path} -> {status} failed: {} {}",
                response.status,
                response.body
            );
        }
    }
}
