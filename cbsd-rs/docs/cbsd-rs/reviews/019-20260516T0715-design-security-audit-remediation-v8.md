# Review: Design 019 — Security Audit Remediation v8

| Field          | Value                                                                 |
| -------------- | --------------------------------------------------------------------- |
| Review         | 019-20260516T0715-design-security-audit-remediation-v8                |
| Date           | 2026-05-16                                                            |
| Design         | `docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` |
| Sibling ref    | WCP design seq 019, timestamp 20260426T1154 (v11)                     |
| Scope          | v8 closure of NF-1-v7, NF-2-v7, NF-3-v7; source-validation of the     |
|                | revised D13-T6 sketch (strum EnumIter, tag enum, cascade narrative)   |
| Reviewer       | Independent (hostile reviewer stance)                                 |
| Predecessor    | `019-20260516T0644-design-security-audit-remediation-v7.md`           |
| Recommendation | Go for implementation planning (two new Nits noted)                   |

---

## Summary

v8 genuinely closes all three Minor findings from the v7 review. NF-1-v7
(duplicate imports) is fixed by removing the offending lines and adding only
`use serde_json::{Value, json};` and `use strum::IntoEnumIterator;`. NF-2-v7
(dead `case_tags` allocation) is fixed by replacing the `HashSet` scaffold with
a `HashMap<&'static str, Value>` that is actually used for lookup. NF-3-v7
(unautomated "witness updated, case forgotten" gap) is closed substantively via
a `#[cfg(test)]` companion enum `ServerMessageTag` with
`#[derive(strum::EnumIter)]`, making the cascade fully compile-forced except for
the `cases()` lookup — which is now runtime-asserted in the same loop. Every
source-code claim in the v8 revision history and sketch header has been
independently verified against `cbsd-proto/src/ws.rs`,
`cbsd-proto/src/build.rs`, `cbsd-proto/src/arch.rs`, `cbsd-proto/src/lib.rs`,
and `cbsd-proto/Cargo.toml`.

Two new Minor findings are introduced in v8:

1. **NF-1-v8 (Minor)** — the revision-history summary (line 55) says the cascade
   has "three compile-time gates" while the body at lines 2516-2519 correctly
   enumerates four. `sentinel_for_tag`'s match on `ServerMessageTag` is
   exhaustive and therefore compile-time, not runtime as the summary implies.
   The body text is correct; the summary is wrong.
2. **NF-2-v8 (Minor / Nit)** — `#[derive(strum::EnumIter, …, Hash)]` on
   `ServerMessageTag` includes `Hash` which is not used in the test body. The
   `cases_map` is keyed by `&'static str`, not by `ServerMessageTag`. `Hash` is
   dead derives on a test-only type.

Neither finding is a blocker. The design is ready for implementation planning.

**Top findings by severity (all new-in-v8):**

1. **NF-1-v8 (Minor)** — cascade gate count mismatch: summary says 3, body says
   4 (body is correct).
2. **NF-2-v8 (Minor / Nit)** — gratuitous `Hash` derive on `ServerMessageTag`;
   unused in the test.

---

## Source-Validation Results

All source files read directly. Line numbers cite the actual file state at time
of review.

