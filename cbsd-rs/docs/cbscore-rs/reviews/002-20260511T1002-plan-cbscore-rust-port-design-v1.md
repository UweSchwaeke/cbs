# Plan Review — cbscore Rust Port: Phase 1 (M0) — v1

**Plan reviewed:**
[`002-20260508T1558-01-types.md`](../plans/002-20260508T1558-01-types.md) and
[`plans/README.md`](../plans/README.md)

**Designs referenced:** 001 (project structure), 002 (Rust port architecture),
004 (configurable version descriptor location).

**Reviewer:** Staff review, 2026-05-11.

---

## Summary Assessment

The Phase 1 plan accurately captures the M0 scope and correctly acknowledges the
one known drift from design 002 (parse-family functions moving to `cbscore`).
The five-commit structure is coherent and the test directions (value-side vs
file-side round-trips) are well-specified. Two issues require fixes before
implementation: one **CRITICAL** (wrong crate dependencies that directly
contradict an explicit design rule and would corrupt the `cbscore-types`
dependency graph) and two **IMPORTANT** (a missing M0 deliverable and a
milestone-marker inconsistency in the README). Four minor items and two
suggestions complete the findings. The plan is **not ready to implement as
written** but is close — fixing the critical and important items is low
mechanical effort.

---

## Strengths

- **Acknowledged drift is internally consistent.** The §Out of scope section and
  Commit 4 both attribute the parse-family move to `cbscore` correctly, cite the
  same reason (`regex` dep exclusion from `cbscore-types`), and both say Phase 2
  as the landing target. No ambiguity for the implementer.
- **Wire-format rules are correctly propagated.** Commit 3 captures the
  kebab-case constraint; Commit 4 captures the NO `rename_all` rule for
  descriptors; Commit 5 captures absent-is-error, unknown-version-hard-error,
  and the snake_case exception for `schema_version` inside kebab-case containers
  — all with accurate design 002 line citations.
- **All ten versioned formats accounted for.** Commit 5 lists every wire-format
  type from design 002 §Wire-Format Versioning table (Config, Secrets,
  VaultConfig, CoreComponent, ContainerDescriptor, VersionDescriptor,
  ImageDescriptor, ReleaseDesc, ReleaseComponent, BuildArtifactReport). None
  missing.
- **Round-trip split is coherent.** The value-side
  (`create → store → load == create`) direction in Commits 3/4 and the file-side
  (`load → store → load == load`) direction in Commit 5 are functionally
  distinct and complementary. The plan explains why the split exists.
- **Error taxonomy is fully covered.** All seventeen entries from design 002
  §Error Taxonomy map to files in Commit 2. `GitError` is absent by design — it
  lives in design 001 §Lift-out invariants only, not in design 002's taxonomy
  table, making Phase 2 the right landing for it.
- **Seq-004 / seq-005 scope boundaries are correctly placed.** `paths.versions`
  (design 004 Step 1) and `descriptor_path()` (design 004 Step 2) are absent
  from Phase 1. Design 004's migration steps 1–4 are all M1-CLI territory;
  deferring them to seq-004 alongside Phase 6 is correct and consistent with
  design 004's resolved decisions.
- **M1-internal subsystem order matches design 002.** The README dependency
  paragraph and the per-phase descriptions follow the exact
  `subprocess → podman/buildah/skopeo → git → s3 → vault → secrets → config IO → runner → builder stages → releases → images`
  order from design 002 §Migration Strategy.
- **IO deferral is clean.** §Out of scope clearly names `Config::load`,
  secrets-manager IO, and descriptor-store walks as Phase 3 items. Commit 5's
  tests use in-memory serde operations, not filesystem IO, which is consistent
  with `cbscore-types` being zero-IO.

---

## Blockers (CRITICAL)

### C1 — `cbscore-types/Cargo.toml` lists `serde_json` and `serde_saphyr` as

`[dependencies]`

**Where:** Plan Commit 1, `cbscore-types/Cargo.toml` entry (line ~68–69 of plan
file).

**What the plan says:** The `cbscore-types/Cargo.toml` entry reads:

> depends on `serde`, `serde_json`, `serde_saphyr`, `chrono`, `camino` with
> `serde1` feature, `thiserror`

**What the designs say:** Design 001 §Cargo Sketch is explicit and emphatic:

