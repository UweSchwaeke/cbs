# Plan Review v2 — seq-004: Configurable `VersionDescriptor` Location

**Date:** 2026-05-13\
**Reviewer:** Staff Engineer (design-reviewer agent)\
**Scope:** Closure-verification pass for v1 findings N1 and N2\
**Plan file:**
`cbsd-rs/docs/cbscore-rs/plans/004-20260513T0900-configurable-version-descriptor-location.md`\
**v1
review:**
`cbsd-rs/docs/cbscore-rs/reviews/004-20260513T0955-plan-configurable-version-descriptor-location-v1.md`\
**v1
commit:** `c197927` (two MINORs opened)\
**Fix commit:** `b595554` (closures applied)

---

## Scope

This is a focused closure-verification pass. Scope is the seq-004 plan only.

The review verifies:

1. N1 closed: §Depends on now carries two correctly-attributed bullets for the
   `desc.rs` dependencies (Phase 1 Commit 3 for the cbscore-types side; Phase 4
   Commit 1 for the cbscore IO side).
2. N1 closed: the Phase 4 Commit 1 bullet records that `write_descriptor` calls
   `tokio::fs::create_dir_all` internally.
3. N1 closed: the old misattributed "Phase 4 Commit 1 —
   `cbscore::versions::desc` module exists" wording does not survive.
4. N2 closed: `grep "or however"` returns 0 hits.
5. N2 closed: `grep "Use whichever"` returns 0 hits.
6. N2 closed: `grep "create_dir_all_async"` returns 0 hits.
7. N2 closed: Commit 3 §Files snippet calls
   `cbscore::versions::desc::write_descriptor(&desc, &path).await?` without a
   preceding `mkdir -p` step.
8. No-drift: five v17 closures (N1 `repo_root`, N2 `errors.rs`, S1
   `#[cfg(target_os = "linux")]`, S2 Phase 6 Commit 4 bypass exclusion, Q1
   `resolve.rs` placement) remain intact.
9. No-drift: migration table steps 1–4 coverage intact; step 5 correctly
   excluded.
10. No-drift: all seven OQ resolutions still reflected.
11. No-drift: §Sequencing cross-reference to Phase 6 lines 67–77 still exact.
12. `prettier --check` passes.

---

## Method

1. Read the plan end-to-end at `b595554`.
2. Ran four targeted greps for the N2 hedge phrases:
   - `grep "or however"` → 0 hits (exit 1).
   - `grep "Use whichever"` → 0 hits (exit 1).
   - `grep "create_dir_all_async"` → 0 hits (exit 1).
3. Grepped §Depends on for `Phase 1 Commit 3`, `Phase 4 Commit 1`,
   `cbscore-types/src/versions/desc`, `cbscore/src/versions/desc`,
   `write_descriptor`, `create_dir_all` — confirmed two correctly-attributed
   bullets; confirmed `write_descriptor` → `tokio::fs::create_dir_all` note
   present.
4. Grepped for the old misattributed phrase "Phase 4 Commit 1 —
   `cbscore::versions::desc` module exists" → 0 hits (exit 1).
5. Re-verified all five v17 closures: `repo_root` (Phase 2 Commit 4 named
   explicitly), `errors.rs` (pinned to `cbscore-types/src/versions/errors.rs`),
   `#[cfg(target_os = "linux")]` (concrete tempdir → cd → rmdir procedure
   intact), Phase 6 Commit 4 bypass exclusion (§Sequencing names the interleave
   point; `config init` bypass deferred to seq-003), `resolve.rs` (pinned to
   `cbscore/src/versions/resolve.rs` with `mod.rs` additions specified).
6. Verified migration table steps 1–4 coverage and step 5 exclusion — unchanged
   from v1 confirmation.
7. Verified all seven OQ tags still appear and resolve correctly — unchanged
   from v1 confirmation.
8. Ran `awk 'NR==67,NR==77'` on `002-20260508T1558-06-cbsbuild-cli.md` and
   compared against §Sequencing — exact match confirmed (seq-004 interleave
   bullet plus slip-handling fallback; both directions consistent).
9. Ran `prettier --check` on the plan — **passes** ("All matched files use
   Prettier code style!").

---

## Closed Findings Confirmed

### N1 — §Depends on bullet 4 misattributes `cbscore-types` `desc.rs` to Phase 4

**Status: CLOSED.**

The single misattributed bullet has been replaced by two targeted bullets:

- Line 41: **seq-002 Phase 1 Commit 3** — `cbscore-types/src/versions/desc.rs`
  exists. seq-004 Commit 1 appends `descriptor_path` to this file (and
  `VersionType::as_dir_name` to `versions/utils.rs`).
- Lines 44–51: **seq-002 Phase 4 Commit 1** — `cbscore/src/versions/desc.rs`
  exists (IO side; `read_descriptor` + `write_descriptor`). seq-004 Commit 2's
  `versions/resolve.rs` sits alongside this file; seq-004 Commit 3 calls
  `write_descriptor` from the refactored write path. Phase 4 Commit 1 §Files
  settles that `write_descriptor` calls `tokio::fs::create_dir_all` internally.

The old "cbscore::versions::desc module exists" wording (which implied Phase 4
was a compile dependency for seq-004 Commit 1) is gone. The `create_dir_all`
note is present, feeding the N2 fix. All three sub-checks pass.

### N2 — Commit 3 §Files: two surviving "pick whichever" hedges

**Status: CLOSED.**

- `grep "or however"` → 0 hits. The path-construction block description now
  reads: "replace the entire existing path-construction block (the
  `repo_root().await?.join("_versions").join(type.as_dir_name()).join(...)`
  chain that Phase 6 Commit 2 lands; the exact spelling does not matter — every
  line of it is replaced)". The hedge has been replaced with a concrete
  description that makes the implementation decision unambiguous.
- `grep "Use whichever"` → 0 hits. The parenthetical instructing the implementer
  to "pick whichever `mkdir -p` helper" has been removed entirely.
- `grep "create_dir_all_async"` → 0 hits. The redundant explicit `mkdir -p` call
  is gone from the code snippet.
- The Commit 3 §Files snippet (lines 247–261) now calls
  `cbscore::versions::desc::write_descriptor(&desc, &path).await?` as the sole
  write step; a trailing comment explains that `write_descriptor` calls
  `tokio::fs::create_dir_all` on `path.parent()` internally so no separate
  parent-create step is needed. All four sub-checks pass.

---

## Findings

None. No new issues were introduced by the `b595554` edits.

---

## Verdict

**Approve — seq-004 plan is finalized and ready for M1 implementation.**

Both v1 findings are closed correctly and completely. No new drift was
introduced. All five v17 closures remain intact. Migration table steps 1–4 are
covered; step 5 is correctly excluded. All seven OQ resolutions are still
reflected. The Phase 6 lines 67–77 cross-reference is exact. `prettier --check`
passes.

**Finding counts:** 0 critical, 0 major, 0 minor, 0 suggestions, 0 open
questions.

| ID  | Severity | Description                                            | Status |
| --- | -------- | ------------------------------------------------------ | ------ |
| N1  | MINOR    | §Depends on bullet 4 misattributes `desc.rs` to Ph 4   | CLOSED |
| N2  | MINOR    | Commit 3 §Files: two surviving "pick whichever" hedges | CLOSED |
