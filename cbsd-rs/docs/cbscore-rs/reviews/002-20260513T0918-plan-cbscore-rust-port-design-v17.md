# Plan Review v17 — cbscore Rust port (plan 002 + plan 004)

**Scope:** Corpus review. Verifies (A) that the seq-002 plan corpus remains at
the v16-approved state (regression check on Phases 1–7 + README), and (B) that
the newly-drafted seq-004 plan is correct, complete, and ready for
implementation.

**Files reviewed:**

- `cbsd-rs/docs/cbscore-rs/plans/README.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md` through
  `002-20260508T1558-07-worker-cutover.md`
- `cbsd-rs/docs/cbscore-rs/plans/004-20260513T0900-configurable-version-descriptor-location.md`

**Designs audited against:**

- `cbsd-rs/docs/cbscore-rs/design/001-20260418T2045-cbscore-project-structure.md`
- `cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`
- `cbsd-rs/docs/cbscore-rs/design/003-20260427T1255-interactive-config-init.md`
- `cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md`
- `cbsd-rs/docs/cbscore-rs/design/005-20260504T1145-optional-version-on-versions-create.md`

**Convention files:**

- `cbsd-rs/docs/cbscore-rs/CLAUDE.md`
- `cbsd-rs/docs/CLAUDE.md`

**Prior review:** `002-20260513T0839-plan-cbscore-rust-port-design-v16.md`

---

## 1. Scope

### A — seq-002 regression check

Spot-checked Phases 1–7 and the README for regression against the v16-approved
state. Verified the five v15 closure items (N1/N2/N3/S1/Q1) confirmed by v16
remain intact. No commit since v16 has touched Phases 1–6; Phase 7 and README
are unchanged from the commit `31ce251` verified in v16.

### B — seq-004 first review

Full review of `004-20260513T0900-configurable-version-descriptor-location.md`
against design 004, the seq-002 phase plans it depends on, and the convention
files.

---

## 2. Method

1. Read the seq-004 plan end-to-end; cross-checked every design-constraint claim
   against design 004's Resolved Decisions (OQ1–OQ7) and Migration table.
2. Verified each §Depends on bullet against the named phase plan file.
3. Checked the §Sequencing cross-reference against Phase 6 plan lines 67–77
   (confirmed exact match).
4. Checked for double-coverage: scanned Phase 1 for `paths.versions`, Phase 6
   Commit 2 for `--versions-dir` (neither present — clean separation confirmed).
5. Verified Python source cross-reference (`versions.py:88`) against the live
   file.
6. Ran `prettier --check` on `004-…-configurable-version-descriptor-location.md`
   and `README.md`. Both returned "All matched files use Prettier code style!"
7. Verified seq-002 corpus unmodified since v16 via grep + diff.

---

## 3. Closed Findings Confirmed (carry-forward from v16)

All five v15 findings confirmed closed by v16 remain intact:

| V15 ID | Description                                         | V17 Status |
| ------ | --------------------------------------------------- | ---------- |
| N1     | `config.rs` field disposition absent from C1 §Files | CLOSED     |
| N2     | Commit 2 §Files imprecise (volume + FROM target)    | CLOSED     |
| N3     | README total estimate "~25–30" (should be "~25–31") | CLOSED     |
| S1     | git worktree procedure unspecified                  | CLOSED     |
| Q1     | Operator YAML migration language missing            | CLOSED     |

No seq-002 phase file was modified since commit `31ce251`. Phases 1–7 and the
README are free of regression.

---

## 4. Findings

### MINORS

#### N1 — `repo_root` function name unconfirmed in Phase 2 Commit 4

**Location:** seq-004 §Depends on bullet 2; seq-004 Commit 2 §Files; Phase 2
Commit 4 §Files.

