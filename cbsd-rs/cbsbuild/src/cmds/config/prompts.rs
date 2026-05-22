// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Prompt-layer abstractions for `cbsbuild config init` /
//! `init-vault`.
//!
//! The `Prompter` trait isolates the dialoguer-driven IO from the
//! flow logic so the flow functions can be unit-tested by swapping
//! in a [`ScriptedPrompter`] under `#[cfg(test)]`. Three primitives
//! mirror the Python `click` calls we depend on:
//!
//! - [`Prompter::input`] ↔ `click.prompt(...)` → `dialoguer::Input`
//! - [`Prompter::password`] ↔ `click.prompt(hide_input=True)` →
//!   `dialoguer::Password`
//! - [`Prompter::confirm`] ↔ `click.confirm(...)` →
//!   `dialoguer::Confirm`
//!
//! See seq-003 plan + design 003 §Module Layout for the
//! architectural context. The [`validate_url`] free function lives
//! here too — it's used by `init-vault`'s Vault-address prompt and
//! `init`'s S3 / registry URL prompts. Per design 003 §URL
//! validation, the validator is NOT threaded through the `Prompter`
//! trait; call sites that need URL validation use dialoguer's
//! `validate_with` hook directly on the `DialoguerPrompter` path,
//! and tests exercise [`validate_url`] in isolation.
//!
//! ## Dead-code allowance
//!
//! This module is Commit 1 of seq-003 — plumbing landed ahead of
//! its consumers. `Prompter`, `DialoguerPrompter`, `PromptError`,
//! and `validate_url` are all `pub(crate)` items consumed by
//! seq-003 Commit 2 (`cbsbuild config init-vault`) and Commit 3
//! (interactive `cbsbuild config init`). The unit tests below
//! exercise the trait via [`ScriptedPrompter`] and the validator
//! directly, so the contracts are pinned down. The module-level
//! `allow(dead_code)` suppresses the inevitable "never used in
//! non-test build" warnings until the consuming commits land — at
//! which point the allowance should be removed and the build
//! re-verified.

#![allow(dead_code)]

use thiserror::Error;

/// Errors surfaced by the prompt layer.
///
/// `DialoguerPrompter` only ever produces [`PromptError::Io`].
/// [`PromptError::EmptyScript`] is reserved for [`ScriptedPrompter`]
/// when its answer queue is exhausted — production paths never
/// surface it.
#[derive(Debug, Error)]
pub(crate) enum PromptError {
    /// Underlying IO failure from `dialoguer::Error::IO` (the only
    /// variant `dialoguer 0.11` produces). Includes the operator
    /// hitting Ctrl+C / EOF on a non-TTY input source.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Test-only signal — produced by [`ScriptedPrompter`] when
    /// its answer queue is exhausted. The label of the missing
    /// prompt is captured in the scripted prompter's
    /// recorded-calls log; tests inspect that log to identify
    /// which prompt was expected next. Production code paths
    /// (using [`DialoguerPrompter`]) never produce this variant.
    #[error("ScriptedPrompter: answer queue exhausted")]
    EmptyScript,
}

/// Three-method prompt interface backing both the interactive
/// flow and the scripted-prompter test infrastructure.
pub(crate) trait Prompter {
    /// Plain-text input prompt with an optional default.
    fn input(&mut self, label: &str, default: Option<&str>) -> Result<String, PromptError>;

    /// Password input prompt (hides typed characters).
    fn password(&mut self, label: &str) -> Result<String, PromptError>;

    /// Yes/no confirmation with an explicit default.
    fn confirm(&mut self, label: &str, default: bool) -> Result<bool, PromptError>;
}

/// Production [`Prompter`] backed by `dialoguer`.
///
/// Empty struct — each method constructs a fresh dialoguer prompt
/// per invocation. Uses dialoguer's default theme (auto-detects
/// TTY and picks `ColorfulTheme` when output is a terminal),
/// matching Python `click`'s ANSI behaviour.
pub(crate) struct DialoguerPrompter;

impl Prompter for DialoguerPrompter {
    fn input(&mut self, label: &str, default: Option<&str>) -> Result<String, PromptError> {
        let mut prompt = dialoguer::Input::<String>::new().with_prompt(label);
        if let Some(d) = default {
            prompt = prompt.default(d.to_owned());
        }
        prompt.interact_text().map_err(map_dialoguer_err)
    }

    fn password(&mut self, label: &str) -> Result<String, PromptError> {
        dialoguer::Password::new()
            .with_prompt(label)
            .interact()
            .map_err(map_dialoguer_err)
    }

    fn confirm(&mut self, label: &str, default: bool) -> Result<bool, PromptError> {
        dialoguer::Confirm::new()
            .with_prompt(label)
            .default(default)
            .interact()
            .map_err(map_dialoguer_err)
    }
}

