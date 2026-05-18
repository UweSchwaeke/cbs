# Review: Design 019 — Security Audit Remediation v5

| Field       | Value                                                                 |
| ----------- | --------------------------------------------------------------------- |
| Review      | 019-20260516T0447-design-security-audit-remediation-v5                |
| Date        | 2026-05-16                                                            |
| Design      | `docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` |
| Sibling ref | WCP design seq 019, timestamp 20260426T1154 (v11)                     |
| Scope       | v5 closure of SF-1, SF-2, MF-1, MF-2, MF-3 from v4 review; new        |
|             | issues introduced by v5                                               |
| Reviewer    | Independent (hostile reviewer stance)                                 |
| Predecessor | `019-20260515T1059-design-security-audit-remediation-v4.md`           |

---

## Executive Summary

v5 makes real progress: MF-2 and MF-3 are fully closed, and SF-2's conditional
soft-delete contract is now coherent and actionable. SF-1 is substantially
closed — the exhaustive-match witness is compile-time sound — but a new Critical
finding supersedes it: the witness sketch in D13-T6 (lines 2025-2027) names a
`ServerMessage::UnauthorizedBuildAction` variant that does not exist in the
production `cbsd-proto/src/ws.rs` file. The design's test code would fail to
compile against the real crate. Additionally, MF-1 (clock injection) is only
partially closed: SI-17's struct sketch canonicalises `std::time::Instant` while
D13-T7's test expectations require `tokio::time::Instant` or a clock trait;
`tokio::time::pause()` only affects the former's tokio counterpart, so the clock
injection described in the test cannot work against the type mandated in the
invariant. Phase A implementers face an unresolvable type choice. Confidence:
60/100.

---

## v4 Finding Closure Status

| Finding | v4 Severity | Closed? | Notes                                                  |
| ------- | ----------- | ------- | ------------------------------------------------------ |
| SF-1    | Significant | Partial | Witness pattern is sound; witness names non-existent   |
|         |             |         | `UnauthorizedBuildAction` variant (new Critical NF-1)  |
| SF-2    | Significant | Yes     | Conditional contract coherent; minor sqlx note (NF-3)  |
| MF-1    | Minor       | Partial | Clock injection approach correct; `std::time::Instant` |
|         |             |         | vs `tokio::time::Instant` contradiction (new NF-2)     |
| MF-2    | Minor       | Yes     | SI-17 caveat complete; operator guidance present       |
| MF-3    | Minor       | Yes     | Terminology fixed; attribution nit is NF-4 (Minor)     |

---

## Critical Findings

### NF-1 — Witness Sketch References Non-Existent Variant

**Location:** Design §D13-T6 test sketch, lines 2025-2027; production file
`cbsd-rs/cbsd-proto/src/ws.rs`, lines 24-54.

**Problem:** The `variant_tag_witness` function in the test sketch includes an
arm for `ServerMessage::UnauthorizedBuildAction`:

```rust
ServerMessage::UnauthorizedBuildAction { .. } =>
    "unauthorized_build_action",
```

The production `ServerMessage` enum in `cbsd-proto/src/ws.rs` has exactly four
variants: `BuildNew`, `BuildRevoke`, `Welcome`, `Error`.
`UnauthorizedBuildAction` does not exist. The test code in the design will not
compile against the real crate. Additionally, the `cases()` Vec at lines
2060-2066 includes a JSON payload for `"unauthorized_build_action"` that cannot
be deserialised into any real `ServerMessage` variant.

This is not a documentation nit. The test sketch is the primary deliverable for
Phase E, and the type it references does not exist in the crate it must compile
against. A developer following the sketch verbatim will have a non-compiling
test from the first `cargo build`.

**Impact:** Phase E cannot produce a working D13-T6 implementation by following
the design as written. The exhaustive-match witness's central value — that the
compiler catches missing variants — breaks immediately because the sketch itself
is missing a real variant (`Error`) and adds a phantom one. Furthermore, the
`cases()` payload for `welcome` at lines 2043-2047 omits the `connection_id`
field that `Welcome` requires (see ws.rs line 43), making the JSON non-parseable
even if the unknown-field round-trip is otherwise correct.

