# Plan Review — cbscore Rust Port: Phase 1 + Phase 2 Confirmation — v5

**Plans reviewed:**

- [`002-20260508T1558-02-subprocess-and-shell-tools.md`](../plans/002-20260508T1558-02-subprocess-and-shell-tools.md)
  — Phase 2, confirmation pass (six v4 findings)
- [`002-20260508T1558-01-types.md`](../plans/002-20260508T1558-01-types.md) —
  Phase 1, fresh-eyes sweep after v4 fixes
- [`plans/README.md`](../plans/README.md) — sanity re-check

**Prior reviews (all findings closed through v4):**

- v1:
  [`002-20260511T1002-plan-cbscore-rust-port-design-v1.md`](./002-20260511T1002-plan-cbscore-rust-port-design-v1.md)
  — 9 findings (C1, I1, I2, M1–M4, S1–S2)
- v2:
  [`002-20260511T1130-plan-cbscore-rust-port-design-v2.md`](./002-20260511T1130-plan-cbscore-rust-port-design-v2.md)
  — NI1 raised
- v3:
  [`002-20260511T1240-plan-cbscore-rust-port-design-v3.md`](./002-20260511T1240-plan-cbscore-rust-port-design-v3.md)
  — NI1 closed, NF1 (non-blocking), Phase 1 declared ready
- v4:
  [`002-20260511T1400-plan-cbscore-rust-port-design-v4.md`](./002-20260511T1400-plan-cbscore-rust-port-design-v4.md)
  — Phase 2 first review; 2 IMPORTANT, 2 MINOR, 2 SUGGESTIONS

**Closing commits reviewed:**

| Commit    | Finding(s) closed |
| --------- | ----------------- |
| `c9d5484` | I1                |
| `127b7db` | I2                |
| `d847f3d` | M1, M2, S1, S2    |

**Reviewer:** Staff review, 2026-05-11.

---

## Summary Assessment

All six v4 findings are closed cleanly. The three commits are well-scoped, the
diffs are internally consistent, and the explanatory prose added by each fix is
precise. One new minor issue was found during the fresh-eyes sweep: the `nix`
crate, referenced explicitly by the S2 RAII smoke test added in `d847f3d`, does
not appear in the `cbscore` `[dependencies]` sketch in design 001 §Cargo Sketch
or in Phase 1 Commit 1's Cargo.toml spec. This creates an undeclared dependency
in the test reference. The issue is minor and non-blocking: it affects a single
optional test hint, not a production code path. Both phases are otherwise
correct and internally consistent. Phase 1 + Phase 2 together are ready for
implementation start, subject to noting the `nix` gap at Commit 1 implementation
time.

---

## v4 Findings: Closure Confirmation

### I1 — `get_version_type` absent from Commit 5 — CLOSED

**Closing commit:** `c9d5484`

**Confirmed:**

- Phase 1 §Out of scope (plan line 52–54) now names all six parse-family
  functions: `parse_version`, `get_version_type`, `get_major_version`,
  `get_minor_version`, `normalize_version`, `parse_component_refs`. The text
  reads "All six functions" — the prior "five" framing is gone.
- Phase 2 Commit 5 §Files (plan line 270–271) lists
  `parse_version, get_version_type, get_major_version, get_minor_version, normalize_version, parse_component_refs`
  — all six present.
- Phase 2 Commit 5 §Design constraints (plan line 280) carries the explicit
  signature `get_version_type(name: &str) -> Result<VersionType, VersionError>`.
- The explanatory bullet (plan lines 285–291) cites design 001 §Downstream
  Consumers line 65 (`cbc` import of `get_version_type`) and explains the
  `regex`-forced placement in `cbscore::versions::utils` rather than
  `cbscore-types`.
- Phase 2 Commit 5 §Testable (plan lines 311–313) adds three `get_version_type`
  cases: Dev, Release, and Test variants — all three cover distinct suffix
  patterns.
- Phase 2 progress table: Commit 5 LOC bumped from ~200 to ~230; phase total
  ~1750 → ~1780 (confirmed: 500 + 400 + 150 + 500 + 230 = 1780, arithmetic
  correct).

---

### I2 — Lift-out invariant check via `cargo tree` unenforceable — CLOSED

**Closing commit:** `127b7db`

**Confirmed:**

- The `cargo tree -p cbscore --depth 3` reference is gone from §End-of-phase
  acceptance.
- The new check (plan lines 338–342) is:
  ```bash
  grep -nE 'use crate::(config|runner|builder|releases|images)' \
      cbsd-rs/cbscore/src/utils/subprocess.rs \
      cbsd-rs/cbscore/src/utils/git.rs \
      cbsd-rs/cbscore/src/utils/git/errors.rs
  ```
- All three target files are named: `subprocess.rs`, `git.rs`, and
  `git/errors.rs`. The `git/errors.rs` path is consistent with Commit 4's file
  spec (`cbsd-rs/cbscore/src/utils/git/errors.rs`).
