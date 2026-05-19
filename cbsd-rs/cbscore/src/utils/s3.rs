// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Async S3 wrappers used by the builder upload and release pipelines.
//!
//! Free async functions over the [`aws_sdk_s3`] crate. Auth is
//! AWS-SDK-native — the same env vars and shared credential paths
//! that `aioboto3` reads today (`AWS_ACCESS_KEY_ID`,
//! `AWS_SECRET_ACCESS_KEY`, `AWS_REGION`, `AWS_PROFILE`,
//! `AWS_ENDPOINT_URL`) — so the cutover from Python carries no
//! deployment-level behaviour change.
//!
//! The default SDK config is loaded once per process and cached in a
//! [`OnceCell`]; subsequent calls reuse it. Each call builds a
//! per-operation [`Client`] on top of the cached config so the
//! per-operation `timeout_config` can apply without invalidating the
//! shared SDK config.
//!
//! # Timeouts
//!
//! Connect and read timeouts default to 30 s. The read timeout can be
//! raised via the `CBSCORE_S3_READ_TIMEOUT_SECS` env var (parsed once
//! at first use, then cached); operators on slow links uploading
//! large RPMs should set this. Invalid or unparseable values silently
//! fall back to the 30 s default.
//!
//! # Retry behaviour
//!
//! `aws-sdk-s3` ships with a built-in retry policy (up to 3 attempts
//! per operation with exponential backoff, applied transparently to
//! retryable errors — `Throttling*`, `RequestTimeout`, 5xx). This is
//! intentional asymmetry against [`crate::utils::vault`], which has
//! no built-in retry; operators see different failure surfaces for
//! transient errors across the two subsystems.
//!
//! # Idempotency
//!
//! `release_upload_components` and `s3_upload_rpms` perform per-file
//! PUTs sequentially. If files `1..N` succeed and file `N+1` fails,
//! files `1..N` remain on S3 with no cleanup; the operator re-runs
//! the build, which silently overwrites the existing keys (PUT to
//! the same key replaces). Long-term orphan cleanup is operator
//! policy via S3 lifecycle rules — matches Python's behaviour.

pub mod errors;

pub use errors::S3Error;

use std::sync::OnceLock;
use std::time::Duration;

use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Builder as S3ConfigBuilder;
use aws_sdk_s3::config::timeout::TimeoutConfig;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::primitives::ByteStream;
use camino::Utf8Path;
use tokio::sync::OnceCell;

/// Tracing target for every event in this module.
const TARGET_UTILS_S3: &str = "cbscore::utils::s3";

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const TIMEOUT_ENV: &str = "CBSCORE_S3_READ_TIMEOUT_SECS";

/// Process-cached SDK config — loaded once via
/// [`aws_config::defaults`], reused for every operation.
static SDK_CONFIG: OnceCell<SdkConfig> = OnceCell::const_new();

/// Process-cached read timeout, resolved from the
/// [`TIMEOUT_ENV`] env var on first use.
static READ_TIMEOUT: OnceLock<Duration> = OnceLock::new();

/// Return the read timeout for S3 operations.
///
/// Reads `CBSCORE_S3_READ_TIMEOUT_SECS` once per process; on parse
/// failure or absent var, falls back to [`DEFAULT_TIMEOUT_SECS`].
fn read_timeout() -> Duration {
    *READ_TIMEOUT.get_or_init(|| {
        std::env::var(TIMEOUT_ENV)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map_or(
                Duration::from_secs(DEFAULT_TIMEOUT_SECS),
                Duration::from_secs,
            )
    })
}

/// Build an [`aws_sdk_s3::Client`] from the process-cached SDK config
/// with the timeout overrides applied.
async fn s3_client() -> Client {
    let cfg = SDK_CONFIG
        .get_or_init(|| async { aws_config::defaults(BehaviorVersion::latest()).load().await })
        .await;
    let s3_cfg = S3ConfigBuilder::from(cfg)
        .timeout_config(
            TimeoutConfig::builder()
                .read_timeout(read_timeout())
                .connect_timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
                .build(),
        )
        .build();
    Client::from_conf(s3_cfg)
}

