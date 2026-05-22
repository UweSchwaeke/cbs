// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Resolve the descriptor-store root and VERSION string for
//! `cbsbuild versions create`.
//!
//! ## Root resolver — `resolve_root` (design 004)
//!
//! Precedence (design 004 OQ1):
//!
//! 1. CLI flag (`--versions-dir PATH`).
//! 2. Config field (`Config.paths.versions`).
//! 3. `git rev-parse --show-toplevel` + `/_versions` — Python-parity
//!    fallback (design 004 OQ2).
//!
//! Layers 1 and 2 canonicalise their input via [`tokio::fs::canonicalize`]
//! and surface `ENOENT`-class failures as [`VersionError::DescriptorRootResolve`].
//! Layer 3 trusts `git rev-parse --show-toplevel`'s output as already
//! absolute and symlink-resolved.
//!
//! When all three layers miss, the resolver returns
//! [`VersionError::NoDescriptorRoot`] with the operator's cwd
//! (captured best-effort) so the four-line OQ5 message can name a
//! concrete starting point for the operator.
//!
//! ## VERSION resolver — `resolve_version` (design 005 / seq-005)
//!
//! Sync, infallible. If the CLI positional was supplied, returns it
//! verbatim. Otherwise mints a fresh `UUIDv7` string (RFC 9562) and
//! returns it. Paired with the title-generator branch in
//! `cbscore::versions::create::make_title` and the regex-or-UUIDv7
//! `validate_version` in `cbscore::versions::utils`.

use camino::{Utf8Path, Utf8PathBuf};
use cbscore_types::config::Config;
use cbscore_types::versions::VersionError;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::utils::git;

/// Resolve the descriptor-store root.
///
/// `cli` is the value passed to `--versions-dir` (or `None` if the
/// flag was absent). `config` is the loaded `cbs-build.config.yaml`
/// — `config.paths.versions` is the second-precedence override.
///
/// On success, the returned path is **absolute and symlink-resolved**
/// — every caller downstream (the patch walker, the runner mount
/// table, [`cbscore_types::versions::desc::descriptor_path`]) can
/// rely on the invariant.
///
/// # Errors
///
/// - [`VersionError::DescriptorRootResolve`] — an operator-supplied
///   path (`cli` or `config.paths.versions`) failed to canonicalise.
///   Most commonly `ENOENT` because the directory does not yet exist;
///   the `Display` text includes a `mkdir -p` hint.
/// - [`VersionError::DescriptorRootNotUtf8`] — the canonicalised path
///   contains non-UTF-8 bytes (rare on Linux operator hosts but
///   representable).
/// - [`VersionError::NoDescriptorRoot`] — neither override is set and
///   the git-fallback failed (cwd is outside a checkout, or `git`
///   itself failed). The four-line `Display` text names every override
///   surface the operator can set.
///
/// # Examples
///
/// ```no_run
/// use camino::Utf8Path;
/// use cbscore::versions::resolve_root;
/// use cbscore_types::config::Config;
/// # async fn doc(config: &Config) -> Result<(), cbscore_types::versions::VersionError> {
/// // CLI flag wins; resolves under `/tmp/cbs-versions`.
/// let root = resolve_root(Some(Utf8Path::new("/tmp/cbs-versions")), config).await?;
/// assert!(root.is_absolute());
/// # Ok(()) }
/// ```
pub async fn resolve_root(
    cli: Option<&Utf8Path>,
    config: &Config,
) -> Result<Utf8PathBuf, VersionError> {
    if let Some(p) = cli {
        tracing::debug!(target: "cbscore::versions", path = %p, "resolving descriptor root: cli flag");
        return canonicalize_root(p).await;
    }
    if let Some(p) = config.paths.versions.as_deref() {
        tracing::debug!(target: "cbscore::versions", path = %p, "resolving descriptor root: config field");
        return canonicalize_root(p).await;
    }
    tracing::debug!(target: "cbscore::versions", "resolving descriptor root: git fallback");
    git::repo_root().await.map_or_else(
        |_| {
            Err(VersionError::NoDescriptorRoot {
                cwd: current_dir_best_effort(),
            })
        },
        |root| Ok(root.join("_versions")),
    )
}

/// Canonicalise an operator-supplied root: resolves symlinks, makes
/// the path absolute, and surfaces a clean error if the path is
/// missing or non-UTF-8.
async fn canonicalize_root(p: &Utf8Path) -> Result<Utf8PathBuf, VersionError> {
    let resolved = tokio::fs::canonicalize(p.as_std_path())
        .await
        .map_err(|source| VersionError::DescriptorRootResolve {
            path: p.to_owned(),
            source,
        })?;
    Utf8PathBuf::from_path_buf(resolved).map_err(|bad| VersionError::DescriptorRootNotUtf8 {
        path: bad.to_string_lossy().into_owned(),
    })
}

