use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

/// A file in the object store, and everything about it that the bucket cannot
/// tell you.
///
/// `storage_key` never leaves the server. What a client gets is a presigned URL
/// built from it, which expires; handing out the key itself would be handing out
/// a permanent one.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Attachment {
    pub id: Uuid,
    #[serde(skip)]
    pub storage_key: String,
    /// The name the file arrived under, sanitised. Shown to the user and used to
    /// name the download.
    pub file_name: String,
    /// What the server determined from the leading bytes — never what the
    /// upload claimed it was.
    pub content_type: String,
    pub byte_size: i64,
    pub uploaded_by: Uuid,
    pub created_at: DateTime<Utc>,
}
