// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Repo resolution — translates [`ContainerRepo`] entries into the
//! Containerfile lines / build-context files the in-container
//! `dnf` step consumes.
//!
//! Three repo flavours per design 002 §Container Production:
//!
//! - **`copr://<user>/<project>`** — emits `dnf copr enable
//!   <user>/<project>` so the in-container build picks the repo up
//!   transparently.
//! - **`file://<path>`** — references a local `.repo` file that the
//!   builder copies into the build context for `dnf
//!   config-manager` to register at install time.
//! - **`http://...` / `https://...`** — passes the URL straight to
//!   `dnf config-manager --add-repo <url>`.
//!
//! Unrecognised schemes surface as
//! [`ContainerError::UnsupportedRepoType`] (Phase 1 Commit 2's
//! Phase-5-tracked variant), preserving the operator-actionable
//! "this repo scheme isn't known to cbscore-rs" message instead of
//! `unreachable!()`-ing through it.

use camino::Utf8PathBuf;
use cbscore_types::containers::ContainerError;
use cbscore_types::containers::desc::ContainerRepo;

/// Resolved form of a [`ContainerRepo`] — what the Containerfile or
/// build-context assembler needs to act on the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedRepo {
    /// `dnf copr enable <user>/<project>`.
    Copr {
        /// Operator-chosen repo identifier (preserved from input).
        name: String,
        /// `<user>/<project>` slug (the `copr://` URL's path).
        slug: String,
    },
    /// Local `.repo` file to be staged into the build context.
    File {
        /// Operator-chosen repo identifier (preserved from input).
        name: String,
        /// Host-side path to the source `.repo` file.
        source: Utf8PathBuf,
        /// Destination path inside the build context (matches
        /// [`ContainerRepo::dest`]).
        dest: String,
    },
    /// `dnf config-manager --add-repo <url>`.
    Url {
        /// Operator-chosen repo identifier (preserved from input).
        name: String,
        /// Full URL (preserved from input).
        url: String,
    },
}

/// Resolve a single [`ContainerRepo`] into its [`ResolvedRepo`]
/// shape.
///
/// # Errors
///
/// Returns [`ContainerError::UnsupportedRepoType`] when
/// `repo.source` is missing a recognised scheme prefix (`copr://`,
/// `file://`, `http://`, `https://`) or when a required field is
/// absent (e.g. `file://` without a `dest`).
///
/// # Examples
///
/// ```
/// use cbscore::containers::repos::{resolve_repo, ResolvedRepo};
/// use cbscore_types::containers::desc::ContainerRepo;
///
/// let copr = ContainerRepo {
///     name: "epel".into(),
///     source: "copr://group_ceph/ceph".into(),
///     dest: None,
/// };
/// let resolved = resolve_repo(&copr).unwrap();
/// assert!(matches!(resolved, ResolvedRepo::Copr { .. }));
/// ```
pub fn resolve_repo(repo: &ContainerRepo) -> Result<ResolvedRepo, ContainerError> {
    if let Some(slug) = repo.source.strip_prefix("copr://") {
        return Ok(ResolvedRepo::Copr {
            name: repo.name.clone(),
            slug: slug.to_owned(),
        });
    }
    if let Some(local) = repo.source.strip_prefix("file://") {
        let dest = repo
            .dest
            .as_ref()
            .ok_or_else(|| ContainerError::UnsupportedRepoType {
                name: repo.name.clone(),
                value: format!("{} (file:// requires a 'dest' field)", repo.source),
            })?;
        return Ok(ResolvedRepo::File {
            name: repo.name.clone(),
            source: Utf8PathBuf::from(local),
            dest: dest.clone(),
        });
    }
    if repo.source.starts_with("http://") || repo.source.starts_with("https://") {
        return Ok(ResolvedRepo::Url {
            name: repo.name.clone(),
            url: repo.source.clone(),
        });
    }
    Err(ContainerError::UnsupportedRepoType {
        name: repo.name.clone(),
        value: repo.source.clone(),
    })
}