| #   | Claim (v8 revision history / sketch header)                                                                                                                                                                                           | Source location                           | Finding                                                                                                                                                                                                                                                                             | Verdict                                                      |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| 1   | `cbsd-proto/Cargo.toml` has no `[dev-dependencies]` section today                                                                                                                                                                     | `cbsd-proto/Cargo.toml:1-13`              | File has `[package]` and `[dependencies]` sections only (`serde`, `serde_json`, `chrono`). No `[dev-dependencies]`.                                                                                                                                                                 | **Matches**                                                  |
| 2   | Workspace pins only `serde`, `serde_json`, `chrono`                                                                                                                                                                                   | `cbsd-rs/Cargo.toml:10-13`                | `[workspace.dependencies]` lists exactly those three. No `strum`. Per-crate `[dev-dependencies]` is correct placement.                                                                                                                                                              | **Matches**                                                  |
| 3   | Existing `mod tests` block at `ws.rs:134-140` imports `use super::*`, `use crate::arch::Arch;`, and `use crate::build::{BuildComponent, BuildDestImage, BuildSignedOffBy, BuildTarget, VersionType}`                                  | `ws.rs:134-140`                           | Lines 136-140 match exactly. `use super::*` is line 136; `Arch` import line 137; `build` import lines 138-140 (same 5 types, comma-separated across lines).                                                                                                                         | **Matches**                                                  |
| 4   | `use super::*` brings `BuildDescriptor`, `BuildId`, `Priority` into test scope via the top-of-file import at `ws.rs:15`                                                                                                               | `ws.rs:15`                                | `use crate::build::{BuildDescriptor, BuildId, Priority};` at line 15; `use super::*` re-exports it into `mod tests`. All three are in scope.                                                                                                                                        | **Matches**                                                  |
| 5   | v8 sketch adds only `use serde_json::{Value, json};` and `use strum::IntoEnumIterator;` — no duplicates                                                                                                                               | Design lines 2262-2263 vs `ws.rs:134-140` | `serde_json` not present in existing test imports; `strum` not present. Both additions are non-duplicating.                                                                                                                                                                         | **Matches**                                                  |
| 6   | `strum = { version = "0.26", features = ["derive"] }` is the correct spec for `EnumIter` derive                                                                                                                                       | strum 0.26 API (knowledge base)           | `EnumIter` is provided by `strum_macros`, re-exported via `strum` when `features = ["derive"]` is enabled. `IntoEnumIterator::iter()` is the resulting trait method. The spec is correct.                                                                                           | **Matches**                                                  |
| 7   | `strum::EnumIter` on a `#[cfg(test)]` enum generates a `#[cfg(test)]`-gated `impl IntoEnumIterator`                                                                                                                                   | proc-macro expansion semantics            | Proc-macro derives expand at the attribute site. A `#[cfg(test)] enum` causes the compiler to not compile the derive input in non-test builds, so the generated impl is also absent. `strum` stays out of release builds.                                                           | **Matches**                                                  |
| 8   | `test_descriptor()` field shapes match `build.rs:121-132` and nested types                                                                                                                                                            | `build.rs:76-132`, `arch.rs:20-26`        | `BuildDescriptor`: 7 fields (`version`, `channel`, `version_type`, `signed_off_by`, `dst_image`, `components`, `build`). All nested types (`BuildSignedOffBy`, `BuildDestImage`, `BuildComponent`, `BuildTarget`) have the field names used in the sketch. `Arch::X86_64` is valid. | **Matches**                                                  |
| 9   | `ServerMessageTag` variant names match real `ServerMessage` variants                                                                                                                                                                  | `ws.rs:24-54`                             | `ServerMessage` has exactly 4 variants: `BuildNew`, `BuildRevoke`, `Welcome`, `Error`. Tag enum has exactly 4 variants with the same names.                                                                                                                                         | **Matches**                                                  |
| 10  | `from_message` is exhaustive on `ServerMessage` and arm bodies reference valid `ServerMessageTag` variants                                                                                                                            | Design lines 2325-2332 vs `ws.rs:24-54`   | All 4 `ServerMessage` variants have arms; each arm body (`Self::BuildNew`, etc.) names a real `ServerMessageTag` variant. Both exhaustive-match gates fire.                                                                                                                         | **Matches**                                                  |
| 11  | `as_wire` arms produce the serde `"type"` discriminators matching `#[serde(rename_all = "snake_case")]` on `ServerMessage`                                                                                                            | `ws.rs:23`, design lines 2337-2344        | `ServerMessage` has `#[serde(tag = "type", rename_all = "snake_case")]`. Mapping: `BuildNew→"build_new"`, `BuildRevoke→"build_revoke"`, `Welcome→"welcome"`, `Error→"error"`. All correct.                                                                                          | **Matches**                                                  |
| 12  | `sentinel_for_tag` field shapes match real variant fields                                                                                                                                                                             | Design lines 2352-2375 vs `ws.rs:27-53`   | `BuildNew`: 5 fields, all correct types. `BuildRevoke`: 1 field `build_id: BuildId`. `Welcome`: 3 fields. `Error`: 3 fields, `min_version`/`max_version` are `Option<u32>`. All match.                                                                                              | **Matches**                                                  |
| 13  | `cases()` payload shapes match variant field schemas                                                                                                                                                                                  | Design lines 2388-2427 vs `ws.rs:27-53`   | `build_new`: `build_id`, `trace_id`, `priority`, `descriptor`, `component_sha256` — all present, plus `future_field`. `build_revoke`: `build_id`, `future_field`. `welcome`: 3 fields, `future_field`. `error`: `reason`, `min_version`, `max_version`, `future_field`. All match.  | **Matches**                                                  |
| 14  | `Priority::default()` returns `Normal`; serializes as `"normal"`                                                                                                                                                                      | `build.rs:28-36`                          | `#[derive(Default)]`, `#[default]` on `Normal`, `#[serde(rename_all = "lowercase")]`. Confirmed.                                                                                                                                                                                    | **Matches**                                                  |
| 15  | Cascade claims: adding `ServerMessage::Foo` triggers `from_message` (compile), then `Self::Foo` on `ServerMessageTag` (compile), then `as_wire` (compile), then `sentinel_for_tag` (compile), then `cases_map.contains_key` (runtime) | Design lines 2493-2519                    | Traced manually — see Cascade Walk section below. Body narrative is accurate. Summary at line 55 incorrectly says "three compile-time gates".                                                                                                                                       | **Partial mismatch — body correct, summary wrong (NF-1-v8)** |
| 16  | `Hash` derive on `ServerMessageTag` is used in the test                                                                                                                                                                               | Design lines 2309, 2429-2490              | `cases_map` is `HashMap<&'static str, Value>`, keyed by `&str`. `tag` is never inserted into a `HashMap` or `HashSet`. `Hash` is not needed.                                                                                                                                        | **Mismatch — Hash is unused (NF-2-v8)**                      |

