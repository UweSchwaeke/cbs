# Design Review: 019 Security Audit Remediation — v2

**Reviewer:** Independent (Staff Eng)\
**Review target:**
`cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md`\
**Prior reviews:**
`cbsd-rs/docs/cbsd-rs/reviews/019-20260514T1752-design-security-audit-remediation-v1.md`\
**Date:**
2026-05-14

---

## Executive Summary

The v2 design makes substantial progress. F-R2 (Secret\<T>), F-R4 (loopback
whitelist), F-R5 (SM-S missing transitions), F-R7 through F-R12 are all
genuinely closed. The three WCP high-risk open items (D11–D13) now carry fully
specified state machines, resolution tables, and test matrices. However, two
significant issues remain open: the `BuildRevokeReason` protocol extension (D13)
leaves the serde wire representation of the enclosing field unspecified,
creating a forward/backward compatibility gap that reintroduces the F-R1
data-loss risk on rolling upgrades; and the CI grep gate's `# allow-expose`
exemption token (D10) uses `#` rather than `//`, which is not a Rust line
comment and will cause either silent CI misses or grep false positives. These
two issues are Significant. Four minor observations follow. The verdict is
**Approve with conditions**: both Significant issues must be resolved in the
plan document before implementation of Phase E and Phase B respectively.

---

## Closure Verdicts for F-R1 through F-R12

| Finding                                    | Verdict                 | Notes                                                                                                              |
| ------------------------------------------ | ----------------------- | ------------------------------------------------------------------------------------------------------------------ |
| F-R1 D13 terminal-pending-report data loss | Partially-closed        | Drain-then-revoke semantics are correct. Serde wire representation of the enclosing field is unspecified; see N-1. |
| F-R2 D10 Secret\<T> Serialize gap          | Closed                  | No-Serialize / no-Deserialize contract is tight; compile-fail tests specified; secrecy crate recommended.          |
| F-R3 D5 PAX + chained-symlink              | Closed                  | PAX-aware path use explicit; two-phase check specified. Test example has a documentation error; see N-3.           |
| F-R4 D1 loopback whitelist                 | Closed                  | url::Host enum covers IPv4 loopback range, IPv6 ::1, and localhost case-insensitive.                               |
| F-R5 SM-S missing transitions              | Closed                  | All D11/D12/D13 transitions shown; rollback arrows present; absorbing-state property stated.                       |
| F-R6 D3 trigger-time scope re-validation   | Closed (with minor gap) | Normal re-validation closed. Owner-deleted edge case unspecified; see N-2.                                         |
| F-R7 D2 userinfo trust gap                 | Closed                  | Residual trust gap documented as a known informational limitation.                                                 |
| F-R8 D9 coverage                           | Closed                  | Project-wide URI-logging policy covers panic handlers and error reporters.                                         |
| F-R9 D8 Windows rename                     | Closed                  | Windows caveat documented; temp-file cleanup explicitly required.                                                  |
| F-R10 D3 custom-role migration             | Closed                  | No-auto-map is explicit; migration comment block required; test specified.                                         |
| F-R11 D4 history.replaceState              | Closed                  | Required immediately after token extraction; test added.                                                           |
| F-R12 Phase E WCP dependency               | Closed                  | WCP plan-document path and commit-completion checkpoint are explicit prerequisites.                                |

---

## Significant Concerns

### N-1 — BuildRevokeReason field serde representation unspecified (D13)

