# Review: Design 019 — Security Audit Remediation v6

| Field       | Value                                                                 |
| ----------- | --------------------------------------------------------------------- |
| Review      | 019-20260516T0626-design-security-audit-remediation-v6                |
| Date        | 2026-05-16                                                            |
| Design      | `docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` |
| Sibling ref | WCP design seq 019, timestamp 20260426T1154 (v11)                     |
| Scope       | v6 closure of NF-1 through NF-5 from the v5 review; new issues        |
|             | introduced by v6                                                      |
| Reviewer    | Independent (hostile reviewer stance)                                 |
| Predecessor | `019-20260516T0447-design-security-audit-remediation-v5.md`           |

---

## Executive Summary

v6 closes four of the five v5 findings genuinely (NF-2, NF-3, NF-4, NF-5) and
repairs the variant-name error that was the root cause of NF-1. However, NF-1 is
**not closed**: the v6 D13-T6 sentinel sketch introduces a new compile-time
failure in the same category — `BuildDescriptor::default()` on line 2126 of the
design. `BuildDescriptor` does not derive or implement `Default` (verified
against `cbsd-proto/src/build.rs`); neither do its required nested types
(`BuildSignedOffBy`, `BuildDestImage`, `BuildComponent`, `BuildTarget`, `Arch`).
The sketch will not compile against the real crate, repeating the same class of
defect that v5 caught but in the constructor rather than the variant. A new
Minor finding (NF-6) identifies a fourth maintenance point in the "Three layers
of protection" scheme that the design's own prose misses. Two additional Minor
observations (NF-7 and NF-8) cover a documentation gap and test-placement
ambiguity. The design cannot advance to implementation until NF-1 is resolved.

---

## v5 Finding Closure Table

| Finding | v5 Severity | Claimed | This Review | Residual gap                               |
| ------- | ----------- | ------- | ----------- | ------------------------------------------ |
| NF-1    | Critical    | Closed  | **Partial** | `BuildDescriptor::default()` does not      |
|         |             |         |             | compile; nested types also lack `Default`. |
|         |             |         |             | Same class of error, relocated.            |
| NF-2    | Minor       | Closed  | **Closed**  | SI-17 now reads `tokio::time::Instant`.    |
| NF-3    | Minor       | Closed  | **Closed**  | `sentinel_for_tag` fully spelt out;        |
|         |             |         |             | runtime loop explicit. See NF-6 caveat.    |
| NF-4    | Minor       | Closed  | **Closed**  | Inline `ALTER TABLE` approach correct;     |
|         |             |         |             | see analysis below.                        |
| NF-5    | Minor       | Closed  | **Closed**  | History entry rephrased to                 |
|         |             |         |             | "a prior revision-history entry".          |

---

## Per-Finding Closure Detail

### NF-1 — Witness Sketch Still References Non-Compilable Code

**Source location:** Design §D13-T6 sketch, line 2126 (sentinel constructor for
`"build_new"`).

v6 correctly rewrote the match arms in `variant_tag_witness` to use the four
real variants — `BuildNew`, `BuildRevoke`, `Welcome`, `Error` — eliminating the
phantom `UnauthorizedBuildAction`. The JSON payloads in `cases()` are also
corrected: `Welcome` now includes `connection_id` and `grace_period_secs`,
`Error` now includes `reason`, `min_version`, `max_version`.

However, the `sentinel_for_tag` function constructs the `BuildNew` sentinel as:

```rust
"build_new" => ServerMessage::BuildNew {
    build_id: BuildId(0),
    trace_id: String::new(),
    priority: Priority::default(),
    descriptor: Box::new(BuildDescriptor::default()),
    component_sha256: String::new(),
},
```