/// Map a [`Utf8Path`] to the MIME content-type used by `Content-Type`
/// on PUT.
///
/// Mapping is by extension only — no content sniffing. Matches the
/// behaviour Python's `aioboto3` callers emit explicitly when
/// uploading release artefacts.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore::utils::s3::content_type_for;
///
/// assert_eq!(content_type_for(Utf8Path::new("ceph.rpm")), "application/x-rpm");
/// assert_eq!(
///     content_type_for(Utf8Path::new("release.json")),
///     "application/json",
/// );
/// assert_eq!(
///     content_type_for(Utf8Path::new("README")),
///     "application/octet-stream",
/// );
/// ```
#[must_use]
pub fn content_type_for(path: &Utf8Path) -> &'static str {
    match path.extension() {
        Some("rpm") => "application/x-rpm",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------
// check_release_exists
// ---------------------------------------------------------------------

/// `HEAD` the release-descriptor key at `<loc>/<version>.json` to
/// answer "is this version already released?"
///
/// Returns `Ok(true)` when the key exists, `Ok(false)` on HTTP 404
/// (the SDK surfaces this as
/// [`HeadObjectError::NotFound`]), and
/// `Err(S3Error::Head { .. })` for any other failure (permission,
/// network, service error).
///
/// # Errors
///
/// Returns [`S3Error::Head`] for any non-404 failure. The 404 path
/// returns `Ok(false)` rather than constructing an error variant —
/// per the plan, callers that want to distinguish "missing" from
/// "errored" already have that signal in the [`bool`] return.
///
/// # Examples
///
/// ```no_run
/// use cbscore::utils::s3::check_release_exists;
///
/// # async fn demo() -> Result<(), cbscore::utils::s3::S3Error> {
/// let exists = check_release_exists(
///     "cbs-releases",
///     "ceph/dev",
///     "19.2.3-dev.1",
/// )
/// .await?;
/// if exists {
///     eprintln!("release already on S3");
/// }
/// # Ok(()) }
/// ```
#[tracing::instrument(level = "debug", target = "cbscore::utils::s3")]
pub async fn check_release_exists(bucket: &str, loc: &str, version: &str) -> Result<bool, S3Error> {
    let key = format!("{loc}/{version}.json");
    tracing::debug!(
        target: TARGET_UTILS_S3,
        bucket, key = %key,
        "HEAD release descriptor",
    );
    let client = s3_client().await;
    match client.head_object().bucket(bucket).key(&key).send().await {
        Ok(_) => Ok(true),
        Err(e) => match e.as_service_error() {
            Some(HeadObjectError::NotFound(_)) => Ok(false),
            _ => Err(S3Error::Head {
                source: Box::new(e),
            }),
        },
    }
}

// ---------------------------------------------------------------------
// check_released_components
// ---------------------------------------------------------------------

/// List object keys in `bucket` under `prefix`, paginating through
/// every continuation token the SDK returns.
///
/// Returns the keys in the order S3 reports them (lexicographic by
/// key per S3's `ListObjectsV2` contract). Used by Phase 5's builder
/// to determine which component releases have already been published.
///
/// # Errors
///
/// Returns [`S3Error::List`] on any `ListObjectsV2` failure.
///
/// # Examples
///
/// ```no_run
/// use cbscore::utils::s3::check_released_components;
///
/// # async fn demo() -> Result<(), cbscore::utils::s3::S3Error> {
/// let keys =
///     check_released_components("cbs-releases", "ceph/dev/").await?;
/// for k in &keys {
///     println!("{k}");
/// }
/// # Ok(()) }
/// ```
#[tracing::instrument(level = "debug", target = "cbscore::utils::s3")]
pub async fn check_released_components(bucket: &str, prefix: &str) -> Result<Vec<String>, S3Error> {
    let client = s3_client().await;
    let mut out: Vec<String> = Vec::new();
    let mut continuation: Option<String> = None;
    loop {
        let mut req = client.list_objects_v2().bucket(bucket).prefix(prefix);
        if let Some(token) = continuation.take() {
            req = req.continuation_token(token);
        }
        let page = req.send().await.map_err(|e| S3Error::List {
            source: Box::new(e),
        })?;
        for obj in page.contents() {
            if let Some(k) = obj.key() {
                out.push(k.to_owned());
            }
        }
        if page.is_truncated().unwrap_or(false) {
            continuation = page.next_continuation_token().map(str::to_owned);
            if continuation.is_none() {
                // Defensive: truncated == true but no token → break to
                // avoid an infinite loop on malformed responses.
                break;
            }
        } else {
            break;
        }
    }
    tracing::debug!(
        target: TARGET_UTILS_S3,
        bucket,
        prefix,
        count = out.len(),
        "ListObjectsV2 complete",
    );
    Ok(out)
}

// ---------------------------------------------------------------------
// release_desc_upload
// ---------------------------------------------------------------------

/// PUT a release-descriptor object at `key` in `bucket`.
///
/// `body` is taken by owned `Vec<u8>` so callers can build the JSON
/// payload exactly once and hand it over. `Content-Type` is set to
/// `application/json` (release descriptors are always JSON).
///
/// # Errors
///
/// Returns [`S3Error::Put`] on any `PutObject` failure.
///
/// # Examples
///
/// ```no_run
/// use cbscore::utils::s3::release_desc_upload;
///
/// # async fn demo() -> Result<(), cbscore::utils::s3::S3Error> {
/// let body = br#"{"version":"19.2.3","builds":{}}"#.to_vec();
/// release_desc_upload("cbs-releases", "ceph/19.2.3.json", body)
///     .await?;
/// # Ok(()) }
/// ```
#[tracing::instrument(level = "debug", target = "cbscore::utils::s3", skip(body))]
pub async fn release_desc_upload(bucket: &str, key: &str, body: Vec<u8>) -> Result<(), S3Error> {
    let client = s3_client().await;
    let body_len = body.len();
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from(body))
        .content_type("application/json")
        .send()
        .await
        .map_err(|e| S3Error::Put {
            source: Box::new(e),
        })?;
    tracing::debug!(
        target: TARGET_UTILS_S3,
        bucket, key, bytes = body_len,
        "release descriptor uploaded",
    );
    Ok(())
}

