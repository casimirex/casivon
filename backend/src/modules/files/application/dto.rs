use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::modules::files::domain::entities::Attachment;

/// What an upload returns: enough to show the file in the form and to send the
/// id back when the form is saved.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttachmentSummary {
    pub id: Uuid,
    pub file_name: String,
    pub content_type: String,
    pub byte_size: i64,
    pub created_at: DateTime<Utc>,
}

impl From<Attachment> for AttachmentSummary {
    fn from(attachment: Attachment) -> Self {
        Self {
            id: attachment.id,
            file_name: attachment.file_name,
            content_type: attachment.content_type,
            byte_size: attachment.byte_size,
            created_at: attachment.created_at,
        }
    }
}

/// A time-limited way to actually fetch the bytes.
///
/// The API deliberately does not stream the file itself. The browser loads this
/// URL straight from the object store, which keeps receipt traffic out of the
/// API — and, more practically, means an `<img src>` works: the session here is
/// a bearer token in a header, and an image tag cannot send one.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttachmentLink {
    /// Presigned, and only good until `expires_at`. Treat it as a secret: it is
    /// the whole of the authorization on the object store side.
    pub url: String,
    pub file_name: String,
    pub content_type: String,
    pub byte_size: i64,
    pub expires_at: DateTime<Utc>,
    /// Whether the client can render this inline. Saves every caller having to
    /// keep its own list of image types in step with the server's.
    pub is_image: bool,
}
