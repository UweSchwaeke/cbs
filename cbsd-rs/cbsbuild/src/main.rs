// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! `cbsbuild` CLI — Rust port of the Python `cbsbuild` entry point.
//!
//! Phase 6 Commit 1 lands the clap tree scaffold plus the
//! cross-cutting setup (tokio runtime, tracing subscriber, exit-code
//! mapping). Subcommand handlers are stubs that return
//! `bail!("not yet implemented")`; commits 2–4 fill them in.
//!
//! Exit codes (per design 002 lines 1246–1248):
//!
//! - `0` — success.
//! - `22` (`errno::EINVAL`) — missing config / invalid input.
//! - `131` (`errno::ENOTRECOVERABLE`) — any other unhandled error.

#![warn(missing_docs)]
#![warn(unreachable_pub)]
// `pub(crate)` is the workspace-internal visibility standard per
// CLAUDE.md §Visibility. clippy's `redundant_pub_crate` lint nags
// to demote to bare `pub` inside private modules of a binary
// crate; the team's chosen convention wins.
#![allow(clippy::redundant_pub_crate)]
#![recursion_limit = "256"]

mod cli;
mod cmds;
mod logging;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;

/// `errno::ENOTRECOVERABLE` — unhandled error exit code.
const EXIT_UNRECOVERABLE: u8 = 131;

/// `errno::EINVAL` — invalid-input / missing-config exit code.
const EXIT_INVAL: u8 = 22;

/// Tokio multi-thread main. Worker count defaults to `num_cpus`;
/// the runtime awaits idle before returning so background tasks
/// finish on normal exit.
#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // tracing-appender's non-blocking writer returns a WorkerGuard
    // whose Drop flushes the background thread. Hold it in main()'s
    // scope so the last log lines reach disk on any exit path.
    let _log_guard = match logging::init_logging(cli.debug) {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!("cbsbuild: failed to initialise logging: {e:#}");
            return ExitCode::from(EXIT_UNRECOVERABLE);
        }
    };

    match cmds::dispatch(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("cbsbuild: {err:#}");
            ExitCode::from(classify_exit(&err))
        }
    }
}

/// Map a handler error to an exit code per design 002 lines
/// 1246–1248.
///
/// Walks the [`anyhow::Error::chain`] looking for a typed
/// cbscore-types error variant that indicates operator input
/// pointed at something that does not exist
/// ([`ConfigError::NotFound`] /
/// [`VersionError::NoSuchDescriptor`]). Those map to
/// [`EXIT_INVAL`]; every other error funnels through
/// [`EXIT_UNRECOVERABLE`].
///
/// Walking the chain (rather than only the outermost error) is
/// load-bearing: subcommand handlers wrap typed errors with
/// `.with_context("loading config at '/etc/cbsd/...'"`) before
/// the error reaches `main`, so the outermost `to_string()` is
/// the context message — the typed-cause downcast is the only
/// reliable way to recover the original kind across `anyhow`
/// wrap layers.
///
/// [`ConfigError::NotFound`]: cbscore_types::config::ConfigError::NotFound
/// [`VersionError::NoSuchDescriptor`]:
///     cbscore_types::versions::VersionError::NoSuchDescriptor
fn classify_exit(err: &anyhow::Error) -> u8 {
    use cbscore_types::config::ConfigError;
    use cbscore_types::versions::VersionError;

    for cause in err.chain() {
        if let Some(cfg) = cause.downcast_ref::<ConfigError>()
            && matches!(cfg, ConfigError::NotFound { .. })
        {
            return EXIT_INVAL;
        }
        if let Some(ver) = cause.downcast_ref::<VersionError>()
            && matches!(ver, VersionError::NoSuchDescriptor { .. })
        {
            return EXIT_INVAL;
        }
    }
    EXIT_UNRECOVERABLE
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use cbscore_types::config::ConfigError;
    use cbscore_types::versions::VersionError;

    #[test]
    fn config_not_found_maps_to_einval() {
        let err = anyhow::Error::from(ConfigError::NotFound {
            path: Utf8PathBuf::from("/etc/cbsd/cbs-build.config.yaml"),
        });
        assert_eq!(classify_exit(&err), EXIT_INVAL);
    }

    #[test]
    fn config_not_found_under_context_wrap_maps_to_einval() {
        // Subcommand handlers typically wrap typed errors via
        // .with_context("loading config at '…'"); the chain walk
        // must still find the ConfigError::NotFound cause.
        let raw = ConfigError::NotFound {
            path: Utf8PathBuf::from("/etc/cbsd/cbs-build.config.yaml"),
        };
        let err =
            anyhow::Error::from(raw).context("loading config at '/etc/cbsd/cbs-build.config.yaml'");
        assert_eq!(classify_exit(&err), EXIT_INVAL);
    }

    #[test]
    fn version_no_such_descriptor_maps_to_einval() {
        let err = anyhow::Error::from(VersionError::NoSuchDescriptor {
            path: Utf8PathBuf::from("/var/cbs/_versions/dev/missing.json"),
        });
        assert_eq!(classify_exit(&err), EXIT_INVAL);
    }

    #[test]
    fn other_config_error_maps_to_unrecoverable() {
        // MissingSchemaVersion is operator-actionable but is NOT
        // a "missing input" case — the file exists, the wire
        // format is wrong. Maps to ENOTRECOVERABLE so the two
        // failure modes stay distinguishable.
        let err = anyhow::Error::from(ConfigError::MissingSchemaVersion {
            path: Utf8PathBuf::from("/etc/cbsd/cbs-build.config.yaml"),
        });
        assert_eq!(classify_exit(&err), EXIT_UNRECOVERABLE);
    }

    #[test]
    fn unrelated_anyhow_error_maps_to_unrecoverable() {
        let err = anyhow::anyhow!("something else went wrong");
        assert_eq!(classify_exit(&err), EXIT_UNRECOVERABLE);
    }
}