---

## Cascade Walk

Developer adds `ServerMessage::Foo { bar: u32 }` without touching anything else:

1. **Gate 1 — `from_message` (compile-time):** The match in
   `ServerMessageTag::from_message` is non-exhaustive. The compiler emits:
   `non-exhaustive patterns: `ServerMessage::Foo { .. }` not covered`. Error is
   clear and points directly at `from_message`.

2. **Gate 2 — tag-enum variant (compile-time):** The developer adds
   `ServerMessage::Foo { .. } => Self::Foo` to `from_message`. But `Self::Foo`
   does not exist on `ServerMessageTag`. The compiler emits:
   `no variant or associated item named `Foo`found for enum`ServerMessageTag``. Error is clear; developer adds `ServerMessageTag::Foo`.

3. **Gate 3 — `as_wire` (compile-time):** `as_wire`'s match on `Self` (=
   `ServerMessageTag`) is now non-exhaustive because `Foo` was added. Compiler
   emits the standard non-exhaustive-patterns error. Developer adds
   `Self::Foo => "foo"`.

4. **Gate 4 — `sentinel_for_tag` (compile-time):** The match in
   `sentinel_for_tag` on `ServerMessageTag` is now non-exhaustive for `Foo`.
   Compiler emits the error. Developer adds the `Foo` arm.

5. **Gate 5 — `cases_map.contains_key` (runtime):** `iter()` now yields
   `ServerMessageTag::Foo`; `as_wire()` returns `"foo"`;
   `cases_map.contains_key("foo")` fails; the test panics with the actionable
   message naming the wire tag and instructing the developer to add a case to
   `cases()`.

Each error message is reasonably actionable. The cascade design is correct. The
body text's "four compile-time gates + one runtime gate" description is
accurate. The revision-history summary's "three compile-time gates" is wrong —
`sentinel_for_tag`'s match is the fourth compile-time gate.

---

## v7 Finding Closure Table

| Finding | v7 Severity | v8 Claim | This Review | Notes                                                                                                                                              |
| ------- | ----------- | -------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| NF-1-v7 | Minor       | Closed   | **Closed**  | Duplicate imports removed. Only `serde_json::{Value, json}` and `strum::IntoEnumIterator` added. Verified non-duplicating against `ws.rs:134-140`. |
| NF-2-v7 | Minor       | Closed   | **Closed**  | Dead `case_tags: HashSet` removed. Replaced with `cases_map: HashMap` used for actual `contains_key` lookup. No dead allocation remains.           |
| NF-3-v7 | Minor       | Closed   | **Closed**  | Gap closed substantively via `ServerMessageTag` + `strum::EnumIter`. All five cascade gates verified. No known gap remains.                        |

All three v7 findings are closed.

---

## New Findings

### NF-1-v8 — Cascade Gate Count Mismatch in Revision-History Summary (Minor)

**Location:** Design line 55 (revision-history for v8).

**Problem:** The revision-history summary reads:

