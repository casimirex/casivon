//! Where uploaded files go, and what is allowed through the door.
//!
//! The trait is the seam: the rest of the application knows it can hand over
//! bytes and get back a link, and nothing else. The rules in the bottom half of
//! this file are pure functions, so what counts as an acceptable upload is
//! decided — and tested — without a network or a bucket.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use uuid::Uuid;

use crate::error::AppResult;

/// 10 MB. Large enough for a photographed receipt or a scanned multi-page
/// invoice, small enough that a mistyped upload cannot fill the bucket.
pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// How long a presigned link stays good for.
///
/// Long enough to open the page, look at the receipt and save it; short enough
/// that a link pasted into a chat window stops working before it is useful to
/// anybody who should not have it. The URL is the only thing standing between a
/// holder and the bytes, so this is the whole of its lifetime as a secret.
pub const DOWNLOAD_URL_TTL: Duration = Duration::from_secs(15 * 60);

#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Stores `bytes` under `key`, overwriting anything already there.
    ///
    /// Keys are generated, never taken from a client, so an overwrite would
    /// mean a uuid collision.
    async fn put(&self, key: &str, content_type: &str, bytes: Vec<u8>) -> AppResult<()>;

    /// A time-limited URL a browser can fetch directly.
    ///
    /// `file_name` is the name the download should be saved under — without it
    /// the browser names the file after the storage key, which is a uuid.
    async fn presigned_get(&self, key: &str, file_name: &str, ttl: Duration) -> AppResult<String>;

    async fn delete(&self, key: &str) -> AppResult<()>;
}

// ---------------------------------------------------------------------------
// What may be uploaded
// ---------------------------------------------------------------------------

/// A file type the application accepts, identified by what the bytes actually
/// are rather than by what the upload claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Jpeg,
    Png,
    WebP,
    Pdf,
}

impl FileKind {
    pub fn content_type(self) -> &'static str {
        match self {
            FileKind::Jpeg => "image/jpeg",
            FileKind::Png => "image/png",
            FileKind::WebP => "image/webp",
            FileKind::Pdf => "application/pdf",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            FileKind::Jpeg => "jpg",
            FileKind::Png => "png",
            FileKind::WebP => "webp",
            FileKind::Pdf => "pdf",
        }
    }

    pub fn is_image(self) -> bool {
        !matches!(self, FileKind::Pdf)
    }
}

/// Identifies a file from its leading bytes.
///
/// Deliberately not the `Content-Type` from the multipart part: that is written
/// by the client and a browser will happily send `image/png` for anything the
/// user picked. Since the type we record is later handed back to a browser as
/// the type to render, trusting the client here would let somebody store HTML
/// or a script and have it served under our name.
///
/// Only the four formats a receipt actually arrives in are recognised. Anything
/// else is refused rather than stored as an unknown blob.
pub fn detect_kind(bytes: &[u8]) -> Option<FileKind> {
    // JPEG: SOI marker, then the start of the first segment.
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(FileKind::Jpeg);
    }
    // PNG: the 8-byte signature, which includes a CRLF/LF pair specifically to
    // catch transfers that mangled line endings.
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(FileKind::Png);
    }
    // WebP is a RIFF container; the format lives four bytes after the length.
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(FileKind::WebP);
    }
    if bytes.starts_with(b"%PDF-") {
        return Some(FileKind::Pdf);
    }
    None
}

/// The human-readable list for an error message, so a refusal says what would
/// have worked.
pub const ACCEPTED_TYPES: &str = "JPEG, PNG, WebP or PDF";

/// Builds the object key for a new upload.
///
/// Nothing the client sent appears in it. The original filename is kept in the
/// database instead, which means a name containing `../`, a null byte or a
/// leading slash cannot reach into the bucket. Dating the prefix keeps a
/// long-lived bucket browsable by hand; the uuid makes the key unguessable,
/// which matters because a presigned URL is the read path and its secrecy is
/// the key's secrecy.
pub fn storage_key(now: DateTime<Utc>, kind: FileKind) -> String {
    format!(
        "receipts/{:04}/{:02}/{}.{}",
        now.year(),
        now.month(),
        Uuid::new_v4(),
        kind.extension()
    )
}

