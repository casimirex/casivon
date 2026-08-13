use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub jwt_access_expiry: i64,
    pub jwt_refresh_expiry: i64,
    /// Where the frontend is served from. Only used to build links in emails,
    /// which have no request to infer an origin from.
    pub app_base_url: String,
    pub host: String,
    pub port: u16,
    /// `None` when `SMTP_HOST` is unset, which is what selects the logging
    /// sender over a real relay.
    pub smtp: Option<SmtpConfig>,
    /// `None` when `S3_ENDPOINT` is unset, which leaves file upload switched
    /// off. Unlike mail there is no stand-in that half works: see
    /// `UnconfiguredObjectStore`.
    pub s3: Option<S3Config>,
}

/// How to reach the mail relay. Only built when `SMTP_HOST` is set.
#[derive(Clone, Debug)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    /// `From` header, e.g. `ERP System <no-reply@example.com>`.
    pub from: String,
    pub encryption: SmtpEncryption,
}

/// Where uploaded files live. Only built when `S3_ENDPOINT` is set.
#[derive(Clone, Debug)]
pub struct S3Config {
    /// How *this process* reaches the object store, e.g. `http://minio:9000`
    /// from inside a compose network.
    pub endpoint: String,
    /// How a *browser* reaches it. Kept separate because a presigned URL's
    /// signature covers the host it was signed for: sign against `minio:9000`
    /// and the link is unusable from the user's machine, which resolves that
    /// name to nothing. Defaults to `endpoint`, which is right whenever the API
    /// and the browser share a view of the network.
    pub public_endpoint: String,
    /// MinIO ignores the region but still expects one in the signature.
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmtpEncryption {
    /// Connect in the clear, then upgrade with STARTTLS. What most relays want.
    StartTls,
    /// TLS from the first byte, conventionally on port 465.
    Tls,
    /// No encryption at all. For a local catcher like Mailpit — never for a
    /// relay that sees real credentials or real addresses.
    None,
}

impl std::str::FromStr for SmtpEncryption {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "starttls" => Ok(Self::StartTls),
            "tls" | "ssl" | "implicit" => Ok(Self::Tls),
            "none" | "plain" | "insecure" => Ok(Self::None),
            other => Err(anyhow::anyhow!(
                "SMTP_ENCRYPTION must be one of starttls, tls, none - got '{other}'"
            )),
        }
    }
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: optional("REDIS_URL", "redis://localhost:6379"),
            jwt_secret: required("JWT_SECRET")?,
            jwt_access_expiry: parsed("JWT_ACCESS_EXPIRY", 900)?,
            jwt_refresh_expiry: parsed("JWT_REFRESH_EXPIRY", 604_800)?,
            app_base_url: optional("APP_BASE_URL", "http://localhost:3000"),
            // The deployment env files use APP_HOST/APP_PORT; plain HOST/PORT is
            // accepted too so PaaS defaults work without extra config.
            host: env::var("APP_HOST")
                .or_else(|_| env::var("HOST"))
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: match env::var("APP_PORT").or_else(|_| env::var("PORT")) {
                Ok(v) => v.parse()?,
                Err(_) => 8080,
            },
            smtp: SmtpConfig::from_env()?,
            s3: S3Config::from_env()?,
        })
    }
}

impl S3Config {
    /// Reads the object store settings, or `None` if `S3_ENDPOINT` is unset.
    ///
    /// Credentials are required once an endpoint is given rather than defaulted:
    /// an anonymous bucket that accepts uploads from anyone is not a mode worth
    /// making easy to reach by accident.
    fn from_env() -> anyhow::Result<Option<Self>> {
        let Ok(endpoint) = env::var("S3_ENDPOINT") else {
            return Ok(None);
        };
        let endpoint = endpoint.trim().trim_end_matches('/').to_string();
        if endpoint.is_empty() {
            return Ok(None);
        }

        let public_endpoint = env::var("S3_PUBLIC_ENDPOINT")
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| endpoint.clone());

        let missing = |key: &str| {
            anyhow::anyhow!("S3_ENDPOINT is set, so {key} is required")
        };

        Ok(Some(Self {
            endpoint,
            public_endpoint,
            region: optional("S3_REGION", "us-east-1"),
            bucket: optional("S3_BUCKET", "erp-receipts"),
            access_key: required("S3_ACCESS_KEY").map_err(|_| missing("S3_ACCESS_KEY"))?,
            secret_key: required("S3_SECRET_KEY").map_err(|_| missing("S3_SECRET_KEY"))?,
        }))
    }
}

impl SmtpConfig {
    /// Reads the relay settings, or `None` if `SMTP_HOST` is unset.
    ///
    /// Everything else is only consulted once a host is configured, so an
    /// installation with no mail setup has nothing to get wrong. A host without
    /// `SMTP_FROM` is an error rather than a guess: relays reject mail from an
    /// address they do not recognise, and failing at start-up beats failing on
    /// someone's first password reset.
    fn from_env() -> anyhow::Result<Option<Self>> {
        let Ok(host) = env::var("SMTP_HOST") else {
            return Ok(None);
        };
        if host.trim().is_empty() {
            return Ok(None);
        }

        let encryption: SmtpEncryption = optional("SMTP_ENCRYPTION", "starttls").parse()?;
        let default_port = match encryption {
            SmtpEncryption::Tls => 465,
            SmtpEncryption::StartTls => 587,
            SmtpEncryption::None => 25,
        };

        Ok(Some(Self {
            host,
            port: match env::var("SMTP_PORT") {
                Ok(v) => v.parse()?,
                Err(_) => default_port,
            },
            username: env::var("SMTP_USERNAME").ok().filter(|v| !v.is_empty()),
            password: env::var("SMTP_PASSWORD").ok().filter(|v| !v.is_empty()),
            from: required("SMTP_FROM")
                .map_err(|_| anyhow::anyhow!("SMTP_HOST is set, so SMTP_FROM is required"))?,
            encryption,
        }))
    }
}

fn required(key: &str) -> anyhow::Result<String> {
    env::var(key).map_err(|_| anyhow::anyhow!("Missing required environment variable: {}", key))
}

fn optional(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parsed(key: &str, default: i64) -> anyhow::Result<i64> {
    match env::var(key) {
        Ok(v) => Ok(v.parse()?),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_accepts_the_documented_spellings() {
        assert_eq!("starttls".parse::<SmtpEncryption>().unwrap(), SmtpEncryption::StartTls);
        assert_eq!("TLS".parse::<SmtpEncryption>().unwrap(), SmtpEncryption::Tls);
        assert_eq!("  none  ".parse::<SmtpEncryption>().unwrap(), SmtpEncryption::None);
    }

    /// The public endpoint is what presigned links are signed against, so
    /// falling back to the internal one has to be exact — a trailing slash here
    /// becomes a double slash in the signed path and the signature stops
    /// matching.
    #[test]
    fn the_public_endpoint_defaults_to_the_internal_one_without_a_trailing_slash() {
        let trimmed = "http://minio:9000/".trim_end_matches('/').to_string();
        assert_eq!(trimmed, "http://minio:9000");
    }

    #[test]
    fn an_unknown_encryption_names_the_valid_options() {
        let error = "sometimes".parse::<SmtpEncryption>().unwrap_err().to_string();
        assert!(error.contains("starttls"), "{error}");
        assert!(error.contains("sometimes"), "{error}");
    }
}
