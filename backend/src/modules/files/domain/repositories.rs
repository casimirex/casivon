use async_trait::async_trait;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::files::domain::entities::Attachment;

#[async_trait]
pub trait AttachmentRepository: Send + Sync {
    async fn create(&self, attachment: &Attachment) -> AppResult<()>;

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Attachment>>;

    /// The employee whose expense claim this file hangs off, if it hangs off
    /// one yet.
    ///
    /// This is the authorization question in one call: a receipt is readable by
    /// exactly whoever may read the claim it belongs to, so reading walks
    /// backwards from the file to the claim rather than trusting whoever holds
    /// the id. `None` means the file has been uploaded but not yet attached to
    /// anything — the window between choosing a file and saving the form.
    async fn owning_employee(&self, attachment_id: Uuid) -> AppResult<Option<Uuid>>;

    /// Of `ids`, those the caller may not attach: someone else's uploads, and
    /// ids that do not exist.
    ///
    /// Both are refused together and identically. Distinguishing them would
    /// answer "does this id exist?" for anybody willing to guess, which is the
    /// question the 404-not-403 rule elsewhere exists to leave unanswered.
    async fn ids_not_uploaded_by(&self, ids: &[Uuid], user_id: Uuid) -> AppResult<Vec<Uuid>>;
}