`BuildDescriptor` does **not** implement `Default`. Inspecting
`cbsd-rs/cbsd-proto/src/build.rs` (lines 121-132):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildDescriptor {
    pub version: String,
    pub channel: Option<String>,
    pub version_type: Option<VersionType>,
    pub signed_off_by: BuildSignedOffBy,
    pub dst_image: BuildDestImage,
    pub components: Vec<BuildComponent>,
    pub build: BuildTarget,
}
```

No `Default` in the derive list. The required nested types also lack `Default`:

- `BuildSignedOffBy` (lines 77-80): no `Default`
- `BuildDestImage` (lines 84-87): no `Default`
- `BuildComponent` (lines 92-98): no `Default`
- `BuildTarget` (lines 102-109): no `Default`
- `Arch` (`cbsd-proto/src/arch.rs`, lines 20-26): no `Default`

Even if `#[derive(Default)]` were added to `BuildDescriptor`, all nested types
would need it too, and `Arch` has no obvious default variant. The sketch as
written fails at `cargo build` with
`error[E0277]: the trait bound BuildDescriptor: Default is not satisfied`.

Additionally, `Priority::default()` at line 2125 is correct — `Priority` derives
`Default` with `Normal` as the default value (build.rs lines 30-36, confirmed by
the `priority_default` unit test). And `BuildId(0)` is correct — `BuildId` is a
public-field tuple struct (`pub struct BuildId(pub i64)`, line 19). These two
are fine.

The JSON `"priority": "normal"` in `cases()` at line 2156 is correct because
`Priority` uses `#[serde(rename_all = "lowercase")]` and `Normal` serializes as
`"normal"`. That is verified.

**Impact:** Phase E cannot produce a working D13-T6 implementation by following
the sketch. The developer must either add `Default` impls to `BuildDescriptor`
and all nested types (a non-trivial change to `cbsd-proto`), or construct the
sentinel manually with explicit field values (the same approach used in the
existing `ws.rs` unit test at lines 143-173). The sketch's current form provides
false assurance that the sentinel compiles.

**Required fix before Phase E:**

Replace `Box::new(BuildDescriptor::default())` with an explicit construction
matching the pattern already used in `cbsd-proto/src/ws.rs` lines 148-172 (the
`server_message_build_new_round_trip` test). Alternatively, add a
`test_descriptor()` helper inside `#[cfg(test)]` in `cbsd-proto` that returns a
minimal but fully-constructed `BuildDescriptor`, and use that in the sentinel.
Do not derive `Default` on `BuildDescriptor` unless there is a genuine semantic
default for every field — there isn't (version, signed_off_by, dst_image, and
build are all required, meaningful fields with no sensible zero value).

### NF-2 — Clock Type in SI-17 (Verified Closed)

SI-17's struct sketch now reads:

```rust
last_authenticated_connect_at: Option<Instant>,
```

with an inline comment (lines 1201-1208) stating the type is
`tokio::time::Instant`, and the surrounding prose (lines 1786-1792) repeating
the pinned type explicitly. D13-T7's test expectations reference
`tokio::time::pause()` / `tokio::time::advance()`, consistent with a
`tokio::time::Instant` field. The contradiction is resolved.

### NF-3 — Runtime Exhaustiveness Check (Verified Closed, with NF-6 caveat)

The `sentinel_for_tag` function (lines 2120-2144) is fully spelt out with one
arm per current variant. The runtime exhaustiveness loop (lines 2203-2214)
iterates a hardcoded tag list, calls `sentinel_for_tag`, passes the result
through `variant_tag_witness`, and asserts the returned tag is in `case_tags`.
The design correctly states what happens when each of the three failure modes
occurs. NF-3 is closed. A new finding (NF-6) identifies a structural gap in the
"Three layers of protection" claim about this loop.

### NF-4 — Inline `ALTER TABLE` for Soft-Delete Fixture (Verified Closed)

