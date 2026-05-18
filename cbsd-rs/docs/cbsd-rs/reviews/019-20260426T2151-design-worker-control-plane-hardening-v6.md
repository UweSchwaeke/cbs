# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v7                                                                     |
| Date           | 2026-04-26 21:51 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior review/security-review documents                 |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until reconnect authorization and remaining state cases are made explicit   |

## Summary

Draft v7 addresses the concrete v5 blockers for dispatch-ack cancellation,
reporter-directed stray revokes, queued rollback cleanup, and the broad
`NotAssigned` unauthorized mapping. The original security review's main
cross-worker spoofing, log spoofing, empty-descriptor, dispatch rollback, and
tail-memory issues are represented in the design.

The remaining problem is not a missing feature but an authorization ambiguity:
`worker_status(building)` is simultaneously listed as a normal connection-owned
build-scoped message and as the mechanism that can resume an assignment on a new
connection. Those rules need a distinct reconnect authorization sequence so
implementers do not either reject legitimate reconnects or weaken the
connection-owned guard for ordinary lifecycle/output messages.

Implementation-only phase-review items are N/A because no implementation plan or
implementation commits are in scope.

## Findings

### High: `worker_status(building)` has conflicting connection ownership rules

D1 says every build-scoped message, including `worker_status` reporting
`Building { build_id }`, is authorized only when `queue.active[build_id]` exists
and the active entry's `connection_id` equals the websocket connection that sent
the message
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:92`,
`:102`). The reconnect section then accepts `worker_status(building)` when the
authenticated registered worker ID matches `builds.worker_id` and the active
entry is for that same registered worker assignment
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:436`,
`:442`). The transition matrix says the side effect is to "resume the assignment
on the new connection"
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:414`).

Those statements are not the same authorization rule. A reconnecting websocket
normally has a new server-assigned connection ID. If D1 is applied literally
before migration, a valid reconnect cannot satisfy the connection-id equality
check. If an implementer instead relaxes the equality check for
`worker_status(building)` without a precise migration rule, the message becomes
a special path that can rewrite active ownership by registered worker ID.

The current server has an implicit connection migration step during handshake:
it finds an existing queue entry with the same `registered_worker_id`, rewrites
matching active builds from the old connection ID to the new connection ID, and
removes the old worker entry (`cbsd-rs/cbsd-server/src/ws/handler.rs:263`,
`:283`). The design never states whether this migration is required, whether it
must happen before `worker_status`, or whether `worker_status(building)` itself
performs the active-entry migration. Since `ActiveBuild` is keyed to
`connection_id` (`cbsd-rs/cbsd-server/src/queue/mod.rs:25`, `:36`), leaving that
sequencing implicit is a security-sensitive ambiguity.

Required design change: define a separate reconnect authorization and migration
sequence. It should say exactly when a new connection may replace the old
connection ID, what old connection states are allowed, whether the migration is
atomic with status handling, and that ordinary `build_accepted`,
`build_started`, `build_output`, `build_finished`, and `build_rejected` still
require post-migration connection ownership. Add explicit tests for accepted
same-worker reconnect, wrong-worker reconnect, and wrong-connection reconnect.

### Medium: idle reconnect can roll back a same-worker assignment without a connection-state precondition

Draft v7 fixes the v5 cross-worker idle problem by limiting
`worker_status(idle)` reconciliation to rows whose persisted `builds.worker_id`
matches the authenticated registered worker ID
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:477`,
`:487`). That prevents an unrelated worker from requeueing another worker's
build, but it does not say whether the prior connection for that same registered
worker must be disconnected, migrated, or otherwise quiesced before an idle
status can roll back a `dispatched` assignment.

This matters because the existing code explicitly treats a second connection
with the same registered worker ID as either reconnection or stale
double-connect and migrates active entries to the new connection
(`cbsd-rs/cbsd-server/src/ws/handler.rs:263`, `:283`). If the design permits the
new connection to report idle and roll back the assignment without first
defining what happened to the old connected worker, a duplicate connection or a
compromised same-worker API key can turn a live assignment into `queued` while
local work may still be running. That is narrower than the original cross-worker
issue, but it still affects the control-plane state machine.

Required design change: state the precondition for same-worker idle rollback.
For example, require that the old active connection is already disconnected or
has been superseded by the connection-migration path, and define whether a live
old connection is force-closed, ignored, or treated as a stale double-connect.
Add a test for idle reconnect while an old same-worker connection is still
connected or within grace.

### Medium: owned `worker_status(building)` while `revoking` is missing from the matrix

The design covers invalid reconnect reports and idle reconnect handling for
`revoking`, but the `worker_status(building)` row does not define the valid
owned behavior when the DB state is already `revoking`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:414`).
The reconnect section only says the DB state must be one where resume is
meaningful
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:436`,
`:442`).

The current implementation has a concrete behavior for this state: when a worker
reconnects building and the DB state is `revoking`, it re-sends the normal build
revoke (`cbsd-rs/cbsd-server/src/ws/handler.rs:678`, `:683`). The design should
either preserve that behavior or replace it. Otherwise an implementer could
classify an assigned worker's `revoking` reconnect as unauthorized, send the
reporter-directed no-state-mutation revoke, or simply resume a build that the
server is trying to stop.

Required design change: add `revoking` to the valid `worker_status(building)`
reconnect state handling and specify whether the server re-sends the normal
stateful revoke, leaves state unchanged and sends a revoke frame, or follows
another centralized revoke/liveness path. Add a test for owned building
reconnect while DB state is `revoking`.

### Low: `InvalidAssignment` remains abstract and untested

