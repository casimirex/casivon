use async_trait::async_trait;

use crate::error::AppResult;
use crate::shared::email::{EmailMessage, EmailSender};

/// Writes mail to the log instead of sending it.
///
/// This is the only sender wired up today: there is no SMTP configuration in
/// the environment, and a delivery integration nobody can point at a server is
/// worse than none — it fails at runtime rather than at start-up. In
/// development this is what you want anyway, since the reset link lands in the
/// console where you are already looking.
///
/// Replacing it means implementing [`EmailSender`] over an SMTP or API client
/// and swapping the one construction in `app.rs`; nothing else refers to it.
pub struct LoggingEmailSender;

#[async_trait]
impl EmailSender for LoggingEmailSender {
    async fn send(&self, message: EmailMessage) -> AppResult<()> {
        tracing::info!(
            to = %message.to,
            subject = %message.subject,
            "email (not delivered — no mail transport configured)\n{}",
            message.body
        );
        Ok(())
    }
}