/// Resolve every entry of `repos` in order. The first error short-
/// circuits — bulk-resolve callers can choose whether to map back
/// into a partial result (currently no caller does).
///
/// # Errors
///
/// Returns the first [`ContainerError::UnsupportedRepoType`] hit by
/// [`resolve_repo`].
pub fn resolve_repos(repos: &[ContainerRepo]) -> Result<Vec<ResolvedRepo>, ContainerError> {
    repos.iter().map(resolve_repo).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copr_resolves() {
        let repo = ContainerRepo {
            name: "ceph-copr".into(),
            source: "copr://group_ceph/ceph".into(),
            dest: None,
        };
        let resolved = resolve_repo(&repo).unwrap();
        let ResolvedRepo::Copr { name, slug } = resolved else {
            panic!("expected Copr");
        };
        assert_eq!(name, "ceph-copr");
        assert_eq!(slug, "group_ceph/ceph");
    }

    #[test]
    fn file_resolves_with_dest() {
        let repo = ContainerRepo {
            name: "local-rpms".into(),
            source: "file:///srv/ces.repo".into(),
            dest: Some("/etc/yum.repos.d/ces.repo".into()),
        };
        let resolved = resolve_repo(&repo).unwrap();
        let ResolvedRepo::File { name, source, dest } = resolved else {
            panic!("expected File");
        };
        assert_eq!(name, "local-rpms");
        assert_eq!(source, Utf8PathBuf::from("/srv/ces.repo"));
        assert_eq!(dest, "/etc/yum.repos.d/ces.repo");
    }

    #[test]
    fn file_without_dest_errors() {
        let repo = ContainerRepo {
            name: "local".into(),
            source: "file:///srv/ces.repo".into(),
            dest: None,
        };
        let Err(ContainerError::UnsupportedRepoType { value, .. }) = resolve_repo(&repo) else {
            panic!("expected UnsupportedRepoType");
        };
        assert!(value.contains("requires a 'dest' field"));
    }

    #[test]
    fn https_url_resolves() {
        let repo = ContainerRepo {
            name: "epel".into(),
            source: "https://dl.fedoraproject.org/pub/epel/9/Everything.repo".into(),
            dest: None,
        };
        let resolved = resolve_repo(&repo).unwrap();
        let ResolvedRepo::Url { name, url } = resolved else {
            panic!("expected Url");
        };
        assert_eq!(name, "epel");
        assert!(url.starts_with("https://"));
    }

    #[test]
    fn http_url_resolves() {
        let repo = ContainerRepo {
            name: "epel-http".into(),
            source: "http://example.com/repos/ces.repo".into(),
            dest: None,
        };
        assert!(matches!(
            resolve_repo(&repo).unwrap(),
            ResolvedRepo::Url { .. }
        ));
    }

    #[test]
    fn unsupported_scheme_errors() {
        let repo = ContainerRepo {
            name: "weird".into(),
            source: "ftp://mirror.example.com/repo.repo".into(),
            dest: None,
        };
        let Err(ContainerError::UnsupportedRepoType { name, value }) = resolve_repo(&repo) else {
            panic!("expected UnsupportedRepoType");
        };
        assert_eq!(name, "weird");
        assert!(value.starts_with("ftp://"));
    }

    #[test]
    fn resolve_repos_short_circuits_on_first_error() {
        let repos = vec![
            ContainerRepo {
                name: "ok".into(),
                source: "copr://group/x".into(),
                dest: None,
            },
            ContainerRepo {
                name: "bad".into(),
                source: "nope".into(),
                dest: None,
            },
            ContainerRepo {
                name: "also-ok".into(),
                source: "https://example.com/r.repo".into(),
                dest: None,
            },
        ];
        let Err(err) = resolve_repos(&repos) else {
            panic!("expected error");
        };
        assert!(matches!(
            err,
            ContainerError::UnsupportedRepoType { name, .. } if name == "bad",
        ));
    }
}
