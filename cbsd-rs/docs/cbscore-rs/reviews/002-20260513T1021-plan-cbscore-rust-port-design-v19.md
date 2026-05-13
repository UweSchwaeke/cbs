# cbscore-rs Plan Corpus Review v19 — §Status Addition Confirmation

## Scope

Narrow confirmation review covering two mechanical commits:

- `7c4be0e` — added `## Status` block to the seq-004 plan
  (`004-20260513T0900-configurable-version-descriptor-location.md`), immediately
  after the H1 title, before `## Progress`.
- `31f94c8` — added an identical `## Status` block to all seven seq-002 phase
  plans (Phase 1 through Phase 7), at the same structural position.

No design files, no implementation files, no review files were part of these
commits. The v19 review's sole purpose is to confirm that the insertions are
drift-free.

## Method

1. `git show <sha> --stat` — confirmed both commits are pure additions (zero
   deletions) across all affected files.
2. Full diff inspection — verified §Status block text, position (after H1,
   before `## Progress`), and blank-line spacing for each of the eight touched
   files.
3. Carry-forward spot-checks — verified every closure tracked since
   v15/v16/v17/v18 (and the seq-004 v1/v2 closures) by grepping the current file
   state against the specific text anchors listed in the v19 scope.
4. Design corpus staleness check —
   `git log --oneline --since="2026-05-13" -- cbsd-rs/docs/cbscore-rs/design/`
   returned no output; no design file has been touched after the v8 (design 005)
   closures.
5. `prettier --check` — run against all nine files (seven seq-002 phase plans +
   README + seq-004 plan); all pass.

## Closed Findings Confirmed (carry-forward from v18 + seq-004 v1/v2)

All closures remain intact in the current working tree. Checked in order:

**seq-002 Phase 7 (Commit 1)**

- `config.rs` three-field disposition: present at line 115.
- §Subscriber layer design subsection (replaces `build/output.rs` pipe reader):
  present at lines 167–171+.

**seq-002 Phase 7 (Commit 2)**

- Alpine `FROM` change (`FROM worker-base AS cbsd-rs-worker` →
  `FROM alpine:3.21 AS cbsd-rs-worker`): present at lines 241–245.
- `./cbsd-rs/scripts:/opt/cbsd-rs:ro` bind-mount removal from `worker-dev`:
  present at line 255.

**seq-002 Phase 6 (Commit 4)**

- Bypass-mode pre-fill **excludes** `versions = /cbs/_versions` (design 004
  Migration step 5, deferred to seq-003): present at lines 316–322.

**seq-002 Phase 2 (Commit 4)**

- `repo_root` explicitly named (Rust name for Python's `get_git_repo_root`;
  seq-004 Commit 2's `resolve_root` depends on it): present at lines 223–224.

**seq-002 Phase 4 (Commit 3)**

- `pub struct RunReport { … pub build_report: Option<serde_json::Value>, … }`:
  present at line 195.

**seq-002 README**

- Total estimate "~25–31 commits across 7 phases": present at line 23.

**seq-004 plan**

- `## Depends on` correctly names **seq-002 Phase 1 Commit 3** and **seq-002
  Phase 4 Commit 1** (the v1 N1 misattribution fix): present at lines 59 and 62.
- No surviving hedges: `or however`, `Use whichever`, `create_dir_all_async` —
  zero matches (the v1 N2 fix).
- `resolve.rs` placement under `cbscore/src/versions/`: present at lines 64
  and 175.
- `cbscore-types/src/versions/errors.rs` placement: present at line 192.
- `#[cfg(target_os = "linux")]` test gate: present at line 238.

## Findings

None.

## Verdict

**Approve — §Status additions are drift-free; plan corpus + seq-004 remain ready
for M1 / M2 implementation.**

Both commits introduced only the agreed §Status blocks at the correct structural
position in each file. No prior content was disturbed. All v15/v16/v17/v18
closures and seq-004 v1/v2 closures remain intact. The design corpus is
untouched since the v8 (design 005) closures. All nine files pass
`prettier --check`.
