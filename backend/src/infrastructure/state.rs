use std::sync::Arc;

use sqlx::PgPool;

use crate::config::AppConfig;
use crate::modules::auth::domain::repositories::RevokedTokenStore;
use crate::shared::currency::CurrencyResolver;
use crate::shared::email::EmailSender;
use crate::shared::posting::DocumentPoster;
use crate::shared::dispatch::StockDispatcher;
use crate::shared::storage::ObjectStore;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: AppConfig,
    /// Signed-out refresh tokens. Behind a trait object so the tests can run
    /// without Redis, and so a deployment could swap the backing store.
    pub revoked_tokens: Arc<dyn RevokedTokenStore>,
    /// Outbound mail. Behind a trait object so a deployment can swap in a real
    /// transport, and so tests can read what would have been sent.
    pub email: Arc<dyn EmailSender>,
    /// Settles what currency a document is raised in and at what rate. Read per
    /// use, since an admin can change the base currency or add rates under
    /// Settings and the next document must pick those up.
    pub fx: Arc<dyn CurrencyResolver>,
    /// Where a sales document reaches the general ledger. Behind a trait object
    /// so sales depends on the event, not on how the books are kept.
    pub poster: Arc<dyn DocumentPoster>,
    /// Uploaded files. Behind a trait object so the tests need no bucket, and
    /// so a deployment can point at MinIO, S3 or anything else that speaks the
    /// same API.
    pub files: Arc<dyn ObjectStore>,
    /// Where issuing an invoice reaches the shelves. Behind a trait object so
    /// sales depends on the event, not on what a warehouse is.
    pub dispatch: Arc<dyn StockDispatcher>,
}

impl AppState {
    pub fn new(
        db: PgPool,
        config: AppConfig,
        revoked_tokens: Arc<dyn RevokedTokenStore>,
        email: Arc<dyn EmailSender>,
        fx: Arc<dyn CurrencyResolver>,
        poster: Arc<dyn DocumentPoster>,
        files: Arc<dyn ObjectStore>,
        dispatch: Arc<dyn StockDispatcher>,
    ) -> Self {
        Self { db, config, revoked_tokens, email, fx, poster, files, dispatch }
    }
}