Draft v7 maps syntactically valid build-scoped authorization failures to
`NotAssigned`, which addresses the v5 oracle concern for unknown, inactive,
terminal, wrong-connection, and wrong-worker cases
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:214`,
`:221`). The second enum value, `InvalidAssignment`, is still described only as
"malformed or internally inconsistent assignment cases." There is no concrete
case list and the test expectations only require authorization failures to map
to `NotAssigned`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:607`,
`:608`).

That leaves implementers to decide which internal inconsistencies become a
worker-visible `InvalidAssignment`, and it can reintroduce a small state oracle
if different internal states are mapped inconsistently.

Required design change: either remove `InvalidAssignment` until a real wire case
exists, or list the exact cases that use it and add tests proving those cases do
not reveal build existence or ownership.

## Prior Review Coverage

The v5 review findings were checked against Draft v7:

| v5 finding                                                     | Status           | Notes                                                                                                                                                                                                               |
| -------------------------------------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dispatch-ack timer cancellation for start/resume paths         | Addressed        | D4 now says any valid owned receipt/execution message cancels the timer, including `build_started`, `build_output`, `build_finished`, transient `build_rejected`, and accepted reconnect `worker_status(building)`. |
| Stray `BuildRevoke` needs distinct no-state-mutation send path | Addressed        | D2 and Reconnect Ownership now define reporter-directed revoke as direct-to-reporter with no DB, timer, watcher, or active-queue side effects.                                                                      |
| Queued rollback reset only specified for pre-delivery failures | Addressed        | D4 now applies the same rollback cleanup to dispatch-ack timeout, transient `build_rejected`, and same-worker idle rollback from `dispatched`.                                                                      |
| Coarse unauthorized reason mapping missing                     | Mostly addressed | D3 maps valid authorization failures to `NotAssigned`; `InvalidAssignment` still needs concrete cases or removal.                                                                                                   |

Earlier v1-v4 findings that were carried forward by v5 were also checked:

| Earlier finding                                              | Status                  | Notes                                                                                                           |
| ------------------------------------------------------------ | ----------------------- | --------------------------------------------------------------------------------------------------------------- |
| Dispatch rollback column resets                              | Addressed               | D4 lists the reset columns and requires a dedicated DB operation.                                               |
| Meaningful-state authorization matrix                        | Mostly addressed        | The matrix exists, but `worker_status(building)` reconnect/migration and `revoking` remain ambiguous.           |
| Descriptor validation centralization                         | Addressed               | D5 requires one typed validator for REST, periodic create/update, and scheduler trigger paths.                  |
| Bounded log tailing contract                                 | Addressed               | D7 selects reverse block scanning, a 1,000-line cap, a 4 MiB byte budget, full-line output, and valid UTF-8.    |
| Enum-based unauthorized action/reason                        | Addressed with residual | Enums are selected; `InvalidAssignment` needs exact semantics.                                                  |
| Protocol version decision                                    | Addressed               | D3 keeps protocol version 2 as a pre-production correction.                                                     |
| Reconnect ownership gap                                      | Partially addressed     | Cross-worker reconnect is covered, but connection migration for valid same-worker reconnect is not explicit.    |
| `cbc` log-tail contract                                      | Addressed               | `cbc` is in package scope, default tail count is 50, and exact `total_lines` is no longer required.             |
| UTF-8/full-line truncation                                   | Addressed               | D7 defines both behaviors.                                                                                      |
| `worker_status(building)` conflicting unauthorized responses | Addressed               | Invalid reports now consistently receive `UnauthorizedBuildAction` followed by reporter-directed `BuildRevoke`. |
| `worker_status(idle)` listed under build-id authorization    | Addressed               | D1 separates idle reconciliation from build-scoped authorization.                                               |

## Deferred Or Ambiguous Items

No items are explicitly deferred by the design; it states that open questions
are resolved. Remaining ambiguous or missing items:

- Same-worker `worker_status(building)` reconnect needs a distinct authorization
  and connection-migration sequence.
- Same-worker `worker_status(idle)` rollback needs an old-connection state
  precondition.
- Owned `worker_status(building)` while DB state is `revoking` needs a defined
  state transition.
- `InvalidAssignment` needs exact cases or should be removed.

## Commit Boundary Notes

No implementation commits exist. The design 019 document and review documents
are a coherent documentation series. They should remain separate from unrelated
Markdown churn already present in the worktree.

## Confidence Score

Design-phase scoring starts at 100. Implementation-only criteria with no plan or
code in scope are N/A unless the design itself creates the defect.

| Item                                                | Points | Description                                                                                                                                                  |
| --------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Starting score                                      | 100    |                                                                                                                                                              |
| D7: reconnect building ownership conflict           | -20    | `worker_status(building)` is both connection-owned and allowed to resume on a new connection without a specified migration sequence.                         |
| D7: idle rollback lacks old-connection precondition | -20    | Same-worker idle reconnect can roll back a dispatched assignment without defining whether the previous active connection is disconnected, migrated, or live. |
| D11: `revoking` reconnect behavior undocumented     | -5     | Owned `worker_status(building)` while DB state is `revoking` is absent from the transition matrix and tests.                                                 |
| D11: `InvalidAssignment` cases undocumented         | -5     | The coarse reason exists but has no exact trigger list or test expectation.                                                                                  |
| **Total**                                           | **50** |                                                                                                                                                              |

Interpretation: 50/100. The design has addressed most prior blockers, but the
remaining issues are in reconnect ownership and revoke-state behavior, which are
central to this hardening work.

## Go / No-Go

No-go for implementation planning as-is.

Required before planning:

1. Define same-worker reconnect authorization and connection migration for
   `worker_status(building)`.
2. Define the precondition for idle reconnect rollback of same-worker
   assignments.
3. Add owned `worker_status(building)` behavior for `revoking` assignments.
4. Define or remove `InvalidAssignment`.
