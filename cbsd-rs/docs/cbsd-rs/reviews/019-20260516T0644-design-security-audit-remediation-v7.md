# Review: Design 019 — Security Audit Remediation v7

| Field       | Value                                                                 |
| ----------- | --------------------------------------------------------------------- |
| Review      | 019-20260516T0644-design-security-audit-remediation-v7                |
| Date        | 2026-05-16                                                            |
| Design      | `docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` |
| Sibling ref | WCP design seq 019, timestamp 20260426T1154 (v11)                     |
| Scope       | v7 closure of NF-1-v6, NF-6, NF-7, NF-8; source-validation of         |
|             | D13-T6 sketch against actual `cbsd-proto` source                      |
| Reviewer    | Independent (hostile reviewer stance)                                 |
| Predecessor | `019-20260516T0626-design-security-audit-remediation-v6.md`           |

---

## Summary

v7 closes all four v6 findings. The critical defect —
`BuildDescriptor::default()` not compiling — is resolved correctly: the sketch
now uses a `test_descriptor()` helper that constructs the type explicitly, field
by field, mirroring the existing `ws.rs` test at lines 148-172. Every
type-shape, variant, field name, and default-availability claim in the v7 prose
(design lines 2143-2160) has been independently verified against the source. The
D13-T6 sketch will compile against the real `cbsd-proto` crate.

Three minor observations are carried forward as new findings (none are inherited
from v6). The most actionable is that the sketch's additional `use` declarations
at design lines 2185-2189 duplicate imports already present in the existing
`mod tests` block, producing unused-import warnings under `-D warnings`. The
other two are low-severity scaffolding notes.

**Top findings by severity (all new-in-v7):**

1. **NF-1-v7 (Minor)** — duplicate `use` declarations in sketch will trigger
   unused-import warnings.
2. **NF-2-v7 (Minor)** — `case_tags: HashSet` is built but effectively unused
   (consumed only by `let _ = &case_tags`); clippy will flag it.
3. **NF-3-v7 (Minor, residual from v6 known-gap)** — "witness updated, case
   forgotten" path remains unautomated; mitigated by PR review only.

None of these are blockers. The design is ready for implementation planning.

---

## Source-Validation Results

Source files read directly. Findings verified at the specified line numbers.

