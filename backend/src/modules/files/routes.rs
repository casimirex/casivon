use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};

use crate::infrastructure::state::AppState;
use crate::modules::files::handlers;
use crate::shared::storage::MAX_UPLOAD_BYTES;

/// Multipart framing, part headers and the boundary all sit outside the file
/// itself, so the transport limit has to be a little above the file limit or a
/// file of exactly the maximum size would be rejected by the layer before the
/// handler could say anything useful about it.
const MULTIPART_OVERHEAD: usize = 64 * 1024;

pub fn file_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            // axum's default body limit is 2 MB, which every other endpoint here
            // is comfortably inside and which no upload would be. Raised for
            // this route alone rather than globally: the reason to allow ten
            // megabytes of receipt is not a reason to accept ten megabytes of
            // JSON anywhere else.
            post(handlers::upload)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES + MULTIPART_OVERHEAD)),
        )
        .route("/:id", get(handlers::download))
}
