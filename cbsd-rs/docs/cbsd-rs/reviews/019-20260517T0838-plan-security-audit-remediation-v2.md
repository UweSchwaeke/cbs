# Review — Plan: Security Audit Remediation (Unified), v2

| Field        | Value                                                                                                                                                                                      |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Review       | 019 — plan-security-audit-remediation, v2                                                                                                                                                  |
| Reviewer     | Claude Sonnet 4.6 (adversarial, source-validated)                                                                                                                                          |
| Date         | 2026-05-17                                                                                                                                                                                 |
| Plan         | `cbsd-rs/docs/cbsd-rs/plans/019-20260516T1033-security-audit-remediation.md`                                                                                                               |
| Designs      | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` (WCP v11) + `cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` (audit-rem v8) |
| Prior review | `cbsd-rs/docs/cbsd-rs/reviews/019-20260516T1146-plan-security-audit-remediation-v1.md` (v1, score 70/100)                                                                                  |
| Scope        | Full plan v2 review: verify v1 findings resolved; find anything v1 missed.                                                                                                                 |

---

## Executive Summary

The v2 plan is a substantial improvement over v1. All three v1 blockers have
been cleanly resolved. The plan correctly identifies every known implementation
gap from the soundness review (G1-G10), maps each to a concrete commit, provides
genuine pitfall guidance, and respects the `git-commits` golden rule at all 21
commits. One new finding (N1) is a crate-boundary violation that must be fixed
before implementation begins: the plan places `ActiveAssignmentReceipt` — a
server-only in-memory state type — inside `cbsd-proto`, which is chartered as
wire-only. Two additional minor findings (N2, N3) are documentation gaps with
zero runtime risk but which will cause implementer confusion. Final score:
**90/100**.

**Recommendation: Go — with one required pre-implementation fix (N1).**

---

## V1 Findings: Status

### C1 — Commit 2 missing `cbsd-worker` (was: Critical, D12)

**Status: RESOLVED.**

Plan v2 commit 2 packages section now explicitly includes:

> `cbsd-worker` (one-arm WARN-and-continue handler for the new
> `ServerMessage::UnauthorizedBuildAction` variant in
> `ws/handler.rs::match server_msg`, so the existing exhaustive match still
> compiles).

The pitfalls section spells out the exact risk (exhaustive match at
`handler.rs:186`, no catch-all `_` arm), identifies the behaviour required (WARN
log, continue), and notes that active stop-work is deferred to commit 6
intentionally. The test list adds a smoke test for
`ServerMessage::UnauthorizedBuildAction` deserialization and consumption.

### C2 — Commit 1 dispatch-ordering framing was ambiguous (was: Minor, D9)

**Status: RESOLVED.**

Commit 1's pitfalls now include an explicit two-path analysis:

> Normal-dispatch flow: `ws/dispatch.rs:122-135` inserts the `ActiveBuild` into
> `queue.active` with `connection_id` already populated at insertion time. …
> **No ordering inversion on this path.**
>
> Reconnect Building flow: `ws/handler.rs:661-665` invokes
> `dispatch::handle_build_started` **before**
> `queue.active.get_mut(&build_id.0).connection_id = …` runs. This is the
> inversion gap G10 refers to.

The framing is now precise and falsifiable.

### C3 — Commit 5 component prefix was `cbsd-rs/server` (was: Minor, D10)

**Status: RESOLVED.**

The summary table now shows commit 5 component as `cbsd-rs` (workspace-span,
because it touches both `cbsd-server` and `cbc`). The subject is
`cbsd-rs: bound log tail with reverse block scanning`.

---

## New Findings

### N1 — `ActiveAssignmentReceipt` placed in `cbsd-proto` (Major)

**Finding:** Commit 2's packages list includes:

> `cbsd-proto` (new `ServerMessage::UnauthorizedBuildAction` variant,
> `WorkerBuildAction` enum, `UnauthorizedBuildReason` enum,
> **`ActiveAssignmentReceipt` enum type used by `cbsd-server`**)

`cbsd-proto` is defined in the project's authoritative documentation as a
"shared types crate (no IO, no async)" that carries **wire format only**. Both
`CLAUDE.md` files confirm this. `ActiveAssignmentReceipt` is not a wire type.
The design itself (audit-rem v8, WCP state invariant SI-25) states:

> receipt state lives in process memory only. After a server restart, no
> `ReceivedByWorker` rows exist; startup recovery uses the existing
> fail-in-flight policy.

Source validation confirms: `ActiveAssignmentReceipt` does not exist anywhere in
the current codebase (grep over all `*.rs` files returns empty). The type is
new. `ActiveBuild` — the struct that gains the `receipt` field — lives in
`cbsd-server/src/queue/mod.rs` (confirmed by reading that file). There is no
reason for this type to cross a crate boundary into `cbsd-proto`.

Placing it in `cbsd-proto` creates two concrete problems:

1. **Implied protocol surface.** Any type in `cbsd-proto` is, by charter and by
   practice, part of the binary wire format. Embedding a server-internal state
   enum there implies it belongs on the wire — a future maintainer will
   reasonably add serde derives and wonder why.

2. **Crate boundary violation.** `cbsd-proto` is depended on by `cbsd-worker`
   and `cbc` as well as `cbsd-server`. Placing a server-only in-memory type
   there forces every consumer to compile a type it can never use.

**Fix:** Declare `ActiveAssignmentReceipt` in `cbsd-server/src/queue/mod.rs`
alongside `ActiveBuild`. Remove it from the `cbsd-proto` packages list in
commit 2. This is a plan correction only; no design document needs to change
(the design correctly places receipt state in the server process).

**Required before implementation begins.** The plan revision is a one-line
packages-list edit with no cascading effect on any other commit.

---

### N2 — Commit 21 pitfalls omit four compile-break sites (Minor)

**Finding:** Commit 21 extends `BuildRevoke` with
`reason: Option<BuildRevokeReason>` (audit-rem D13). The packages list correctly
includes all three affected crates (`cbsd-proto`, `cbsd-server`, `cbsd-worker`).
However, the pitfalls section says only:

> Adding a new field to `BuildRevoke` is a compile break on the worker side —
> the existing destructure at `cbsd-worker/src/ws/handler.rs:389` uses a
> named-field pattern without `..`.

This identifies the worker destructure but omits three server-side construction
sites verified in source:

- `cbsd-server/src/ws/dispatch.rs:500`:
  `ServerMessage::BuildRevoke { build_id: BuildId(build_id) }`
- `cbsd-server/src/main.rs:422`:
  `cbsd_proto::ws::ServerMessage::BuildRevoke { build_id: cbsd_proto::BuildId(*build_id) }`
- `cbsd-server/src/ws/handler.rs:698`: `ServerMessage::BuildRevoke { build_id }`

All three are struct-literal constructions. Adding a required field (or an
`Option<>` field without `#[serde(default)]` in the struct literal) produces
"missing field `reason`" compile errors at all three sites. Commit 21 MUST
update all four sites (three construction, one destructure) in the same commit
to compile cleanly.