**Issue:** The seq-004 §Depends on bullet says "seq-002 Phase 2 Commit 4 —
`cbscore::utils::git::repo_root` exists." Commit 2 §Files calls the same
function `cbscore::utils::git::repo_root`. Phase 2 Commit 4 §Files lists
`git_ls_remote`, `git_clone`, `git_fetch`, `git_describe`, `git_switch`,
`git_branch_show_current`, `git_rev_parse`, and "etc." — it does not explicitly
name `repo_root`. The Python original is `get_git_repo_root()` (verified at
`cbscore/utils/git.py:90`); the Rust name is unspecified in the Phase 2 plan.

**Risk:** If the Phase 2 implementer names the function `git_repo_root` (the
more mechanical port of `get_git_repo_root`) rather than `repo_root`, the
seq-004 Commit 2 import path will be wrong and the compile step will fail at
that commit. The "etc." in Phase 2 does not lock the name.

**Resolution:** Either (a) add `repo_root` (or `git_repo_root`, with a note on
the chosen name) to Phase 2 Commit 4 §Files explicitly, or (b) add a
parenthetical to seq-004 §Depends on bullet 2: "(`repo_root` or the Rust
equivalent of Python's `get_git_repo_root`; verify the actual function name
against Phase 2 Commit 4 when implementing)." Option (b) is lower-friction since
the seq-002 plan is already approved.

---

#### N2 — `VersionError::NoDescriptorRoot` placement left as a decision for the implementer

**Location:** seq-004 Commit 2 §Files, `versions/errors.rs` bullet.

**Issue:** The plan says: "likely `cbscore-types/src/versions/errors.rs` per the
error-taxonomy split, but the variant text needs the cwd context; pick the
placement that lets the cwd field render in `Display` without a layering
violation." This is an open implementation decision deferred to commit time.

The problem the plan is trying to avoid is real: `VersionError` lives in
`cbscore-types` (established in Phase 1 Commit 2), but adding
`NoDescriptorRoot { cwd: Utf8PathBuf }` to that type and implementing `Display`
in `cbscore-types` is entirely valid. `Utf8PathBuf` is already a dep of
`cbscore-types` (camino), and `Display` can render `cwd` directly. A layering
violation would only occur if the `Display` impl needed to call `cbscore`
functions — it does not. The four-line OQ5 error text is pure string formatting.

Leaving this ambiguous risks an implementer placing `NoDescriptorRoot` in
`cbscore/src/versions/errors.rs` — a separate errors file that does not exist in
the plan — and then having to reconcile `VersionError` from two locations at the
call sites.

**Resolution:** State the decision explicitly in Commit 2 §Files: "Add
`NoDescriptorRoot { cwd: Utf8PathBuf }` to
`cbscore-types/src/versions/errors.rs` and implement `Display` there. The
`Utf8PathBuf` field renders directly in `Display` without importing any
`cbscore` function; no layering violation occurs." This matches the existing
`VersionError` home (Phase 1 Commit 2) and eliminates the implementer decision
point.

---

### SUGGESTIONS

#### S1 — `delete-cwd` test for `<unknown>` fallback is likely non-portable

**Location:** seq-004 Commit 2 §Testable, last bullet.

**Issue:** The plan says to simulate `current_dir()` failure by "deleting the
cwd in the test thread, if portable." Deleting the working directory under a
live process works on Linux (the directory becomes inaccessible but the process
continues; `std::env::current_dir()` returns `ENOENT`). On macOS and other
platforms the behaviour differs. Marking the test `#[cfg(target_os = "linux")]`
or using a test-only hook (a `#[cfg(test)]` injectable error path) is more
reliable.

**Suggestion:** Replace "if portable, or by passing a sentinel through a
test-only hook" with a concrete recommendation: use a `#[cfg(test)]` injectable
closure or a `#[cfg(target_os = "linux")]` gate on the cwd-deletion test. The
`<unknown>` rendering path is trivially correct by inspection once the
`unwrap_or_else` is present; a comment is a reasonable substitute for a
potentially brittle test.

---

#### S2 — Phase 6 Commit 4 bypass constraint slightly overstates design 004 coverage

