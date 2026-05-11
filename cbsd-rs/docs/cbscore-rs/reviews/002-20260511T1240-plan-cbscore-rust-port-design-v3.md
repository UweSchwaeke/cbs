# Plan Review — cbscore Rust Port: Phase 1 (M0) — v3

**Plan reviewed:**
[`002-20260508T1558-01-types.md`](../plans/002-20260508T1558-01-types.md) and
[`plans/README.md`](../plans/README.md)

**v1 review:**
[`002-20260511T1002-plan-cbscore-rust-port-design-v1.md`](./002-20260511T1002-plan-cbscore-rust-port-design-v1.md)

**v2 review:**
[`002-20260511T1130-plan-cbscore-rust-port-design-v2.md`](./002-20260511T1130-plan-cbscore-rust-port-design-v2.md)

**Fix commit reviewed:** `a4e8405`

**Designs referenced:** 001 (project structure), 002 (Rust port architecture),
004 (configurable version descriptor location).

**Reviewer:** Staff review, 2026-05-11.

---

## Summary Assessment

NI1 is cleanly closed. The fix adds `tracing-subscriber` with the `env-filter`
feature to the Commit 1 `cbscore-types/Cargo.toml` spec, supplies a
well-reasoned in-line rationale citing the correct design 001 sections, flags
the tension with §Cargo Sketch as a non-blocking follow-up, and leaves the
existing `serde_json` / `serde_saphyr` prohibition intact. All nine v1 closures
survive the edit. No new findings of any severity were introduced. The plan is
ready for implementation.

---

## NI1 Closure Verification

### NI1 — `tracing-subscriber` dep absent from `cbscore-types` — CLOSED

**What the fix does (commit `a4e8405`):** The Commit 1
`cbscore-types/Cargo.toml` bullet (plan lines 75–92) is rewritten. The previous
text listed five deps ending at `tracing`; the new text names the same five and
then adds:

> `tracing-subscriber` is **added beyond the design 001 §Cargo Sketch**: it is
> required by the `set_debug_logging()` helper in `logger.rs` (Commit 2), which
> design 001 §Crate Responsibilities lines 218–219 places in `cbscore-types`,
> and which design 001 §Downstream Consumers line 65 commits external consumers
> (`cbc`) to importing from there.

**Rationale citation correctness:** The note correctly cites §Crate
Responsibilities lines 218–219 (places `set_debug_logging()` in `cbscore-types`)
and §Downstream Consumers line 65 (`cbc` imports from there). Both citations
match the v2 reviewer's recommended rationale verbatim. The resolution sentence
— "The set-of-truth tension between §Cargo Sketch and §Crate Responsibilities is
resolved in favour of §Crate Responsibilities (the function lives where the
design says it lives; the dep follows the function)" — is clear and technically
sound.

**Non-blocking flag:** The text closes with "flagged as a follow-up §Cargo
Sketch edit on design 001, not blocking on this plan." This matches Option A
from v2's resolution directions and satisfies the reviewer's requirement for an
explicit note.

**`serde_json` / `serde_saphyr` prohibition intact:** The same bullet
immediately continues with the bold callout: "`serde_json` and `serde_saphyr`
deliberately do not appear in `[dependencies]`". The prohibition text is
unchanged from v2 and correctly retains the citation to design 001 lines
366–370.

**Feature flag correctness:** `tracing-subscriber` is listed with the
`env-filter` feature. `tracing_subscriber::EnvFilter` is gated behind the
`env-filter` cargo feature in the `tracing-subscriber 0.3` crate — this is the
correct and necessary feature flag. Without it the
`tracing_subscriber::EnvFilter` type is not compiled and the Commit 2
`logger.rs` would fail to compile regardless of the dep being present. The
feature selection is accurate.

---

## v1 Findings: Regression Check

### C1 — `serde_json` / `serde_saphyr` in `cbscore-types [dependencies]`

Still CLOSED. The `serde_json` / `serde_saphyr` prohibition callout appears
verbatim at plan lines 87–92. The NI1 fix did not touch that text beyond
repositioning it inside the expanded bullet. The Commit 1 Testable line (plan
lines 105–108) still includes the `cargo tree -p cbscore-types --depth 1` check
asserting their absence.

### I1 — `logger.rs` absent from Phase 1

Still CLOSED. `cbsd-rs/cbscore-types/src/logger.rs` remains in Commit 2 §Files
(plan lines 121–130) with the same `pub const TARGET_*` constants and
`pub fn set_debug_logging()` description. Nothing in the NI1 fix touched
Commit 2.

### I2 — M1 / M2 cut markers misplaced in README

Still CLOSED. The README dependency-graph line reads:

```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5 → Phase 6 → Phase 7
                                                        (M1 cut)    (M2 cut)
```

The NI1 fix commit touched only `002-20260508T1558-01-types.md`; the README was
not modified. The markers remain in the positions confirmed correct by v2.

### M1 — `tracing` missing from `cbscore-types/Cargo.toml`

Still CLOSED. `tracing` is the fourth dep named in the Commit 1 bullet (plan
line 77: "…`thiserror`, `tracing`, plus `tracing-subscriber`…"). The NI1 fix
explicitly preserves `tracing` as one of "the first five" matching design 001
§Cargo Sketch lines 358–364.

