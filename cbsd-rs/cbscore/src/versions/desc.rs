// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `VersionDescriptor` file IO — read and write the JSON
//! descriptor format consumed by the runner (Phase 4) and the
//! `cbsbuild versions {create,show}` CLI surface (Phase 6).
//!
//! JSON only — descriptors are JSON with snake-case keys per
//! design 002 §Wire-Format Versioning (line 665–670). The
//! `schema_version` marker is emitted first via
//! [`VersionedVersionDescriptor`]'s `Serialize` impl.
//!
//! No trailing newline — matches Python `cbscore/versions/desc.py`'s
//! `model_dump_json(indent=2)` byte-for-byte (pydantic's pretty
//! JSON emitter ends at the closing brace).

use camino::Utf8Path;
use cbscore_types::versions::{VersionDescriptor, VersionError, VersionedVersionDescriptor};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

const TARGET_VERSIONS_DESC: &str = "cbscore::versions::desc";

/// Load a [`VersionDescriptor`] from a JSON file at `path`.
///
/// # Errors
///
/// - [`VersionError::NoSuchDescriptor`] when the file does not exist
///   ([`std::io::ErrorKind::NotFound`] mapped to the structured shape
///   so the CLI surfaces a clean operator message).
/// - [`VersionError::InvalidDescriptor`] for any other IO failure or
///   JSON parse error.
/// - [`VersionError::MissingSchemaVersion`] when the file is missing
///   the snake-case `schema_version` key.
/// - [`VersionError::UnknownSchemaVersion`] when the file declares a
///   `schema_version` higher than this build supports.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
///
/// # async fn demo() -> Result<(), cbscore_types::versions::VersionError> {
/// let desc = cbscore::versions::desc::read_descriptor(
///     Utf8Path::new("/var/cbs/_versions/dev/19.2.3.json"),
/// )
/// .await?;
/// let _ = desc;
/// # Ok(()) }
/// ```
pub async fn read_descriptor(path: &Utf8Path) -> Result<VersionDescriptor, VersionError> {
    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(VersionError::NoSuchDescriptor {
                path: path.to_owned(),
            });
        }
        Err(e) => {
            return Err(VersionError::InvalidDescriptor {
                path: path.to_owned(),
                message: format!("IO failure: {e}"),
            });
        }
    };
    let value: serde_value::Value =
        serde_json::from_slice(&bytes).map_err(|e| VersionError::InvalidDescriptor {
            path: path.to_owned(),
            message: format!("JSON parse: {e}"),
        })?;
    let desc = VersionedVersionDescriptor::from_value(value, path)?.into_latest();
    tracing::debug!(
        target: TARGET_VERSIONS_DESC,
        path = %path,
        version = %desc.version,
        "descriptor loaded",
    );
    Ok(desc)
}

/// Serialise `desc` as pretty JSON (2-space indent, snake-case
/// keys, `schema_version: 1` first) and write it atomically to
/// `path`.
///
/// Behaviour:
///
/// 1. `tokio::fs::create_dir_all` on `path.parent()` — the
///    descriptor store under `_versions/<type>/` may not yet
///    exist on a fresh deployment.
/// 2. Serialise via [`serde_json::to_string_pretty`] through
///    [`VersionedVersionDescriptor::new`]; pretty defaults to
///    2-space indent. No trailing newline (matches Python).
/// 3. Open a sibling tempfile (`.<basename>.tmp.<pid>`) in the
///    same parent dir, `sync_all`, then `rename` to the final
///    path. Rename within the same dir is atomic on Linux, so a
///    concurrent reader never observes a partial file even if
///    `write_descriptor` is interrupted (signal, panic, write
///    error). Mirrors [`crate::config::store`] and
///    [`crate::secrets::utils::write_secure_file`].
///
/// Mode is left at the process umask default (typically 0644).
/// Descriptors are not secret — they carry version numbers,
/// component refs, and signing-key references (not key material).
/// Operators needing tighter permissions tighten their process
/// umask before invoking `cbsbuild versions create`; cbscore-rs
/// does not override it.
///
/// # Errors
///
/// Returns [`VersionError::InvalidDescriptor`] on serialise
/// failure or any IO failure during dir create, tempfile write,
/// fsync, or rename — the wrapped message names the offending
/// step.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
/// use cbscore_types::versions::VersionDescriptor;
///
/// # async fn demo(desc: &VersionDescriptor) -> Result<(), cbscore_types::versions::VersionError> {
/// cbscore::versions::desc::write_descriptor(
///     desc,
///     Utf8Path::new("/var/cbs/_versions/dev/19.2.3.json"),
/// )
/// .await?;
/// # Ok(()) }
/// ```
pub async fn write_descriptor(
    desc: &VersionDescriptor,
    path: &Utf8Path,
) -> Result<(), VersionError> {
    if let Some(parent) = path.parent()
        && !parent.as_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| VersionError::InvalidDescriptor {
                path: path.to_owned(),
                message: format!("create_dir_all: {e}"),
            })?;
    }
    let body = serde_json::to_string_pretty(&VersionedVersionDescriptor::new(desc.clone()))
        .map_err(|e| VersionError::InvalidDescriptor {
            path: path.to_owned(),
            message: format!("serialize JSON: {e}"),
        })?;
    write_atomic(path, body.as_bytes()).await?;
    tracing::debug!(
        target: TARGET_VERSIONS_DESC,
        path = %path,
        bytes = body.len(),
        "descriptor written",
    );
    Ok(())
}