**Required fix before Phase E begins:**

1. Audit the witness arms against the actual variants in `ws.rs` at the time the
   test is written. Currently: `BuildNew`, `BuildRevoke`, `Welcome`, `Error`.
   Add `Error`, remove `UnauthorizedBuildAction`.
2. Audit the `cases()` Vec payloads to ensure each JSON object is parseable as
   the corresponding variant. For `Welcome`, add `"connection_id": "..."` and
   `"grace_period_secs": 0`. For `Error`, add a `"reason": "..."` field.
3. Add a maintenance note in the sketch that the witness MUST be re-audited
   against ws.rs any time a phase introduces new `ServerMessage` variants,
   citing the production file path.

---

## Significant Findings

None beyond NF-1 (elevated to Critical above).

---

## Minor Findings

### NF-2 — Clock Type Contradiction Between SI-17 and D13-T7

**Location:** §SM-C struct sketch, line 1182; §D13-T7 test expectations, lines
1978-1979; §D13-T7 note, lines 2001-2005.

**Problem:** SI-17 (State Invariant 17) defines the field as:

```rust
last_authenticated_connect_at: Option<std::time::Instant>,
```

D13-T7's test expectations state: "The supervisor's clock source for
`last_authenticated_connect_at` is wired through a small trait or a
`tokio::time::Instant` so the test's paused clock controls the predicate."

`tokio::time::pause()` and `tokio::time::advance()` control only
`tokio::time::Instant` (and `tokio::time::sleep`). They have no effect on
`std::time::Instant`. If the production supervisor stores a `std::time::Instant`
as SI-17 mandates, calling `tokio::time::pause()` in the test has no effect on
`at.elapsed()` — the test would use real wall time and be just as race-prone as
before MF-1 was raised.