**Problem.** D13 introduces `BuildRevokeReason` as an optional `reason` field on
`ServerMessage::BuildRevoke` and calls it "a forward-compatible serde addition."
The design shows the enum definition with `#[serde(rename_all = "snake_case")]`
but specifies neither the type of the enclosing field on `BuildRevoke` (is it
`Option<BuildRevokeReason>` or a required field?) nor the serde annotation that
makes it optional (`#[serde(default)]`). For the addition to be truly
forward-compatible — so that an old cbsd-worker binary that predates D13 can
parse a `BuildRevoke` message from a D13-aware server without rejecting it — the
field must be declared as `Option<BuildRevokeReason>` AND annotated
`#[serde(default)]` on `BuildRevoke`. Without `#[serde(default)]`, serde's
default behaviour for `Option` fields with `deny_unknown_fields` (or any strict
deserializer config) is to require the key to be present or produce an error for
unexpected keys; the behaviour depends on whether `cbsd-proto`'s derive uses
`deny_unknown_fields`, which the design does not address.

**Impact.** During any rolling upgrade where the server is updated to include
D13 before the worker, the server sends
`BuildRevoke { reason: "migration_supersede" }` on the old connection. An old
worker, unable to parse the now-present `reason` field, discards the message or
panics on deserialisation. The worker's supervisor never receives the migration
revoke, stays in `terminal-pending-report` with the old WCP v11 behaviour
(discard on any revoke), and the drain-then-revoke data-loss scenario from F-R1
reappears. The design's claim that "protocol version stays at 2 (per WCP v11's
pre-production no-compat-shim policy)" is correct for an
in-production-no-compat-shim context, but the pre-production claim does not
eliminate the rolling upgrade risk between server and worker processes on the
same host.

**Recommendation.** The plan document for Phase E must specify the exact wire
shape:

```rust
// in cbsd-proto, on BuildRevoke:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub reason: Option<BuildRevokeReason>,
```

The worker MUST treat `reason == None` (old-server messages) as
`Admin`-semantics, preserving backward compatibility. The design should add this
field annotation and the None-handling rule explicitly. A compile-tested serde
round-trip (absent field → None; unknown variant → Err) should be added to the
Phase E test matrix.

---

### N-2 — D10 CI grep gate uses `#` not `//` as Rust comment marker

**Problem.** Section D10 (lines 718–724) specifies a CI grep gate that rejects
`.expose_secret()` calls unless the same line contains an `# allow-expose`
comment. `#` is not a Rust line comment delimiter; `//` is. A Rust source line
of the form:

```rust
let raw = secret.expose_secret(); // allow-expose: audit ref #123
```

contains `// allow-expose` but NOT `# allow-expose`. The grep pattern
`\.expose_secret\(\)` without `# allow-expose` would match the above line and
flag a false-positive rejection, breaking the CI gate for every legitimately
annotated call site. Conversely, a shell grep looking for the literal
`# allow-expose` token would silently miss all real Rust exemptions because `#`
opens an attribute (`#[...]`) in Rust, not a comment.

**Impact.** The CI gate, as specified, is either (a) permanently broken for
exempting legitimate call sites, or (b) trivially bypassed by any caller who
writes the `// allow-expose` form that the language naturally uses. Either
outcome defeats the gate's purpose.

**Recommendation.** Change the exemption token throughout D10 and its tests to
`// allow-expose`. Revise the test that asserts "a synthetic diff introducing
`.expose_secret()` without an `// allow-expose` comment is rejected" to use the
corrected token. Confirm the grep pattern matches the Rust comment syntax.

---

## Minor Observations

### N-3 — D5 chained-symlink test example is internally inconsistent

The test description (lines 439–445) states: "entry 2 is a regular file at
`inner/../../escape`. Phase 1 logical check on entry 2 **passes** (`escape`
after lexical normalization is inside root), but phase 2 real-path check catches
that `inner` is a symlink."

This is incorrect. Lexical normalization of
`unpack_root.join("inner/../../escape")` resolves to one directory above
`unpack_root` (e.g. `/tmp/escape` when root is `/tmp/unpack`). Phase 1 **would
catch** this, not pass it. The scenario as written does not require Phase 2 to
save it.

