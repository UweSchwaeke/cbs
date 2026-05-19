// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Runner subsystem — spawns the host-side podman container that
//! re-enters `cbsbuild` inside as PID 1.
//!
//! Drives the full build state machine (Idle → Preparing → Spawning
//! → Running → Finished / Failed / Stopped → Cleanup) — see
//! [`run`] for the orchestration loop and [`gen_run_name`] /
//! [`stop`] for the small helpers the Phase 6
//! `cbsbuild runner stop` CLI surface exposes.

pub mod run;

pub use run::{RunOpts, RunReport, run};

use std::time::Duration;

use cbscore_types::runner::RunnerError;
use rand::seq::IndexedRandom;

use crate::utils::podman::podman_stop;

const TARGET_RUNNER: &str = "cbscore::runner";

/// Default prefix for [`gen_run_name`] — matches Python
/// `runner.gen_run_name(prefix="ces_")`.
pub const DEFAULT_RUN_NAME_PREFIX: &str = "ces_";

/// Random-suffix length appended by [`gen_run_name`] — 10 lowercase
/// ASCII letters, matching Python `random.choices(ascii_lowercase,
/// k=10)`.
pub const RUN_NAME_SUFFIX_LEN: usize = 10;

/// Default timeout passed to [`stop`] — 1 second, matching Python
/// `cbscore.utils.podman.podman_stop`'s default and the runner's
/// SIGTERM-propagation budget per design 002 line 859.
pub const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(1);

/// Generate a fresh container name as `<prefix><10 random
/// lowercase ASCII letters>`.
///
/// `prefix` defaults to [`DEFAULT_RUN_NAME_PREFIX`] (`"ces_"`) when
/// `None`. Per design 002 lines 833–844, the suffix uses
/// `random.choices(ascii_lowercase, k=10)`-equivalent sampling —
/// the Rust port draws 10 independent picks from `'a'..='z'` via
/// [`rand::seq::IndexedRandom::choose`], producing the same surface
/// shape as the Python source.
///
/// # Examples
///
/// ```
/// use cbscore::runner::{gen_run_name, DEFAULT_RUN_NAME_PREFIX};
///
/// let n = gen_run_name(None);
/// assert!(n.starts_with(DEFAULT_RUN_NAME_PREFIX));
/// assert_eq!(n.len(), DEFAULT_RUN_NAME_PREFIX.len() + 10);
///
/// let n2 = gen_run_name(Some("test-"));
/// assert!(n2.starts_with("test-"));
/// assert_eq!(n2.len(), "test-".len() + 10);
/// ```
// `.expect("ALPHABET is non-empty by const construction")` cannot
// panic — the const ALPHABET literal has 26 entries, so
// `IndexedRandom::choose` always returns `Some`. No `# Panics` doc
// warranted.
#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn gen_run_name(prefix: Option<&str>) -> String {
    const ALPHABET: &[char] = &[
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r',
        's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
    ];
    let p = prefix.unwrap_or(DEFAULT_RUN_NAME_PREFIX);
    let mut rng = rand::rng();
    let suffix: String = (0..RUN_NAME_SUFFIX_LEN)
        .map(|_| {
            *ALPHABET
                .choose(&mut rng)
                .expect("ALPHABET is non-empty by const construction")
        })
        .collect();
    format!("{p}{suffix}")
}

/// Stop a single container by `name`, or every running container
/// when `name` is `None` (the `podman stop --all` form). `timeout`
/// is passed via `podman stop --time <secs>`.
///
/// Delegates to [`crate::utils::podman::podman_stop`] for the
/// argv construction and subprocess drive; this wrapper exists so
/// the Phase 6 `cbsbuild runner stop` CLI surface has a single
/// crate-level entry point that returns the runner's domain error
/// ([`RunnerError`]) rather than [`PodmanError`].
///
/// # Errors
///
/// Returns [`RunnerError::Podman`] wrapping the underlying
/// [`PodmanError`] on a non-zero `podman stop` exit; the `#[from]`
/// impl on [`RunnerError`] (Phase 1 Commit 2) handles the wrap.
///
/// # Examples
///
/// ```no_run
/// use cbscore::runner::{stop, DEFAULT_STOP_TIMEOUT};
///
/// # async fn demo() -> Result<(), cbscore_types::runner::RunnerError> {
/// // Stop one named container.
/// stop(Some("ces_abcdef0123"), DEFAULT_STOP_TIMEOUT).await?;
/// // Or stop every running container.
/// stop(None, DEFAULT_STOP_TIMEOUT).await?;
/// # Ok(()) }
/// ```
///
/// [`PodmanError`]: cbscore_types::utils::podman::PodmanError
#[tracing::instrument(level = "debug", target = "cbscore::runner")]
pub async fn stop(name: Option<&str>, timeout: Duration) -> Result<(), RunnerError> {
    tracing::debug!(
        target: TARGET_RUNNER,
        target_container = name.unwrap_or("--all"),
        timeout_secs = timeout.as_secs(),
        "runner::stop",
    );
    podman_stop(name, timeout).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn gen_run_name_default_prefix() {
        let n = gen_run_name(None);
        assert!(n.starts_with(DEFAULT_RUN_NAME_PREFIX));
        assert_eq!(n.len(), DEFAULT_RUN_NAME_PREFIX.len() + RUN_NAME_SUFFIX_LEN);
        // The suffix must be 10 lowercase ASCII letters.
        for ch in n[DEFAULT_RUN_NAME_PREFIX.len()..].chars() {
            assert!(ch.is_ascii_lowercase(), "non-lowercase: {ch:?}");
        }
    }

    #[test]
    fn gen_run_name_custom_prefix() {
        let n = gen_run_name(Some("test-"));
        assert!(n.starts_with("test-"));
        assert_eq!(n.len(), "test-".len() + RUN_NAME_SUFFIX_LEN);
    }

    #[test]
    fn gen_run_name_empty_prefix() {
        let n = gen_run_name(Some(""));
        assert_eq!(n.len(), RUN_NAME_SUFFIX_LEN);
    }

    #[test]
    fn gen_run_name_is_random() {
        // 1000 calls should produce distinct strings with overwhelming
        // probability (26^10 ≈ 1.4 * 10^14 namespace, birthday-paradox
        // collision probability at 1000 samples is ~3.6 * 10^-9).
        let names: HashSet<String> = (0..1000).map(|_| gen_run_name(None)).collect();
        assert_eq!(names.len(), 1000, "duplicate names produced");
    }
}