The design offers two resolution paths ("small trait or a
`tokio::time::Instant`") but commits to neither, and the existing invariant
sketch picks a third incompatible type (`std::time::Instant`). Phase A
implementers cannot simultaneously satisfy SI-17's struct sketch and D13-T7's
clock-injection requirement.

**Impact:** Without resolving the type, either (a) SI-17 is implemented
literally and D13-T7's clock injection silently does nothing, leaving a flaky
boundary test, or (b) the implementer diverges from the invariant to make the
test work, creating undocumented deviation from the spec. This is Minor (not
Critical) because the production predicate logic is sound regardless of which
Instant type is chosen — only the test harness is broken by the contradiction.

**Required fix before Phase A begins:**

Pick one of the two approaches and make SI-17 and D13-T7 consistent:

- **Option A (clock trait):** Define the trait shape (e.g.,
  `trait Clock: Send + Sync { fn now(&self) -> Instant; }`) in SI-17. Update the
  struct sketch to use `Option<Instant>` where `Instant` is the trait-associated
  or generic type, not `std::time::Instant` directly. D13-T7 injects a
  `FakeClock` that returns a controlled value. SI-17 cites the trait name.
- **Option B (`tokio::time::Instant`):** Change SI-17's struct sketch to
  `Option<tokio::time::Instant>`. Note that `tokio::time::Instant::now()`
  returns the paused time when the tokio runtime is paused — this is the correct
  hook for D13-T7. Document the caveat that tokio Instant is still
  `CLOCK_MONOTONIC`-backed in production (same suspension note as SI-17 already
  has).

Either option resolves the contradiction. Option B is simpler and requires no
new trait; Option A is more testable without a tokio runtime. Choose one.

---

### NF-3 — D13-T6 Runtime Exhaustiveness Check Is Partially Elided

**Location:** Test sketch, lines 2075-2100; prose at lines 2112-2115.

**Problem:** The code at lines 2081-2086 builds a `HashSet` of `case_tags` from
`cases()` and then stops with a comment:

```rust
// (Construct one example value per variant; pass each through
// the witness; assert each returned tag is in case_tags.)
// …
```

This sentinel-construction step is the entire point of the "third helper test."
To call `variant_tag_witness(msg: &ServerMessage)`, you must have a
`&ServerMessage` value for each variant. The sketch elides this, leaving Phase E
implementers to invent the mechanism. If they implement the sentinel
construction as another hand-maintained list of `ServerMessage` constructors,
they recreate the same single-point-of-maintenance problem the exhaustive-match
witness was designed to eliminate — two lists instead of one.

**Impact:** Minor. The compile-time witness is still the primary gate; the
runtime check is defense-in-depth. But the incomplete sketch will cause
implementers to spend engineering time designing something the spec should
prescribe, or they may skip it entirely.

**Recommendation:** Complete the sketch. The sentinel values can be constructed
as a `Vec<ServerMessage>` with one minimal example per variant (e.g., the same
values the existing ws.rs unit tests already construct). Point to those existing
test helpers. Alternatively, note that the compile-time witness is sufficient
for the invariant and demote the runtime check to optional.

---

### NF-4 — `cfg(feature = "soft-delete-schema")` Incompatible with sqlx Migration Embedding

**Location:** §D3 test sketch, lines 2117-2145; prose at line 348, §D3
conditional contract.

**Problem:** The design gates the soft-delete test fixture under
`cfg(feature = "soft-delete-schema")`. sqlx embeds migrations via the
`sqlx::migrate!()` macro, which resolves the migrations directory at compile
time without any feature-conditional logic. A feature flag cannot conditionally
include or exclude a `.sql` migration file from the embedded migration set.
Running `sqlx::migrate!()` in a test configured with `soft-delete-schema` would
apply the column-adding migration to the test DB, but the same macro in the same
binary without the feature flag would not include it — this is not how sqlx
migration embedding works; `sqlx::migrate!()` scans the whole directory, not a
feature-selected subset.

To make the feature gate work, the design would need either: (a) a separate
migrations directory for test-only schema additions, referenced via a distinct
`sqlx::migrate!("migrations-test/")` call inside
`#[cfg(feature = "soft-delete-schema")]` test setup, or (b) a runtime
conditional that applies the column-adding migration only in test code,
bypassing the `migrate!()` macro.

**Impact:** Minor. The soft-delete schema is not present in production today, so
no production query is broken. But Phase B implementers following the design
literally will encounter a build or runtime error when trying to run the
feature-gated test.

**Recommendation:** Specify the exact mechanism: a `migrations-test/` directory
with its own `sqlx::migrate!()` call inside test setup, or an inline
`sqlx::query!()` `ALTER TABLE` inside `#[cfg(test)]`. Remove the implication
that feature flags interact cleanly with `sqlx::migrate!()`.

---

### NF-5 — MF-3 Attribution Cites Wrong Version

**Location:** Revision history table, line 42 (v5 entry for MF-3).

**Problem:** The v5 revision history entry for MF-3 states it fixes "the v3
revision-history entry's use of 'SM-C transition'." The v4 review (MF-3, filed
against v4) cited the v4 revision history entry at v4 line 42 as the source of
the bad terminology. The v3 design document itself did not use "SM-C transition"
in its history. The attribution is off by one version.

**Impact:** Documentation only. No correctness impact.

**Recommendation:** Update the v5 revision history to read "v4 revision-history
entry" rather than "v3 revision-history entry."

---

## Strengths

- The conditional D3 contract (lines 347-387) is coherent: hard-delete schema
  produces a plain `WHERE email = ?` query; soft-delete schema produces the
  extended query. Phase B implementers have an actionable decision tree. The
  SF-2 contradiction is fully resolved.
- MF-2 closure is complete. SI-17's suspension caveat (lines 1752-1769) is
  accurate, bounded, and includes concrete operator guidance (`CLOCK_BOOTTIME`
  via the `nix` crate). This is the correct treatment for a known limitation
  with acceptable production impact.
- MF-3 closure is complete. The phrase "recent-reconnect signal" at the
  appropriate history entry removes the false implication of SM-C state
  transitions.