/// Capture the cwd for the `NoDescriptorRoot` diagnostic; never
/// propagates the underlying `io::Error`.
///
/// Linux's `getcwd(2)` returns `ENOENT` when the cwd has been deleted
/// out from under the process — the diagnostic still needs to render
/// rather than panic, so we fall back to `<unknown>` rather than
/// trying to surface the failure.
fn current_dir_best_effort() -> Utf8PathBuf {
    std::env::current_dir()
        .ok()
        .and_then(|p| Utf8PathBuf::try_from(p).ok())
        .unwrap_or_else(|| Utf8PathBuf::from("<unknown>"))
}

/// Resolve the VERSION string for `cbsbuild versions create`.
///
/// Returns `cli.to_owned()` when the operator passed a positional,
/// otherwise mints a fresh `UUIDv7` string (RFC 9562 canonical
/// hyphenated 36-char lowercase form). Sync, infallible —
/// [`Uuid::now_v7`] reads the system clock without IO.
///
/// The `UUIDv7` form sorts lexicographically by its leading 48-bit
/// timestamp, so descriptor filenames under `<root>/<type>/`
/// remain in chronological creation order when listed (design 005
/// §Goals; design 005 §Operator UX).
///
/// # Examples
///
/// ```
/// use cbscore::versions::resolve_version;
///
/// // Supplied VERSION is passed through verbatim.
/// assert_eq!(resolve_version(Some("19.2.3")), "19.2.3");
///
/// // Absent VERSION yields a UUIDv7 string.
/// let generated = resolve_version(None);
/// let parsed = uuid::Uuid::parse_str(&generated).unwrap();
/// assert_eq!(parsed.get_version(), Some(uuid::Version::SortRand));
/// ```
#[must_use]
pub fn resolve_version(cli: Option<&str>) -> String {
    cli.map_or_else(|| Uuid::now_v7().to_string(), str::to_owned)
}