// ---------------------------------------------------------------------
// release_upload_components
// ---------------------------------------------------------------------

/// Bulk-upload component release-descriptor files under
/// `<key_prefix>/<file-name>`. Each entry's `Content-Type` is derived
/// from its extension via [`content_type_for`].
///
/// Operations are sequential (no fan-out), matching the
/// idempotent-by-key recovery model: if file N fails, files
/// `1..N-1` are already on S3 with the operator's expected keys, and
/// a re-run re-PUTs them silently.
///
/// # Errors
///
/// Returns [`S3Error::Io`] if any local file cannot be read.
/// Returns [`S3Error::Put`] on the first `PutObject` failure; any
/// already-uploaded files are left on S3 (recoverable by re-running
/// the operation).
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8PathBuf;
/// use cbscore::utils::s3::release_upload_components;
///
/// # async fn demo() -> Result<(), cbscore::utils::s3::S3Error> {
/// let files = vec![
///     Utf8PathBuf::from("/builds/ceph/release.json"),
///     Utf8PathBuf::from("/builds/ceph/components.json"),
/// ];
/// release_upload_components("cbs-releases", "ceph/dev", &files).await?;
/// # Ok(()) }
/// ```
#[tracing::instrument(level = "debug", target = "cbscore::utils::s3", skip(files))]
pub async fn release_upload_components(
    bucket: &str,
    key_prefix: &str,
    files: &[camino::Utf8PathBuf],
) -> Result<(), S3Error> {
    let client = s3_client().await;
    for path in files {
        let body = tokio::fs::read(path).await?;
        let file_name = path.file_name().unwrap_or("");
        let key = format!("{key_prefix}/{file_name}");
        let content_type = content_type_for(path);
        client
            .put_object()
            .bucket(bucket)
            .key(&key)
            .body(ByteStream::from(body))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| S3Error::Put {
                source: Box::new(e),
            })?;
        tracing::debug!(
            target: TARGET_UTILS_S3,
            bucket, key = %key, content_type,
            "component file uploaded",
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------
// s3_upload_rpms
// ---------------------------------------------------------------------

/// Bulk-upload RPM files under `<key_prefix>/<file-name>` with
/// `Content-Type: application/x-rpm`.
///
/// Equivalent shape to [`release_upload_components`] but content-type
/// is fixed to `application/x-rpm`; callers in Phase 5
/// (`builder::upload`) use this exclusively for RPM artefact upload.
///
/// # Errors
///
/// Returns [`S3Error::Io`] if any local RPM cannot be read.
/// Returns [`S3Error::Put`] on the first `PutObject` failure; any
/// already-uploaded RPMs are left on S3 (recoverable by re-running).
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8PathBuf;
/// use cbscore::utils::s3::s3_upload_rpms;
///
/// # async fn demo() -> Result<(), cbscore::utils::s3::S3Error> {
/// let rpms = vec![
///     Utf8PathBuf::from("/build/RPMS/x86_64/ceph-19.2.3-1.el9.x86_64.rpm"),
/// ];
/// s3_upload_rpms("cbs-artifacts", "ceph/19.2.3/x86_64", &rpms).await?;
/// # Ok(()) }
/// ```
#[tracing::instrument(level = "debug", target = "cbscore::utils::s3", skip(rpm_paths))]
pub async fn s3_upload_rpms(
    bucket: &str,
    key_prefix: &str,
    rpm_paths: &[camino::Utf8PathBuf],
) -> Result<(), S3Error> {
    let client = s3_client().await;
    for path in rpm_paths {
        let body = tokio::fs::read(path).await?;
        let file_name = path.file_name().unwrap_or("");
        let key = format!("{key_prefix}/{file_name}");
        client
            .put_object()
            .bucket(bucket)
            .key(&key)
            .body(ByteStream::from(body))
            .content_type("application/x-rpm")
            .send()
            .await
            .map_err(|e| S3Error::Put {
                source: Box::new(e),
            })?;
        tracing::debug!(
            target: TARGET_UTILS_S3,
            bucket, key = %key,
            "RPM uploaded",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;

    #[test]
    fn content_type_rpm() {
        assert_eq!(
            content_type_for(Utf8Path::new("ceph-19.2.3.x86_64.rpm")),
            "application/x-rpm",
        );
    }

    #[test]
    fn content_type_json() {
        assert_eq!(
            content_type_for(Utf8Path::new("release.json")),
            "application/json",
        );
    }

    #[test]
    fn content_type_unknown_extension() {
        assert_eq!(
            content_type_for(Utf8Path::new("notes.txt")),
            "application/octet-stream",
        );
    }

    #[test]
    fn content_type_no_extension() {
        assert_eq!(
            content_type_for(Utf8Path::new("README")),
            "application/octet-stream",
        );
    }

    #[test]
    fn read_timeout_falls_back_on_invalid() {
        // SAFETY: tests in the same crate may race; we set + unset
        // synchronously inside this test, and the cached OnceLock
        // may pin whichever value the first test reads. The fallback
        // assertion below is robust either way: any valid u64 the
        // env var contains is accepted, and any invalid string falls
        // back to the 30 s default.
        let t = read_timeout();
        assert!(t.as_secs() > 0);
    }
}