**Location:** Phase 6 plan (`002-…-06-cbsbuild-cli.md`) Commit 4 §Design
constraints, not the seq-004 plan.

**Issue (pre-existing, not introduced by seq-004):** Commit 4 §Design
constraints says the bypass-mode pre-fill "match design 004 §Bypass-mode
pre-fill and the corresponding Python defaults." Design 004 §Bypass-mode
pre-fill adds `versions = /cbs/_versions` to the pre-fill set, but that addition
is explicitly design 004 Step 5 (post-M1, owned by seq-003). Phase 6 Commit 4
will not include it. The cited reference
(`design 004 §Bypass-mode pre-fill line 358`) therefore overstates what Commit 4
actually delivers.

This inconsistency is pre-existing in the seq-002 plan and was not introduced by
seq-004. It cannot cause a compile or test failure (the bypass pre-fill omits
the `versions` field, which is `Option` and defaults to `None`). Raising it here
for completeness; flagged as a suggestion, not a blocker.

**Suggestion:** Add a clarifying parenthetical to Phase 6 Commit 4 §Design
constraints: "…match design 004 §Bypass-mode pre-fill (excluding the `versions`
field, which is deferred to seq-003 Step 5; `paths.versions` will be absent —
i.e. `None` — in Commit 4's generated config)."

---

### OPEN QUESTIONS

#### Q1 — `cbscore::versions::mod.rs` vs a sub-module for `resolve_root`

**Location:** seq-004 Commit 2 §Files.

**Issue:** The plan places `resolve_root` in "`cbscore/src/versions/mod.rs` (or
wherever `cbscore::versions` is rooted after Phase 4)." Phase 2 Commit 5 creates
`cbscore/src/versions/mod.rs` with `pub mod utils`. Phase 4 Commit 1 creates
`cbscore/src/versions/desc.rs` and adds `pub mod desc` to that same `mod.rs`.
Phase 6 Commit 2 adds `pub mod create`. The plan's fallback "or wherever
`cbscore::versions` is rooted" is adequate, but the parenthetical raises a
question that won't be answerable until Phase 4 is done.

If `cbscore::versions::mod.rs` already carries IO-bearing functions after Phase
4 and Phase 6 (`desc`, `create`), placing `resolve_root` there is consistent. If
the Phase 4/6 implementer instead roots desc and create as sub-modules and keeps
`mod.rs` as a pure re-export facade, `resolve_root` still has a natural home as
a top-level function in `mod.rs`.

No action required: the hedged language is correct. Noted here so the seq-004
implementer knows to read Phase 4 Commit 1 and Phase 6 Commit 2 before placing
the function.

---

## 5. seq-004 Positive Findings

The following audit items are confirmed clean:

1. **Migration table coverage:** Steps 1–4 correctly mapped to Commits 1–3; Step
   5 correctly excluded and attributed to seq-003 / post-M1.
2. **OQ1 (config + CLI flag + precedence):** Both surfaces present; precedence
   order CLI > config > fallback is consistent across §Goal, Commit 2, and
   Commit 3. CLI flag named `--versions-dir`, config field
   `Config.paths.versions`.
3. **OQ2 (Python-parity fallback):** `<git-root>/_versions` fallback described
   correctly in Commit 2 and confirmed in End-of-feature acceptance test 1.
4. **OQ3 (`<root>/<type>/<VERSION>.json` layout):** `descriptor_path` is the
   single encoding; Commit 3 removes the old hardcoded chain and replaces with
   the helper call.
5. **OQ4 (read sites stay explicit-path):** §Out of scope block correctly names
   read-side auto-discovery as rejected. No reader modification described
   anywhere in the plan.
6. **OQ5 (friendly error text):** Four-line error message quoted verbatim in
   Commit 2 §Files. Display test listed in §Testable.
7. **OQ6 (no schema bump):** Named in §Out of scope.
8. **OQ7 (bypass pre-fill deferred):** Commit 3 does not touch config init; §Out
   of scope correctly attributes both the interactive prompt and the bypass
   pre-fill to seq-003.