/// Atomic write helper — sibling tempfile + fsync + rename in the
/// same parent dir.
async fn write_atomic(path: &Utf8Path, contents: &[u8]) -> Result<(), VersionError> {
    let parent = path
        .parent()
        .ok_or_else(|| VersionError::InvalidDescriptor {
            path: path.to_owned(),
            message: "cannot derive parent dir for atomic write".into(),
        })?;
    let tmp_path = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name().unwrap_or("descriptor.json"),
        std::process::id(),
    ));
    let mut tmp = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp_path)
        .await
        .map_err(|e| VersionError::InvalidDescriptor {
            path: path.to_owned(),
            message: format!("create tempfile '{tmp_path}': {e}"),
        })?;
    tmp.write_all(contents)
        .await
        .map_err(|e| VersionError::InvalidDescriptor {
            path: path.to_owned(),
            message: format!("write tempfile '{tmp_path}': {e}"),
        })?;
    tmp.sync_all()
        .await
        .map_err(|e| VersionError::InvalidDescriptor {
            path: path.to_owned(),
            message: format!("fsync tempfile '{tmp_path}': {e}"),
        })?;
    drop(tmp);
    tokio::fs::rename(&tmp_path, path)
        .await
        .map_err(|e| VersionError::InvalidDescriptor {
            path: path.to_owned(),
            message: format!("rename to '{path}': {e}"),
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::versions::desc::{VersionComponent, VersionImage, VersionSignedOffBy};

    fn sample_desc() -> VersionDescriptor {
        VersionDescriptor {
            version: "19.2.3-dev.1".into(),
            title: "Release Development version 19.2.3 (DEV 1)".into(),
            signed_off_by: VersionSignedOffBy {
                user: "ops".into(),
                email: "ops@example.com".into(),
            },
            image: VersionImage {
                registry: "quay.io".into(),
                name: "ceph-builder".into(),
                tag: "el9".into(),
            },
            components: vec![VersionComponent {
                name: "ceph".into(),
                repo: "https://github.com/ceph/ceph".into(),
                ref_: "v19.2.3".into(),
            }],
            distro: "centos".into(),
            el_version: 9,
        }
    }

    #[tokio::test]
    async fn round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("19.2.3.json")).expect("utf8 path");
        let desc = sample_desc();
        write_descriptor(&desc, &path).await.expect("write");
        let loaded = read_descriptor(&path).await.expect("read");
        assert_eq!(loaded, desc);
    }

    #[tokio::test]
    async fn write_creates_parent_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = camino::Utf8PathBuf::from_path_buf(dir.path().join("dev/19.2.3.json"))
            .expect("utf8 path");
        assert!(!path.parent().unwrap().exists());
        write_descriptor(&sample_desc(), &path)
            .await
            .expect("write");
        assert!(path.exists());
        assert!(path.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn write_emits_schema_version_first_no_trailing_newline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("19.2.3.json")).expect("utf8 path");
        write_descriptor(&sample_desc(), &path)
            .await
            .expect("write");
        let body = tokio::fs::read_to_string(&path).await.expect("read");
        // The pretty printer opens with `{\n  "schema_version": 1,\n...`
        assert!(
            body.starts_with("{\n  \"schema_version\": 1"),
            "expected schema_version first, got:\n{body}",
        );
        // No trailing newline — matches Python's
        // `pydantic.model_dump_json(indent=2)` byte-for-byte.
        assert!(
            !body.ends_with('\n'),
            "Python writer does not emit a trailing newline; Rust must not either",
        );
    }

    #[tokio::test]
    async fn read_missing_file_returns_no_such_descriptor() {
        let path = camino::Utf8PathBuf::from("/nonexistent/19.2.3.json");
        let Err(err) = read_descriptor(&path).await else {
            panic!("expected NoSuchDescriptor, got Ok");
        };
        assert!(matches!(err, VersionError::NoSuchDescriptor { .. }));
    }

    #[tokio::test]
    async fn read_missing_schema_version_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("19.2.3.json")).expect("utf8 path");
        tokio::fs::write(
            &path,
            br#"{"version":"19.2.3","title":"x","signed_off_by":{"user":"u","email":"e"},"image":{"registry":"r","name":"n","tag":"t"},"components":[],"distro":"centos","el_version":9}"#,
        )
        .await
        .expect("write");
        let Err(err) = read_descriptor(&path).await else {
            panic!("expected MissingSchemaVersion, got Ok");
        };
        assert!(matches!(err, VersionError::MissingSchemaVersion { .. }));
    }

    #[tokio::test]
    async fn read_future_schema_version_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path =
            camino::Utf8PathBuf::from_path_buf(dir.path().join("19.2.3.json")).expect("utf8 path");
        tokio::fs::write(
            &path,
            br#"{"schema_version":99,"version":"19.2.3","title":"x","signed_off_by":{"user":"u","email":"e"},"image":{"registry":"r","name":"n","tag":"t"},"components":[],"distro":"centos","el_version":9}"#,
        )
        .await
        .expect("write");
        let Err(err) = read_descriptor(&path).await else {
            panic!("expected UnknownSchemaVersion, got Ok");
        };
        assert!(matches!(
            err,
            VersionError::UnknownSchemaVersion {
                found: 99,
                max_supported: 1,
                ..
            }
        ));
    }
}
