# Plan Review — cbscore Rust Port: Phase 1 (M0) — v2

**Plan reviewed:**
[`002-20260508T1558-01-types.md`](../plans/002-20260508T1558-01-types.md) and
[`plans/README.md`](../plans/README.md)

**v1 review:**
[`002-20260511T1002-plan-cbscore-rust-port-design-v1.md`](./002-20260511T1002-plan-cbscore-rust-port-design-v1.md)

**Fix commits reviewed:** `5c32195`, `9e8ca20`, `c2a283d`, `d745a5f`

**Designs referenced:** 001 (project structure), 002 (Rust port architecture),
004 (configurable version descriptor location).

**Reviewer:** Staff review, 2026-05-11.

---

## Summary Assessment

All nine v1 findings are closed. The fixes are mechanically correct: the
`cbscore-types` dep-graph hygiene is sound, the logger module is present with
the right symbols named, the README markers are repositioned, the citation
source is corrected, the Phase 3 subsystem order is fixed, the four-file
`config/` split is specified with an explanatory intro, the `paths.versions`
out-of-scope bullet is in place, and the workspace version is pinned. One new
**IMPORTANT** issue was introduced by the I1 fix: `logger.rs` calls
`tracing_subscriber::EnvFilter`, but `cbscore-types/Cargo.toml` (per design 001
§Cargo Sketch) does not include `tracing-subscriber` — the module as written
will not compile. This must be resolved before implementation begins.

---

## v1 Findings: Closure Status

### C1 — `serde_json` / `serde_saphyr` in `cbscore-types [dependencies]` — CLOSED

Commit 1 spec (lines 75–83 of the plan) explicitly names only `serde` (derive
feature), `chrono` (serde feature), `camino` (serde1 feature), `thiserror`, and
`tracing` in `[dependencies]`. The bold callout — "`serde_json` and
`serde_saphyr` deliberately do not appear in `[dependencies]`" — matches design
001 lines 366–370 verbatim, and correctly defers both to `[dev-dependencies]` of
Commit 5. The Testable line includes the `cargo tree -p cbscore-types --depth 1`
check.

### I1 — `logger.rs` absent from Phase 1 — CLOSED (new issue introduced; see NI1)

Commit 2 §Files now includes `cbsd-rs/cbscore-types/src/logger.rs` with
`pub const TARGET_*: &str` constants and `pub fn set_debug_logging()`. The
commit title and progress-table row are updated to "add error taxonomy + logger
module" at ~280 LOC; the phase total is ~1730. The addition is semantically
correct — however, the `set_debug_logging()` specification names
`tracing_subscriber::EnvFilter` without addressing the dep gap this creates in
`cbscore-types/Cargo.toml`. See NI1.

### I2 — M1 / M2 cut markers misplaced in README — CLOSED

The dependency-graph line now reads:

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
                                                        (M1 cut)    (M2 cut)