The two-phase check is still architecturally correct and necessary — the real
attack that Phase 2 is needed for uses a path component that is a symlink but
involves no `..` in the entry path itself (e.g., entry 1 creates
`inner → valid_subdir`, then entry 2 later references `inner` as a directory
prefix where `inner` turns out to be a symlink to a path that escapes via a
chain). The example should be corrected so it demonstrates a case that actually
reaches Phase 2 before failing.

**Impact.** Documentation error only. Phase 2 is still required and the
two-phase specification is correct. However, an implementer who writes the test
as specified will observe Phase 1 catching it and may incorrectly conclude their
Phase 1 implementation is overly aggressive or that Phase 2 is redundant.

**Recommendation.** Replace the chained-symlink test example with one where
Phase 1 genuinely passes: for instance, entry 1 is a symlink `a → b` (both `a`
and `b` appear to be inside the root), followed by entry 2 creating `b` as a
symlink `b → ../../outside`, and entry 3 accessing `a/file`. Phase 1 passes on
`a/file` (no `..`), but Phase 2 walks the `a` → `b` → `../../outside` chain and
catches the escape.

---

### N-4 — D3 owner-deleted edge case unspecified

The trigger-time re-validation in D3 (lines 267–283) specifies behaviour when
the owner loses a capability. It does not specify what happens if the owner row
is deleted entirely from the `users` table (e.g., an admin removes the account).
The `periodic_tasks.owner_email` foreign key behaviour at deletion time is not
mentioned; if there is no `ON DELETE CASCADE` or `ON DELETE SET NULL`, a trigger
firing for a now-deleted owner will encounter a missing-user lookup. Depending
on the query path, this surfaces as a SQL foreign-key violation (if FK
enforcement is on) or as a capability-lookup returning empty (which the existing
guard treats as "all capabilities lost" — which happens to produce the correct
behaviour of disabling the task).

This is a minor gap: the accidental correct behaviour (empty caps → disable)
makes it unlikely to cause harm in practice, but the design should state this
case explicitly so an implementer does not add short-circuit logic that
re-enables the task or silently skips the trigger.

**Recommendation.** Add one sentence to D3's trigger-time re-validation block:
"If the owner no longer exists in the `users` table, treat as all-caps-lost:
disable the task and write a `last_error` indicating the owner was not found."

---

### N-5 — secrecy crate API: expose_secret() is a trait method

D10 presents `pub fn expose_secret(&self) -> &T` as the "single named accessor"
on the `Secret<T>` newtype. The `secrecy` crate (recommended in the same
section) implements this on the `ExposeSecret` trait, not as an inherent method.
Callers that use the `secrecy` crate must import `secrecy::ExposeSecret` to
bring the method into scope. If the implementation adopts `secrecy` without the
trait import, `.expose_secret()` will fail to compile at call sites. This is an
informational note; the design already recommends `secrecy` and notes it as
"contract specification, not preferred implementation," so the implementer is
expected to evaluate the crate. The plan document should note the required
import.

---

## Strengths

