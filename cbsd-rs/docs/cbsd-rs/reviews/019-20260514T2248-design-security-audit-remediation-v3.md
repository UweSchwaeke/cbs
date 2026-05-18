# cbsd-rs design review — security audit remediation (v3)

| Field          | Value                                                                                    |
| -------------- | ---------------------------------------------------------------------------------------- |
| Review         | 019 v3                                                                                   |
| Date (UTC)     | 2026-05-14 22:48                                                                         |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md`            |
| Sibling design | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md`        |
| Prior review   | `cbsd-rs/docs/cbsd-rs/reviews/019-20260514T2227-design-security-audit-remediation-v2.md` |
| Reviewer       | Independent                                                                              |
| Verdict        | **Approve with conditions**                                                              |

---

## Executive Summary

v3 closes four of the five v2 findings cleanly (N-1 on serde wire shape, N-2 on
the CI-gate comment syntax, N-3 on the D5 test suite, N-5 on the `ExposeSecret`
import contract). N-4 is partially closed: the prose invariant and the formal
State Invariant 15 are correct, but the D3 test matrix in the Test Expectations
Summary contains no explicit owner-deleted lifecycle test, and the design does
not resolve whether the owner row is genuinely deleted or merely inactive, which
matters for the database query.

v3 also introduces two new Significant findings. First, the design mandates that
no cbsd-proto struct may carry `#[serde(deny_unknown_fields)]` as a global
rolling-upgrade invariant, but provides no pinning test that would prevent a
future contributor from adding the attribute and silently breaking wire
compatibility — the five serde compat tests assert correctness of the current
shape, not prevention of future regression. Second, the D13 anti-coercion
defense requires the worker to confirm "via its local SM-C state" that a
migration is genuinely in progress before honoring a `MigrationSupersede`
revoke, but SM-C is listed in the design only as a source of trigger inputs with
no states, no transitions, no time window, and no concrete predicate — making
the defense unimplementable as specified.

---

## Per-Finding Closure Table (N-1 through N-5)

| Finding | v2 severity   | v3 closure | Notes                                                 |
| ------- | ------------- | ---------- | ----------------------------------------------------- |
| N-1     | Significant   | Closed     | Introduces two new issues; see below                  |
| N-2     | Significant   | Closed     | `// allow-expose` correct                             |
| N-3     | Minor         | Closed     | Phase 2 reframed; tests corrected                     |
| N-4     | Minor         | Partial    | Invariant correct; test absent; soft-delete ambiguous |
| N-5     | Informational | Closed     | Self-correcting via compile error                     |

---

## Detailed Per-Finding Analysis

### N-1 — BuildRevokeReason serde wire representation

**v2 recommendation:** Pin `reason` as `Option<BuildRevokeReason>` with
`#[serde(default, skip_serializing_if = "Option::is_none")]` and specify
compatibility tests.

**v3 response (lines 1013–1088, 1689–1748):**

v3 adds the full serde annotation block at lines 1050–1071:

```
#[serde(default, skip_serializing_if = "Option::is_none")]
pub reason: Option<BuildRevokeReason>,
```

and specifies five serde compat tests in the Test Expectations Summary (lines
1711–1748):

1. Old worker receives new BuildRevoke with reason → deserializes without error
2. New worker receives old BuildRevoke without reason → `reason` is `None`
3. BuildRevokeReason values round-trip through JSON
4. Unknown reason string deserializes as `Unknown` variant
5. BuildRevokeReason::MigrationSupersede serializes as `"migration_supersede"`

The `#[serde(deny_unknown_fields)]` constraint is introduced at lines 1050–1052
as a global rule: "No cbsd-proto struct or enum may carry
`#[serde(deny_unknown_fields)]`."

**Closure verdict: Closed for the original finding.**

The serde wire shape is now fully specified. The `#[serde(default, ...)]`
annotation and the five compatibility tests directly address what N-1 asked for.

**New issue introduced (NF-1 — see New Findings below):** The
`deny_unknown_fields` constraint is a global invariant with no automated
enforcement. This is a separate finding from N-1.

---

### N-2 — CI gate comment syntax

**v2 finding:** Design used `# allow-expose` (shell-style comment); correct form
is `// allow-expose` (Rust line comment).

**v3 response (D10 section, lines 703–827):**

Every occurrence of the exemption marker in v3 uses `// allow-expose`. The
grep-gate command at lines 796–806 correctly targets `expose_secret` in source
and inverts the `// allow-expose` annotation. The CI listing is internally
consistent.

**Closure verdict: Closed.**

---

### N-3 — D5 chained-symlink test inconsistency

**v2 finding:** The chained-symlink test example (test D5-4) described a Phase 2
check asserting what Phase 1 should have caught, making the test purpose
unclear.

