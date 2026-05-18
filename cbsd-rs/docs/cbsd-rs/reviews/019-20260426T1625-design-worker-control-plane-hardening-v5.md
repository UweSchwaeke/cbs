# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v5                                                                     |
| Date           | 2026-04-26 16:25 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior review/security-review documents                 |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until the remaining timer and revoke-path ambiguities are resolved          |

## Summary

Draft v6 addresses the two v4 review findings:

- `worker_status(idle)` is split out from build-scoped `build_id` authorization
  and is limited to the authenticated worker's own persisted assignments.
- Invalid `worker_status(building)` claims now send both
  `UnauthorizedBuildAction` and `BuildRevoke` to the reporting worker.

The design now covers the original security review's major trust-boundary
issues, but it still leaves two control-plane paths ambiguous enough to cause
production misbehavior: ack timers after implicit start/resume, and the exact
mechanism used to revoke a stray reporting worker without revoking the real
assigned worker.

Implementation-only phase-review items are N/A because no implementation plan or
implementation commits are in scope.

## Findings

### High: start/resume paths can leave the dispatch ack timer armed

The transition matrix says `build_accepted` cancels the dispatch-ack timer, but
`build_started` is valid from DB state `dispatched` and only says it sets DB
state to `started`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:378`,
`:379`). Reconnect `worker_status(building)` has the same gap: when DB state is
`dispatched`, it may perform an implicit accept and move DB state to `started`,
but the design does not say that this also cancels the dispatch-ack timer
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:376`).

This matters in the current code because the ack timeout task removes the active
entry and writes the build back to `queued` when its cancellation token is not
cancelled (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:230`, `:256`, `:267`).
`handle_build_started` currently updates the DB by build ID only and does not
cancel that token (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:305`). If the future
implementation keeps the design's current side-effect list, an assigned worker
can legitimately start or resume a dispatched build and later have the ack
timeout requeue that already-started build, enabling duplicate execution or
stale active-state cleanup.

Required design change: state that any accepted owned message proving delivery
or execution start cancels the dispatch-ack timer. That includes `build_started`
from `dispatched` and reconnect `worker_status(building)` when it performs
implicit accept/start. Add tests for both paths.

### Medium: stray `BuildRevoke` needs a distinct no-state-mutation send path

Draft v6 says an invalid `worker_status(building)` sends
`UnauthorizedBuildAction` followed by `BuildRevoke { build_id }`, and that the
server does not move active ownership
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:132`,
`:137`, `:406`, `:412`). That is the right policy, but the design does not
explicitly distinguish this stray-local-build stop command from the normal
revoke operation.

The existing server helper named `send_build_revoke` looks up the active owner
from `queue.active`, sets the build to `revoking`, and sends the revoke to the
assigned connection (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:477`, `:490`,
`:507`). Reusing that helper for a wrong-worker reconnect would revoke the real
assigned build instead of only telling the reporting worker to stop its stray
local process. For unknown, queued, inactive, or terminal reports, that helper
also cannot target the reporter because there may be no active owner.

Required design change: define a separate reporter-directed revoke send path for
invalid `worker_status(building)` claims. It must send only to the reporting
connection, must not change DB state to `revoking`, must not cancel the real
assignment's timers, and must not remove or rewrite `queue.active`.

### Medium: queued rollback column resets are only specified for pre-delivery failures

D4 defines a dedicated rollback operation with exact column resets, but only for
post-DB, pre-delivery dispatch failures
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:216`,
`:242`). The transition matrix separately says transient `build_rejected` rolls
back to `queued` and removes active state
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:384`),
but does not say whether it uses the same DB reset list.

The current code has multiple requeue paths that only write `state = queued`:
transient rejection (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:452`) and dispatch
ack timeout (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:267`). Since dispatch
persisted `worker_id` and `trace_id` earlier
(`cbsd-rs/cbsd-server/src/db/builds.rs:262`), any queued retry path that omits
the reset leaves stale assignment provenance visible until the next successful
dispatch overwrites it.

Required design change: state whether all assigned-build requeue paths reuse the
dedicated rollback DB operation, or explicitly justify which queued states may
retain prior `worker_id`/`trace_id`. The safer contract is to clear the same
assignment/provenance columns whenever a dispatched assignment is returned to
`queued`.

### Low: coarse unauthorized reason values are not mapped

The design intentionally keeps worker-facing unauthorized reasons coarse and
adds two values, `NotAssigned` and `InvalidAssignment`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:185`,
`:189`). It also says the worker protocol must not let a registered worker
distinguish unknown build IDs, inactive builds, builds owned by another worker,
wrong connections, wrong worker identities, or invalid internal DB state
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:197`,
`:200`).