**State machine consolidation (D11–D13).** The SM-W / SM-S / SM-R triad is the
clearest articulation of the WCP state space produced in any of the 019
documents to date. The transition ownership table ("which decision owns which
transitions") and the composed sequence diagram are directly implementable. The
boundary between "reporter- directed cleanup" (D13) and "state-mutating revoke"
is precisely drawn and consistently enforced across all four SM-W phases.

**D12 liveness resolution table.** The table-driven approach with explicit
`AwaitingReceipt` vs `ReceivedByWorker` branching closes the ambiguity from WCP
v10 cleanly. The rationale for `ReceivedByWorker + dispatched → failure` (not
requeue) is correctly grounded in side-effect avoidance, and the server-restart
invariant (no reconstruction of receipt state) eliminates the cross-restart
semantic gap.

**D10 Secret\<T> contract tightness.** Forbidding all formatting surfaces
(Pointer, Octal) not just Debug and Display is the right call. The compile-fail
test matrix using `trybuild` is specific enough to prevent future regressor
drift. The recommendation to adopt `secrecy` over in-house reimplementation
reflects appropriate humility about crypto-adjacent plumbing.

**D5 two-phase containment.** Making the PAX-awareness explicit as a design
decision (not an implementation detail) is important: the distinction between
POSIX 100-byte fields and PAX-extended fields is the historical root of most
`tar`-escape CVEs, and pinning it in the design prevents future implementers
from accidentally using the wrong accessor.

**Phase E prerequisite gating.** Naming the exact artifact that must exist
(`cbsd-rs/docs/cbsd-rs/plans/019-<timestamp>-worker-control-plane- hardening.md`)
and the completion checkpoint (all WCP plan commits landed) is operationally
specific. This is the level of precision needed to prevent Phase E from being
started against absent API surfaces.

**D1 strict CBSD_DEV parsing.** Treating any non-allowlist value as disabled
(including `0`, `false`, empty string) is the correct safe default. Combining
this with the loopback-only `NoVerifier` guard gives defense-in-depth for dev
mode.

---

## Open Questions

1. **D13 serde field type (N-1 above).** Will the plan specify
   `Option<BuildRevokeReason>` with `#[serde(default)]`, and will the
   None-is-Admin fallback be stated as a normative worker requirement?
2. **Phase A + Phase E sequencing with worker binary releases.** When Phase E
   lands on the server, any worker binary predating Phase A (which carries the
   D11 `accepted`-phase fix) will be running against a D13- capable server. Is
   there a coordinated rollout requirement, or is the expectation that Phase A
   always lands at least one release before Phase E?
3. **D6 per-log-line cap trust argument.** The design notes that D6's trust
   argument is "fully in force only after both Phase C and the WCP
   implementation have landed." Is there a planned plan-level tracking item that
   links Phase C and WCP landing as a joint prerequisite for the full trust
   claim, or is this handled only by the prose note?

---

## Confidence Score

| Item                                                                                                              | Points | Description                                                                                                            |
| ----------------------------------------------------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------- |
| Starting score                                                                                                    | 100    |                                                                                                                        |
| D7: N-1 BuildRevokeReason serde representation unspecified — reintroduces F-R1 data loss risk on rolling upgrades | -20    | Security gap (D7): forward-compat claim is incomplete without field type + serde(default)                              |
| D10: N-2 `# allow-expose` CI gate uses wrong comment delimiter                                                    | -5     | Convention violation (D10): CI gate is unimplementable as written; legitimate exemptions will be flagged as violations |
| D11: N-3 D5 chained-symlink test example internally inconsistent                                                  | -5     | Spec deviation (D8): test asserts Phase 1 passes a case that Phase 1 actually catches; will mislead implementer        |
| D11: N-4 D3 owner-deleted edge case unspecified                                                                   | -5     | Missing documentation (D11): silent correct-by-accident behaviour is not a substitute for a stated invariant           |
| **Total**                                                                                                         | **65** |                                                                                                                        |

Score 65/100 — Significant issues. Must address before proceeding.

---

## Verdict

**Approve with conditions.**

The two Significant issues must be resolved before implementation:

1. **(N-1, blocking Phase E):** The plan document for Phase E must specify the
   exact serde wire shape of the `reason` field on `BuildRevoke`
   (`Option<BuildRevokeReason>` with `#[serde(default)]`) and must state the
   None-is-Admin fallback as a normative worker requirement. A serde round-trip
   test for the absent-field case must appear in Phase E's test matrix.

2. **(N-2, blocking Phase B):** The exemption token in D10 must be corrected to
   `// allow-expose` (Rust line comment syntax) throughout the design text and
   in all CI gate test examples. The plan document for Phase B must carry the
   corrected token.

The minor observations (N-3 through N-5) should be addressed in the design
document before the plan is authored, but they do not block plan authoring if
the author notes the corrections in the plan.
