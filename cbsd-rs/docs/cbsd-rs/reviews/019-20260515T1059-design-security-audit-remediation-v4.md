# Review: Design 019 — Security Audit Remediation v4

| Field       | Value                                                                      |
| ----------- | -------------------------------------------------------------------------- |
| Review      | 019-20260515T1059-design-security-audit-remediation-v4                     |
| Date        | 2026-05-15                                                                 |
| Design      | `docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md`      |
| Sibling ref | WCP design seq 019, timestamp 20260426T1154 (v11)                          |
| Scope       | v4 closure of NF-1, NF-2, NF-3 from v3 review; new issues introduced by v4 |
| Reviewer    | Independent (hostile reviewer stance)                                      |
| Predecessor | `019-20260514T2248-design-security-audit-remediation-v3.md`                |

---

## Executive Summary

v4 makes genuine progress: NF-2 is substantially closed (a concrete,
implementable SM-C predicate and D13-T7 test now exist), and NF-3's invariant
language is correct. However, two Significant findings remain open. First, test
D13-T6 pins only the `BuildRevoke` variant but SI-18 prohibits
`deny_unknown_fields` on **all** `ServerMessage` variants; a contributor adding
the attribute to `BuildNew`, `BuildOutput`, or any other variant will not be
caught. Second, the D3 soft-delete MUST at line 325 requires
`WHERE deleted_at IS NULL` in the production query, but line 1919 explicitly
states the production schema does not add `deleted_at` for v4 — the MUST is
unimplementable against the production schema as specified. Phase B cannot start
until finding SF-2 is resolved. Phase E test D13-T6 must be expanded before any
new `ServerMessage` variant is introduced. Confidence: 60/100.

---

## v3 Finding Closure Status

| Finding | v3 Severity | Closed? | Notes                                                                           |
| ------- | ----------- | ------- | ------------------------------------------------------------------------------- |
| NF-1    | Significant | Partial | D13-T6 added, but covers only `BuildRevoke`; SI-18 covers all variants          |
| NF-2    | Significant | Yes     | Concrete predicate + D13-T7 test added; residual clock concerns are Minor       |
| NF-3    | Minor       | Partial | Invariant correct; production schema cannot satisfy the D3 MUST for soft-delete |

---

## Significant Findings

### SF-1 — D13-T6 Scope Narrower Than SI-18

**Location:** Design §D13, test D13-T6 (lines 1897–1905); State Invariant 18
(lines 1707–1712).

**Problem:** SI-18 reads: "No `ServerMessage` variant or sub-type SHALL carry
`#[serde(deny_unknown_fields)]`." The invariant is protocol-wide. Test D13-T6
constructs a single JSON object
(`{"type":"build_revoke","build_id":42,"future_field":"x"}`) and asserts
deserialization succeeds into `BuildRevoke { build_id: 42, reason: None }`. This
covers exactly one variant. `BuildNew`, `BuildAck`, `BuildOutput`,
`BuildFinished`, and any future variant added in Phase E or later are not
exercised. A developer adding `#[serde(deny_unknown_fields)]` to `BuildOutput`
will not be caught by D13-T6, and an old worker receiving a `build_output`
message with a new optional field will panic-deserialize in production.

**Impact:** The test masquerades as an invariant guard but provides
single-variant coverage. Any new `ServerMessage` variant introduced during Phase
E (D12+D13) or afterward could silently re-introduce the forward-compatibility
regression NF-1 was meant to prevent.

**Required fix:** D13-T6 must be expanded to one sub-case per `ServerMessage`
variant. Each sub-case injects an unknown field into the JSON and asserts
deserialization still succeeds and the unknown field is silently ignored.
Alternatively, introduce a `trybuild`-style compile-time denial test that
rejects any `ServerMessage` derivation that uses `deny_unknown_fields`.

---

### SF-2 — D3 Soft-Delete MUST Unimplementable Against Production Schema

**Location:** Design §D3, lines 325–342 (normative soft-delete text);
D3-T-owner-soft-deleted test, lines 1907–1923.