9. **Cross-plan sequencing:** Phase 6 §Out of scope lines 67–77 and seq-004
   §Sequencing are mutually consistent; slip-handling fallback is described in
   both directions.
10. **No double-coverage:** Phase 1 §Out of scope explicitly defers
    `Config.paths.versions` to seq-004. Phase 6 Commit 2 §Design constraints
    explicitly states the hardcoded path will be refactored by seq-004. The
    `--versions-dir` flag appears nowhere in Phase 6 Commit 2.
11. **Python FIXME cross-reference:** `cbscore/src/cbscore/cmds/versions.py:88`
    confirmed accurate. Line 88 is
    `repo_path.joinpath("_versions") # FIXME: make this configurable`.
12. **File paths consistent with workspace layout:** All three Commit 1 file
    paths (`cbscore-types/src/config/paths.rs`,
    `cbscore-types/src/versions/utils.rs`, `cbscore-types/src/versions/desc.rs`)
    are consistent with design 001 and the Phase 1 plan.
13. **LOC estimate realistic:** ~500 LOC over 3 commits for ~50 LOC of net new
    Rust plus tests is consistent with design 004's sketch; Commit 1 at ~120 LOC
    is below the 200-line floor but justified (pure-type additions with three
    testable items — consistent with Phase 2 Commit 3 and Phase 4 Commit 1
    precedents).
14. **Tracing target:** `"cbscore::versions"` in Commit 2 is consistent with
    design 002 §Logging convention (line 1194: "Each module sets its target").
15. **`&'static str` vs `&str`:** The plan's
    `as_dir_name(&self) -> &'static str` is more precise than the design
    sketch's `&str`. Both are correct; the plan's choice is the better
    implementation shape for a `match` over enum variants returning string
    literals.
16. **Prettier compliance:** `prettier --check` passes on both
    `004-…-configurable-version-descriptor-location.md` and `README.md`.
17. **README seq-004 bullet:** Updated correctly; links to the plan file with
    accurate commit count (3) and LOC (~500).
18. **Plan structure parity with seq-002:** §Progress, §Goal, §Depends on, §Out
    of scope, per-commit §Files / §Design constraints / §Testable,
    §End-of-feature acceptance all present. Tracking table format (|#|Commit|
    ~LOC|Status|) matches seq-002 phase files exactly.

---

## 6. seq-002 Regression Summary

All seven phases and the README are free of regression. No file in the seq-002
corpus has been modified since commit `31ce251` (v16 closure). The v16-approved
state is intact.

---

## 7. Verdict

**Approve — seq-002 corpus + seq-004 plan ready for implementation.**

The seq-002 plan corpus remains at the v16-approved state with no regression.
The seq-004 plan is well-structured, internally consistent, and faithfully
implements design 004 steps 1–4. Two minor findings (N1 function-name precision,
N2 error-placement ambiguity) and one suggestion (S1 test portability) are
present; none is a blocker. N1 and N2 are resolvable with one-sentence edits per
finding; both can be applied inline when implementing Commit 1 and Commit 2
respectively without re-review.

**Finding counts:** 0 blockers, 0 major concerns, 2 minors, 2 suggestions, 1
open question.

| ID  | Severity   | Description                                                 | Action       |
| --- | ---------- | ----------------------------------------------------------- | ------------ |
| N1  | MINOR      | `repo_root` function name not locked in Phase 2 Commit 4    | Fix inline   |
| N2  | MINOR      | `NoDescriptorRoot` placement left as open decision          | Fix inline   |
| S1  | SUGGESTION | `delete-cwd` test portability on non-Linux                  | Optional     |
| S2  | SUGGESTION | Phase 6 C4 bypass constraint overstates design 004 coverage | Optional     |
| Q1  | OPEN       | `resolve_root` placement in `mod.rs` vs sub-module          | Read at impl |