**v3 response (lines 399–514):**

v3 redesigns the D5 test suite around three targeted tests:

- Test D5-T1: Symlink that resolves outside the unpack directory → rejected by
  Phase 2 real-path walk
- Test D5-T2: Chained symlink (symlink → intermediate symlink → escape) →
  rejected by Phase 2
- Test D5-T3: Normal relative path inside unpack directory → accepted

The TOCTOU framing is explicit: Phase 2 is positioned as defense-in-depth
against symlink creation after Phase 1's logical-path check. The test
descriptions are internally consistent and match the two-phase architecture.

**Closure verdict: Closed.**

---

### N-4 — D3 owner-deleted edge case

**v2 finding:** No specification for what happens when a periodic task's owner
account is deleted between the task's creation and its trigger time.

**v3 response (lines 299–310, State Invariant 15 at lines 1590–1599):**

v3 adds this text at lines 299–310 (D3 authorization section):

> "If the owner account no longer exists at trigger time, treat the task as
> unauthorized and disable it (transition to `Disabled` state with a
> system-generated audit log entry)."

State Invariant 15 formalizes the same rule:

> "Invariant 15 (owner-deleted): If a periodic task's owner row is absent at
> trigger time, the task must transition to `Disabled` before the authorization
> re-validation step executes. The absence check and the `Disabled` transition
> are atomic within the trigger transaction."

The prose is correct and the formal invariant is clear.

**Partial closure — two gaps remain:**

**Gap 1 — No test in the Test Expectations Summary.** The D3 test matrix (lines
1689–1710) lists authorization tests for `:own` and `:any` scope but contains no
test labeled "owner deleted between creation and trigger → task disabled." The
prose and the formal invariant say the same code path handles this, but no test
pins the case, so a regression in the database query would not be caught by the
specified test suite.

**Gap 2 — Soft-delete ambiguity.** The invariant says "owner row is absent."
Many production systems implement user deletion as a soft-delete (a flag or
`deleted_at` column) rather than a hard `DELETE`. If cbsd-rs uses soft deletion,
`SELECT user WHERE id = owner_id` would still return a row, and "absent" would
be the wrong predicate. The design does not state whether user deletion is hard
or soft, and the CLAUDE.md Correctness Invariants section does not address this.
An implementer reading only this design document cannot determine which query to
write.

**Closure verdict: Partial.** Invariant correct; test absent; soft-delete
semantics unspecified.

---

### N-5 — `secrecy::ExposeSecret` import contract

**v2 finding:** `secrecy::ExposeSecret` is a trait, not an inherent method.
Without `use secrecy::ExposeSecret;` at the call site, `.expose_secret()` does
not compile. The design did not acknowledge this.

**v3 response (lines 703–827, D10 section):**

v3 does not add an explicit `use secrecy::ExposeSecret;` requirement to the D10
prose, but the trait-method nature of `.expose_secret()` means the design is
self-enforcing: any call site that omits the `use` statement will fail to
compile. The CI `// allow-expose` gate and the `Secret<T>` adoption requirement
together ensure that every call site already passes through human review. The
compile error is the enforcement mechanism.

**Closure verdict: Closed.** Self-correcting via compile error; no additional
design text required.

---

## New Findings

### NF-1 — `deny_unknown_fields` invariant has no pinning test (Significant)

**Location:** D13 wire shape specification, lines 1050–1052; Test Expectations
Summary, lines 1711–1748.

**Problem:** The design asserts a global cbsd-proto invariant: no struct or enum
may carry `#[serde(deny_unknown_fields)]`. This invariant is load-bearing for
rolling upgrades — an old worker receiving a new-server message with an extra
field must silently succeed, not panic or return an error. The five serde compat
tests listed in the Test Expectations Summary (lines 1711–1748) verify correct
behavior of the _current_ wire shape: they assert that `reason: None`
round-trips correctly, that unknown reason strings deserialize as `Unknown`, and
so on. Not one of them is framed as "add an unknown field to a BuildRevoke JSON
object and confirm deserialization succeeds" — which is the test that would pin
the absence of `deny_unknown_fields` as a regression gate.

Code verification confirms `deny_unknown_fields` does not appear in any
cbsd-proto source file today. The invariant is therefore currently satisfied by
convention. But as cbsd-proto grows (additional `ServerMessage` variants, new
per-variant fields, protocol versioning), any contributor who adds
`#[serde(deny_unknown_fields)]` to a struct will silently break rolling upgrades
without any test failing.

**Impact:** A future proto change introduces `deny_unknown_fields`. A
server-first rolling upgrade is performed. Old workers begin rejecting new
server messages. Workers fall back to reconnect loops. Builds stall. The failure
is not immediately traceable to the serde attribute change.

