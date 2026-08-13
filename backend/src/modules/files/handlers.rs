use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::Extension;
use chrono::Utc;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::infrastructure::state::AppState;
use crate::modules::files::application::dto::{AttachmentLink, AttachmentSummary};
use crate::modules::files::domain::entities::Attachment;
use crate::modules::files::domain::repositories::AttachmentRepository;
use crate::modules::files::infrastructure::attachment_repo::PgAttachmentRepository;
use crate::modules::hr::application::use_cases::HrScope;
use crate::modules::hr::infrastructure::repositories::employee_repo::PgEmployeeRepository;
use crate::shared::auth::CurrentUser;
use crate::shared::response::{ApiResponse, Created, ErrorResponse};
use crate::shared::storage::{
    detect_kind, sanitize_file_name, storage_key, ACCEPTED_TYPES, DOWNLOAD_URL_TTL,
    MAX_UPLOAD_BYTES,
};

/// Stores an uploaded file and returns its id.
///
/// Open to any signed-in user, because any of them may file an expense claim.
/// Ownership is enforced where it means something — attaching the file to a
/// claim, and reading it back — rather than here, where refusing would only
/// stop people uploading their own receipts.
#[utoipa::path(
    post, path = "/api/v1/files", tag = "Files",
    request_body(content = String, description = "multipart/form-data with a single `file` part", content_type = "multipart/form-data"),
    responses(
        (status = 201, body = ApiResponse<AttachmentSummary>),
        (status = 413, description = "The file is over the size limit", body = ErrorResponse),
        (status = 422, description = "Not one of the accepted file types, or no file part", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearer" = [])),
)]
pub async fn upload(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    mut multipart: Multipart,
) -> AppResult<Created<AttachmentSummary>> {
    let mut found = None;

    // Walks the parts rather than taking the first: a browser form may send
    // other fields alongside, and picking whatever arrived first would make the
    // result depend on field order in the HTML.
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() != Some("file") {
            continue;
        }

        // Read before the borrow of `field` ends - `bytes()` consumes it.
        let claimed_name = field.file_name().map(sanitize_file_name);
        let bytes = field.bytes().await.map_err(multipart_error)?;
        found = Some((claimed_name, bytes));
        break;
    }

    let Some((claimed_name, bytes)) = found else {
        return Err(AppError::Validation(
            "No file was uploaded. Send the file as a multipart part named `file`.".to_string(),
        ));
    };

    // Checked here as well as by the body limit on the route, so that a file
    // just over the line gets a sentence explaining itself rather than the
    // layer's bare rejection.
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::TooLarge(format!(
            "That file is {:.1} MB. The limit is {} MB.",
            bytes.len() as f64 / (1024.0 * 1024.0),
            MAX_UPLOAD_BYTES / (1024 * 1024)
        )));
    }

    if bytes.is_empty() {
        return Err(AppError::Validation("That file is empty.".to_string()));
    }

    // The type comes from the bytes, never from the part's `Content-Type`.
    let Some(kind) = detect_kind(&bytes) else {
        return Err(AppError::Validation(format!(
            "That file does not look like {ACCEPTED_TYPES}. Check you picked the right one — \
             renaming a file does not change what is inside it."
        )));
    };

    let now = Utc::now();
    let key = storage_key(now, kind);
    let byte_size = bytes.len() as i64;

    // Bytes first, row second. The other order would leave a database row
    // pointing at an object that does not exist if the upload failed, and a
    // download would 500 on a receipt that looks present in the UI. This order
    // fails the other way: an orphaned object nobody references, which costs
    // storage and nothing else.
    state.files.put(&key, kind.content_type(), bytes.to_vec()).await?;

    let attachment = Attachment {
        id: Uuid::new_v4(),
        storage_key: key,
        // A part with no filename is legal; give the file something to be
        // called rather than storing an empty string.
        file_name: claimed_name.unwrap_or_else(|| format!("receipt.{}", kind.extension())),
        content_type: kind.content_type().to_string(),
        byte_size,
        uploaded_by: user.id,
        created_at: now,
    };

    PgAttachmentRepository::new(state.db.clone()).create(&attachment).await?;

    Ok(Created(attachment.into()))
}

/// A time-limited link to the file, for whoever is allowed to see it.
///
/// The rule is inherited rather than invented: a receipt is readable by exactly
/// whoever may read the expense claim it is attached to. So this resolves the
/// claim behind the file and asks [`HrScope`] the same question the HR endpoints
/// ask.
#[utoipa::path(
    get, path = "/api/v1/files/{id}", tag = "Files",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = ApiResponse<AttachmentLink>),
        (status = 404, description = "No such file, or not yours to read", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearer" = [])),
)]
pub async fn download(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> AppResult<ApiResponse<AttachmentLink>> {
    let attachments = PgAttachmentRepository::new(state.db.clone());

    // One message for "no such file" and for "not yours", because a 403 here
    // would confirm the id is real to anybody willing to guess — the same
    // reasoning as the HR endpoints, which answer 404 for a colleague's record.
    let missing = || AppError::NotFound("File not found".to_string());

    let attachment = attachments.find_by_id(id).await?.ok_or_else(missing)?;

    let allowed = match attachments.owning_employee(id).await? {
        // Attached to a claim: whoever may read the claim may read the receipt.
        Some(employee_id) => {
            let employees = PgEmployeeRepository::new(state.db.clone());
            HrScope::resolve(&employees, &user).await?.allows(employee_id)
        }
        // Not attached to anything yet — the window between choosing a file and
        // saving the form. Only the person who uploaded it has any claim on it.
        None => attachment.uploaded_by == user.id,
    };

    if !allowed {
        return Err(missing());
    }

    let url = state
        .files
        .presigned_get(&attachment.storage_key, &attachment.file_name, DOWNLOAD_URL_TTL)
        .await?;

    Ok(ApiResponse::new(AttachmentLink {
        url,
        is_image: attachment.content_type.starts_with("image/"),
        file_name: attachment.file_name,
        content_type: attachment.content_type,
        byte_size: attachment.byte_size,
        expires_at: Utc::now()
            + chrono::Duration::from_std(DOWNLOAD_URL_TTL).unwrap_or(chrono::Duration::zero()),
    }))
}

/// Turns a multipart failure into the right status.
///
/// Worth the detour for one case: when the body limit on the route trips, axum
/// reports it here as a 413. Passing that through keeps a too-large upload
/// answering 413 in our own envelope, instead of a bare framework rejection the
/// frontend cannot read a message out of.
fn multipart_error(error: axum::extract::multipart::MultipartError) -> AppError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return AppError::TooLarge(format!(
            "That upload is over the {} MB limit.",
            MAX_UPLOAD_BYTES / (1024 * 1024)
        ));
    }

    AppError::Validation(format!("The upload could not be read: {error}"))
}