- The expected outcome "zero matches" is stated (plan line 345: "Expected: zero
  matches").
- The one-sentence explanation (plan lines 345–349) explains that any match
  breaks the lift-out invariant by introducing a cross-module `use` into the
  named subtrees, and notes the check is cheap enough for pre-commit or CI.
- The prose correctly explains _why_ `cargo tree` was unsound (plan lines
  332–336): crate-level transitive deps from Phase 3's `aws-sdk-s3` and
  `vaultrs` additions would produce false positives.

---

### M1 — Commits 3 and 5 below 200-line floor with no rationale — CLOSED

**Closing commit:** `d847f3d`

**Confirmed:**

- Commit 3 (plan lines 193–198): "Commit-size rationale" paragraph states ~150
  LOC is below the 400-line sweet spot and explains the reason: `images/` module
  tree is a Phase 5 extension point; bundling with `utils::buildah` would
  conflate unrelated subsystem namespaces and complicate review. Coherent and
  persuasive.
- Commit 5 (plan lines 256–263): "Commit-size rationale" paragraph states ~230
  LOC sits at the lower end of the sweet spot, stands alone because the parse
  family is semantically distinct from the subprocess wrappers, introduces a new
  top-level `versions::` module, and lands as a clean "closes the Phase 1 §Out
  of scope drift" boundary in `git log`. Also notes the Commit 4 + 5 bundle
  (~730 LOC) would be within the sweet spot but conflate two unrelated
  subsystems. Coherent and well-argued.
- Both paragraphs are prose-clean (no grammatical issues found).

---

### M2 — `SkopeoOpts` single `tls_verify` vs. per-side CLI flags — CLOSED

**Closing commit:** `d847f3d`

**Confirmed:**

- Commit 3 §Design constraints (plan lines 171–188): `SkopeoOpts` is now
  declared with per-side fields:
  ```rust
  pub struct SkopeoOpts {
      pub src_tls_verify: bool,
      pub dst_tls_verify: bool,
      pub src_creds:      Option<RegistryCreds>,
      pub dst_creds:      Option<RegistryCreds>,
  }
  ```
- The implementer cross-check note is present (plan lines 184–188): "The
  implementer should cross-check `cbscore/images/skopeo.py` at commit time and
  confirm the Python wrapper exposes the same per-side semantics — if Python
  collapses them into a single boolean, decide whether to widen the API or match
  Python literally."
- §Testable (plan lines 202–206) asserts per-side flag mapping: `skopeo_copy`
  produces `--src-tls-verify=<bool> --dest-tls-verify=<bool>` with each flag
  mapped from the matching `SkopeoOpts` field; `src_creds` / `dst_creds` produce
  `--src-creds` / `--dest-creds` when `Some`.
- `RegistryCreds` usage is correct: Phase 1 Commit 3 defines it as a single-leaf
  type with no built-in symmetry assumption; using it independently per side via
  `Option<RegistryCreds>` is well-typed and contains no implicit coupling.

---

### S1 — `git_ls_remote` consumer not flagged in §Out of scope — CLOSED

**Closing commit:** `d847f3d`

**Confirmed:**

- §Out of scope (plan lines 54–59) now includes a dedicated bullet naming
  `git_ls_remote`'s consumer: "`version_create_helper`" in
  `cbscore::versions::create`, with the design 002 §Version creation lines
  706–711 citation, and an explicit statement that the orchestrator and the
  `cbsbuild versions create` CLI surface both land in Phase 6.
- The bullet is precise about what lands in Phase 2 (the raw `git_ls_remote`
  wrapper, Commit 4) versus what is deferred (the resolution logic that iterates
  over a component list). No ambiguity about where the wrapper function itself
  lives (Commit 4 / Phase 2 / `cbscore::utils::git`).

---

### S2 — RAII drop-guard not exercised in §Testable — CLOSED

**Closing commit:** `d847f3d`

**Confirmed:**

- Commit 1 §Testable (plan lines 110–116) adds the smoke test: spawn `sleep 60`
  via `async_run_cmd` inside a `tokio::select!` with a 50ms timer; let the timer
  branch win and cancel the subprocess branch; capture the child PID from
  `RunOpts` or a test hook and verify `nix::sys::signal::kill(pid, None)`
  returns `Err(Errno::ESRCH)` (process gone).
- Marked optional with `#[ignore]` if flaky in CI — present in the text.
- Tied to Phase 4 SIGTERM propagation rationale — present in the text (plan line
  115–116: "Verifies the outer-cancellation kill path the runner relies on for
  SIGTERM propagation (Phase 4).").
- The test method is concrete and matches the design 002 contract exactly.

---

## Fresh-Eyes Sweep

### LOC arithmetic

500 + 400 + 150 + 500 + 230 = 1780. Matches the "~1780 LOC, 5 commits" header.
No inconsistency.

### Phase 1 §Out of scope cross-reference integrity

Phase 1 §Out of scope now lists all six parse-family functions and correctly
states they land in Phase 2. Phase 1 Commit 4 (plan line 233) retains the
corresponding cross-reference: "the parse-family signatures … live in
`cbscore::versions::utils` and land in Phase 2 — see § Out of scope above." No
broken cross-reference introduced.

### `RegistryCreds` per-side usage vs. type definition

`RegistryCreds` in Phase 1 Commit 3 is "tagged on `creds` — single leaf per
outer value." Using it as `Option<RegistryCreds>` twice in `SkopeoOpts` (once
`src_creds`, once `dst_creds`) is a straightforward independent use of an
`Option`-wrapped type. No implicit symmetry assumption is embedded in the type;
each side independently holds its own credential or `None`. No contradiction.

### S1 bullet: `git_ls_remote` wrapper placement

The S1 bullet correctly defers only the consumer (`version_create_helper`
resolution logic), not the wrapper function itself. `git_ls_remote` remains in
Commit 4 / `cbscore::utils::git`. No ambiguity introduced.

### README re-check

README unchanged since v4. Phase 2 entry, commit-count column ("4–5"), and Phase
2 file link are all correct. No regression.

### Commit 3 design-constraints duplication (new observation)

Phase 2 Commit 3 §Design constraints (plan lines 169–170 and 190–191) contains a
verbatim duplicate pair of bullets:

```
- Subprocess via `utils::subprocess::async_run_cmd`.
- Errors return `ImageDescriptorError` from `cbscore-types::images::errors`.
```

These two lines appear once near the top of the §Design constraints block (after
the `SkopeoOpts` struct) and again immediately below the `SkopeoOpts` inline
block. The duplication is a copy-paste artifact from the M2 rewrite; it does not
affect correctness or implementer behaviour. The second pair (lines 190–191) is
the redundant one and can be removed.

---

## New Findings

### N-M1 (MINOR) — `nix` crate undeclared in `cbscore` dep sketch

**Where:** Phase 2, Commit 1 §Testable (plan line 114); Phase 1, Commit 1 §Files
/ Cargo.toml spec (plan lines 96–98).

**What the plan says:** The RAII drop-guard smoke test (S2 closure in `d847f3d`)
calls `nix::sys::signal::kill(pid, None)` to verify process reaping.

**What the dep specs say:** Design 001 §Cargo Sketch lists `cbscore`'s
`[dependencies]` (lines 380–423). `nix` is not listed. Phase 1 Commit 1's
`cbscore/Cargo.toml` spec (plan lines 96–98) says `cbscore` depends on
`cbscore-types`, `tokio` full, `tracing`, `regex`, `which`, and "the IO-side
crates that fill in over Phases 2–5" — but does not enumerate `nix` by name.

**Failure mode:** At implementation time, the test references
`nix::sys::signal::kill` but `nix` is absent from `Cargo.toml`. The symptom is a
compile error on the test module. More importantly, the dep sketch in design 001
and Phase 1 is the blueprint for what the implementer writes into `Cargo.toml`
on Day 1. If `nix` is not in the sketch, it won't be added, and the test either
silently disappears or the implementer substitutes a weaker check (e.g.,
`/proc/<pid>` file existence, which is racy).

**Severity context:** This affects only the optional smoke test, not any
production code path. The test is already marked `#[ignore]`-able. The failure
is a compile-time error that the implementer will catch immediately; it cannot
cause a silent correctness regression. This is minor, not a blocker.

**Resolution:** Add `nix` to the `cbscore` Cargo.toml spec in Phase 1 Commit 1
§Files, e.g.:

```toml
nix = { version = "0.29", features = ["signal"] }   # RAII test: kill(pid, 0) → ESRCH
```

The comment ties the dep to the test rationale. Alternatively, add a
parenthetical in the Phase 2 Commit 1 §Testable note: "(requires adding `nix`
with the `signal` feature to `cbscore/Cargo.toml` — not in design 001 §Cargo
Sketch; add at implementation time)." Either phrasing closes the gap.