| #   | Claim (v7 prose, lines 2143-2160)                                                                                                                                 | Source location                     | Finding                                                                                                                                                                | Verdict                                                                                 |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| 1   | `BuildId` is `pub struct BuildId(pub i64)`; no `Default`                                                                                                          | `build.rs:18-19`                    | `pub struct BuildId(pub i64)`. No `Default` in derive list.                                                                                                            | **Matches**                                                                             |
| 2   | `Priority` derives `Default`; `#[default]` on `Normal`; serde `rename_all = "lowercase"`                                                                          | `build.rs:28-36`                    | `#[derive(Default)]` on separate line; `#[default]` on `Normal`; `#[serde(rename_all = "lowercase")]` present. `priority_default` test at `build.rs:190-192` confirms. | **Matches**                                                                             |
| 3   | `BuildDescriptor` does NOT impl `Default`                                                                                                                         | `build.rs:121-132`                  | Derive list: `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`. No `Default`.                                                                                      | **Matches**                                                                             |
| 4   | `BuildSignedOffBy` does NOT impl `Default`                                                                                                                        | `build.rs:76-80`                    | Derive list: `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`. No `Default`.                                                                                      | **Matches**                                                                             |
| 5   | `BuildDestImage` does NOT impl `Default`                                                                                                                          | `build.rs:83-87`                    | Derive list: `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`. No `Default`.                                                                                      | **Matches**                                                                             |
| 6   | `BuildComponent` does NOT impl `Default`                                                                                                                          | `build.rs:91-98`                    | Derive list: `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`. No `Default`.                                                                                      | **Matches**                                                                             |
| 7   | `BuildTarget` does NOT impl `Default`                                                                                                                             | `build.rs:101-109`                  | Derive list: `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`. No `Default`.                                                                                      | **Matches**                                                                             |
| 8   | `Arch` does NOT impl `Default`; `X86_64` matches `default_arch()`                                                                                                 | `arch.rs:20-26`; `build.rs:115-117` | `Arch` derive list has no `Default`. `fn default_arch() -> Arch { Arch::X86_64 }` at `build.rs:115-117`.                                                               | **Matches**                                                                             |
| 9   | `ServerMessage::Welcome` fields: `protocol_version: u32`, `connection_id: String`, `grace_period_secs: u64`                                                       | `ws.rs:41-46`                       | Exact match.                                                                                                                                                           | **Matches**                                                                             |
| 10  | `ServerMessage::Error` fields: `reason: String`, `min_version: Option<u32>`, `max_version: Option<u32>`                                                           | `ws.rs:49-53`                       | Exact match.                                                                                                                                                           | **Matches**                                                                             |
| 11  | `ServerMessage::BuildNew` fields: `build_id`, `trace_id`, `priority`, `descriptor`, `component_sha256`                                                            | `ws.rs:27-33`                       | `build_id: BuildId`, `trace_id: String`, `priority: Priority`, `descriptor: Box<BuildDescriptor>`, `component_sha256: String`. Exact match.                            | **Matches**                                                                             |
| 12  | `ServerMessage::BuildRevoke` has single `build_id` field (today)                                                                                                  | `ws.rs:38`                          | `BuildRevoke { build_id: BuildId }`. Exact match; post-D13 `reason` field not yet present.                                                                             | **Matches**                                                                             |
| 13  | Existing test pattern at `ws.rs:142-182` (model for `test_descriptor()`)                                                                                          | `ws.rs:142-182`                     | `server_message_build_new_round_trip` constructs `BuildDescriptor` with all 7 fields explicit; same field order and types as v7 `test_descriptor()`.                   | **Matches**                                                                             |
| 14  | `use crate::arch::Arch; use crate::build::{BuildComponent, BuildDescriptor, BuildDestImage, BuildSignedOffBy, BuildTarget}` resolve against real module structure | `lib.rs:13-21`                      | `pub mod arch;`, `pub mod build;`, `pub mod ws;`. All named types are `pub` in `build.rs`. `Arch` is `pub` in `arch.rs`. Paths resolve correctly.                      | **Matches (with NF-1-v7 caveat — these are duplicate inside the existing test module)** |

**Compile-correctness verdict:** The v7 sketch will compile against the real
`cbsd-proto` crate. `test_descriptor()` constructs every field explicitly using
valid types. `sentinel_for_tag` uses `test_descriptor()` (not
`BuildDescriptor::default()`). The class of defect caught in v5 and v6 is
eliminated.

**Serde round-trip correctness:** `serde_json::to_value(test_descriptor())` will
produce a valid JSON object because `BuildDescriptor` derives `Serialize` and
all leaf types are `String`, `Option<String>`, `Vec<BuildComponent>`, and nested
structs with only `String` fields (plus `Arch` which derives `Serialize`). The
resulting JSON will deserialize back through `BuildDescriptor::deserialize`
without error because all required fields are present and correctly typed.

**`priority` field placement:** In the `"build_new"` case JSON (design line
2298), `"priority": "normal"` is at the message level, not nested inside
`descriptor`. This matches the source: `priority` is a field of
`ServerMessage::BuildNew` (`ws.rs:30`), not of `BuildDescriptor`
(`build.rs:121-132`). Correct.