> `serde_json` and `serde_saphyr` deliberately do **not** appear here — the
> types only carry `#[derive(Serialize, Deserialize)]` and never perform IO. The
> format crates live in `cbscore` (below), which owns file loading and dumping.
> Keeping them out of `cbscore-types` means a lean dependency graph for every
> downstream of this crate (the `cbsd-worker` direct dep, any external
> consumer).

Adding them as `[dependencies]` means `cbsd-worker` and any future consumer of
`cbscore-types` (e.g. `cbsd-proto`, tools that only need descriptor types)
transitively pull in the YAML and JSON parsers — exactly the bloat the design
chose the three-crate split to avoid.

**Note:** `serde_json` and `serde_saphyr` **will** be needed for the integration
tests in Commit 5, but they belong in `[dev-dependencies]` of `cbscore-types`,
not `[dependencies]`. The distinction matters: `dev-dependencies` are never
transitively visible to downstream crates.

**Fix:** Remove `serde_json` and `serde_saphyr` from the `cbscore-types`
`[dependencies]` line in Commit 1's spec. Add a note that they land as
`[dev-dependencies]` in Commit 5 (when the integration tests are added) or in a
dedicated Cargo.toml edit within Commit 5.

---

## Major Concerns (IMPORTANT)

### I1 — `logger.rs` (tracing target hierarchy) is absent from Phase 1

**Where:** Nowhere in the Phase 1 plan file.

**What the designs say:** Design 001 §Workspace Layout includes
`cbscore-types/src/logger.rs` in the tree diagram. Design 001 §Crate
Responsibilities "What goes here" bullet lists:

> `tracing` target hierarchy (`cbscore`, `cbscore::runner`, `cbscore::builder`,
> ...) and `set_debug_logging()` equivalent.

Design 002 §Capability Mapping lists `tracing` as a `cbscore-types`-level
dependency. Design 001 §Cargo Sketch includes `tracing = "0.1"` in
`cbscore-types/Cargo.toml`. Consumer `cbc` imports `logger.set_debug_logging`
(design 001 §Downstream Consumers table) — once `cbc` depends on
`cbscore-types`, that symbol must be present.

The tracing hierarchy module is zero-IO (it just defines module-path constants
and a `set_debug_logging` helper that configures an `EnvFilter`) and belongs in
M0 alongside the types crate. Omitting it leaves `cbscore-types` incomplete
against its own design spec.

**Fix:** Add a `cbsd-rs/cbscore-types/src/logger.rs` entry to Commit 2
(alongside the error taxonomy, where the module tree is being established) or
Commit 1 (as part of the scaffold). The module itself is a handful of
`pub const TARGET_*: &str = "..."` definitions plus a
`pub fn set_debug_logging()` that calls `tracing_subscriber::EnvFilter` — small
enough to go anywhere in Phase 1.

### I2 — M1 cut and M2 cut markers are both misplaced in the dependency graph

**Where:** README.md dependency graph section.

**What the README shows:**

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
                                              (M1 cut)    (M2 cut)
```

`(M1 cut)` sits at character position 46 — between Phase 5 and Phase 6.
`(M2 cut)` sits at character position 58 — between Phase 6 and Phase 7.

**What the designs say:** Design 002 §M1 end state is "cargo run the `cbsbuild`
CLI, execute a build of the real ceph component" — that is Phase 6 (M1.5 in the
status table). The status table itself labels Phase 6 as "M1.5 — `cbsbuild` clap
CLI + logging + exit codes + end-to-end Ceph build acceptance". M1 = cbscore-rs
1.0.0 is not achieved until Phase 6 lands. Placing `(M1 cut)` before Phase 6
implies Phase 6 is post-M1, which contradicts its own "M1.5" label.

Similarly, Phase 7 is labelled "M2 — `cbsd-worker` switches from
`cbscore-wrapper.py` to direct Cargo dep on `cbscore`". Design 002 §M2 confirms
this is the M2 milestone. `(M2 cut)` appearing before Phase 7 implies Phase 7 is
post-M2, which contradicts its M2 label.

**Fix:** Shift both markers one phase to the right:

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
                                                        (M1 cut)    (M2 cut)
```

---

## Minor Issues

### M1 — `cbscore-types/Cargo.toml` in Commit 1 is missing `tracing`

**Where:** Plan Commit 1, `cbscore-types/Cargo.toml` entry.