/// Extract the leading 48-bit Unix-milliseconds timestamp from a
/// `UUIDv7` and return it as a `chrono::DateTime<Utc>`.
///
/// Returns `None` when the input is not a `UUIDv7` (defensive — the
/// caller is expected to have checked `get_version() ==
/// Some(Version::SortRand)`) or when the embedded timestamp is
/// out of the range `chrono::DateTime::<Utc>::from_timestamp`
/// accepts.
///
/// # Returns
///
/// `Some(DateTime<Utc>)` for a `UUIDv7` with an in-range timestamp;
/// `None` for non-v7 UUIDs and for v7 UUIDs whose embedded
/// timestamp does not fit `i64` seconds since the epoch (the
/// latter is unreachable on a sane host — `UUIDv7` stores 48 bits
/// of milliseconds, well within `i64` seconds even after the
/// 10,895-year overflow).
///
/// Used by `cbscore::versions::create::make_title` to format the
/// "Release {`type_desc`} version created at {iso8601}" title body
/// when VERSION is a `UUIDv7` (seq-005 Commit 2).
pub(crate) fn uuid_v7_timestamp(uuid: &Uuid) -> Option<DateTime<Utc>> {
    if uuid.get_version() != Some(uuid::Version::SortRand) {
        return None;
    }
    let ts = uuid.get_timestamp()?;
    let (secs, nanos) = ts.to_unix();
    let secs = i64::try_from(secs).ok()?;
    DateTime::<Utc>::from_timestamp(secs, nanos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbscore_types::config::{Config, PathsConfig};
    use std::process::Command;
    use tempfile::TempDir;

    /// Build a minimal `Config` with `paths.versions = override_versions`
    /// and every other field stubbed.
    fn stub_config(override_versions: Option<&str>) -> Config {
        Config {
            paths: PathsConfig {
                components: vec!["/c".into()],
                scratch: "/s".into(),
                scratch_containers: "/s/c".into(),
                ccache: None,
                versions: override_versions.map(Utf8PathBuf::from),
            },
            storage: None,
            signing: None,
            logging: None,
            secrets: vec![],
            vault: None,
        }
    }

    /// CLI flag wins over a config field that points elsewhere.
    #[tokio::test]
    async fn cli_flag_wins_over_config_field() {
        let cli_dir = TempDir::new().expect("temp dir");
        let cfg_dir = TempDir::new().expect("temp dir");
        let cli_path = Utf8PathBuf::try_from(cli_dir.path().to_owned()).unwrap();
        let cfg_path = Utf8PathBuf::try_from(cfg_dir.path().to_owned()).unwrap();
        let config = stub_config(Some(cfg_path.as_str()));
        let root = resolve_root(Some(cli_path.as_path()), &config)
            .await
            .expect("resolve");
        assert_eq!(
            root.canonicalize_utf8().unwrap(),
            cli_path.canonicalize_utf8().unwrap(),
        );
    }

    /// Config field wins when no CLI flag is supplied.
    #[tokio::test]
    async fn config_field_wins_over_fallback() {
        let cfg_dir = TempDir::new().expect("temp dir");
        let cfg_path = Utf8PathBuf::try_from(cfg_dir.path().to_owned()).unwrap();
        let config = stub_config(Some(cfg_path.as_str()));
        let root = resolve_root(None, &config).await.expect("resolve");
        assert_eq!(
            root.canonicalize_utf8().unwrap(),
            cfg_path.canonicalize_utf8().unwrap(),
        );
    }

    /// Non-existent operator-supplied path surfaces
    /// `DescriptorRootResolve` with the underlying `NotFound`.
    #[tokio::test]
    async fn missing_cli_path_returns_descriptor_root_resolve() {
        let missing = Utf8Path::new("/tmp/cbs-test-does-not-exist-deadbeef-cafebabe");
        let config = stub_config(None);
        let err = resolve_root(Some(missing), &config)
            .await
            .expect_err("must fail");
        match err {
            VersionError::DescriptorRootResolve { path, source } => {
                assert_eq!(path, missing.to_owned());
                assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("expected DescriptorRootResolve, got {other:?}"),
        }
    }

    /// `canonicalize_root` resolves symlinks (the documented contract).
    #[tokio::test]
    async fn canonicalize_resolves_symlink() {
        let target_dir = TempDir::new().expect("temp dir");
        let link_holder = TempDir::new().expect("temp dir");
        let link = link_holder.path().join("link");
        std::os::unix::fs::symlink(target_dir.path(), &link).expect("symlink");
        let link_utf8 = Utf8PathBuf::try_from(link).unwrap();

        let resolved = canonicalize_root(link_utf8.as_path()).await.expect("ok");

        let expected = target_dir
            .path()
            .canonicalize()
            .expect("target canonicalize");
        let expected_utf8 = Utf8PathBuf::try_from(expected).unwrap();
        assert_eq!(resolved, expected_utf8);
    }

    /// Inside a temp git repo, the no-override path yields
    /// `<repo>/_versions`.
    //
    // `await_holding_lock` is allowed here: the `CWD_LOCK` is the
    // correct serialisation primitive for tests that mutate the
    // process cwd (a single tokio runtime shares cwd across all of
    // its tasks), and the `set_current_dir` → resolve_root().await
    // → `set_current_dir(prev)` sequence must hold the lock for the
    // whole window to keep the cwd stable. An async-aware mutex
    // would solve the same problem at higher cost; the sync lock is
    // intentional.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn fallback_inside_git_repo_yields_versions_subdir() {
        let repo = TempDir::new().expect("temp dir");
        let status = Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(repo.path())
            .status()
            .expect("git init");
        assert!(status.success(), "git init failed");

        // We need to run `resolve_root` from inside the repo. The test
        // mutates the process cwd, which is shared across `#[tokio::test]`
        // threads on the same runtime — gate the mutation with a static
        // mutex so this test does not race the cwd-deleted-after-creation
        // test below.
        let _guard = CWD_LOCK.lock().expect("cwd lock");
        let prev_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(repo.path()).expect("cd repo");
        let config = stub_config(None);
        let root = resolve_root(None, &config).await;
        std::env::set_current_dir(&prev_cwd).expect("restore cwd");

        let root = root.expect("resolve");
        let expected = Utf8PathBuf::try_from(
            repo.path()
                .canonicalize()
                .expect("canonicalize repo")
                .join("_versions"),
        )
        .unwrap();
        assert_eq!(root, expected);
    }

    /// Outside any git checkout, the no-override path produces
    /// `NoDescriptorRoot { cwd }` with the operator's cwd.
    // `await_holding_lock`: same rationale as
    // `fallback_inside_git_repo_yields_versions_subdir`.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn fallback_outside_git_repo_returns_no_descriptor_root() {
        let outside = TempDir::new().expect("temp dir");

        let _guard = CWD_LOCK.lock().expect("cwd lock");
        let prev_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(outside.path()).expect("cd outside");
        let captured_cwd = std::env::current_dir().expect("cwd after cd");
        let config = stub_config(None);
        let root = resolve_root(None, &config).await;
        std::env::set_current_dir(&prev_cwd).expect("restore cwd");

        match root {
            Err(VersionError::NoDescriptorRoot { cwd }) => {
                let expected = Utf8PathBuf::try_from(captured_cwd).unwrap();
                assert_eq!(cwd, expected);
            }
            other => panic!("expected NoDescriptorRoot, got {other:?}"),
        }
    }

    /// The `NoDescriptorRoot` `Display` impl renders the OQ5 four-line
    /// message with the cwd substituted. Snapshot-compare against the
    /// expected string.
    #[test]
    fn no_descriptor_root_display_renders_oq5_text() {
        let err = VersionError::NoDescriptorRoot {
            cwd: Utf8PathBuf::from("/tmp/operator"),
        };
        let expected = "cannot resolve descriptor store location.\n  no --versions-dir flag was supplied,\n  no `paths.versions` field is set in cbs-build.config.yaml,\n  and the current directory (/tmp/operator) is not inside a git checkout.\n  set one of the above to choose where descriptors live.";
        assert_eq!(err.to_string(), expected);
    }

    /// Best-effort cwd capture renders `<unknown>` when
    /// `std::env::current_dir()` fails. We can't easily simulate the
    /// failure portably, so the smoke check is just "the helper
    /// returns *something* and never panics".
    #[test]
    fn current_dir_best_effort_never_panics() {
        let _ = current_dir_best_effort();
    }

    // Shared mutex for tests that mutate the process cwd. Tokio's
    // multi-threaded test runtime can interleave `#[tokio::test]`
    // tasks across threads; serialising cwd mutation prevents the
    // "git repo" and "outside git repo" tests from racing each
    // other's `set_current_dir` calls.
    use std::sync::Mutex;
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    // ----------------------------------------------------------------
    // resolve_version (seq-005)
    // ----------------------------------------------------------------

    /// Operator-supplied VERSION is passed through verbatim — no
    /// canonicalisation, no validation (validation lives in
    /// `cbscore::versions::utils::validate_version`).
    #[test]
    fn resolve_version_passes_supplied_string_through_verbatim() {
        assert_eq!(resolve_version(Some("19.2.3")), "19.2.3");
        assert_eq!(
            resolve_version(Some("ces-v19.2.3-dev.1")),
            "ces-v19.2.3-dev.1"
        );
        // The resolver doesn't validate — that's a downstream
        // concern. A malformed input passes through here and is
        // rejected by validate_version at the CLI call site.
        assert_eq!(resolve_version(Some("foobar")), "foobar");
    }

    /// Absent VERSION yields a `UUIDv7` string that parses as v7.
    #[test]
    fn resolve_version_none_yields_uuidv7() {
        let generated = resolve_version(None);
        let parsed = Uuid::parse_str(&generated).expect("UUIDv7 parses");
        assert_eq!(parsed.get_version(), Some(uuid::Version::SortRand));
    }

    /// Two consecutive `resolve_version(None)` calls return two
    /// distinct `UUIDv7s`. Collision is astronomically unlikely (74
    /// random bits) but the test pins the no-determinism contract.
    #[test]
    fn resolve_version_none_yields_distinct_uuids_on_repeat() {
        let a = resolve_version(None);
        let b = resolve_version(None);
        assert_ne!(a, b);
    }

    // ----------------------------------------------------------------
    // uuid_v7_timestamp (seq-005)
    // ----------------------------------------------------------------

    /// `UUIDv7` minted at a fixed Unix timestamp returns a `DateTime`
    /// matching the timestamp to the second. The `Timestamp::from_unix_time`
    /// const ctor avoids needing a `ClockSequence` context arg —
    /// good for deterministic fixtures.
    #[test]
    fn uuid_v7_timestamp_extracts_fixed_unix_seconds() {
        // 1777895100 = 2026-05-04T11:45:00Z UTC
        let ts = uuid::Timestamp::from_unix_time(1_777_895_100, 0, 0, 0);
        let uuid = Uuid::new_v7(ts);
        let extracted = uuid_v7_timestamp(&uuid).expect("v7 has a timestamp");
        let expected = DateTime::<Utc>::from_timestamp(1_777_895_100, 0).unwrap();
        // UUIDv7 stores millisecond precision; extracted timestamp
        // should match the second exactly (the nanoseconds field is
        // zero in our fixture).
        assert_eq!(extracted, expected);
    }

    /// `UUIDv4` input returns `None` — defensive arm for callers that
    /// bypass the `get_version()` check.
    #[test]
    fn uuid_v7_timestamp_returns_none_for_uuidv4() {
        let v4 = Uuid::new_v4();
        assert_eq!(uuid_v7_timestamp(&v4), None);
    }
}
