//! Object storage over the S3 API — MinIO in development, a managed bucket in
//! production, the same code for both.

use std::time::Duration;

use async_trait::async_trait;
use s3::{creds::Credentials, Bucket, BucketConfiguration, Region};

use crate::config::S3Config;
use crate::error::{AppError, AppResult};
use crate::shared::storage::ObjectStore;

/// Talks to an S3-compatible object store.
///
/// Holds *two* handles on the same bucket. They differ only in the endpoint
/// baked into their region, and that difference is the whole reason both exist:
/// a presigned URL's signature covers the host it was signed for, so a link
/// signed against `http://minio:9000` — the name this process reaches the store
/// by inside a container network — is unusable in a browser, which cannot
/// resolve it. Uploads go through `internal`; links are signed by `public`.
///
/// Where the API and the browser see the same address, both are built from the
/// same endpoint and the distinction costs nothing.
pub struct S3ObjectStore {
    internal: Box<Bucket>,
    public: Box<Bucket>,
}

impl S3ObjectStore {
    /// Builds the client and confirms the bucket is there, creating it if not.
    ///
    /// Done at start-up, and fatal, for the same reason the SMTP transport is
    /// built at start-up: a wrong endpoint or a bad key should be discovered by
    /// whoever just deployed, not by the first person who tries to attach a
    /// receipt to their expenses.
    pub async fn connect(config: &S3Config) -> anyhow::Result<Self> {
        let credentials =
            Credentials::new(Some(&config.access_key), Some(&config.secret_key), None, None, None)
                .map_err(|e| anyhow::anyhow!("S3 credentials were rejected: {e}"))?;

        let bucket_for = |endpoint: &str| -> anyhow::Result<Box<Bucket>> {
            let region = Region::Custom {
                region: config.region.clone(),
                endpoint: endpoint.to_string(),
            };
            // Path style addressing: MinIO serves a bucket as the first path
            // segment (`host/bucket/key`), where AWS uses a subdomain
            // (`bucket.host/key`). The subdomain form needs wildcard DNS, which
            // a container called `minio` does not have.
            Ok(Bucket::new(&config.bucket, region, credentials.clone())?.with_path_style())
        };

        let internal = bucket_for(&config.endpoint)?;
        let public = bucket_for(&config.public_endpoint)?;

        if !internal.exists().await.map_err(|e| {
            anyhow::anyhow!(
                "cannot reach the object store at {} ({e}). Check S3_ENDPOINT, S3_ACCESS_KEY \
                 and S3_SECRET_KEY.",
                config.endpoint
            )
        })? {
            tracing::info!(bucket = %config.bucket, "bucket does not exist yet, creating it");
            // `create_with_path_style`, not `create`: the plain one addresses
            // the bucket as a subdomain, which against MinIO means a request to
            // `erp-receipts.localhost` — a name that resolves to the same host
            // and answers 404. The bucket then silently is not created, and the
            // failure only shows up on the first upload.
            //
            // Private, because every read goes through a presigned URL and
            // nothing in here should be world-readable.
            let created = Bucket::create_with_path_style(
                &config.bucket,
                internal.region.clone(),
                credentials.clone(),
                BucketConfiguration::private(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("could not create bucket '{}': {e}", config.bucket))?;

            // Checked rather than assumed: the client reports a refusal in the
            // response instead of erroring, so an unchecked call would log
            // "creating it" and carry on with no bucket.
            if !created.success() {
                anyhow::bail!(
                    "could not create bucket '{}': the object store answered {} {}",
                    config.bucket,
                    created.response_code,
                    created.response_text
                );
            }
        }

        tracing::info!(
            bucket = %config.bucket,
            endpoint = %config.endpoint,
            public_endpoint = %config.public_endpoint,
            "file uploads are stored in object storage"
        );

        Ok(Self { internal, public })
    }
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn put(&self, key: &str, content_type: &str, bytes: Vec<u8>) -> AppResult<()> {
        let response = self
            .internal
            .put_object_with_content_type(key, &bytes, content_type)
            .await
            .map_err(|e| {
                tracing::error!("uploading {key} failed: {e}");
                AppError::Internal
            })?;

        // The client returns the response rather than erroring on a 4xx, so a
        // rejected upload would otherwise be recorded in the database as a
        // stored file and only discovered when somebody tried to read it.
        if !(200..300).contains(&response.status_code()) {
            tracing::error!(
                "object store refused {key} with {}: {}",
                response.status_code(),
                response.as_str().unwrap_or("<unreadable body>")
            );
            return Err(AppError::Internal);
        }

        Ok(())
    }

    async fn presigned_get(&self, key: &str, file_name: &str, ttl: Duration) -> AppResult<String> {
        // Names the download, so a PDF saves as "Taxi receipt.pdf" rather than
        // as the uuid the key is made of. The name is sanitised on the way in
        // (`sanitize_file_name`), which is what keeps the quotes here honest.
        let disposition = format!("inline; filename=\"{file_name}\"");
        let queries = std::collections::HashMap::from([
            ("response-content-disposition".to_string(), disposition),
        ]);

        self.public
            .presign_get(key, ttl.as_secs() as u32, Some(queries))
            .await
            .map_err(|e| {
                tracing::error!("could not sign a download link for {key}: {e}");
                AppError::Internal
            })
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        self.internal.delete_object(key).await.map_err(|e| {
            tracing::error!("deleting {key} failed: {e}");
            AppError::Internal
        })?;
        Ok(())
    }
}

/// What you get when `S3_ENDPOINT` is unset: an object store that refuses.
///
/// Note that this is *not* the same shape as `LoggingEmailSender`, which stands
/// in for a missing relay by writing the message to the log. That works because
/// a logged email is still a readable email — nothing is lost, it just arrives
/// somewhere unusual. There is no equivalent for a file: a receipt accepted and
/// then dropped reports success and is gone, and the user finds out months later
/// when an auditor asks for it. Refusing at the door is the honest failure.
///
/// The rest of the application is unaffected — only uploads and downloads say
/// no, and they say why.
pub struct UnconfiguredObjectStore;

impl UnconfiguredObjectStore {
    fn refuse<T>() -> AppResult<T> {
        Err(AppError::Validation(
            "File storage is not configured on this server, so files cannot be uploaded or \
             read. Set S3_ENDPOINT, S3_ACCESS_KEY and S3_SECRET_KEY to enable it."
                .to_string(),
        ))
    }
}

#[async_trait]
impl ObjectStore for UnconfiguredObjectStore {
    async fn put(&self, _key: &str, _content_type: &str, _bytes: Vec<u8>) -> AppResult<()> {
        Self::refuse()
    }

    async fn presigned_get(
        &self,
        _key: &str,
        _file_name: &str,
        _ttl: Duration,
    ) -> AppResult<String> {
        Self::refuse()
    }

    async fn delete(&self, _key: &str) -> AppResult<()> {
        Self::refuse()
    }
}
