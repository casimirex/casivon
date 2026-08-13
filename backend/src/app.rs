use std::sync::Arc;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::config::AppConfig;
use crate::error::AppError;
use crate::infrastructure::email::LoggingEmailSender;
use crate::infrastructure::s3_storage::{S3ObjectStore, UnconfiguredObjectStore};
use crate::infrastructure::smtp_email::SmtpEmailSender;
use crate::infrastructure::state::AppState;
use crate::modules;
use crate::modules::auth::domain::repositories::RevokedTokenStore;
use crate::modules::auth::infrastructure::redis_revoked_tokens::RedisRevokedTokens;
use crate::modules::accounting::infrastructure::document_poster::PgDocumentPoster;
use crate::modules::inventory::infrastructure::stock_dispatcher::PgStockDispatcher;
use crate::modules::settings::infrastructure::currency_resolver::PgCurrencyResolver;
use crate::shared::auth::auth_middleware;
use crate::shared::currency::CurrencyResolver;
use crate::shared::email::EmailSender;
use crate::shared::storage::ObjectStore;
use crate::openapi::ApiDoc;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Connects to the database, applies migrations, then builds the router.
pub async fn create_app(config: AppConfig) -> anyhow::Result<Router> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await?;

    let revoked_tokens = Arc::new(RedisRevokedTokens::connect(&config.redis_url)?);
    let email = build_email_sender(&config)?;
    let files = build_object_store(&config).await?;
    let fx = Arc::new(PgCurrencyResolver::new(pool.clone()));

    Ok(build_router(pool, config, revoked_tokens, email, fx, files))
}

/// Builds the router over an existing pool.
///
/// Split out from [`create_app`] so integration tests can mount the real
/// application — every route, middleware and use case — onto the throwaway
/// database `#[sqlx::test]` hands them, without a listening socket.
pub fn build_router(
    pool: sqlx::PgPool,
    config: AppConfig,
    revoked_tokens: Arc<dyn RevokedTokenStore>,
    email: Arc<dyn EmailSender>,
    fx: Arc<dyn CurrencyResolver>,
    files: Arc<dyn ObjectStore>,
) -> Router {
    // Not a parameter like the others: whether this posts anything is decided by
    // the account mapping in the database, not by which implementation is
    // injected. Tests get the real poster and control it the way an operator
    // would — by mapping the accounts, or leaving them unmapped.
    let poster = Arc::new(PgDocumentPoster::new(pool.clone()));
    // Not a parameter either, and for the same reason: whether invoicing moves
    // stock is decided by the warehouse chosen in the database, not by which
    // implementation is injected. Tests get the real dispatcher and control it
    // the way an operator would — by choosing a warehouse, or leaving it unset.
    let dispatch = Arc::new(PgStockDispatcher::new(pool.clone(), poster.clone()));
    let state =
        AppState::new(pool, config.clone(), revoked_tokens, email, fx, poster, files, dispatch);

    let public_routes = Router::new()
        .route("/api/v1/auth/register", post(modules::auth::handlers::register))
        .route("/api/v1/auth/login", post(modules::auth::handlers::login))
        .route("/api/v1/auth/refresh", post(modules::auth::handlers::refresh_token))
        // Signing out takes the refresh token rather than the access token, so
        // it still works once the access token has expired.
        .route("/api/v1/auth/logout", post(modules::auth::handlers::logout))
        // Reset is necessarily public: someone who cannot sign in is exactly
        // who needs it.
        .route("/api/v1/auth/forgot-password", post(modules::auth::handlers::forgot_password))
        .route("/api/v1/auth/reset-password", post(modules::auth::handlers::reset_password))
        // Opened from a mail client, where nobody is signed in.
        .route("/api/v1/auth/verify-email", post(modules::auth::handlers::verify_email))
        .route(
            "/api/v1/auth/resend-verification",
            post(modules::auth::handlers::resend_verification),
        )
        .route("/api/v1/health", get(health_check));

    let protected_routes = Router::new()
        .nest("/api/v1/users", modules::auth::routes::user_routes())
        .nest("/api/v1/crm", modules::crm::routes::crm_routes())
        .nest("/api/v1/sales", modules::sales::routes::sales_routes())
        .nest("/api/v1/inventory", modules::inventory::routes::inventory_routes())
        .nest("/api/v1/purchasing", modules::purchasing::routes::purchasing_routes())
        .nest("/api/v1/accounting", modules::accounting::routes::accounting_routes())
        .nest("/api/v1/hr", modules::hr::routes::hr_routes())
        .nest("/api/v1/projects", modules::projects::routes::project_routes())
        .nest("/api/v1/settings", modules::settings::routes::settings_routes())
        .nest("/api/v1/search", modules::search::routes::search_routes())
        .nest("/api/v1/files", modules::files::routes::file_routes())
        .layer(middleware::from_fn_with_state(config.clone(), auth_middleware));

    Router::new()
        // Docs are public: an API reference nobody can read without a token is
        // not much of a reference.
        .merge(SwaggerUi::new("/api/docs").url("/api/v1/openapi.json", ApiDoc::openapi()))
        .merge(public_routes)
        .merge(protected_routes)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// An SMTP relay when one is configured, otherwise the log.
///
/// Chosen at start-up so a misconfigured relay fails while someone is watching,
/// rather than on the first password reset. The logging sender is not a silent
/// fallback for a broken relay — it is what you get when no relay is configured
/// at all, and it says so in every line it writes.
fn build_email_sender(config: &AppConfig) -> anyhow::Result<Arc<dyn EmailSender>> {
    match &config.smtp {
        Some(smtp) => {
            tracing::info!(
                host = %smtp.host,
                port = smtp.port,
                encryption = ?smtp.encryption,
                "sending mail through SMTP"
            );
            Ok(Arc::new(SmtpEmailSender::connect(smtp)?))
        }
        None => {
            tracing::warn!(
                "SMTP_HOST is not set — mail will be written to the log instead of delivered. \
                 Password reset links will appear here rather than in anyone's inbox."
            );
            Ok(Arc::new(LoggingEmailSender))
        }
    }
}

/// Object storage when an endpoint is configured, otherwise a store that
/// refuses.
///
/// Connecting here rather than lazily is deliberate, and matches the mail relay
/// above: a wrong endpoint or a rejected key fails now, in front of whoever
/// deployed it, instead of on somebody's first receipt.
async fn build_object_store(config: &AppConfig) -> anyhow::Result<Arc<dyn ObjectStore>> {
    match &config.s3 {
        Some(s3) => Ok(Arc::new(S3ObjectStore::connect(s3).await?)),
        None => {
            tracing::warn!(
                "S3_ENDPOINT is not set — file upload is switched off. Attaching a receipt to \
                 an expense claim will be refused with a message saying so."
            );
            Ok(Arc::new(UnconfiguredObjectStore))
        }
    }
}

async fn health_check() -> Result<&'static str, AppError> {
    Ok("OK")
}
