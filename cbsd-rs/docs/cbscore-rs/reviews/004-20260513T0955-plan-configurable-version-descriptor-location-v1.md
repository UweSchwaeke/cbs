# Plan Review v1 — seq-004: Configurable `VersionDescriptor` Location

**Date:** 2026-05-13\
**Reviewer:** Staff Engineer (design-reviewer agent)\
**Scope:** First dedicated plan review of seq-004\
**Plan file:**
`cbsd-rs/docs/cbscore-rs/plans/004-20260513T0900-configurable-version-descriptor-location.md`\
**Design
audited against:**
`cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md`\
**Prior
coverage:** Corpus reviews v17 (`002-20260513T0918`) and v18
(`002-20260513T0940`); five v17 findings (N1, N2, S1, S2, Q1) confirmed closed
at commit `e3cb122`.

---

## Scope

This is a focused plan review. Scope is the seq-004 plan only; the seq-002
corpus is out of scope (confirmed clean at v18).

The review verifies:

1. Design 004 Migration table steps 1–4 are fully covered; step 5 is correctly
   excluded.
2. All seven OQ resolutions (OQ1–OQ7) are reflected in the plan.
3. Commit boundaries are sensible and each commit is independently compilable.
4. Cross-plan dependencies are named specifically enough to verify.
5. No surviving hedges remain after the e3cb122 closures.
6. Plan structure matches seq-002 phase conventions.
7. §Sequencing cross-reference is correct (Phase 6 lines 67–77).
8. Every §Testable bullet is testable as stated.
9. LOC estimates are realistic.
10. `prettier --check` passes.

---

## Method

1. Read the seq-004 plan end-to-end.
2. Cross-checked every design-constraint and §Depends on claim against design
   004 and the cited phase plans.
3. Verified the Phase 6 §Out of scope block lines 67–77 against the §Sequencing
   cross-reference (`awk 'NR==67,NR==77'` on `002-…-06-cbsbuild-cli.md`). Exact
   match confirmed.
4. Verified Phase 1 Commit 3 establishes `cbscore-types/src/versions/desc.rs`
   (grep on `002-…-01-types.md`).
5. Verified Phase 4 Commit 1 establishes `cbscore/src/versions/desc.rs` and
   specifies that `write_descriptor` calls `tokio::fs::create_dir_all`
   internally (grep + read on `002-…-04-runner.md`).
6. Compared the OQ5 error text verbatim between plan and design.
7. Scanned for surviving hedges:
   `grep -c "or wherever|if portable|pick the placement|etc\."` → 0 hits on the
   previously-flagged phrases. Separately grepped for `"or however"` and
   `"Use whichever"` — both present in Commit 3 §Files.
8. Ran `prettier --check` on the plan — **passes** ("All matched files use
   Prettier code style!").
9. Verified the README seq-004 bullet links the correct plan file with correct
   commit count (3) and LOC (~500).

---

## Findings

### MINOR

#### N1 — §Depends on bullet 4 misattributes the `cbscore-types` `desc.rs` dependency to Phase 4

**Location:** seq-004 §Depends on, fourth bullet.

**Issue:** The bullet reads: "seq-002 Phase 4 Commit 1 —
`cbscore::versions::desc` module exists. Commit 1 adds `descriptor_path` to the
`cbscore-types` side of that module path."

`cbscore/src/versions/desc.rs` (the `cbscore::versions::desc` module) is created
by Phase 4 Commit 1. But the file seq-004 Commit 1 actually edits is
`cbscore-types/src/versions/desc.rs` — a different crate's `desc.rs` that
already exists from Phase 1 Commit 3. Phase 4 Commit 1 is not a compile
dependency for seq-004 Commit 1; Phase 1 is.

The phrasing "adds `descriptor_path` to the `cbscore-types` side of that module
path" is accurate about what seq-004 Commit 1 does but implies the module on the
cbscore-types side was created by Phase 4, which it was not. A reader parsing
the §Depends on list to understand ordering will conclude Phase 4 must land
before seq-004 Commit 1, when Phase 1 is the actual requirement. At the plan's
interleave point (after Phase 6 Commit 4), Phase 4 is always done, so there is
no operational risk — but the statement is imprecise.

**Resolution:** Split bullet 4 into two targeted statements. For example:

- "**seq-002 Phase 1 Commit 3** — `cbscore-types/src/versions/desc.rs` exists.
  seq-004 Commit 1 appends `descriptor_path` and `VersionType::as_dir_name` to
  this file."
- "**seq-002 Phase 4 Commit 1** — `cbscore/src/versions/desc.rs` exists (IO
  side; `read_descriptor` + `write_descriptor`). seq-004 Commit 2's
  `versions/resolve.rs` sits alongside this file under `cbscore/src/versions/`,
  and seq-004 Commit 3 calls `write_descriptor` from the refactored write path."