/// Translate a [`dialoguer::Error`] into a [`PromptError`].
///
/// `dialoguer 0.11` exposes a single `Error::IO(IoError)` variant;
/// this mapping is therefore 1:1. Kept as a separate function so
/// future dialoguer-error additions surface as a compile error
/// here rather than a silent variant promotion.
fn map_dialoguer_err(err: dialoguer::Error) -> PromptError {
    let dialoguer::Error::IO(io) = err;
    PromptError::Io(io)
}

/// Validate that `s` parses as a URL.
///
/// Used by the Vault-address, S3-storage-URL, and registry-URL
/// prompts. Catches syntactic-malformed input — empty strings,
/// missing scheme (`vault.local` without `https://`), or garbled
/// text — at prompt time rather than surfacing them as opaque
/// SDK errors at first connect. Semantic validity (host
/// reachable, port correct, TLS cert valid) is **not** checked
/// here — that remains the SDK's job.
///
/// **Documented limitation — scheme typos pass this check.**
/// `url::Url::parse` accepts arbitrary scheme strings as
/// syntactically valid (`htps://example.com` parses with
/// `scheme = "htps"`). Detecting a scheme typo requires a scheme
/// allowlist, which this design intentionally does not add — see
/// design 003 §URL validation "Documented limitation" block.
///
/// The returned error string is what the operator sees when the
/// `DialoguerPrompter` route re-prompts via dialoguer's
/// `validate_with` hook.
///
/// # Errors
///
/// Returns `Err(<message>)` when `s` does not parse as a URL.
/// The message starts with `"invalid URL: "` and contains the
/// underlying parser diagnostic.
///
/// # Examples
///
/// ```
/// # // Path used inside the bin crate, illustrative only.
/// # fn validate_url(s: &str) -> Result<(), String> {
/// #     url::Url::parse(s).map(|_| ()).map_err(|e| format!("invalid URL: {e}"))
/// # }
/// assert!(validate_url("https://vault.example.com").is_ok());
/// assert!(validate_url("vault.local").is_err());
/// ```
pub(crate) fn validate_url(s: &str) -> Result<(), String> {
    url::Url::parse(s)
        .map(|_| ())
        .map_err(|e| format!("invalid URL: {e}"))
}

#[cfg(test)]
pub(crate) use scripted::{PromptAnswer, ScriptedPrompter};

#[cfg(test)]
mod scripted {
    use std::collections::VecDeque;

    use super::{PromptError, Prompter};

    /// One scripted answer for a future [`Prompter`] call.
    ///
    /// The variant must match the [`Prompter`] method the caller
    /// invokes; mismatches panic (a test-script bug always
    /// produces a panic, never a runtime error).
    #[derive(Debug, Clone)]
    pub(crate) enum PromptAnswer {
        Input(String),
        Password(String),
        Confirm(bool),
    }

    /// One recorded [`Prompter`] call — captured in
    /// [`ScriptedPrompter::calls`] so tests can inspect the prompt
    /// flow without depending on output assertions.
    #[derive(Debug, Clone)]
    pub(crate) enum PromptCall {
        Input {
            label: String,
            default: Option<String>,
        },
        Password {
            label: String,
        },
        Confirm {
            label: String,
            default: bool,
        },
    }

    /// Test-only [`Prompter`] driven by a pre-scripted answer
    /// queue.
    ///
    /// Tests construct a queue of [`PromptAnswer`]s in the order
    /// the production code is expected to invoke prompts.
    /// `ScriptedPrompter` pops one answer per call and records the
    /// invocation in [`Self::calls`]. When the queue is empty, the
    /// next call returns [`PromptError::EmptyScript`]; the test
    /// then inspects [`Self::calls`] to identify which prompt
    /// fired last and which would have been next.
    ///
    /// Mismatched variant (e.g. an `Input` answer when the
    /// production code calls `confirm`) panics — that's always a
    /// test-script bug, never a runtime concern.
    #[derive(Debug)]
    pub(crate) struct ScriptedPrompter {
        answers: VecDeque<PromptAnswer>,
        /// Recorded calls, in invocation order. Tests inspect
        /// this to locate which prompt was expected.
        pub(crate) calls: Vec<PromptCall>,
    }

    impl ScriptedPrompter {
        pub(crate) fn new<I: IntoIterator<Item = PromptAnswer>>(answers: I) -> Self {
            Self {
                answers: answers.into_iter().collect(),
                calls: Vec::new(),
            }
        }
    }

    impl Prompter for ScriptedPrompter {
        fn input(&mut self, label: &str, default: Option<&str>) -> Result<String, PromptError> {
            self.calls.push(PromptCall::Input {
                label: label.to_owned(),
                default: default.map(str::to_owned),
            });
            match self.answers.pop_front() {
                None => Err(PromptError::EmptyScript),
                Some(PromptAnswer::Input(s)) => Ok(s),
                Some(other) => panic!(
                    "ScriptedPrompter: prompt::input called for '{label}' but next \
                     scripted answer is {other:?}",
                ),
            }
        }