---

### N-Nit1 (NIT) — Commit 3 design-constraints duplicate bullet pair

**Where:** Phase 2, Commit 3 §Design constraints (plan lines 169–170 and
190–191).

Two bullets — "Subprocess via `utils::subprocess::async_run_cmd`" and "Errors
return `ImageDescriptorError` from `cbscore-types::images::errors`" — appear
verbatim twice in the same block. The second pair (lines 190–191) is a
copy-paste artifact from the M2 rewrite and can be deleted. No correctness
impact; purely cosmetic.

---

## Verdict

**All six v4 findings are confirmed closed. One new minor finding (N-M1: `nix`
undeclared in the `cbscore` dep sketch) and one cosmetic nit (N-Nit1: duplicate
bullet pair in Commit 3) were found during the fresh-eyes sweep.**

**New findings by severity:** 0 blockers (CRITICAL), 0 IMPORTANT, 1 MINOR
(N-M1), 1 nit (N-Nit1, non-blocking, cosmetic).

**Phase 1 + Phase 2 together are ready for implementation start.** N-M1 is
non-blocking: it affects only an optional smoke test and will surface as an
immediate compile error at implementation time rather than a silent regression.
The implementer should add `nix` with the `signal` feature to
`cbscore/Cargo.toml` when landing Commit 1, before writing the smoke test in the
same commit.