**Problem:** The D3 normative text (line 325) states: "The query MUST filter the
soft-delete marker: `WHERE email = ? AND deleted_at IS NULL`." This is a
MUST-level implementation requirement. Line 1919 simultaneously states: "the
production schema does not need to add the column for v4 to apply." These two
statements are directly contradictory for Phase B implementers:

- If Phase B writes `WHERE email = ? AND deleted_at IS NULL` against the
  production schema (which has no `deleted_at` column), the query fails at
  runtime with a column-not-found error.
- If Phase B omits the filter to avoid the runtime error, the D3 MUST is
  violated on a security-critical lookup path.

The test fixture at line 1919 adds the column for test purposes ("the test
fixture adds `deleted_at` for the purposes of this test"). This means the test
passes against a schema that does not exist in production, proving a requirement
that cannot be met.

The "or equivalent" list (`deleted_at`, `disabled_at`, `is_deleted`, status enum
value) compounds the problem: it provides no canonical marker, so Phase B
implementers have no authoritative column name to target.

**Impact:** Phase B cannot correctly implement D3 without a schema migration
that is not specified anywhere in v4. If the migration is omitted, the
deployment either has a broken query or a bypassed security check. This is a
spec-internal contradiction on a security-critical path. The practical
exploitability is deferred (no soft-deleted rows exist in production today),
which keeps this Significant rather than Critical.

**Required fix before Phase B starts:**

1. Decide on the canonical soft-delete marker (e.g.,
   `deleted_at TIMESTAMP DEFAULT NULL`). Enumerate it in D3 with no "or
   equivalent" alternatives.
2. Add a migration to `migrations/` that adds the column to the `users` table
   (or equivalent owner table).
3. Remove the statement at line 1919 that exempts the production schema from
   carrying the column. Test fixtures must match the production schema.
4. Update D3-T-owner-soft-deleted to use the production migration, not a bespoke
   fixture column.

---

## Minor Findings

### MF-1 — D13-T7 Boundary Tests Are Clock-Race-Prone

**Location:** D13-T7 test specification, lines 1811–1819.

**Problem:** The test description specifies boundary cases at
`MIGRATION_RECENT_WINDOW ± 1ms` using real `std::time::Instant`.
`Instant::now()` reads `CLOCK_MONOTONIC`, which advances during test execution.
On a loaded CI runner, the elapsed time between capturing
`last_authenticated_connect_at` and evaluating the predicate can exceed 1ms,
causing a boundary test to flip intermittently. No `tokio::time::pause()` /
mock-clock injection mechanism is specified.

**Impact:** D13-T7 may become a flaky test on CI under load. This is Minor
because the production predicate logic is correct; only the test harness is
fragile.

**Recommendation:** Specify that D13-T7 uses `tokio::time::pause()` and
`tokio::time::advance()` to inject deterministic elapsed time, or abstract
`Instant::now()` behind a seam that tests can control.

---

### MF-2 — Host-Suspension False Positive on `last_authenticated_connect_at`

**Location:** §SM-C (lines 1129–1200); SI-17 (lines 1699–1706).

**Problem:** `std::time::Instant` on Linux uses `CLOCK_MONOTONIC`.
`CLOCK_MONOTONIC` does not advance during VM or container suspension (compare
`CLOCK_BOOTTIME` which does). SI-17 states the field is "never actively
cleared." Consider: a worker authenticates and sets
`last_authenticated_connect_at = now()`. The host is then suspended for 60
seconds. On resume, `elapsed()` returns ≈0s (CLOCK_MONOTONIC did not advance),
and `migration_plausible()` returns `true` for the next 30s of host uptime —
even though from a wall-clock perspective, the window elapsed during suspension.

**Impact:** In a container/VM environment with scheduled suspend/resume, the
anti-coercion predicate could approve a stale timestamp as "recent." The attack
surface is narrow (requires host-level suspension control) and the predicate is
defense-in- depth, so this is Minor. It should be documented as a known
limitation in SI-17.

**Recommendation:** Add a note to SI-17 acknowledging the CLOCK_MONOTONIC /
host-suspension interaction and its bounded impact. For completeness, consider
documenting `CLOCK_BOOTTIME` as an alternative if host-suspension robustness is
required.

---

### MF-3 — Revision History Terminology Mismatch

**Location:** Revision history table, line 42; §SM-C, lines 1129–1200.

**Problem:** The v4 revision history entry reads "NF-2 closed by SM-C
anti-coercion predicate (SM-C transition)." SM-C, as defined in §SM-C, is
described as a "trigger input, not a state machine with persistent state." The
anti-coercion predicate (`migration_plausible()`) lives on the worker supervisor
struct, not inside SM-C transitions. The phrase "SM-C transition" in the
revision history implies SM-C has state transitions it does not have, creating
ambiguity for Phase B implementers reading the history.

**Impact:** Documentation confusion only; no correctness impact.

**Recommendation:** Update the revision history entry to: "NF-2 closed by worker
supervisor `migration_plausible()` predicate guarding connection re-use."

---

## Strengths

- The SM-C predicate (§SM-C, lines 1129–1200) is now concrete and implementable.
  `migration_plausible()` returns a `bool` from a single `Option<Instant>`
  field. No architectural ambiguity.
- `MIGRATION_RECENT_WINDOW = Duration::from_secs(30)` is explicit and justified
  in prose.
- SI-17 correctly records that `last_authenticated_connect_at` is set on
  successful authentication and is never actively cleared — this prevents a
  reset-on-disconnect bypass.
- D13-T7 covers the false-predicate case (field absent → always false) and both
  sides of the time boundary, which is the right shape for a security predicate
  test.
- SI-18's blanket prohibition on `deny_unknown_fields` is the correct invariant
  formulation for rolling-upgrade forward compatibility.
- The `Secret<T>` / `secrecy` crate approach for D10 (token redaction by
  construction) is correct and idiomatic Rust.
- Phase E blocking on WCP design landing is explicitly stated and correct.

---

## Open Questions

1. **Canonical soft-delete column**: What is the authoritative column name and
   type for the soft-delete marker in the production schema? This must be
   decided before Phase B begins.
2. **Migration ownership**: Which migration file (new or existing) will add the
   soft-delete column to the `users` table?
3. **D13-T6 variant enumeration**: Should the test enumerate all current
   `ServerMessage` variants explicitly, or use a reflection / `trybuild`
   approach to enforce the invariant at compile time?
4. **`CLOCK_BOOTTIME` trade-off**: Is host-suspension robustness a requirement
   for the SM-C predicate, or is the CLOCK_MONOTONIC limitation acceptable given
   the deployment model?

---

## Confidence Score

| Item                                                                   | Points | Criterion                                                                    |
| ---------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------- |
| Starting score                                                         | 100    |                                                                              |
| SF-1: D13-T6 covers only BuildRevoke, not all ServerMessage variants   | -15    | D5 — untested critical path                                                  |
| SF-2: D3 soft-delete MUST contradicts production schema exemption      | -10    | D8 — spec deviation (security-critical invariant is internally inconsistent) |
| MF-1: D13-T7 boundary tests use real Instant; race-prone on CI         | -5     | D5 — test harness fragility                                                  |
| MF-2: CLOCK_MONOTONIC host-suspension false positive undocumented      | -5     | D11 — missing documentation of known limitation                              |
| MF-3: Revision history says "SM-C transition" but no SM-C state exists | -5     | D10 — convention/documentation violation                                     |
| **Total**                                                              | **60** |                                                                              |

Score 60/100 — Significant issues. Must address before proceeding (per
confidence-scoring scale: 50–74).

---

## Verdict

**Approve with conditions.** The following conditions MUST be met before Phase B
or Phase E implementation begins:

**C1 (blocks Phase E):** Expand D13-T6 to cover every `ServerMessage` variant,
not just `BuildRevoke`. Each variant must have a sub-case that injects an
unknown JSON field and asserts silent success.

**C2 (blocks Phase B):** Resolve the D3 soft-delete schema contradiction:

- Choose a canonical marker column and add it to production migrations.
- Remove the production-schema exemption at line 1919.
- Update D3-T-owner-soft-deleted to use the production migration schema.

C3 (recommended, non-blocking): Address MF-1 by specifying deterministic clock
injection for D13-T7 boundary tests.
