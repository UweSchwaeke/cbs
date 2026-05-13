# Plan Review — cbscore Rust Port Design v18

**Date:** 2026-05-13\
**Reviewer:** Staff Engineer (design-reviewer agent)\
**Commit reviewed:** `e3cb122`\
**Scope:** Confirmation pass — v17 findings N1, N2, S1, S2, Q1

---

## Scope

This is a focused confirmation review. Commit `e3cb122` claimed to close all
five v17 findings. The goal is to verify:

1. Each closure is present at the cited location with the expected language.
2. No prior hedge or ambiguous phrasing survives.
3. No new drift was introduced.
4. Cross-plan consistency between Phase 2 and seq-004 remains tight.
5. Prettier passes on all three edited files.
6. The v15/v16 closures confirmed in prior passes have not been accidentally
   reverted.

Files in scope:

- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-02-subprocess-and-shell-tools.md`
  (N1 closure)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-06-cbsbuild-cli.md` (S2
  closure)
- `cbsd-rs/docs/cbscore-rs/plans/004-20260513T0900-configurable-version-descriptor-location.md`
  (N2, S1, Q1 closures)

---

## Method

1. Read `git show e3cb122 --stat` to confirm exactly three files were touched.
2. Read the full diff for each file to inspect each closure verbatim.
3. Ran `grep -n "or wherever"` and `grep -n "if portable"` across all three
   files — zero matches.
4. Ran `grep -n "pick the placement"` and `grep -n "likely cbscore-types"` on
   seq-004 — zero matches.
5. Verified the surviving `layering violation` occurrence (seq-004 line 171) is
   the affirmative closure statement ("no layering violation occurs"), not a
   hedge.
6. Cross-checked seq-004's `cbscore::utils::git::repo_root` reference (Depends
   On §) against Phase 2 Commit 4's new explicit `repo_root` parenthetical.
7. Cross-checked Phase 6 Commit 4's deferral language ("design 004 Migration
   step 5 … deferred to seq-003 (post-M1)") against design 004 §Migration table
   row 5, which reads: "Steps 1–4 land in M1. Step 5 lands when design 003 is
   implemented post-M1." Language is consistent.
8. Spot-checked
   `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-07-worker-cutover.md` for
   the v15/v16 closure items: config.rs three-field disposition,
   `./cbsd-rs/scripts:/opt/cbsd-rs:ro` bind-mount path, operator silent-ignore
   - changelog language, `FROM alpine:3.21` target, six-step worktree procedure,
     and README "~25–31 commits" estimate. All intact.
9. Spot-checked design 005's §Design Sketch opening paragraph for the v2
   MINOR-N1 closure (patch-walker guard enumerated). Intact.
10. Ran `prettier --check` on all three edited files — all pass.

---

## Closed Findings Confirmed

### N1 (MINOR) — `repo_root` named explicitly in Phase 2 Commit 4 §Files

**Location:** `002-…-02-subprocess-and-shell-tools.md`, Phase 2 Commit 4 §Files,
git.rs bullet.

**Prior text:** "…`git_branch_show_current`, `git_rev_parse`, etc. Match the
Python signatures."

**Current text:** "…`git_branch_show_current`, `git_rev_parse`, `repo_root`
(Rust name for Python's `get_git_repo_root`; seq-004 Commit 2's `resolve_root`
depends on this exact name), etc. Match the Python signatures."

**Verdict: CLOSED.** The function name is locked at `repo_root`, the Python
source name is noted, and the cross-plan dependency is called out explicitly.
The parenthetical is surgical — no surrounding text was altered.

---

### N2 (MINOR) — `VersionError::NoDescriptorRoot` placement pinned

unambiguously

**Location:** `004-…-configurable-version-descriptor-location.md`, Commit 2
§Files, errors.rs bullet.

