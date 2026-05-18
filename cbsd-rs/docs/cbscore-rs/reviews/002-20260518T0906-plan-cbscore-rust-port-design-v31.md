# Plan Review v31 — Pre-Implementation Audit Pass 11 Closure Confirmation

**Review target:** seq-002 plan corpus (Phase 1 + Phase 2)\
**Commit under review:** `bfc5951`\
**Reviewer:** Staff Engineer (design-reviewer agent)\
**Date:** 2026-05-18

---

## §Scope

Focused confirmation review of the 2 pre-implementation audit pass-11 findings
(K1, K2) claimed closed in commit `bfc5951`. Also confirms no-drift on three
structural invariants established by passes 1–10. A `prettier --check` pass on
both edited files is included.

## §Method

For each finding, the closure text was located directly in the current plan file
at the relevant commit section. Quoted phrases are verified verbatim; line
references are recorded where the text lands. The no-drift checks read the live
plan corpus state — not git diff — and compare against the known-good baselines
recorded in the v30 review and project memory.

---

## §Closure Verification

### K1 — Phase 1 C2 `RunnerError` block adds `BinaryNotFound { source: std::io::Error }` + pinned Display text

**Claimed change:** Phase 1 Commit 2 `RunnerError` variant list gains
`BinaryNotFound { source: std::io::Error }` and the pinned Display string
(`"could not locate the cbsbuild binary on disk: {source}"`).

**Sub-check (a): variant in the `RunnerError` enum.**

Phase 1 Commit 2 §Files `runner/errors.rs` bullet reads:

> `BinaryNotFound { source: std::io::Error }` (Phase 4 Commit 3's runner calls
> `std::env::current_exe()` to find its own host-side path for the
> cbsbuild-binary self-mount; on failure this variant carries the underlying
> `io::Error` for diagnostic chain traversal)

Present at lines 316–319. **Sub-check (a): Closed.**

**Sub-check (b): pinned Display text.**

The §Design rules "Operator-facing Display text" block reads:

> `RunnerError::BinaryNotFound { source }`:
> `"could not locate the cbsbuild binary on disk: {source}"`.

Present at lines 349–350. **Sub-check (b): Closed.**

**Finding K1: Closed.**

---

### K2 — Phase 2 C2 `podman_stop` signature + `None → --all` branch + §Testable both-branch coverage

**Claimed change:** Phase 2 Commit 2 §Files `podman.rs` bullet names
`podman_stop(name: Option<&str>, timeout: Duration)`, documents the
`None → --all` branch with a Python 1:1 alignment note, and §Testable gains a
bullet exercising both `Some(...)` and `None` forms with expected command-line
tokens.

**Sub-check (a): function signature with `Option<&str>` parameter.**

Phase 2 Commit 2 §Files reads:

> `podman_stop(name: Option<&str>, timeout: Duration) -> Result<(), PodmanError>`
> (when `name` is `Some(n)`, emits `podman stop --time <secs> <n>`; when `name`
> is `None`, emits `podman stop --time <secs> --all`

Present at lines 144–147. `Option<&str>` is spelled out explicitly. **Sub-check
(a): Closed.**

**Sub-check (b): Python 1:1 alignment note.**

The prose continues:

> matches Python `cbscore/utils/podman.py:podman_stop`'s optional-name behaviour
> 1:1, so Phase 4 Commit 2's `runner::stop(None, …)` delegates directly without
> routing through a second helper

Present at lines 148–150. **Sub-check (b): Closed.**

**Sub-check (c): §Testable covers both `Some(...)` and `None` branches.**

Phase 2 Commit 2 §Testable reads:

> `podman_stop` command construction covers both branches:
> `podman_stop(Some("ces_abcdef0123"), Duration::from_secs(1))` produces
> `podman stop --time 1 ces_abcdef0123`;
> `podman_stop(None, Duration::from_secs(1))` produces
> `podman stop --time 1 --all`. Exercises the Option-typed name path Phase 4
> Commit 2's `runner::stop` wraps for the `--all` form.

Present at lines 175–180. Both `Some` and `None` forms are present with the
expected command-line tokens stated verbatim. **Sub-check (c): Closed.**

**Finding K2: Closed.**

---

## §No-Drift Spot Checks

Three structural invariants from passes 1–10 were spot-checked against the live
plan corpus state.

| Invariant                                                                                                                                                                                 | Expected                                                                                                                                                                                                                                                    | Observed                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | Status |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| `BinaryNotFound` declared exactly once (Phase 1 C2); Display text matches Phase 4 C3's spec                                                                                               | Declared in `runner/errors.rs` variant list (Phase 1 C2) with Display `"could not locate the cbsbuild binary on disk: {source}"`; Phase 4 C3 §Design constraints references the variant by name but does not re-declare it                                  | Phase 1 C2 lines 316–319 carry the sole variant declaration; Phase 4 C3 lines 437–441 reference it as `RunnerError::BinaryNotFound { source: std::io::Error }` with the Display text `"could not locate the cbsbuild binary on disk: {source}"` — identical spelling, no duplicate declaration; `grep -rn BinaryNotFound` across all plan files returns exactly 4 hits, all consistent: 2 declaration-or-pinned-text lines in Phase 1 C2 and 2 reference-only lines in Phase 4 C3 | PASS   |
| `podman_stop` signature in Phase 2 C2 reads `name: Option<&str>` and `--all` branch is documented; Phase 4 C2's wrapper still says `runner::stop(None, ...) → podman stop --time 1 --all` | Phase 2 C2 signature is `podman_stop(name: Option<&str>, timeout: Duration)`; Phase 4 C2 `stop` wrapper reads `pub async fn stop(name: Option<&str>, timeout: Duration)` and documents `None → stops all cbscore-prefixed containers via podman stop --all` | Phase 2 C2 line 144–145 carries the `Option<&str>` signature; Phase 4 C2 lines 167–170 carry `pub async fn stop(name: Option<&str>, timeout: Duration)` with the `None → podman stop --all` prose — type and semantics are consistent across both plan files                                                                                                                                                                                                                      | PASS   |
| No new dangling references introduced by K1/K2 edits                                                                                                                                      | All corpus references to `BinaryNotFound` and `podman_stop` resolve to existing declarations; no stale variant names or mismatched signatures                                                                                                               | `grep -rn BinaryNotFound` across `plans/`: 4 hits in 2 files (Phase 1 + Phase 4), all consistent; `grep -rn podman_stop` across `plans/`: 14 hits across 4 files (Phase 2, Phase 4, Phase 7, Phase 4 worker-cutover); all call-site uses of `podman_stop` in Phase 4 and Phase 7 pass `name=cid` (`Some` form) or `None` form, consistent with the `Option<&str>` signature in Phase 2 C2                                                                                         | PASS   |

---

## §Formatting

`prettier --check` on both files modified in commit `bfc5951`:

```
prettier --check \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-02-subprocess-and-shell-tools.md

All matched files use Prettier code style!
```

Exit code: 0.

---

## §Verdict

> **Approve — K1+K2 (2 findings) closed; pre-impl audit pass 11 fully resolved;
> plan corpus ready for Phase 1 implementation start.**