```

`(M1 cut)` starts at column 56, which underlies the `6 →` portion of the graph
line (Phase 6 ends at col 56, the arrow is col 58). `(M2 cut)` starts at column
68, which is past the end of Phase 7 (col 66). The placement matches the exact
diagram prescribed by v1. `(M1 cut)` overlaps the Phase 7 label by four
characters visually, but this was present in the v1 desired diagram and
correctly conveys the intent: M1 milestone is reached at the end of Phase 6, M2
at the end of Phase 7.

### M1 — `tracing` missing from `cbscore-types/Cargo.toml` — CLOSED

`tracing` now appears in the Commit 1 dep list, matching design 001 §Cargo
Sketch line 363.

### M2 — Wrong citation for "Correctness Invariants" — CLOSED

Both corrected occurrences now cite
`` `cbsd-rs/docs/cbscore-rs/CLAUDE.md` § Correctness Invariants item 1 ``. Zero
occurrences of "design 002 § Correctness Invariants" remain. The v1 review cited
three occurrences; the closing commit notes only two — confirmed accurate:
Commit 4 Testable (line 247–249) carries the `create → store → load == create`
notation without the explicit citation, which was already the correct form
before the fix.

### M3 — Phase 3 description subsystem order — CLOSED

README Phase 3 description now reads "M1.2 — S3, Vault, secrets manager, config
IO", matching the design 002 §M1 order (`… s3 → vault → secrets → config IO …`).

### M4 — `config/` subtree collapsed into `config/mod.rs` — CLOSED

Commit 3 §Files now lists four files (`config/mod.rs`, `config/paths.rs`,
`config/storage.rs`, `config/vault.rs`) with an explicit intro sentence: "The
`config/` subtree mirrors design 001 §Workspace Layout lines 101–105 — four
files, not a single `mod.rs`." Each file's responsibilities match design 001
lines 101–105.

### S1 — `PathsConfig.versions` not called out in §Out of scope — CLOSED

§Out of scope (lines 49–51) now contains: "The
`Config.paths.versions: Option<Utf8PathBuf>` field. Owned by seq-004 (design 004
OQ1 Step 1); lands in Phase 6 alongside the `--versions-dir` CLI flag and the
resolver. Phase 1's `PathsConfig` ships without it."

### S2 — Initial workspace version not specified — CLOSED

Commit 1 spec (lines 72–74) names `[workspace.package] version = "0.1.0"` with a
citation to design 001 §Versioning line 488–489 and the requirement that every
member crate inherits via `version.workspace = true`.

---

## New Findings

### NI1 — `logger.rs` calls `tracing_subscriber::EnvFilter` but `cbscore-types` has no `tracing-subscriber` dep — IMPORTANT

**Where:** Plan Commit 2 §Files, `logger.rs` description (lines 115–121); Commit
1 `cbscore-types/Cargo.toml` spec (lines 75–77).

**What the plan says:** Commit 2 specifies `logger.rs` as providing
`pub fn set_debug_logging()` that "configures a
`tracing_subscriber::EnvFilter`".

**What the design says:** Design 001 §Cargo Sketch for `cbscore-types` lists
exactly five deps — `camino`, `chrono`, `serde`, `thiserror`, `tracing` (lines
358–363). `tracing-subscriber` is absent from `cbscore-types` by design: it
appears only in `cbscore` (line 387) and `cbsbuild` (line 435). Design 001's
§Crate Responsibilities confirms the omission is intentional — the no-IO,
no-subscriber constraint is what makes `cbscore-types` a lean dep for narrow
consumers like `cbc`.

**Failure mode:** The Commit 2 spec as written cannot compile. Calling
`tracing_subscriber::EnvFilter::from_env()` (or any `tracing_subscriber` API)
inside `cbscore-types` requires the crate to depend on `tracing-subscriber`,
which is not listed in Commit 1's `cbscore-types/Cargo.toml` spec. An
implementer following the plan literally must make a choice that contradicts
either the Commit 1 Cargo spec or the Commit 2 logger spec.

**Resolution directions:**

1. **Add `tracing-subscriber` to `cbscore-types` (recommended path):** Update
   the Commit 1 spec to add
   `tracing-subscriber = { version = "0.3", features = ["env-filter"] }` to
   `cbscore-types/Cargo.toml`, and explicitly note the deviation from design 001
   §Cargo Sketch with the rationale — `cbc` depends on `cbscore-types` for
   `logger.set_debug_logging` (design 001 §Downstream Consumers), so the
   subscriber must live here. The dep adds ~600 KB to the compiled
   `cbscore-types` closure but adds no IO or runtime async overhead.

2. **Restrict `logger.rs` to constants only:** Respecify `logger.rs` to export
   only the `pub const TARGET_*: &str` constants; move `set_debug_logging()` to
   `cbscore` (where `tracing-subscriber` already lives). Then update the
   §Downstream Consumers note in the plan to reflect that `cbc` would need to
   depend on `cbscore` to call `set_debug_logging()` — which adds the full IO
   library closure to `cbc`, contradicting design 001's stated motivation for
   the three-crate split (lines 73–76). This path works only if a future design
   revision accepts the heavier `cbc` dep.

Option 1 is the path of least resistance and keeps `cbc`'s dep graph as light as
possible. The design 001 §Cargo Sketch footnote at lines 366–370 would need a
small annotation clarifying the `tracing-subscriber` inclusion.

---

## Open Questions

None beyond NI1.

---

## Verdict

**One new IMPORTANT finding (NI1) must be resolved before implementation.** The
plan is otherwise in excellent shape: all nine v1 findings are cleanly closed,
both files pass `prettier --check`, and the prose flow of the inserted citations
is coherent. Fix NI1 (two-line Cargo.toml spec update + rationale note in Commit

1. and the plan is ready to drive implementation.