---

#### N2 — Commit 3 §Files carries two surviving open-decision hedges

**Location:** seq-004 Commit 3 §Files, the `cbsbuild/src/cmds/versions.rs`
bullet.

**Issue:** Two phrases leave implementation decisions open that Phase 4 Commit 1
already settled:

1. **Line 234:** `repo_root.join("_versions").join(type.as_str()).join(...)` or
   however Phase 6 Commit 2 spelled it" — the "or however" is a hedge on the
   exact form of the hardcoded chain being replaced. Phase 6 Commit 2 §Files
   does not pin the exact source form; the design 004 §Write site shows the full
   replacement snippet without specifying the old chain. This forces the
   implementer to discover the exact spelling at implementation time.

2. **Lines 252–255:** "(Use whichever `mkdir -p` helper is already in
   `cbscore::utils::fs` or equivalent; design 004 §Write site says either
   `desc.write` carrying the `create_dir_all` or the call site doing it
   explicitly is correct — pick whichever matches the existing convention from
   Phase 6 Commit 2.)" — Phase 4 Commit 1 already decided this:
   `write_descriptor` calls `tokio::fs::create_dir_all` internally ("Creates the
   parent dir if absent via `tokio::fs::create_dir_all` — same `mkdir -p`
   semantic as `Config::store`", Phase 4 Commit 1 §Files). The code snippet in
   the plan also calls `create_dir_all_async()` explicitly before `desc.write`,
   which makes the snippet subtly redundant with Phase 4's spec (two `mkdir -p`
   calls for one write). The "pick whichever" comment should have been retired
   once Phase 4 settled this.

Neither hedge can cause a compile failure — the path replacement will work
regardless, and a double `create_dir_all` is harmless. But they leave decisions
open that the plan is supposed to close.