Those statements are compatible only if the design defines exactly when each
coarse value is used. Without that mapping, implementers can accidentally turn
the two-value enum back into a smaller assignment-state oracle.

Required design change: define the mapping. For example, all authorization
failures for a syntactically valid build-scoped message could return
`NotAssigned`, while `InvalidAssignment` is reserved for malformed or internally
inconsistent cases that do not reveal build existence or ownership.

## Prior Review Findings

The v4 review findings were checked against Draft v6:

| Prior finding                                     | Status           | Notes                                                                                         |
| ------------------------------------------------- | ---------------- | --------------------------------------------------------------------------------------------- |
| `worker_status(building)` response conflict       | Addressed        | Draft v6 consistently requires `UnauthorizedBuildAction` followed by `BuildRevoke`.           |
| `worker_status(idle)` build-id authorization text | Addressed        | D1 now separates idle reconciliation from build-scoped checks and defines success/no-op idle. |
| Idle reconnect lifecycle gap                      | Addressed        | Reconciliation is limited to the authenticated worker's own persisted assignment.             |
| Worker-facing unauthorized oracle                 | Mostly addressed | Reasons are coarse, but the two coarse enum values still need a non-leaking mapping.          |
| Reconnect ownership gap                           | Addressed        | Building reconnect requires registered worker ID and active assignment validation.            |
| `cbc` log-tail contract                           | Addressed        | `cbc` is in scope, defaults tail to 50, and no longer requires exact `total_lines`.           |
| UTF-8/full-line tail truncation                   | Addressed        | D7 requires full lines and valid UTF-8 only.                                                  |
| Rollback DB columns for pre-delivery failures     | Addressed        | D4 lists exact defensive reset columns.                                                       |
| Descriptor validation centralization              | Addressed        | D5 requires one typed validator for REST, periodic, and scheduler paths.                      |
| Bounded tail algorithm                            | Addressed        | D7 selects reverse block scanning, 1,000 lines, and a 4 MiB byte budget.                      |
| Enum-based unauthorized action/reason             | Addressed        | D3 uses protocol enums.                                                                       |
| Protocol version decision                         | Addressed        | D3 keeps protocol version 2 as an in-flight pre-production correction.                        |

## Deferred Or Ambiguous Items

- Dispatch-ack timer cancellation is deferred for `build_started` and implicit
  start/resume through `worker_status(building)`.
- Invalid reconnect `BuildRevoke` is not explicitly specified as a
  reporter-directed message with no DB or active-assignment side effects.
- Requeue-to-queued paths after assignment are not tied to the rollback column
  reset operation.
- Worker-facing `UnauthorizedBuildReason` values are coarse, but their mapping
  is still unspecified.

## Commit Boundary Notes

No implementation commits exist. The design 019 document and its design reviews
form one coherent documentation thread. They should remain separate from the
broad unrelated Markdown churn already present in the working tree. The security
review can be committed immediately before design 019 or in the same
documentation-only series because it is the evidence source for this design.

## Top Findings

1. High: valid start/resume paths can leave the dispatch ack timer armed,
   allowing an already-started build to be requeued later.
2. Medium: invalid reconnect revoke must be a direct message to the reporting
   worker, not the existing state-mutating revoke path for the assigned worker.
3. Medium: rollback column resets are specified for pre-delivery failure, but
   not for other assigned-build requeue paths.
4. Low: the two coarse unauthorized reason values need a non-leaking mapping.

## Confidence Score

| Item                                                    | Points | Description                                                                                                |
| ------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------- |
| Starting score                                          | 100    |                                                                                                            |
| D7: ack timer can requeue a started build               | -20    | `build_started` and implicit reconnect start do not specify dispatch-ack timer cancellation.               |
| D7: stray revoke path can mutate the real assignment    | -20    | Design does not forbid reusing the existing state-mutating `send_build_revoke` helper.                     |
| D1: queued rollback reset deferred for assigned retries | -20    | Transient rejection and ack-timeout requeue semantics do not state whether assignment columns are cleared. |
| D11: unauthorized reason mapping undocumented           | -5     | Coarse reason values exist, but the design does not define a non-leaking mapping.                          |
| **Total**                                               | **35** |                                                                                                            |

Interpretation: 35/100. The design resolves the previous review blockers, but
the remaining gaps are in security-sensitive control-plane transitions and
should be fixed before planning.

## Go / No-Go

No-go for implementation planning as-is.

Required before planning:

1. Define ack-timer cancellation for all valid receipt/start paths.
2. Define a reporter-directed stray revoke send path with no DB or active-state
   mutation.
3. Decide whether all assigned-build requeue paths clear persisted assignment
   columns.
4. Map `UnauthorizedBuildReason` values so the worker-facing protocol remains
   non-enumerating.