**Prior text:** "…likely `cbscore-types/src/versions/errors.rs` per the
error-taxonomy split, but the variant text needs the cwd context; pick the
placement that lets the cwd field render in `Display` without a layering
violation…"

**Current text:** "`cbsd-rs/cbscore-types/src/versions/errors.rs` — add
`NoDescriptorRoot { cwd: Utf8PathBuf }` variant to `VersionError` (which already
lives in `cbscore-types` per Phase 1 Commit 2's error taxonomy) and implement
its `Display` arm in the same file. `Utf8PathBuf` is already a dep of
`cbscore-types` via `camino` (Phase 1 Commit 1); rendering `cwd` is pure string
formatting that does **not** call any `cbscore` IO function, so no layering
violation occurs."

**Verdict: CLOSED.** File path is unambiguous; no "or wherever" / "pick the
placement" hedge survives. The rationale (camino already a dep; Display is pure
string formatting) is stated explicitly as the v17 review required.

---

### S1 (SUGGESTION) — Delete-cwd test gated `#[cfg(target_os = "linux")]` with

concrete procedure

**Location:** `004-…-configurable-version-descriptor-location.md`, Commit 2
§Testable, last test bullet.

**Prior text:** "Unit test: when `current_dir()` itself fails (simulate by
deleting the cwd in the test thread, if portable, or by passing a sentinel
through a test-only hook), the cwd renders as `<unknown>` rather than
panicking."

**Current text:** "Unit test (`#[cfg(target_os = "linux")]`): when
`current_dir()` itself fails, the cwd renders as `<unknown>` rather than
panicking. Simulate the failure by creating a temp directory, `cd`-ing into it,
`rmdir`-ing it, then calling `resolve_root` — on Linux, `getcwd(2)` returns
`ENOENT` against a deleted cwd and Rust's `std::env::current_dir()` propagates
that as `Err`. Non-Linux platforms behave differently here; the test is gated on
Linux rather than coded for portability. The `<unknown>` rendering path is
otherwise trivially correct by inspection of the `unwrap_or_else` chain."

**Verdict: CLOSED.** `#[cfg(target_os = "linux")]` gate is explicit. The
`tempdir → cd → rmdir → call resolve_root` procedure is concrete and self-
contained. The "if portable" hedge is gone. The non-Linux rationale is
documented. The assertion on `getcwd(2)` returning `ENOENT` is correct: Linux
does not invalidate open directory file descriptors on unlink, but a deleted cwd
does produce `ENOENT` from `getcwd(2)` because the directory entry is gone from
the parent.

---

### S2 (SUGGESTION) — Phase 6 Commit 4 bypass-mode pre-fill explicitly excludes

`versions`

**Location:** `002-…-06-cbsbuild-cli.md`, Commit 4 §Design constraints.

**Prior text:** "…match design 004 §Bypass-mode pre-fill and the corresponding
Python defaults."

**Current text:** "…match design 004 §Bypass-mode pre-fill and the corresponding
Python defaults — **excluding** the `versions` field (`/cbs/_versions`), which
is design 004 Migration step 5 and is deferred to seq-003 (post-M1). Commit 4's
generated config carries `paths.versions = None`; the M1 surface is complete
without the versions pre-fill."

**Cross-plan consistency check:** Design 004 §Migration table row 5 reads: "Add
the optional 'Versions path' prompt. Add `versions = "/cbs/_versions"` to the
bypass-mode pre-fill set. … Steps 1–4 land in M1. Step 5 lands when design 003
is implemented post-M1." The plan's deferral language ("design 004 Migration
step 5 … deferred to seq-003 (post-M1)") matches the design exactly.

**Verdict: CLOSED.** The exclusion is named, the field value is noted, the
deferral target (seq-003 / post-M1) is correct, and `paths.versions = None` is
stated as the M1 outcome. No ambiguity survives.

---

### Q1 (OPEN QUESTION) — `resolve_root` placed in a new `versions/resolve.rs`

sub-module