**Resolution for hedge 1:** Replace "or however Phase 6 Commit 2 spelled it"
with a concrete description: "replace the hardcoded path construction in the
`versions create` handler (Phase 6 Commit 2 writes to the `repo_root().await?`
fallback directly; the exact call is resolved at implementation time by reading
that commit's handler body)." Alternatively, note that the exact old form does
not matter — the entire path-computation block is replaced by the two-line
`resolve_root` + `descriptor_path` call, whatever form the old block takes.

**Resolution for hedge 2:** Remove the "Use whichever" parenthetical entirely
and align the code snippet with Phase 4 Commit 1's settled behaviour: since
`write_descriptor` calls `create_dir_all` internally, the explicit
`create_dir_all_async()` call in the snippet is redundant. Either drop it from
the snippet (trusting `write_descriptor`) or keep it with a note that it is a
no-op when the parent already exists. The parenthetical instructing the
implementer to "pick whichever" must go.

---

## Positive Findings

The following audit items are confirmed clean:

1. **Migration table coverage (steps 1–4):** Commit 1 covers step 1
   (`PathsConfig.versions` + `as_dir_name` + `descriptor_path`), Commit 2 covers
   step 3 (`resolve_root` + `NoDescriptorRoot`), Commit 3 covers step 4
   (`--versions-dir` flag + write-path cutover). Step 2 (`descriptor_path`
   helper) is embedded in Commit 1. Step 5 (bypass pre-fill + `config init`
   prompt) is correctly excluded and attributed to seq-003 / post-M1.
2. **OQ1 (config field + CLI flag, precedence CLI > config > fallback):**
   Reflected at §Goal bullet 3, Commit 2 §Files resolver logic, Commit 3 §Files
   and §Design constraints, and §End-of-feature acceptance test 2. The flag is
   named `--versions-dir`; the config field is `Config.paths.versions`.
   Consistent throughout.
3. **OQ2 (`<git-root>/_versions` fallback):** Described in Commit 2 §Files
   resolver step 3 and confirmed by acceptance test 1.
4. **OQ3 (`<root>/<type>/<VERSION>.json` via `descriptor_path`):** Single
   encoding in Commit 1; Commit 3 removes the old hardcoded chain and replaces
   it with the helper. §Out of scope correctly names multi-root and
   auto-discovery as rejected.
5. **OQ4 (read sites unaffected):** §Out of scope explicitly rejects
   auto-discovery. No reader modification anywhere in the plan.
6. **OQ5 (friendly four-line error text):** Verbatim text present in Commit 2
   §Files `Display` arm. Plan version adds the `{cwd}` substitution over the
   design's "For example:" text — a correct enhancement. Display snapshot test
   required in §Testable.
7. **OQ6 (no schema_version bump):** Stated in §Out of scope.
8. **OQ7 (bypass pre-fill + interactive prompt deferred):** §Out of scope
   correctly attributes both to seq-003 / post-M1. Commit 3 §Design constraints
   confirms no `config init` changes.
9. **Commit boundaries compilable in isolation:** Commit 1 (pure-type,
   `cbscore-types` only), Commit 2 (IO, `cbscore` only, depends on Commit 1),
   Commit 3 (CLI wiring, depends on Commits 1–2). Each compiles independently;
   no forward references.
10. **Phase 6 §Out of scope lines 67–77 cross-reference:** Exact match
    confirmed. Lines 67–77 span the full seq-004 bullet including both the
    recommended interleave and the slip-handling fallback. §Sequencing is
    mutually consistent in both directions.
11. **`repo_root` dependency (N1 closure from v17):** Phase 2 Commit 4 §Files
    now explicitly names
    `repo_root (Rust name for Python's get_git_repo_root; seq-004 Commit 2's resolve_root depends on this exact name)`.
    The Commit 2 §Files `cbscore::utils::git::repo_root` reference is
    unambiguous.
12. **`NoDescriptorRoot` placement (N2 closure from v17):** Pinned to
    `cbscore-types/src/versions/errors.rs` with rationale (camino dep already
    present; Display is pure string formatting; no layering violation). No hedge
    survives.
13. **`#[cfg(target_os = "linux")]` deleted-cwd test (S1 closure from v17):**
    Gate is explicit; `tempdir → cd → rmdir → call resolve_root` procedure is
    concrete; "if portable" hedge is gone; `getcwd(2) → ENOENT` rationale is
    documented.
14. **`resolve_root` sub-module placement (Q1 closure from v17):** Pinned to
    `cbscore/src/versions/resolve.rs` with `mod.rs` additions specified and
    file-per-IO-function justification cited.
15. **LOC estimates realistic:** ~120 LOC (Commit 1, pure type additions with
    tests), ~200 LOC (Commit 2, resolver + error variant + tests), ~180 LOC
    (Commit 3, CLI wiring + tests) for ~500 total. Consistent with design 004's
    ~50 net-new Rust plus test overhead. All three commits fall below the
    800-line split threshold.
16. **§Testable bullets are testable as stated:** Every bullet names a concrete
    assertion. No vague "verify it works" language. The deleted-cwd and
    `<unknown>` rendering tests are gated appropriately.
17. **Tracing target `"cbscore::versions"`:** Consistent with design 002
    §Logging convention.
18. **`as_dir_name` return type `&'static str`:** More precise than design 004's
    `&str` sketch; correct for a `match` returning string literals.
19. **`#[serde(default)]` and
    `#[serde(skip_serializing_if = "Option::is_none")]` guidance:** Round-trip
    absent-field test is specified with conditional attribute application for
    consistency with existing path fields.
20. **Plan structure:** §Progress table, §Goal, §Depends on, §Out of scope,
    per-commit §Files / §Design constraints / §Testable, §End-of-feature
    acceptance all present. Tracking table format matches seq-002 phase files.
    README seq-004 bullet links the correct plan file.
21. **No double-coverage with seq-002:** Phase 1 §Out of scope explicitly defers
    `Config.paths.versions` to seq-004. Phase 6 Commit 2 §Design constraints
    records that the hardcoded path will be refactored by seq-004.
    `--versions-dir` appears nowhere in Phase 6 Commit 2.
22. **`prettier --check` passes** on
    `004-20260513T0900-configurable-version-descriptor-location.md`.

---

## Verdict

**Near-approve — two minor findings must be closed before M1 implementation
starts.**

The seq-004 plan is well-structured, internally consistent, and faithfully
implements design 004 steps 1–4. All seven OQ resolutions are reflected. The
five v17/v18 closures are confirmed intact. Two new minor findings (N1: §Depends
on bullet 4 misattribution; N2: two surviving hedges in Commit 3 §Files) require
one-sentence to one-paragraph edits each. Neither introduces any compile or test
risk, but both leave implementer decisions open that the plan is supposed to
close, which is the stated acceptance criterion for this review pass. Fix both,
then update §Status to **Approved** without requiring a v2 review pass.

**Finding counts:** 0 critical, 0 major, 2 minor, 0 suggestions, 0 open
questions.

| ID  | Severity | Description                                            | Action required |
| --- | -------- | ------------------------------------------------------ | --------------- |
| N1  | MINOR    | §Depends on bullet 4 misattributes `desc.rs` to Ph 4   | Edit bullet     |
| N2  | MINOR    | Commit 3 §Files: two surviving "pick whichever" hedges | Edit §Files     |