Design 001 §Cargo Sketch includes `tracing = "0.1"` in `cbscore-types`
`[dependencies]`. The plan's Commit 1 spec omits it. The tracing crate is needed
both by `logger.rs` (see I1) and by any type in `cbscore-types` that emits a
`tracing::event!`. Fix: add `tracing = "0.1"` to the `cbscore-types/Cargo.toml`
line in Commit 1's file list.

### M2 — Wrong citation source for "Correctness Invariants"

**Where:** Phase 1 plan, Goal section (line ~26) and Commit 3/4/5 **Testable**
blocks.

The plan cites "design 002 § Correctness Invariants line 1" for the
`create → store → load == create` round-trip direction. Design 002 has no
`§ Correctness Invariants` section; that section lives in
`cbsd-rs/docs/cbscore-rs/CLAUDE.md` § Correctness Invariants, item 1
("Round-trip wire-format stability"). Design 002 references "Correctness
Invariant 2" (the CLI UX parity break) but does not host the invariant list
itself. Fix: correct the citation to `CLAUDE.md § Correctness Invariants item 1`
(three occurrences).

### M3 — Phase 3 description lists subsystems out of design order

**Where:** README.md status table, Phase 3 description.

The Phase 3 description reads "M1.2 — S3, Vault, config IO, secrets manager".
Design 002 §M1 specifies the M1-internal order as
`… s3 → vault → secrets → config IO → …` — secrets comes **before** config IO.
The README description reverses these two. While the intra-phase commit ordering
will be decided when Phase 3 is drafted, placing config IO ahead of secrets in
the placeholder description is an inaccurate summary of the design's dependency
order (secrets manager depends on S3/Vault; config IO has no dependency on the
secrets manager). Fix: reorder to "S3, Vault, secrets manager, config IO".

### M4 — Commit 3 collapses the `config/` subtree into `config/mod.rs`

**Where:** Phase 1 plan, Commit 3 **Files** block.

Design 001 §Workspace Layout explicitly shows a four-file split: `config/mod.rs`
(Config, SigningConfig, LoggingConfig), `config/paths.rs` (PathsConfig),
`config/storage.rs` (StorageConfig, S3StorageConfig, …), and `config/vault.rs`
(VaultConfig, VaultAppRoleConfig, …). The plan puts all of these into
`config/mod.rs`. This is not a correctness problem (Rust doesn't require the
sub-module layout) but it silently diverges from design 001's stated structure,
which will confuse any reviewer comparing the landed code to the design. Fix:
either adopt the design 001 sub-module layout in the Commit 3 spec, or
explicitly note the consolidation and the reason for it.

---

## Suggestions

### S1 — §Out of scope should name the `PathsConfig.versions` field omission

The plan's Commit 3 creates `PathsConfig` but omits `paths.versions` (design 004
OQ1, Step 1). This is correct — seq-004 adds that field in Phase 6 alongside the
CLI flag and resolver. However, the §Out of scope section does not call this out
explicitly. An implementer reading only Phase 1 and seeing design 004's
`PathsConfig` sketch with `versions: Option<Utf8PathBuf>` may wonder whether to
include it. A one-line addition to §Out of scope — "`Config.paths.versions`
(design 004 Step 1 — seq-004 adds this field in Phase 6)" — removes any
ambiguity.

### S2 — Commit 1 does not specify the initial workspace version

Design 001 §Versioning specifies that `[workspace.package] version = "x.y.z"` is
the single version field all member crates inherit. The plan mentions adding
`[workspace.dependencies]` pins (line ~67 of the plan) but does not specify what
value to write for `version` in `[workspace.package]`. The pre-M1 development
convention (design 001 §Versioning: "version 0.x is reserved for in-progress
pre-release builds") implies `0.1.0`. Naming this explicitly in Commit 1's spec
avoids an implementer picking an arbitrary value.

---

## Open Questions

None that require author response before fixing the above. The design coverage
is thorough and the scope is well-bounded.

---

## Verdict

**Revise before implementing.** One CRITICAL item (wrong `[dependencies]` on
`cbscore-types`) and two IMPORTANT items (missing `logger.rs` deliverable,
misplaced milestone markers) must be fixed. The four minor issues should be
addressed in the same pass. The acknowledged drift from design 002 is closed and
consistent. All other design-to-plan mappings are accurate.