**Runtime risk: zero.** The compiler catches every missing site immediately. The
commit packages all three crates, so the implementer will touch all of them.
This is a documentation gap only — but a reviewer who misses it may stop at the
worker destructure and overlook the three server sites, creating an incomplete
implementation.

**Fix:** Add the three server-side construction sites to commit 21's pitfalls
section with their exact file/line references.

---

### N3 — Phase independence framing is misleading (Minor / Observation)

**Finding:** The plan overview states:

> Phase 2 (audit-remediation cross-cutting) has no ordering dependency on Phase
> 1 or Phase 3 — its commits may interleave freely.

Commit 9's own pitfalls section contradicts this:

> D6's trust argument is fully in force only after Phase 1's W2 (ownership
> checks) lands; the plan orders Phase 1 before Phase 2, so D6's trust position
> is sound at commit 9.

The audit-rem design (v8) is even more explicit, documenting the window if Phase
C lands before WCP ownership rules:

> if Phase C lands before WCP's ownership rules are in force, there is a window
> in which log output is size-bounded at the message level but not
> ownership-gated at the build level.

There is no _compile_ dependency and no _functional break_ — the plan correctly
orders Phase 1 before Phase 2 regardless. But the overview claim "no ordering
dependency" is technically false: there is a security ordering dependency if the
phases were actually interleaved. A developer reading only the overview could
dequeue Phase 2 commits before Phase 1 completes and create the window the
design warns about.

**Risk: managed by plan ordering.** This is not a blocker. The plan's commit
sequence is correct; only the prose characterisation is loose.

**Fix:** Replace "has no ordering dependency" with "has no _compile_ or
_functional_ dependency, but SHOULD land after Phase 1 is complete to avoid a
window in which message-level size bounds are in force without the ownership
check that makes the trust argument sound."

---

## Per-Commit Assessment

All 21 commits were reviewed against source code, design documents, and the
`git-commits` skill. The following summarises the key assessment per commit.
Unless noted, each commit passes the smell test: describable in one sentence,
previous commit compiles, independently revertable, no dead code at commit-time.

**Commit 1 (rollback DB operation):** Clean. Pitfalls cover the six-column reset
requirement and the reconnect-path ordering fix precisely. Test list is
adequate. ~320 LOC is in range.

**Commit 2 (build-scoped authorization):** Sound after N1 fix. The
WARN-and-continue arm for `cbsd-worker` is now explicit. The pitfalls covering
`ActiveAssignmentReceipt` semantics, centralised ownership-check helper, and
dispatch-ack cancellation are correct. The worker-side smoke test is a strong
addition. ~720 LOC is at the upper edge — acceptable given the commit cannot be
split without violating the golden rule (adding a new `ServerMessage` variant
requires the consumer update in the same commit).

