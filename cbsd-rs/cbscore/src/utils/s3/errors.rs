// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Error variants surfaced by `cbscore::utils::s3`.
//!
//! Lives in `cbscore` (not `cbscore-types`) per plan 002-03 §Commit 1
//! — the cbscore-types error taxonomy doesn't include S3; callers in
//! `releases::s3` (Phase 5) wrap [`S3Error`] into their domain
//! [`cbscore_types::releases::ReleaseError`] via [`#[from]`].
//!
//! Per design 002 §Error Taxonomy line 239–240, framework errors
//! (`aws_sdk_s3`, `reqwest`, …) that cannot be exhaustively matched
//! are boxed to keep the enum compact (a bare `SdkError<…>` is >1 KB).

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error;
use aws_sdk_s3::operation::put_object::PutObjectError;
use thiserror::Error;

/// Errors surfaced by the async S3 wrappers.
///
/// Each variant boxes its `aws_sdk_s3` payload — the SDK error types
/// are large (`SdkError<E>` carries the full response context); leaving
/// them unboxed would balloon every `Result<T, S3Error>` and bleed
/// into every caller's stack frames.
///
/// # Examples
///
/// ```
/// use cbscore::utils::s3::S3Error;
///
/// // Construct an Io variant from a synthetic IO error for testing.
/// let e: S3Error = std::io::Error::new(
///     std::io::ErrorKind::PermissionDenied,
///     "denied",
/// )
/// .into();
/// assert!(matches!(e, S3Error::Io { .. }));
/// ```
#[derive(Debug, Error)]
pub enum S3Error {
    /// HEAD object failed for a reason other than "not found".
    /// `check_release_exists` maps an HTTP 404 to `Ok(false)` before
    /// constructing this variant — any [`Head`](S3Error::Head) error
    /// reaching the caller is a real failure (permission, network,
    /// service).
    #[error("S3 head failed: {source}")]
    Head {
        /// Wrapped SDK error.
        #[from]
        source: Box<SdkError<HeadObjectError>>,
    },

    /// `ListObjectsV2` failed.
    #[error("S3 list failed: {source}")]
    List {
        /// Wrapped SDK error.
        #[from]
        source: Box<SdkError<ListObjectsV2Error>>,
    },

    /// `PutObject` failed.
    #[error("S3 put failed: {source}")]
    Put {
        /// Wrapped SDK error.
        #[from]
        source: Box<SdkError<PutObjectError>>,
    },

    /// Catch-all for the aggregate AWS SDK error type when an
    /// operation surfaces an error not covered by its per-operation
    /// variant.
    #[error("S3 error: {source}")]
    Other {
        /// Wrapped aggregate error.
        #[from]
        source: aws_sdk_s3::Error,
    },

    /// Local IO failure while reading a file off disk before upload
    /// (e.g. an RPM that vanished between build and
    /// `s3_upload_rpms`).
    #[error("local IO failure during S3 operation: {source}")]
    Io {
        /// Wrapped IO error.
        #[from]
        source: std::io::Error,
    },
}
