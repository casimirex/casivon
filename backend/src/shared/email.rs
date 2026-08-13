use async_trait::async_trait;

use crate::error::AppResult;

/// One outbound message. Plain text only — every mail this application sends is
/// a short transactional notice, and HTML would buy nothing but a second body
/// to keep in step.
#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, message: EmailMessage) -> AppResult<()>;
}