> Adding a `ServerMessage` variant cascades through three compile-time gates
> (witness, tag-enum, as_wire) before reaching the runtime sentinel/case gates —
> no manual list maintenance, no "known gap" remaining.

This is incorrect. `sentinel_for_tag` is an exhaustive match on
`ServerMessageTag`, making it a fourth compile-time gate (the compiler rejects a
missing arm before the test runs). The body correctly enumerates four
compile-time gates at design lines 2493-2509 and the cascade list at lines
2516-2519 explicitly marks `sentinel_for_tag` as "compile." Only the
`cases_map.contains_key` assertion is a runtime gate.

The claim "reaching the runtime sentinel/case gates" implies both the sentinel
and the case lookup are runtime. The sentinel is compile-time; only the case
lookup is runtime.

**Impact:** Minor — the body text is correct and will guide implementers
accurately. The summary is misleading but does not affect implementation. Might
confuse a reviewer reading only the revision history.

**Recommendation:** Change line 55 to:

> Adding a `ServerMessage` variant cascades through four compile-time gates
> (witness on `ServerMessage`, tag-enum variant, `as_wire`, and
> `sentinel_for_tag`) before reaching the runtime `cases_map` gate.

This aligns the summary with the body at lines 2493-2519.

---

### NF-2-v8 — Unused `Hash` Derive on `ServerMessageTag` (Minor / Nit)

**Location:** Design line 2309.

**Problem:** `ServerMessageTag` is declared as:

```rust
#[derive(strum::EnumIter, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ServerMessageTag { ... }
```

Tracing the test body (`no_deny_unknown_fields_on_server_message`, lines
2429-2490):

- `Debug` — used in `{:?}` format in `assert!` panic messages.
- `Clone` / `Copy` — used implicitly: `tag` is iterated
  (`for tag in ServerMessageTag::iter()`) and later passed to
  `sentinel_for_tag(tag)` by value.
- `PartialEq` / `Eq` — used in `assert_eq!(tag, witnessed, ...)`.
- `Hash` — `cases_map` is `HashMap<&'static str, Value>`, keyed by
  `&'static str`. `tag` is never used as a `HashMap` or `HashSet` key. `Hash` is
  not exercised anywhere in the test body.

Under `cargo clippy -D warnings`, Clippy does not currently warn about
unnecessary derives (it is not a default lint), so this does not break the
pre-commit check. However, it is non-idiomatic: the rust-2024 skill flags
deriving traits that have no callers as an unnecessary footprint.

**Impact:** Minimal. Does not break compilation or correctness. Adds an unused
trait bound to a test-only type.

**Recommendation:** Remove `Hash` from the derive list:

```rust
#[derive(strum::EnumIter, Debug, Clone, Copy, PartialEq, Eq)]
enum ServerMessageTag { ... }
```

If a future test uses `ServerMessageTag` as a map key, restore `Hash` at that
point. This is a one-token fix; Phase E can apply on sight.

---

## Strum API Verification

The design specifies `strum = { version = "0.26", features = ["derive"] }`.
strum's `derive` feature re-exports `strum_macros`, making
`#[derive(strum::EnumIter)]` available. The generated code implements
`strum::IntoEnumIterator` for the annotated enum, providing an `iter()`
associated function that yields every variant in declaration order. The import
`use strum::IntoEnumIterator;` brings the trait into scope, enabling
`ServerMessageTag::iter()`. This is the correct and canonical API. The
`#[cfg(test)]` scoping ensures the generated impl is absent from release builds.

strum is not yet used anywhere in the `cbsd-rs` workspace (grep confirms zero
matches). Adding it only to `cbsd-proto`'s `[dev-dependencies]` is the correct
scope — there is no case for a workspace-level dep when usage is isolated to a
single crate's tests.

---

## HashMap Iteration Order (Nit)

The second loop in `no_deny_unknown_fields_on_server_message` iterates
`cases_map: HashMap<&str, Value>`. HashMap iteration order is not stable. This
does not affect correctness — each case is independently asserted with no
dependency on iteration order — but test failure output will list cases in a
non-reproducible order. This is a cosmetic observation only. Switching to
`BTreeMap` would give stable iteration order at no algorithmic cost for four
entries. Not a blocking concern; mentioned for completeness.

---

## Strengths

v8 closes a substantive gap with a clean design:

- **`ServerMessageTag` + `strum::EnumIter` is the right mechanism.** The tag
  enum is test-only (`#[cfg(test)]`), requires no runtime overhead, introduces
  no production dependency, and creates four compile-time cascade gates with a
  single runtime gate at the end. This is the idiomatic Rust solution for the
  "enumerate all enum variants" problem without unsafe or macro tricks.

- **Import discipline is correct.** The v8 sketch adds exactly the two imports
  that are not already present in the existing `mod tests` block. The comment at
  design lines 2255-2261 explicitly explains why no other imports are needed and
  cites the source lines. This is the right level of documentation after two
  cycles of import-related defects.

- **`sentinel_for_tag` exhaustiveness eliminates the v7 "case forgotten" gap's
  tail.** By making `sentinel_for_tag` an exhaustive match on
  `ServerMessageTag`, adding a tag-enum variant without a sentinel arm is a
  compile error rather than a silent omission.

- **`cases_map.contains_key` assertion error message is actionable.** The panic
  message at design lines 2453-2459 names the wire tag, names the enum variant
  (via `{:?}`), and provides the file path where the fix must be made. Developer
  experience is good.

- **`cases()` payload forward-compatibility is correct.** The `build_revoke`
  case omits `reason`, matching the pre-D13 wire format. The separate
  serde-compatibility tests cover the reason-field handling explicitly.

- **Cascade walk at lines 2514-2519** precisely describes the developer
  experience, step by step. The only error is the discrepancy with the
  revision-history summary (NF-1-v8).

---

## Open Questions / Deferred Items

1. **NF-1-v8 resolution.** The revision-history summary line 55 should say "four
   compile-time gates." Phase E can fix the one word on sight; a design revision
   is not required unless the author wants the document to be fully consistent.

2. **NF-2-v8 resolution.** Remove `Hash` from the `ServerMessageTag` derive
   list. One-token Phase E fix.

3. **WCP-proposed `UnauthorizedBuildAction` and other future variants.** The
   design correctly notes (lines 2220-2226) that when the WCP variant lands, the
   developer must add it to `from_message`, `sentinel_for_tag`, and `cases`.
   Under v8's scheme, the first two are compile-forced; only the third requires
   the source comment as a guide. This is the documented, acceptable state.

4. **`resolver = "2"` in `cbsd-rs/Cargo.toml`** (pre-existing). The rust-2024
   skill recommends `resolver = "3"` for 2024-edition workspaces. This is a
   pre-existing issue outside v8's scope but worth tracking.

---

## Confidence Score

| Item                                                                                                                                              | Points | Description                                                                        |
| ------------------------------------------------------------------------------------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------- |
| Starting score                                                                                                                                    | 100    |                                                                                    |
| NF-1-v8: revision-history summary says "three compile-time gates" while body correctly says four; `sentinel_for_tag` is compile-time, not runtime | -5     | D10 — convention violation: documentation inconsistency inside the design document |
| NF-2-v8: `Hash` derive on `ServerMessageTag` is not used anywhere in the test body; `cases_map` is keyed by `&str`, not by the tag enum           | -5     | D4 — non-idiomatic: deriving traits with no callers                                |
| **Total**                                                                                                                                         | **90** |                                                                                    |

Score 90/100 — ready to proceed. Per the confidence-scoring scale (90-100:
"Ready to merge. Minor or no issues"), both deductions are cosmetic and
addressable on sight during Phase E without a further design revision.

---

## Go / No-Go

**Go for implementation planning.**

The D13-T6 sketch is source-validated and compile-correct against the real
`cbsd-proto` crate. All three v7 findings are substantively closed. The cascade
mechanism is sound and the four-gate + one runtime structure is correctly
implemented in the sketch.

**Phase E actions (non-blocking for planning, preferred before commit):**

- **P1:** In the revision-history summary at design line 55, change "three
  compile-time gates" to "four compile-time gates" and "(witness, tag-enum,
  as_wire)" to "(witness, tag-enum, as_wire, sentinel_for_tag)". (Closes
  NF-1-v8. One-sentence edit.)
- **P2:** Remove `Hash` from the `ServerMessageTag` derive list. Change
  `#[derive(strum::EnumIter, Debug, Clone, Copy, PartialEq, Eq, Hash)]` to
  `#[derive(strum::EnumIter, Debug, Clone, Copy, PartialEq, Eq)]`. (Closes
  NF-2-v8. One-token edit.)

P1 and P2 can both be applied during Phase E without triggering another review
cycle.