**`build_revoke` forward-compatibility:** The `"build_revoke"` case (design
lines 2305-2315) includes only `"build_id": 42` and `"future_field": "x"` (no
`"reason"` field). `BuildRevoke` today has only `build_id: BuildId`
(`ws.rs:38`); serde uses the tagged enum's internal tag (`"type"`) without
`deny_unknown_fields`, so the unknown `"future_field"` field is silently
discarded. Post-D13, when `reason: Option<BuildRevokeReason>` is added with
`#[serde(default)]`, a payload without `"reason"` will still deserialize with
`reason: None`. The case is correctly forward-compatible.

**Same-crate `#[non_exhaustive]` claim (design lines 2407-2408):** Correct. The
`#[non_exhaustive]` attribute restricts **external-crate** pattern matching to
require a `_` wildcard arm. Within the same crate, match exhaustiveness operates
on the complete variant set regardless of `#[non_exhaustive]`. Since D13-T6
lives in `cbsd-proto/src/ws.rs`'s own `mod tests`, the exhaustive-match
guarantee is preserved even if `ServerMessage` gains `#[non_exhaustive]` in the
future.

---

## v6 Finding Closure Table

| Finding | v6 Severity | v7 Claim | This Review | Notes                                                                                         |
| ------- | ----------- | -------- | ----------- | --------------------------------------------------------------------------------------------- |
| NF-1-v6 | Critical    | Closed   | **Closed**  | `test_descriptor()` replaces all `::default()` calls. Sketch compiles.                        |
| NF-6    | Minor       | Closed   | **Closed**  | Hardcoded tag list removed. Runtime loop iterates `cases()` directly.                         |
| NF-7    | Minor       | Closed   | **Closed**  | `minimal_descriptor_json()` replaced by `test_descriptor_json()`, defined inline.             |
| NF-8    | Minor       | Closed   | **Closed**  | Test placement specified: `cbsd-proto/src/ws.rs::tests`. Rationale stated at lines 2170-2179. |

All four v6 findings are closed. The Critical finding class (test sketch fails
to compile) is genuinely eliminated.

---

## New Findings

### NF-1-v7 — Duplicate `use` Declarations in Sketch (Minor)

**Location:** Design lines 2185-2191 (sketch preamble inside `mod tests`).

**Problem:** The sketch opens with:

```rust
use crate::arch::Arch;
use crate::build::{
    BuildComponent, BuildDescriptor, BuildDestImage,
    BuildSignedOffBy, BuildTarget,
};
use serde_json::{Value, json};
use std::collections::HashSet;
```

The existing `mod tests` block in `cbsd-proto/src/ws.rs` (lines 135-140) already
contains:

```rust
use super::*;
use crate::arch::Arch;
use crate::build::{
    BuildComponent, BuildDestImage, BuildSignedOffBy, BuildTarget, VersionType,
};
```

`use super::*` at line 136 re-exports everything from the outer `ws.rs` scope,
which includes `use crate::build::{BuildDescriptor, BuildId, Priority}` (line
15). Combined with the explicit imports at lines 137-140, the following are
already in scope when D13-T6's test function is added:

- `Arch` — already imported at line 137
- `BuildComponent`, `BuildDestImage`, `BuildSignedOffBy`, `BuildTarget` —
  already imported at lines 138-140
- `BuildDescriptor`, `BuildId`, `Priority` — via `use super::*` from line 15