**Recommendation:** Add a sixth serde compat test to the Test Expectations
Summary:

> Test D13-T6: Deserialize a `BuildRevoke` JSON object containing one unknown
> field (`"extra": "value"`) into `ServerMessage`. Assert deserialization
> succeeds and the unknown field is silently ignored. This test must fail if
> `#[serde(deny_unknown_fields)]` is added to `ServerMessage` or `BuildRevoke`.

Explicitly note in the test description that this test is a regression gate for
the `deny_unknown_fields` invariant, not merely a correctness check.

---

### NF-2 — SM-C operational semantics undefined (Significant)

**Location:** D13 anti-coercion defense, lines 1089–1102; SM-C definition, lines
1165–1460.

**Problem:** The D13 section specifies an anti-coercion defense against a
malicious server sending a fabricated `MigrationSupersede` revoke reason to a
worker that has no in-progress migration:

> "Before honoring a `MigrationSupersede` revoke, the worker confirms via its
> local SM-C state that a migration is genuinely in progress."

SM-C is described in the State Machines section as a source of trigger inputs to
SM-W. The design explicitly states SM-C "is not a full state machine." It lists
the events that SM-C can emit (`MigrationDetected`, `MigrationComplete`,
`ConnectionRestored`) but does not define:

- What state SM-C tracks (is there a boolean `migration_in_progress` flag? A
  timestamp? A counter?)
- What the concrete predicate is ("migration genuinely in progress" is
  undefined)
- What the time window is (a migration can be detected but not yet complete at
  the moment the `BuildRevoke` arrives — the predicate would be true, but is
  this the intended behavior?)
- What happens if the predicate is false: reject the revoke outright? Log and
  reject? Log and honor anyway? Apply a different code path?

The design calls this "the primary anti-coercion defense" at line 1098. A
defense that cannot be implemented from the specification is not a defense.

An implementer reading this design document in isolation must invent the SM-C
predicate. Two implementers will invent different predicates. Neither will be
wrong by the letter of the design.

**Impact:** The anti-coercion defense is either not implemented (because the
implementer does not know what to check) or implemented inconsistently across
refactors (because the predicate is not pinned). An attacker who can send a
`BuildRevoke { reason: "migration_supersede" }` to a worker with no active
migration can cause the worker to abandon its current build, degrading
availability. Whether this is exploitable depends on what authentication the
worker-to-server channel provides — but the design does not close the gap at the
design level.

**Recommendation:** Add a concrete SM-C specification. Minimal viable version:

> "SM-C maintains a boolean flag `migration_in_progress`, initialized to
> `false`. It is set to `true` when a `MigrationDetected` event is received and
> reset to `false` when `MigrationComplete` or `ConnectionRestored` is received.
> The anti-coercion predicate at D13 is: `migration_in_progress == true`. If a
> `BuildRevoke { reason: MigrationSupersede }` arrives when the flag is `false`,
> the worker must log a warning and honor the revoke as if the reason were
> `Unknown` (i.e., re-queue behavior applies). The flag is local to the current
> connection instance and resets on reconnect."

Provide one test in the Test Expectations Summary:

> Test D13-T7: Worker receives `BuildRevoke { reason: MigrationSupersede }` when
> SM-C `migration_in_progress` flag is `false`. Assert the worker logs a warning
> and applies the Unknown-reason code path (not the MigrationSupersede code
> path).

---

### NF-3 — D3 owner-deleted lifecycle test absent from test matrix (Minor)

**Location:** Test Expectations Summary, D3 authorization tests, lines
1689–1710.

**Problem:** As noted under N-4, the Test Expectations Summary for D3 contains
no explicit test for the owner-deleted case. The formal invariant (State
Invariant 15) requires an atomic absence check and `Disabled` transition within
the trigger transaction. Without a test that exercises this path, an implementer
who uses a soft-delete model (returning a row with a `deleted_at` flag) will see
all other D3 tests pass while violating Invariant 15.

**Impact:** Soft-delete or query-scoping regression in user deletion goes
undetected at the D3 layer. Periodic tasks owned by deleted users continue to
trigger and execute with the deleted user's stored scope.

**Recommendation:** Add one test to the D3 section of the Test Expectations
Summary:

> Test D3-T-owner-deleted: Create a periodic task owned by user U. Delete user
> U. Advance the trigger clock past the task's next scheduled time. Assert the
> task transitions to `Disabled` state and no authorization re-validation
> attempt is made. Assert the audit log records a system disable event.

Also add a one-sentence clarification to State Invariant 15 specifying whether
"absent" means `DELETE` (hard delete) or `deleted_at IS NOT NULL` (soft delete),
so the database query can be written unambiguously.