The design now specifies an inline
`ALTER TABLE users ADD COLUMN deleted_at TIMESTAMP NULL` executed inside the
test's `setup_soft_delete_fixture` function, after `sqlx::migrate!()`. SQLite
supports `ALTER TABLE … ADD COLUMN` for adding a nullable column to an existing
table (confirmed; this is one of SQLite's supported `ALTER TABLE` forms). The
`cfg(feature = "soft-delete-schema")` gate is at the test-function and
setup-function level, not on `sqlx::migrate!()` itself, which resolves the
v4-NF-4 incompatibility. The `cfg` gate is compatible with Cargo feature flags
since it gates Rust code, not SQL files. NF-4 is closed.

### NF-5 — Attribution Rephrasing (Verified Closed)

The v6 history entry for NF-5 reads "a prior revision-history entry" (lines
56-58), removing the specific "v3" citation that was off by one. NF-5 is closed.

---

## Critical Findings

### NF-1-v6 — `BuildDescriptor::default()` Does Not Compile

_(This is the same finding-class as v5's NF-1, relocated from variant names to
constructors. It is re-raised as Critical because the effect is identical: a
test sketch that cannot compile provides zero enforcement of the invariant.)_

See the "NF-1" closure detail section above for the complete technical analysis.
The short version: `BuildDescriptor` does not implement `Default`; the sketch at
line 2126 (`Box::new(BuildDescriptor::default())`) fails to compile against the
real `cbsd-proto` crate. The fix is to use an explicit field-by-field
construction or a named `test_descriptor()` helper.

---

## Significant Findings

None beyond NF-1-v6 (elevated to Critical above).

---

## Minor Findings

### NF-6 — Four Maintenance Points, Not Three

**Location:** §D13-T6, "Three layers of protection" prose, lines 2233-2254.

**Problem:** The design describes "Three layers of protection" for the
exhaustiveness scheme:

1. Compile-time witness (`variant_tag_witness` match)
2. Runtime exhaustiveness loop (sentinel + case cross-check)
3. Runtime per-variant deserialization

The runtime exhaustiveness loop iterates a **hardcoded tag list**:

```rust
for tag in &["build_new", "build_revoke", "welcome", "error"] {
```

(lines 2203-2204)

This list is a fourth maintenance point. A developer who adds a new
`ServerMessage` variant, updates `variant_tag_witness` (compile-time gate forces
this), adds a `sentinel_for_tag` arm, and adds a `cases()` entry — but forgets
to append the new tag to the hardcoded list — will have a compile-clean,
test-green result. The new variant is silently absent from the runtime loop.

The design's own "Three layers" analysis discusses what happens when the witness
is updated but sentinel or cases are forgotten. It does not discuss the
converse: what happens when the hardcoded list lags the other three structures.
There is no guard that catches this omission.

**Impact:** Minor, because the compile-time witness is the primary gate and will
catch any truly missing variant. But the runtime loop's value as
"defense-in-depth" is overstated — it has a blind spot.

**Recommendation:** Either:

(a) Replace the hardcoded tag list with a dynamic enumeration. Construct the tag
list by calling `variant_tag_witness` on one example of each variant and
collecting the results. This requires having all sentinels available without
hard-coding the list — one approach is to build the list from a `all_sentinels`
function that returns `Vec<ServerMessage>` (one per variant, using the same arms
as `sentinel_for_tag` but without the `panic` arm). This turns the hardcoded
list into a derived structure and eliminates the fourth maintenance point.

(b) Acknowledge the four-point maintenance requirement explicitly in the comment
block and add a fifth assertion: assert that the number of tags in the loop
equals the number of arms in `variant_tag_witness`. Since the witness is
exhaustive, this count is indirectly pinned by the compiler; the assertion
documents the expectation. This is less elegant but simpler to implement.

The design should update the "Three layers" prose to either accurately describe
four maintenance points or adopt approach (a) to eliminate the fourth.

### NF-7 — `minimal_descriptor_json()` Is Undefined

**Location:** §D13-T6, `cases()` function body, line 2157.

**Problem:** The `"build_new"` case in `cases()` calls
`minimal_descriptor_json()`:

```rust
("build_new", json!({
    "type": "build_new",
    ...
    "descriptor": minimal_descriptor_json(),
    ...
})),
```

This helper is not defined anywhere in the sketch or in the surrounding design
text. Its return type and shape — specifically whether it includes all required
fields of `BuildDescriptor` — are unspecified.

Phase E implementers must invent this helper. If they construct it without
reference to `BuildDescriptor`'s required fields, the JSON round-trip test
passes even if the payload is malformed (serde will reject it, but that
rejection is what the test is probing against — a malformed `descriptor` would
cause the `serde_json::from_value` to fail for the wrong reason).

**Impact:** Minor. The test's core assertion is about unknown-field tolerance,
not about `descriptor` field validity. But an underspecified helper adds Phase E
engineering time and creates a risk that the constructed case doesn't actually
exercise the real parse path.

**Recommendation:** Define `minimal_descriptor_json()` inline in the sketch,
returning a `serde_json::Value` that matches `BuildDescriptor`'s required
fields. The existing `ws.rs` test at lines 148-172 provides the reference shape.
Alternatively, cite that test explicitly and note that the helper should use the
same field values.

### NF-8 — D13-T6 Test Placement Unspecified

**Location:** §D13-T6, sketch preamble and imports, line 2095.

**Problem:** The sketch begins:

```rust
use cbsd_proto::{BuildDescriptor, BuildId, Priority, ServerMessage};
```

This import path is consistent with a test in an external crate that depends on
`cbsd-proto`. However, the v5 review (Strengths section) noted that the witness
works precisely because "`ServerMessage` has no `#[non_exhaustive]` attribute,
and the test lives in `cbsd-proto` (the same crate)." If the test lives in the
same crate, the import would be `use crate::{...}` or relative, not
`use cbsd_proto::{...}`.

The design does not specify whether D13-T6 belongs in:

- `cbsd-proto/src/ws.rs` as a `#[cfg(test)]` module (same crate, `use super::*`)
- `cbsd-proto/tests/` as an integration test (external access,
  `use cbsd_proto::*`)
- `cbsd-server` or `cbsd-worker` (which depend on `cbsd-proto` and also have
  test infrastructure)

This matters: only a same-crate test with a non-`#[non_exhaustive]` enum
provides compiler-enforced exhaustiveness. An integration test in
`cbsd-proto/tests/` also works (integration tests have the same access as
external crates, but `ServerMessage` is still exhaustively matchable without
`#[non_exhaustive]`). A test in `cbsd-server` or `cbsd-worker` also works. The
import style in the sketch implies an external consumer but does not explicitly
say where the test file lives.

**Impact:** Minor. The exhaustiveness guarantee holds regardless of placement
(as long as `ServerMessage` remains non-`#[non_exhaustive]`). But the ambiguity
will cause Phase E to make a placement decision without design guidance.

**Recommendation:** Add one sentence specifying the test file location. The
natural home is `cbsd-proto/src/ws.rs` alongside the existing
`server_message_build_new_round_trip` test, using `use super::*` imports. If the
design intends a separate file, say so explicitly.

---

## Strengths

v6 makes substantive progress on four of five v5 findings:

- **NF-2 closure is complete and technically correct.** Pinning SI-17 to
  `tokio::time::Instant` is the right choice. The inline comment in the struct
  sketch (lines 1201-1208) explaining the relationship to `std::time::Instant`
  under production vs. test runtimes is excellent — it pre-empts a likely
  implementation question.
- **NF-3 closure is substantially correct.** `sentinel_for_tag` is now fully
  spelt out. The three-layer analysis in lines 2233-2254 is clear and accurately
  describes what happens in three of the four failure modes. The fourth
  (hardcoded-list lag, NF-6) is a gap, not a fundamental flaw.
- **NF-4 closure is clean.** The inline `ALTER TABLE` approach is the correct
  solution to the `sqlx::migrate!()` compatibility problem. The
  `setup_soft_delete_fixture` sketch is actionable and the limitation is clearly
  documented (the column exists only for the scope of this test).
- **NF-5 closure is complete.** The rephrasing removes the off-by-one
  attribution without introducing new ambiguity.
- **The variant-name part of NF-1 is genuinely corrected.** All four actual
  `ServerMessage` variants (`BuildNew`, `BuildRevoke`, `Welcome`, `Error`) are
  now present in the witness and in `cases()`. The JSON payloads match the real
  field shapes. `Priority::default()` and `BuildId(0)` are correct. The
  error-class is narrowed to the constructor path only.
- **SI-18's description of D13-T6** (lines 1812-1814) is accurate to the current
  scope: it correctly states the test covers `BuildRevoke` unknown-field
  round-trip. When D13-T6 expands to all variants, this line should be updated,
  but the current wording is not misleading.
- **The WCP variant forward-compatibility note** (lines 2088-2092) is good
  practice: it explicitly names `UnauthorizedBuildAction` and instructs
  developers to extend the witness, sentinel, and cases simultaneously.

---

## Open Questions

1. **`BuildDescriptor` Default implementation**: Should `cbsd-proto` add
   `#[derive(Default)]` to `BuildDescriptor` and its nested types? This would
   require deciding on semantics for zero-value fields (`version: ""`,
   `signed_off_by: { user: "", email: "" }`, etc.) that have no meaningful
   production default. The safer alternative is an explicit `test_descriptor()`
   helper.

2. **`all_sentinels()` as a list-free enumeration**: Should the runtime
   exhaustiveness loop be driven by an `all_sentinels()` function instead of a
   hardcoded list, eliminating the fourth maintenance point?

3. **D13-T6 test file placement**: Which file and module does Phase E place this
   test in? `cbsd-proto/src/ws.rs` is the most natural location given the
   existing test structure.

---

## Confidence Score

| Item                                                                     | Points | Criterion                                                      |
| ------------------------------------------------------------------------ | ------ | -------------------------------------------------------------- |
| Starting score                                                           | 100    |                                                                |
| NF-1-v6: `BuildDescriptor::default()` not compilable; same class as NF-1 | -20    | D5 — test sketch that cannot compile enforces nothing          |
| NF-6: hardcoded tag list is a fourth maintenance point not covered by    | -5     | D11 — incomplete/misleading documentation of protection scheme |
| three-layer analysis                                                     |        |                                                                |
| NF-7: `minimal_descriptor_json()` undefined in sketch                    | -5     | D11 — missing specification for required test helper           |
| NF-8: D13-T6 test placement unspecified                                  | -5     | D11 — implementation ambiguity for Phase E                     |
| **Total**                                                                | **65** |                                                                |

Score 65/100 — significant issues, must address before proceeding (per
confidence-scoring scale: 50-74).

The NF-1-v6 deduction dominates. The pattern is the same as v5: the sketch
cannot compile against the real crate, which means the compile-time gate
provides zero enforcement. The three Minor findings are independent and each
deducts -5 for documentation/specification gaps that will create Phase E
engineering overhead or silent maintenance risk.

---

## Go / No-Go

**No-Go for implementation planning.**

**C1 (blocks Phase E, Critical):** Correct the `sentinel_for_tag` function to
construct `BuildNew` without `BuildDescriptor::default()`. Use explicit field
values or extract a `test_descriptor()` helper (following the pattern in
`cbsd-proto/src/ws.rs` lines 148-172). Verify the corrected sketch compiles by
attempting `cargo build` in `cbsd-proto` with the sentinel added.

**C2 (recommended before Phase E):** Resolve the hardcoded tag list (NF-6) by
either adding a fourth-point acknowledgement to the "Three layers" prose or
replacing the list with an `all_sentinels()` function.

**C3 (recommended before Phase E):** Define `minimal_descriptor_json()` inline
in the sketch (NF-7) and specify D13-T6's test file location (NF-8).

C2 and C3 are non-blocking for a v7 review but should be resolved before the
Phase E commit lands.