**Location:** `004-…-configurable-version-descriptor-location.md`, Commit 2
§Files, resolve_root bullet.

**Prior text:** "`cbsd-rs/cbscore/src/versions/mod.rs` (or wherever
`cbscore::versions` is rooted after Phase 4) — add `pub async fn resolve_root…`"

**Current text:** "`cbsd-rs/cbscore/src/versions/resolve.rs` — new sub-module
carrying the resolver. `cbsd-rs/cbscore/src/versions/mod.rs` gains
`pub mod resolve;` plus `pub use resolve::resolve_root;` so callers reach it as
`cbscore::versions::resolve_root`. This file-per-IO-function layout matches
`versions/desc.rs` (Phase 4 Commit 1's `read_descriptor`) and
`versions/create.rs` (Phase 6 Commit 2's `version_create_helper`)."

**Verdict: CLOSED.** The file path is pinned to `versions/resolve.rs`. The
`mod.rs` additions (`pub mod resolve;` + `pub use resolve::resolve_root;`) are
specified. The justification (file-per-IO-function pattern matching desc.rs and
create.rs) is cited. The "or wherever" hedge is gone. The `mod.rs` reference in
the closure ("mod.rs gains `pub mod resolve;`") is not ambiguous: it refers to
`cbscore/src/versions/mod.rs`, which is the entry point for the `versions`
sub-module tree and the same file that gains `pub mod desc;` in Phase 4 Commit

1. No confusion possible.

---

## Findings

None. No new issues were identified.

---

## Prior Closures Intact

The following closures confirmed in v15/v16 were spot-checked and remain intact
in `002-…-07-worker-cutover.md`:

- **v16 N1/Q1:** config.rs three-field disposition (`cbscore_wrapper_path`
  removed, `cbscore_config_path` role-change documented,
  `sigkill_escalation_timeout_secs` removed with RAII rationale). Operator
  silent-ignore + M2 changelog language present.
- **v16 N2:** Bind-mount `./cbsd-rs/scripts:/opt/cbsd-rs:ro` named;
  `FROM worker-base AS cbsd-rs-worker` → `FROM alpine:3.21 AS cbsd-rs-worker`
  target change documented.
- **v16 N3:** README reads "~25–31 commits across 7 phases".
- **v16 S1:** Six-step worktree procedure in Commit 3 §Testable; "pre-M2
  reference build failed" failure message present; toolchain/image consistency
  guarantee stated.

Design 005 v2 MINOR-N1 (§Design Sketch opening paragraph enumerating the
patch-walker guard) remains intact.

---

## Verdict

**Approve — v17 N1+N2+S1+S2+Q1 all closed; plan corpus + seq-004 ready for
implementation.**

All five closures are present at the cited locations, contain the expected
language, and retire every hedge or ambiguous phrasing flagged in v17. The
surgical nature of the edits is confirmed: three files touched, no structural
changes, no commit-order changes, no test-coverage changes. Cross-plan
consistency between Phase 2 (`repo_root`) and seq-004
(`cbscore::utils::git::repo_root`) is tight. The Phase 6 bypass-pre-fill
deferral language in plan 002-06 aligns exactly with design 004 §Migration table
row 5. Prettier passes on all three files. No new findings.

The plan corpus (seq-002 phases 1–7, seq-004) is implementation-ready with zero
open findings.

---

| ID  | Severity   | Summary                             | Status |
| --- | ---------- | ----------------------------------- | ------ |
| N1  | MINOR      | `repo_root` name locked in P2 C4    | CLOSED |
| N2  | MINOR      | `NoDescriptorRoot` placement pinned | CLOSED |
| S1  | SUGGESTION | Delete-cwd test gated Linux         | CLOSED |
| S2  | SUGGESTION | Bypass pre-fill excludes `versions` | CLOSED |
| Q1  | OPEN       | `resolve_root` sub-module placement | CLOSED |