---

## Strengths

**D10 secret handling is thorough.** The combination of `Secret<T>`, the
`// allow-expose` exemption marker, the CI grep gate, and the prohibition on
`Serialize`/`Deserialize` for `Secret<T>` forms a layered defense. The design
correctly anticipates the edge cases (startup config logging, vault path
emission) and provides explicit exemption guidance.

**D5 two-phase architecture is honest.** The v3 reframing of Phase 2 as
defense-in-depth, with Phase 1 as the primary logical check, accurately
describes the actual value of each phase. The three replacement test cases
(D5-T1 through D5-T3) correctly target the TOCTOU window that Phase 2 is meant
to close.

**D1 strict truthy parsing is well-specified.** The allowlist (`1`, `true`,
`yes`, `on`, case-insensitive) and the loopback-URL gate for `NoVerifier` are
both concrete. The design correctly notes that the current code's
`!v.is_empty()` check is the bug being fixed, which makes the intended behavior
unambiguous for the implementer.

**State Invariants section is a genuine contribution.** Invariants 1–15 cover
the most dangerous consistency boundaries (dispatch-mutex ordering, pool sizing,
FK enforcement, trace-id lifecycle). Documenting these formally, rather than
relying on code comments alone, makes the design auditable and gives reviewers a
checklist.

**BuildRevokeReason::Unknown variant.** The forward-compatibility design for
unknown reason strings (deserialize as `Unknown`, apply default behavior) is the
correct approach for a protocol that will evolve. The test pinning this (test 4
in the compat suite) is exactly right.

---

## Open Questions

1. **User deletion model.** Does cbsd-rs implement hard deletion or soft
   deletion for user accounts? State Invariant 15 and the D3 owner-deleted rule
   depend on knowing the correct predicate for "user row is absent."

2. **SM-C predicate and time window.** If the SM-C migration flag is the
   intended implementation, what is the expected lifecycle of the flag across a
   reconnect? Should `ConnectionRestored` reset it before or after the worker
   re-registers its active builds?

3. **`deny_unknown_fields` enforcement scope.** The design states the invariant
   applies to all cbsd-proto structs and enums. Does this include structs
   defined in `cbsd-proto` that are not part of the WebSocket message envelope
   (e.g., `BuildDescriptor`, `Arch`)? If so, the scope of the invariant is wider
   than the five compat tests cover.

4. **D9 logging invariant placement.** The design specifies (lines 856–872) that
   no secret material may appear in log output, and cross-references the
   `// allow-expose` policy. Should this be added to the `cbsd-rs/CLAUDE.md`
   Correctness Invariants section (currently Invariants 1–7) to make it
   enforceable at the project-wide level, not just within this design doc?

---

## Confidence Score

| Item                                                    | Points | Description                                                                                             |
| ------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------- |
| Starting score                                          | 100    |                                                                                                         |
| D5: `deny_unknown_fields` invariant has no pinning test | -15    | Global rolling-upgrade constraint enforced by convention only; regression possible without test failure |
| D11: SM-C operational semantics undefined               | -5     | Anti-coercion defense at D13 lines 1089–1102 is unimplementable without a concrete SM-C predicate       |
| D11: Owner-deleted lifecycle test absent from D3 matrix | -5     | Prose invariant correct but no test pins the path; soft-delete ambiguity compounds the gap              |
| **Total**                                               | **75** |                                                                                                         |

Score 75: Significant issues. Must address before proceeding.

---

## Go / No-Go

**Approve with conditions.**

v3 is a meaningful improvement over v2. The four cleanly closed findings (N-1 on
wire shape, N-2 on CI gate syntax, N-3 on test suite, N-5 on import contract)
represent substantive design work, not cosmetic changes.

The two new Significant findings (NF-1 and NF-2) do not invalidate the overall
architecture, but they do leave implementation-critical gaps: a regression gate
is missing (NF-1), and a load-bearing defense is underspecified (NF-2). Neither
requires a full redesign. NF-2 in particular is fixable with a single concrete
paragraph and one test case.

**Conditions before implementation begins:**

1. Add test D13-T6 (unknown-field deserialization succeeds) to the Test
   Expectations Summary as an explicit regression gate for the
   `deny_unknown_fields` invariant. Mark it as a blocking test.
2. Add a concrete SM-C predicate definition (at minimum: the boolean flag
   approach described in NF-2) and test D13-T7 to the Test Expectations Summary.
3. Resolve the soft-delete ambiguity in State Invariant 15 and add test
   D3-T-owner-deleted to the D3 test matrix.

Items 1–3 are design-text additions only. No architectural changes are required.
