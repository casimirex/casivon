use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppResult;
use crate::modules::files::domain::entities::Attachment;
use crate::modules::files::domain::repositories::AttachmentRepository;

pub struct PgAttachmentRepository {
    pool: PgPool,
}

impl PgAttachmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AttachmentRepository for PgAttachmentRepository {
    async fn create(&self, attachment: &Attachment) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO attachments
                 (id, storage_key, file_name, content_type, byte_size, uploaded_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(attachment.id)
        .bind(&attachment.storage_key)
        .bind(&attachment.file_name)
        .bind(&attachment.content_type)
        .bind(attachment.byte_size)
        .bind(attachment.uploaded_by)
        .bind(attachment.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> AppResult<Option<Attachment>> {
        let attachment = sqlx::query_as::<_, Attachment>(
            "SELECT id, storage_key, file_name, content_type, byte_size, uploaded_by, created_at
             FROM attachments WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(attachment)
    }

    async fn owning_employee(&self, attachment_id: Uuid) -> AppResult<Option<Uuid>> {
        let employee_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT r.employee_id
             FROM expense_lines l
             JOIN expense_reports r ON r.id = l.expense_report_id
             WHERE l.receipt_attachment_id = $1
             LIMIT 1",
        )
        .bind(attachment_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(employee_id)
    }

    async fn ids_not_uploaded_by(&self, ids: &[Uuid], user_id: Uuid) -> AppResult<Vec<Uuid>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Asks the database for the ids that *are* the caller's rather than the
        // ones that are not, so that an id which does not exist at all falls out
        // of the difference below and is refused on the same footing as one
        // belonging to somebody else.
        let mine: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM attachments WHERE id = ANY($1) AND uploaded_by = $2",
        )
        .bind(ids)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(ids.iter().copied().filter(|id| !mine.contains(id)).collect())
    }
}
