use async_trait::async_trait;
use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::{authentication::Credentials, AsyncSmtpTransport},
    AsyncTransport, Message, Tokio1Executor,
};

use crate::config::{SmtpConfig, SmtpEncryption};
use crate::error::{AppError, AppResult};
use crate::shared::email::{EmailMessage, EmailSender};

/// Delivers mail through an SMTP relay.
///
/// The transport is built once and cloned per send — `lettre` keeps a
/// connection pool behind it, so rebuilding per message would mean a fresh TCP
/// and TLS handshake for every password reset.
pub struct SmtpEmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpEmailSender {
    pub fn connect(config: &SmtpConfig) -> anyhow::Result<Self> {
        let from: Mailbox = config.from.parse().map_err(|e| {
            anyhow::anyhow!(
                "SMTP_FROM is not a valid address ({e}). Expected `Name <you@example.com>` \
                 or `you@example.com`."
            )
        })?;

        let mut builder = match config.encryption {
            // `relay` and `starttls_relay` verify the server's certificate;
            // `builder_dangerous` is plaintext and is why "none" is spelled out
            // in the configuration rather than being a silent fallback.
            SmtpEncryption::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)?,
            SmtpEncryption::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)?
            }
            SmtpEncryption::None => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
            }
        }
        .port(config.port);

        // A relay that wants no credentials is normal for a local catcher and
        // for a host that authenticates by IP.
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            builder = builder.credentials(Credentials::new(username.clone(), password.clone()));
        }

        Ok(Self { transport: builder.build(), from })
    }
}

#[async_trait]
impl EmailSender for SmtpEmailSender {
    async fn send(&self, message: EmailMessage) -> AppResult<()> {
        let to: Mailbox = message.to.parse().map_err(|e| {
            tracing::error!("refusing to send to an unparseable address: {e}");
            AppError::Validation(format!("'{}' is not a valid email address", message.to))
        })?;

        let email = Message::builder()
            .from(self.from.clone())
            .to(to)
            // Without this the message goes out with no Content-Type at all,
            // and a client falls back to ASCII — which turns every non-ASCII
            // character, an em-dash or an accented name, into mojibake.
            .header(ContentType::TEXT_PLAIN)
            .subject(&message.subject)
            .body(message.body)
            .map_err(|e| {
                tracing::error!("could not build the message: {e}");
                AppError::Internal
            })?;

        self.transport.send(email).await.map_err(|e| {
            // The caller decides what to tell the user — `forgot_password`
            // deliberately says nothing, so that a delivery failure cannot be
            // used to tell a registered address from an unregistered one.
            tracing::error!("SMTP delivery failed: {e}");
            AppError::Internal
        })?;

        Ok(())
    }
}