/// Makes a client-supplied filename safe to echo back in a header.
///
/// The name is stored and later interpolated into `Content-Disposition`. A
/// quote or a newline there would end the header value early and let the rest
/// be read as another header, so both go. Path separators go too: the name is
/// only ever a label, but it is also what a browser writes to disk.
pub fn sanitize_file_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' => '-',
            '"' | '\'' | '\r' | '\n' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    let trimmed = cleaned.trim().trim_matches('.').trim();

    // A name of dots and slashes leaves nothing behind; give the file something
    // to be called rather than storing an empty string.
    if trimmed.is_empty() {
        return "receipt".to_string();
    }

    // Long enough for any real filename, short enough to keep the header sane.
    trimmed.chars().take(120).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(b"the rest of a png");
        bytes
    }

    #[test]
    fn the_four_accepted_formats_are_recognised() {
        assert_eq!(detect_kind(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(FileKind::Jpeg));
        assert_eq!(detect_kind(&png()), Some(FileKind::Png));
        assert_eq!(detect_kind(b"RIFF\x24\x00\x00\x00WEBPVP8 "), Some(FileKind::WebP));
        assert_eq!(detect_kind(b"%PDF-1.7\n"), Some(FileKind::Pdf));
    }

    #[test]
    fn anything_else_is_refused() {
        assert_eq!(detect_kind(b"<html><script>alert(1)</script>"), None);
        assert_eq!(detect_kind(b"GIF89a"), None);
        assert_eq!(detect_kind(b""), None);
        // A RIFF container that is not WebP - a WAV file, say.
        assert_eq!(detect_kind(b"RIFF\x24\x00\x00\x00WAVEfmt "), None);
    }

    /// The point of sniffing: an extension and a `Content-Type` both saying
    /// "png" count for nothing against the bytes.
    #[test]
    fn a_renamed_file_is_judged_on_its_bytes() {
        assert_eq!(detect_kind(b"just some text, saved as receipt.png"), None);
    }

    #[test]
    fn a_truncated_webp_header_does_not_panic() {
        assert_eq!(detect_kind(b"RIFF\x24\x00\x00"), None);
    }

    #[test]
    fn keys_are_dated_extensioned_and_unique() {
        let now = DateTime::parse_from_rfc3339("2026-03-09T12:00:00Z").unwrap().with_timezone(&Utc);
        let first = storage_key(now, FileKind::Pdf);
        let second = storage_key(now, FileKind::Pdf);

        assert!(first.starts_with("receipts/2026/03/"), "{first}");
        assert!(first.ends_with(".pdf"), "{first}");
        assert_ne!(first, second);
    }

    #[test]
    fn a_key_never_contains_anything_the_client_sent() {
        let now = Utc::now();
        let key = storage_key(now, FileKind::Jpeg);
        assert!(!key.contains(".."), "{key}");
        assert_eq!(key.matches('/').count(), 3, "{key}");
    }

    #[test]
    fn file_names_cannot_break_out_of_a_header_or_a_directory() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), "-..-etc-passwd");
        assert_eq!(sanitize_file_name("in\"voice.pdf"), "in_voice.pdf");
        assert_eq!(sanitize_file_name("bill\r\nX-Evil: yes"), "bill__X-Evil: yes");
    }

    #[test]
    fn a_name_with_nothing_usable_left_gets_a_default() {
        assert_eq!(sanitize_file_name("   "), "receipt");
        assert_eq!(sanitize_file_name("..."), "receipt");
        assert_eq!(sanitize_file_name(""), "receipt");
    }

    #[test]
    fn an_ordinary_name_is_left_alone() {
        assert_eq!(sanitize_file_name("Taxi 2026-03-02.jpg"), "Taxi 2026-03-02.jpg");
    }

    #[test]
    fn a_very_long_name_is_cut_rather_than_refused() {
        let long = "a".repeat(500);
        assert_eq!(sanitize_file_name(&long).chars().count(), 120);
    }
}