- The `variant_tag_witness` pattern's compile-time gate is sound in principle.
  `ServerMessage` has no `#[non_exhaustive]` attribute, and the test lives in
  `cbsd-proto` (the same crate), so the exhaustive match is genuinely
  compiler-enforced. The concept is correct; only the enumeration in the sketch
  is wrong.
- SI-18's blanket prohibition on `deny_unknown_fields` (line 1770-1775) remains
  correctly formulated.
- The `migration_plausible()` predicate (lines 1176-1238) is concrete, testable,
  and has clear semantics for the true/false branches. The predicate logic is
  sound regardless of which `Instant` type is used.

---

## Open Questions

1. **`UnauthorizedBuildAction` origin**: Is this variant planned for Phase E
   (WCP design dependency) and was listed prematurely in the witness sketch? If
   so, the sketch must either defer to Phase E or include an explicit
   placeholder comment noting it is not yet in `cbsd-proto`.
2. **Clock type canonical choice**: Which resolves the NF-2 contradiction — a
   clock trait or `tokio::time::Instant`? This must be decided before Phase A
   implements the supervisor.
3. **Runtime exhaustiveness check completeness**: Should the "third helper test"
   be completed in the spec, or demoted to optional since the compile-time
   witness is the primary gate?
4. **`soft-delete-schema` feature mechanism**: Will this be a separate test
   migrations directory or an inline `ALTER TABLE` in test setup?

---

## Confidence Score

| Item                                                                   | Points | Criterion                                              |
| ---------------------------------------------------------------------- | ------ | ------------------------------------------------------ |
| Starting score                                                         | 100    |                                                        |
| NF-1: witness sketch names non-existent variant; cases() JSON invalid  | -20    | D5 — test that cannot compile has zero gate value      |
| NF-2: `std::time::Instant` vs `tokio::time::Instant` contradiction     | -10    | D8 — spec deviation; Phase A cannot satisfy both MUSTs |
| NF-3: runtime exhaustiveness check elided (`// …`)                     | -5     | D11 — missing implementation guidance for Phase E      |
| NF-4: sqlx feature-gate incompatible with `sqlx::migrate!()` embedding | -5     | D8 — spec deviation; test mechanism non-functional     |
| NF-5: revision history cites wrong version for MF-3 fix                | -5     | D10 — convention/documentation violation               |
| SF-1 residual: `Error` variant absent from witness; `Welcome` missing  | -5     | D5 — incomplete test coverage for known variant        |
| required field in cases() JSON                                         |        |                                                        |
| **Total**                                                              | **50** |                                                        |

Score 50/100 — Major rework needed (per confidence-scoring scale: 0-49). The
NF-1 deduction alone drops the score materially because a test sketch that
cannot compile provides zero enforcement of the invariant it is supposed to
protect.

---

## Verdict

**Revise and re-review.** The following conditions MUST be met before Phase A or
Phase E implementation begins:

**C1 (blocks Phase E, Critical):** Correct the `variant_tag_witness` and
`cases()` sketch to match the actual `ServerMessage` variants in
`cbsd-proto/src/ws.rs` at the time of writing: `BuildNew`, `BuildRevoke`,
`Welcome`, `Error`. Remove `UnauthorizedBuildAction` (phantom variant). Add
`Error`. Fix the `Welcome` JSON payload to include `connection_id` and
`grace_period_secs`. Add a maintenance note that the witness must be re-audited
against `ws.rs` whenever Phase E adds new variants.

**C2 (blocks Phase A, Minor → blocking by dependency):** Resolve the
`std::time::Instant` / `tokio::time::Instant` contradiction between SI-17 and
D13-T7. Pick one type (or a clock trait) and make both sections consistent.
Without this, the D13-T7 clock injection is silently inoperative.

C3 (recommended, non-blocking): Complete or demote the runtime exhaustiveness
check sketch (NF-3). Specify the `soft-delete-schema` test mechanism concretely
(NF-4).
