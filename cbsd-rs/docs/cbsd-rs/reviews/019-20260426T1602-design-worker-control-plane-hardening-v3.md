# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v3                                                                     |
| Date           | 2026-04-26 16:02 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior security review docs                             |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until the idle reconnect lifecycle gap is resolved                          |

## Summary

Draft v4 addresses the previous v2 review findings:

- reconnect `worker_status(building)` is now in the ownership model and matrix
- mismatched reconnect handling is warning plus `BuildRevoke` to the reporter
- `cbc` is included in the affected package set and tail-client contract
- log tailing has a 1,000-line cap, 4 MiB scan budget, full-line output, and a
  valid UTF-8 boundary rule
- unauthorized action and reason fields are enum-based
- protocol version 2 is explicitly retained

The design is much closer to implementation-ready, but it still leaves
`worker_status(idle)` outside the authorization model even though the current
server can use idle reconnect status to requeue or fail other workers' active
builds. It also sends detailed unauthorized reason codes to workers without
deciding whether a registered worker should receive a build-existence and
assignment-state oracle.

## Findings

### High: `worker_status(idle)` remains an unowned lifecycle mutation path

D1 covers `worker_status` only when it reports `Building { build_id }`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:84`).
The reconnect section and tests repeat that same building-only scope
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:371`,
`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:476`).

That is not enough for the current code. The worker sends `WorkerStatus::Idle`
after connect (`cbsd-rs/cbsd-worker/src/ws/handler.rs:145`), and the server's
idle branch scans every active build whose connection is not the reporting
connection and whose owner is disconnected or dead
(`cbsd-rs/cbsd-server/src/ws/handler.rs:716`). It then requeues `dispatched`
builds and fails `started` builds (`cbsd-rs/cbsd-server/src/ws/handler.rs:738`).
This is a build lifecycle side effect triggered by a worker-originated message,
but the design does not say that the effects must be limited to the reporting
worker's own persisted assignment.

The connection migration path makes this sharper. On reconnect, the server
migrates active build references from an old connection to the new connection
before processing status (`cbsd-rs/cbsd-server/src/ws/handler.rs:263`). If the
same registered worker reports idle after losing local state, the active build
may already point at the new connection, so the current idle scan skips it. If a
different worker reports idle while another worker is disconnected, the current
scan can act on that other worker's build before the grace-period monitor owns
the decision.

Required design change: define `worker_status(idle)` semantics explicitly. The
future plan needs to say whether idle status is allowed to mutate build state at
all. If it is, it must only reconcile builds whose persisted `builds.worker_id`
matches the authenticated registered worker ID, and it must respect the liveness
grace-period policy. The tests should include idle reconnect for the same
assigned worker after local state loss and idle reconnect from an unrelated
worker while another worker has an active disconnected build.

### Medium: wire-level unauthorized reasons create an assignment-state oracle

D2 says the worker response must not disclose another worker's identity or
internal connection ID
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:111`).
D3 then puts `UnknownBuild`, `InactiveBuild`, `BuildOwnedByAnotherConnection`,
`WorkerIdentityMismatch`, and `InvalidBuildState` into the server-to-worker wire
enum
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:168`),
and Observability requires the worker to log `reason`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:457`).

This does not disclose the other worker's name or connection ID, but it does let
any registered worker probe arbitrary build IDs and distinguish "does not
exist", "exists but inactive", "exists and assigned elsewhere", and "assigned to
this registered worker but not this connection". The design's threat model
explicitly includes malicious or stale registered workers; after this hardening
they should not be able to mutate other builds, but the design should decide
whether this read-side oracle is acceptable.

Required design change: either explicitly accept this disclosure as an
operator-diagnostics tradeoff, or split server-only diagnostic reasons from a
coarser worker-facing reason such as `not_assigned` or `invalid_assignment`. The
enum-based protocol decision can remain; the missing part is the confidentiality
policy for the enum values.

## Prior Review Findings

The previous v2 review findings were checked against Draft v4:

| Prior finding                         | Status    | Notes                                                                                        |
| ------------------------------------- | --------- | -------------------------------------------------------------------------------------------- |
| Reconnect ownership gap               | Addressed | `worker_status(building)` now requires registered worker ID and persisted assignment checks. |
| Reconnect mismatch behavior           | Addressed | The design sends `BuildRevoke` to the reporting worker and does not move active ownership.   |
| `cbc` log-tail contract missing       | Addressed | `cbc` is listed and must stop requiring `total_lines`; default changes to 50.                |
| UTF-8 truncation rule missing         | Addressed | D7 requires valid UTF-8 and dropping partial leading code points.                            |
| Rollback DB columns unspecified       | Addressed | D4 lists the exact cleared columns and requires a dedicated rollback operation.              |
| Descriptor validation not centralized | Addressed | D5 requires one typed validator across REST, periodic, and scheduler trigger paths.          |
| Bounded tail algorithm unspecified    | Addressed | D7 selects reverse block scanning with `MAX_TAIL_LINES` and `MAX_TAIL_BYTES`.                |
| Stringly unauthorized action/reason   | Addressed | D3 uses `WorkerBuildAction` and `UnauthorizedBuildReason` enums.                             |
| Protocol version decision             | Addressed | D3 keeps protocol version 2 as a pre-production correction.                                  |

## Deferred Or Ambiguous Items

- `worker_status(idle)` is not covered, despite current code using it to requeue
  or fail active builds.
- The design does not state whether detailed unauthorized reason enums are safe
  to expose to registered workers, or should be server-log-only diagnostics.
- No implementation plan exists yet; commit boundary review is limited to the
  current uncommitted design/review/security-review documents.

## Commit Boundary Notes

No implementation commits exist. The design 019 document and its three design
reviews form a coherent documentation thread. They should be kept separate from
the broad unrelated Markdown churn already present in the working tree. If the
security review remains uncommitted, it can be committed either immediately
before the design or in the same documentation-only series because it is the
evidence source for design 019.

## Top Findings

1. High: `worker_status(idle)` can still be a worker-originated build lifecycle
   mutation path unless the design constrains it to the authenticated worker's
   own assignment or removes its build side effects.
2. Medium: detailed wire-level unauthorized reasons may disclose build existence
   and assignment state to any registered worker; the design needs an explicit
   confidentiality decision.

## Confidence Score

| Item                                         | Points | Description                                                                                         |
| -------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------- |
| Starting score                               | 100    |                                                                                                     |
| D7: idle reconnect lifecycle gap             | -20    | `worker_status(idle)` can currently requeue/fail builds without an ownership rule in the design.    |
| D1: idle status reconciliation deferred      | -20    | The design claims all policy choices are resolved but omits a current lifecycle status side effect. |
| D7: worker-facing unauthorized reason oracle | -20    | Detailed reason enums can reveal build existence and assignment state to registered workers.        |
| D11: reason-disclosure policy not documented | -5     | The design does not justify whether detailed worker-facing unauthorized reasons are acceptable.     |
| **Total**                                    | **35** |                                                                                                     |

Interpretation: 35/100. Draft v4 resolves the previous review's explicit
blockers, but the remaining idle-status gap is security-significant enough to
block implementation planning.

## Go / No-Go

No-go for implementation planning as-is.

Required before planning:

1. Add explicit `worker_status(idle)` reconciliation semantics and tests.
2. Decide whether detailed unauthorized reason codes are safe on the wire or
   should be collapsed for workers and kept detailed only in server logs.