**Commit 3 (reconnect + idle ownership):** Clean. Two-phase lock discipline is
correctly called out. Idle-reconcile worker-id filter (gap G8) is the primary
fix and is clearly described. Receipt-state dependency on commit 2 is correctly
noted. ~450 LOC is in range.

**Commit 4 (empty components rejection):** Clean. Justification for the
sub-200-LOC exception is valid. Centralised validator requirement is correctly
called out. Scheduler trigger disabling (not retry) is the correct semantic per
the design.

**Commit 5 (log tail bounded scan):** Clean. UTF-8 boundary, partial-line drop,
and single-over-budget line semantics are all called out. `cbc` response shape
migration is included. Component prefix `cbsd-rs` is correct for a
workspace-spanning commit. ~330 LOC is in range.

**Commit 6 (worker supervisor):** Large (~750 LOC) but justified — the design
correctly explains there is no clean split boundary. The supervisor replaces
local-variable state that is intertwined with the websocket loop. Pitfalls cover
output spool budget (64 MiB), terminal-pending-report semantics, and the
reconnect status derivation. This is the riskiest single commit in the plan from
an implementation complexity standpoint; the pitfalls text is proportionally
detailed.

**Commits 7–18 (audit-remediation cross-cutting):** All pass the smell test.
Highlights:

- Commit 9 (body/WS size limits): the trust caveat against Phase 1 is present in
  pitfalls even if the overview framing is loose (N3).
- Commit 14 (`Secret<T>` wrap): the `use secrecy::ExposeSecret;` import
  requirement at every call site is called out, which is the most common
  `secrecy` crate pitfall.
- Commit 16 (API key index): sub-50-LOC exception is justified — a single
  migration file and its `.sqlx/` cache delta. Combining with any adjacent
  commit would mix unrelated concerns.
- Commit 18 (SI-18 regression test): correct to make this its own commit; it
  produces a permanent compile-time regression guard in `cbsd-proto`.

**Commits 19–21 (WCP-extension):**

- Commit 19 (accepted-phase reconnect): correctly depends on commit 6's
  supervisor. ~250 LOC is at the floor but justified — this is the full feature.
- Commit 20 (dead worker resolution): the liveness query, receipt-state read,
  and rollback call are in range at ~400 LOC. Startup-recovery integration point
  is correctly noted.
- Commit 21 (migration revoke + drain): ~700 LOC is at the upper edge. Pitfalls
  correctly identify the worker-side destructure compile-break but miss the
  three server-side construction sites (N2).

---

## Confidence Score

| Item                                                          | Points | Description                                                               |
| ------------------------------------------------------------- | ------ | ------------------------------------------------------------------------- |
| Starting score                                                | 100    |                                                                           |
| N1: `ActiveAssignmentReceipt` in `cbsd-proto`                 | -5     | D10 — crate convention violation; server-only type in wire crate          |
| N2: Commit 21 pitfalls omit 3 server-side compile-break sites | -5     | D11 — undocumented pitfall for implementer at four known source locations |
| **Total**                                                     | **90** |                                                                           |

N3 (phase independence framing) is an observation, not a deduction. The plan
ordering is correct; only the prose is imprecise.

---

## Summary

**Score: 90/100 — Ready to proceed with one required fix.**

### Required pre-implementation fix

**N1**: Remove `ActiveAssignmentReceipt` from the `cbsd-proto` packages list in
commit 2. Declare the type in `cbsd-server/src/queue/mod.rs` alongside
`ActiveBuild`. This is a one-line plan edit; no design document changes are
needed.

### Recommended plan improvements (non-blocking)

**N2**: Add the three server-side `BuildRevoke` construction sites to commit
21's pitfalls: `cbsd-server/src/ws/dispatch.rs:500`,
`cbsd-server/src/main.rs:422`, `cbsd-server/src/ws/handler.rs:698`.

**N3**: Tighten the Phase 2 independence prose to say "no compile or functional
dependency" and note the security ordering preference.

### Top findings by severity

1. **N1 (Major):** `ActiveAssignmentReceipt` assigned to `cbsd-proto` is a
   crate-boundary violation. The type is server-only in-memory state; placing it
   in the wire crate violates the chartered boundary and misleads future
   maintainers about the protocol surface.

2. **N2 (Minor):** Commit 21 pitfalls name the worker destructure
   (`handler.rs:389`) as the `BuildRevoke` compile break but omit the three
   server-side construction sites. An implementer following only the pitfalls
   section will miss these and get three additional compiler errors.

3. **N3 (Minor):** The plan overview describes Phase 2 as having "no ordering
   dependency" on Phase 1, but commit 9's own pitfalls and the audit-rem design
   explicitly document a security ordering window if Phase 1 is not completed
   first. The plan ordering is correct; the characterisation is not.