### M2 — Wrong citation for "Correctness Invariants"

Still CLOSED. The two occurrences in Commit 3 and the Goal section cite
`` `cbsd-rs/docs/cbscore-rs/CLAUDE.md` § Correctness Invariants item 1 ``. The
NI1 fix did not touch any of those lines.

### M3 — Phase 3 description subsystem order

Still CLOSED. README Phase 3 still reads "M1.2 — S3, Vault, secrets manager,
config IO". Unchanged.

### M4 — `config/` subtree collapsed into `config/mod.rs`

Still CLOSED. Commit 3 §Files lists four files (`config/mod.rs`,
`config/paths.rs`, `config/storage.rs`, `config/vault.rs`) with the intro
sentence explaining the split. Unchanged.

### S1 — `PathsConfig.versions` not in §Out of scope

Still CLOSED. The §Out of scope bullet for
`Config.paths.versions: Option<Utf8PathBuf>` (plan lines 49–51) is present and
unchanged.

### S2 — Initial workspace version not specified

Still CLOSED. Commit 1 spec names `[workspace.package] version = "0.1.0"` (plan
lines 72–74) with the design 001 §Versioning citation. Unchanged.

---

## Fresh-Eyes Sweep

### Prose flow and markdown structure

The expanded Commit 1 bullet is a single long parenthetical (lines 75–92). The
structure is coherent: it names the five design-matching deps, then introduces
`tracing-subscriber` with a "why" explanation, then closes with the `serde_json`
/ `serde_saphyr` prohibition. The paragraph reads as one continuous thought
about what goes into and stays out of `[dependencies]`. There is no structural
break, no dangling list item, and no formatting regression from the NI1 fix.

### Testable line — should it assert `tracing-subscriber` IS present?

The Commit 1 Testable line (plan lines 105–108) reads:
"`cargo tree -p cbscore-types --depth 1` does **not** list `serde_json` or
`serde_saphyr`". It does not positively assert that `tracing-subscriber` appears
in the dep graph.

This is a minor gap. `cargo tree --depth 1` would show `tracing-subscriber`
immediately if it is in `[dependencies]`, so a reviewer running the negative
check would notice the absence if it were missing. However, an explicit positive
assertion — "does list `tracing-subscriber` with the `env-filter` feature" —
would close the loop symmetrically and match the pattern the v2 reviewer used
when describing what the Testable line should verify. This is not a blocker: the
dep is fully specified in the Files bullet, and `cargo build --workspace`
failing would also catch it. Raised for awareness; see finding NF1 below.

### Other locations in the plan mentioning `cbscore-types` deps

The only other mention of `cbscore-types/Cargo.toml` contents is in Commit 2
§Files (the `logger.rs` description at lines 121–130), which correctly names
`tracing_subscriber::EnvFilter` as the implementation mechanism. No other commit
spec mentions the dep list for `cbscore-types` directly. Commit 5 §Files
mentions adding `serde_json` / `serde_saphyr` as `[dev-dependencies]`, which is
consistent with the Commit 1 prohibition. No inconsistency found.

### Do v1/v2 review docs or the README need updating?

The v1 and v2 review documents are historical records; they are not updated
retroactively. The README was not modified by the NI1 fix and requires no
update. The I1 commit message rationale (in the closing commits for v1) does not
mention `tracing-subscriber` because `logger.rs` was added before the dep gap
was noticed — this is expected and requires no retroactive correction. No update
needed to any of these documents.

---

## New Findings

### NF1 — Testable line for Commit 1 lacks a positive assertion for `tracing-subscriber`

**Severity:** Minor.

**Where:** Plan lines 105–108, Commit 1 Testable block.

**What:** The Testable line asserts `cargo tree -p cbscore-types --depth 1` does
**not** show `serde_json` / `serde_saphyr`. It does not assert that
`tracing-subscriber` (now a named dep) **does** appear. The `tracing-subscriber`
dep is the most material addition from the NI1 fix; a Testable line that
verifies the negative but not the positive is asymmetric.

**Why it matters:** If a future plan edit accidentally drops
`tracing-subscriber` while keeping the negative list intact, the Testable line
would pass while the build would fail in Commit 2. The gap is small because
`cargo build --workspace` at the end of the same Testable line would catch it,
but the explicit dependency-graph check should be complete.

**Resolution:** Add to the Testable block:
"`cargo tree -p cbscore-types --depth 1` lists `tracing-subscriber` with the
`env-filter` feature." One sentence; no structural change to the block.

**Non-blocking:** The current text is unambiguous in the Files bullet. The
missing assertion does not create an implementer ambiguity or a correctness risk
sufficient to delay implementation.

---

## Open Questions

None.

---

## Verdict

**NI1 is closed. The plan is ready to drive implementation.** One new minor
finding (NF1) is noted: the Commit 1 Testable line does not positively assert
that `tracing-subscriber` appears in the dep graph. This is non-blocking — it
can be addressed in a trivial one-line edit before or after implementation
begins, at the author's discretion.

**New findings by severity:** 0 blockers, 0 important, 1 minor (NF1), 0
suggestions.