The sketch's repeated `use crate::arch::Arch;` and `use crate::build::{…}` lines
are therefore **duplicate imports**. Under `cargo clippy -D warnings` (or the
workspace's default clippy configuration), these will produce `unused_imports`
warnings that fail the pre-commit check.

`use serde_json::{Value, json};` and `use std::collections::HashSet;` are NOT
duplicates — the existing `mod tests` block does not import these.

**Impact:** Minor. The sketch will not compile cleanly under `-D warnings` as
written. Phase E will need to remove the duplicate `use crate::arch::Arch` and
`use crate::build::{…}` lines and retain only `use serde_json::{Value, json};`
and `use std::collections::HashSet;` as new additions.

**Recommendation:** Update the sketch's import block to:

```rust
// New imports only — the existing mod tests block already provides
// Arch, BuildComponent, BuildDescriptor, BuildDestImage,
// BuildSignedOffBy, BuildTarget, BuildId, Priority via use super::*
// and the pre-existing explicit imports.
use serde_json::{Value, json};
use std::collections::HashSet;
```

This is a one-line design fix; Phase E can also resolve it on sight.

---

### NF-2-v7 — Dead `case_tags` Binding (Minor)

**Location:** Design lines 2336-2337 and 2396 (test body of
`no_deny_unknown_fields_on_server_message`).

**Problem:** The test builds a `HashSet`:

```rust
let case_tags: HashSet<&'static str> =
    cases_list.iter().map(|(t, _)| *t).collect();
```

The only consumer is:

```rust
let _ = &case_tags;
```

at line 2396, which is explicitly a no-op. The comment says it is "kept so a
future check can be added without restructuring." This is dead scaffolding: it
allocates a `HashSet` at test runtime, is never used for any assertion, and
triggers clippy's `unused_variables` lint (or at minimum `dead_code` analysis).
In a workspace with `-D warnings`, this produces a warning that blocks the
pre-commit check.

**Impact:** Minor. Same class as NF-1-v7: the sketch does not compile clean
under strict clippy.

**Recommendation:** Either:

(a) Remove `case_tags` entirely. It is not needed by either loop. When the
future "is every witnessable variant in cases?" check is added, it can
reintroduce the `HashSet` at that time.

(b) If the scaffolding intent is important to preserve, make it a compile-time
assertion using `const` or a doc-comment explanation — not a runtime allocation
that clippy will flag.

Option (a) is the cleaner fix and aligns with the rust-2024 skill's anti-pattern
guidance against dead code.

---

### NF-3-v7 — Residual "Witness Updated, Case Forgotten" Gap (Minor, known)

**Location:** Design lines 2420-2429 ("Known gap" admission).

**Problem:** The design correctly acknowledges this gap and describes its
mitigation (PR review + witness source comment). This finding is included for
completeness and to track that the gap remains open across review cycles.

The gap: a developer adds a new `ServerMessage` variant, fixes the compile error
in `variant_tag_witness` (forced), adds a `sentinel_for_tag` arm (forced
eventually by the runtime panic when the case is exercised), but forgets to add
the `cases()` entry. The new variant is not covered by the SI-18 test.

The design's proposed future mitigation (`strum` or similar enum-introspection)
is reasonable. An alternative that requires no external crates: a compile-time
`const` count of witness arms (via a `const fn`) compared against
`cases().len()` asserted in the test. This is not trivial in stable Rust because
counting match arms at compile time is not straightforward, but it is worth
noting as a path that does not require `strum`.

**Impact:** Minor. The compile-time witness gates the primary failure mode. The
case-forgotten sub-path produces a test suite that passes but silently under-
covers a new variant. PR review is a real gate.

**Recommendation:** No immediate action required. Acknowledge in Phase E
implementation notes. If `strum` is adopted elsewhere in the workspace, upgrade
the test at that point.

---

## Strengths

v7 makes a genuine, complete fix to the Critical NF-1-v6 defect:

- **`test_descriptor()` is correct and sufficient.** It constructs all seven
  fields of `BuildDescriptor` explicitly, matching the existing
  `server_message_build_new_round_trip` pattern. Field names, types, and values
  are all valid. The helper is `#[cfg(test)]`-scoped (inside `mod tests`), so it
  does not pollute the production API.
- **`test_descriptor_json()` via `serde_json::to_value` is the right approach.**
  It reuses the production `Serialize` impl rather than maintaining a parallel
  hand-coded JSON blob. If `BuildDescriptor`'s schema changes, the compilation
  of `test_descriptor()` breaks first, alerting the developer.
- **NF-6 closure is clean.** Removing the hardcoded tag list and iterating
  `cases()` directly is the correct fix. The "three layers / three coordinated
  lists" description is now accurate.
- **Test placement rationale (lines 2170-2179) is precise.** The explanation of
  why same-crate placement is load-bearing, and why `#[non_exhaustive]` does not
  break the guarantee within the crate, is technically correct and clearly
  written.
- **`sentinel_for_tag`'s panic arm** is the right design for catching
  "sentinel-missing" at runtime during the next test run after a case is added.
  The panic message is actionable.
- **Forward-compatibility of the `build_revoke` case** is correctly handled: the
  `reason` field is absent from the JSON payload, which will continue to work
  after D13 because the field will be `Option<…>` with `#[serde(default)]`.
- **Source-validation claim** (lines 2139-2156) is a genuine improvement: v7
  explicitly states it verified every field claim against the source, and the
  verification holds. This is the right process change after two cycles of
  compile-error defects.

---

## Open Questions / Deferred Items

1. **NF-1-v7 / NF-2-v7 resolution before Phase E commit:** Phase E should remove
   the duplicate `use` declarations and the unused `case_tags` binding before
   the commit lands. These are easily spotted during implementation; the design
   need not be updated again if the Phase E commit fixes them directly.

2. **`strum` adoption path for the known gap:** If the project later adopts
   `strum` for enum iteration, D13-T6 can be upgraded to a four-layer scheme
   without a design revision. A `TODO` comment in the test body is sufficient to
   capture this intent.

3. **`variant_tag_witness` return type:** The function returns `&'static str`.
   This is fine for the current use (equality checks). If the function is later
   used for routing or lookup, consider whether a typed enum tag (e.g., a
   `MessageTag` enum) would be safer. Not a blocker.

---

## Confidence Score

| Item                                                                                                                            | Points | Criterion                                                             |
| ------------------------------------------------------------------------------------------------------------------------------- | ------ | --------------------------------------------------------------------- |
| Starting score                                                                                                                  | 100    |                                                                       |
| NF-1-v7: duplicate `use crate::arch::Arch` and `use crate::build::{…}` in sketch; unused_imports under -D warnings              | -5     | D10 — convention violation (pre-commit check will reject)             |
| NF-2-v7: `case_tags: HashSet` built and immediately suppressed with `let _ = &case_tags`; clippy dead-code/unused-variable lint | -5     | D6 — dead code (unused runtime allocation)                            |
| NF-3-v7: "witness updated, case forgotten" gap unautomated; PR review only                                                      | -5     | D11 — missing automated gate for a documented maintenance requirement |
| **Total**                                                                                                                       | **85** |                                                                       |

Score 85/100 — acceptable; address noted issues before Phase E commit lands (per
confidence-scoring scale: 75-89 means "acceptable with noted improvements, fix
before next stage").

The three deductions are all minor implementation-detail issues that Phase E can
resolve on the fly without a design revision. None affect the correctness of the
invariant being enforced.

---

## Go / No-Go

**Go for implementation planning.**

The D13-T6 sketch is compile-correct against the real `cbsd-proto` source. All
four v6 findings are closed. The design is implementable.

**Phase E actions (non-blocking for planning, required before commit):**

- **P1:** When adding the D13-T6 test to `cbsd-proto/src/ws.rs`, extend the
  existing `use` block minimally — add only `use serde_json::{Value, json};` and
  `use std::collections::HashSet;`. Do not re-declare `use crate::arch::Arch;`
  or `use crate::build::{…}` imports already present. (Closes NF-1-v7.)
- **P2:** Remove the `case_tags: HashSet` binding and the `let _ = &case_tags;`
  suppression. Neither loop uses it. (Closes NF-2-v7.)
- **P3:** Add a `// TODO: upgrade to four-layer scheme if strum is adopted`
  comment near the "Known gap" block if the project wants to track this.
  (Acknowledges NF-3-v7.)

P1 and P2 are one-line-each cleanups. P3 is optional.