        fn password(&mut self, label: &str) -> Result<String, PromptError> {
            self.calls.push(PromptCall::Password {
                label: label.to_owned(),
            });
            match self.answers.pop_front() {
                None => Err(PromptError::EmptyScript),
                Some(PromptAnswer::Password(s)) => Ok(s),
                Some(other) => panic!(
                    "ScriptedPrompter: prompt::password called for '{label}' but \
                     next scripted answer is {other:?}",
                ),
            }
        }

        fn confirm(&mut self, label: &str, default: bool) -> Result<bool, PromptError> {
            self.calls.push(PromptCall::Confirm {
                label: label.to_owned(),
                default,
            });
            match self.answers.pop_front() {
                None => Err(PromptError::EmptyScript),
                Some(PromptAnswer::Confirm(b)) => Ok(b),
                Some(other) => panic!(
                    "ScriptedPrompter: prompt::confirm called for '{label}' but \
                     next scripted answer is {other:?}",
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scripted::PromptCall;

    // ---------- validate_url ----------

    #[test]
    fn validate_url_accepts_https() {
        assert!(validate_url("https://example.com").is_ok());
    }

    #[test]
    fn validate_url_accepts_localhost_with_port() {
        assert!(validate_url("https://localhost:9000").is_ok());
        assert!(validate_url("http://localhost:9000/path").is_ok());
    }

    #[test]
    fn validate_url_accepts_s3_scheme() {
        // `url::Url::parse` accepts non-http schemes.
        assert!(validate_url("s3://my-bucket/prefix").is_ok());
    }

    #[test]
    fn validate_url_accepts_arbitrary_scheme() {
        // `url::Url::parse` does not filter unknown schemes — `htps`
        // parses as scheme=`htps` and the URL is syntactically
        // valid. Pinning the behaviour here so a future regression
        // (or a scheme-allowlist change) is caught. See design 003
        // §URL validation "Documented limitation" block.
        assert!(validate_url("htps://typo.example.com").is_ok());
    }

    #[test]
    fn validate_url_rejects_unparseable() {
        assert!(validate_url("not a url at all").is_err());
    }

    #[test]
    fn validate_url_rejects_empty_string() {
        assert!(validate_url("").is_err());
    }

    #[test]
    fn validate_url_rejects_missing_scheme() {
        // Design 003 §URL validation names this specifically:
        // `vault.local` without a scheme is the trip-wire.
        assert!(validate_url("vault.local").is_err());
    }

    // ---------- ScriptedPrompter ----------

    #[test]
    fn scripted_prompter_drives_input_and_confirm_in_order() {
        let mut p = ScriptedPrompter::new([
            PromptAnswer::Input("foo".into()),
            PromptAnswer::Confirm(true),
        ]);
        assert_eq!(p.input("First label", None).unwrap(), "foo");
        assert!(p.confirm("Second label", false).unwrap());
        assert_eq!(p.calls.len(), 2);
        match &p.calls[0] {
            PromptCall::Input { label, default } => {
                assert_eq!(label, "First label");
                assert_eq!(default.as_deref(), None);
            }
            other => panic!("expected Input, got {other:?}"),
        }
        match &p.calls[1] {
            PromptCall::Confirm { label, default } => {
                assert_eq!(label, "Second label");
                assert!(!default);
            }
            other => panic!("expected Confirm, got {other:?}"),
        }
    }

    #[test]
    fn scripted_prompter_empty_queue_returns_empty_script_error() {
        let mut p = ScriptedPrompter::new([]);
        let err = p.input("unanswered", None).expect_err("must fail");
        assert!(matches!(err, PromptError::EmptyScript));
        // The recorded-calls log still captured the attempted prompt,
        // so the test can identify what was expected next.
        assert_eq!(p.calls.len(), 1);
        match &p.calls[0] {
            PromptCall::Input { label, .. } => assert_eq!(label, "unanswered"),
            other => panic!("expected Input, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "ScriptedPrompter: prompt::confirm called for")]
    fn scripted_prompter_type_mismatch_panics() {
        let mut p = ScriptedPrompter::new([PromptAnswer::Input("oops".into())]);
        let _ = p.confirm("expected-input-but-got-confirm", false);
    }

    #[test]
    fn scripted_prompter_password_records_call() {
        let mut p = ScriptedPrompter::new([PromptAnswer::Password("secret".into())]);
        assert_eq!(p.password("token").unwrap(), "secret");
        assert!(matches!(
            &p.calls[0],
            PromptCall::Password { label } if label == "token",
        ));
    }

    #[test]
    fn scripted_prompter_input_default_recorded() {
        let mut p = ScriptedPrompter::new([PromptAnswer::Input("custom".into())]);
        assert_eq!(p.input("label", Some("default-value")).unwrap(), "custom");
        match &p.calls[0] {
            PromptCall::Input { default, .. } => {
                assert_eq!(default.as_deref(), Some("default-value"));
            }
            other => panic!("expected Input, got {other:?}"),
        }
    }
}
