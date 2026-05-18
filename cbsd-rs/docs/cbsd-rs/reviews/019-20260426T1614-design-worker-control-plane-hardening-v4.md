# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v4                                                                     |
| Date           | 2026-04-26 16:14 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior security review docs                             |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until the two remaining protocol/authorization ambiguities are resolved     |

## Summary

Draft v5 addresses the two v3 review blockers:

- `worker_status(idle)` reconciliation is now explicitly limited to builds whose
  persisted `builds.worker_id` matches the authenticated registered worker ID.
- Worker-facing unauthorized reasons are now coarse, while detailed causes are
  retained only in server logs.

The design now covers the original security-review findings and most of the
implementation-critical details: lifecycle spoofing, arbitrary log writes, empty
descriptors, rollback columns, reconnect ownership, bounded tailing,
`cbc logs tail`, enum-based unauthorized actions, and protocol version 2. It is
close, but not quite ready for an implementation plan because two protocol
branches still conflict with each other.

## Findings

### Medium: `worker_status(building)` has conflicting unauthorized responses

D2 says unauthorized build messages receive a non-fatal response and then adds a
special reconnect behavior only when the reporting worker claims a build
assigned to another worker: send `BuildRevoke { build_id }` to the reporter
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:117`,
`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:122`).
D3 then introduces `UnauthorizedBuildAction` as the non-fatal authorization
error shape
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:151`).
The transition-matrix preamble says unauthorized messages for unknown, inactive,
terminal, or differently owned builds receive `UnauthorizedBuildAction`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:356`),
but the `worker_status(building)` row says unauthorized behavior is to send
`BuildRevoke` and not rewrite `queue.active`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:362`).

Those paths are not equivalent. A worker reporting `Building { build_id }` for
an unknown, queued, inactive, terminal, or differently owned build is claiming
local execution. If the server sends only `UnauthorizedBuildAction`, the worker
gets diagnostics but not the existing stop-work command. The current reconnect
handler already sends `BuildRevoke` for queued, terminal, and not-found building
claims (`cbsd-rs/cbsd-server/src/ws/handler.rs:650`,
`cbsd-rs/cbsd-server/src/ws/handler.rs:685`,
`cbsd-rs/cbsd-server/src/ws/handler.rs:696`), so the design should make the new
contract explicit instead of leaving implementers to choose between the D3
generic unauthorized path and the matrix row.

Required design change: define exactly which `worker_status(building)` failures
send `BuildRevoke`, which send `UnauthorizedBuildAction`, and whether any case
sends both. The tests should assert the selected behavior for at least
differently owned, unknown/not-found, queued, and terminal reported build IDs.

### Low: `worker_status(idle)` is listed under a build-id authorization rule

D1 defines the worker build-message authorization rule in terms of "the
message's `build_id`" and an active entry whose `connection_id` equals the
sending websocket
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:81`).
It then says the rule applies to `worker_status` when it reports `Idle`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:91`).
But `WorkerMessage::WorkerStatus { state: Idle, build_id: None }` has no
referenced build ID in the current protocol (`cbsd-rs/cbsd-proto/src/ws.rs:77`),
and the later idle-reconnect design correctly describes it as reconciliation
over the authenticated worker's own persisted assignments rather than
authorization for a single message build ID
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:417`).

The later section is the right policy and addresses the v3 finding, but D1's
generic rule still points future implementers at an impossible check and an
`UnauthorizedBuildAction` shape that requires a `build_id`. This is a small but
real planning hazard because idle status should normally be success/no-op when
there is no owned active assignment, not an unauthorized build action with no
build to report.

Required design change: split D1 into two checks: build-scoped message
authorization for messages with a `build_id`, and idle-status reconciliation for
the authenticated worker's own persisted assignments. State explicitly that an
idle status from a worker with no owned active assignment is success/no-op and
does not emit `UnauthorizedBuildAction`.

## Prior Review Findings

The v3 review findings were checked against Draft v5:

| Prior finding                         | Status    | Notes                                                                                     |
| ------------------------------------- | --------- | ----------------------------------------------------------------------------------------- |
| Idle reconnect lifecycle gap          | Addressed | The design limits idle reconciliation to the authenticated worker's persisted assignment. |
| Worker-facing unauthorized oracle     | Addressed | Worker reasons are now coarse; detailed internal reasons are server-log-only.             |
| Reconnect `worker_status(building)`   | Addressed | Registered worker ID and persisted assignment checks are required.                        |
| Reconnect mismatch revoke behavior    | Addressed | Differently owned building reports send `BuildRevoke` to the reporting worker.            |
| `cbc` log-tail contract               | Addressed | `cbc` is in scope, defaults tail to 50, and stops requiring exact `total_lines`.          |
| UTF-8/full-line tail truncation       | Addressed | D7 requires full lines and valid UTF-8 only.                                              |
| Rollback DB columns                   | Addressed | D4 lists the exact defensive column reset set.                                            |
| Descriptor validation centralization  | Addressed | D5 requires one typed validator for REST, periodic, and scheduler paths.                  |
| Bounded tail algorithm                | Addressed | D7 selects reverse block scanning, 1,000 lines, and a 4 MiB byte budget.                  |
| Enum-based unauthorized action/reason | Addressed | D3 uses protocol enums for action and coarse reason.                                      |
| Protocol version decision             | Addressed | D3 keeps protocol version 2 as an in-flight pre-production correction.                    |

## Deferred Or Ambiguous Items

- `worker_status(building)` does not have a single unambiguous unauthorized
  response contract across D2, D3, and the transition matrix.
- `worker_status(idle)` is correctly constrained later in the document, but D1
  still describes it as if it had a message-local `build_id`.
- No implementation plan exists yet; commit boundary review is limited to the
  current uncommitted design/review/security-review documentation.

## Commit Boundary Notes

No implementation commits exist. The design 019 document and four design reviews
form one coherent design-review thread. They should remain separate from the
broad unrelated Markdown churn already present in the working tree. The security
review can be committed immediately before design 019 or in the same
documentation-only series because it is the evidence source for this design.

## Top Findings

1. Medium: `worker_status(building)` has conflicting unauthorized response
   semantics, leaving stale local execution behavior under-specified.
2. Low: `worker_status(idle)` is covered by the right later policy, but D1 still
   frames it as a build-id scoped authorization check.

## Confidence Score

| Item                                                          | Points | Description                                                                                   |
| ------------------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------- |
| Starting score                                                | 100    |                                                                                               |
| D1: building reconnect response contract remains ambiguous    | -20    | The design leaves a required authorization/revoke behavior to be chosen during planning.      |
| D11: idle status authorization wording is internally confused | -5     | D1 applies a `build_id`-based response model to an idle message that has no referenced build. |
| **Total**                                                     | **75** |                                                                                               |

Interpretation: 75/100. Draft v5 is substantially stronger and most decisions
are implementation-ready, but the remaining ambiguity is in the worker control
protocol itself and should be fixed before a plan is written.

## Go / No-Go

No-go for implementation planning until the two clarifications above are made.
After that, the design should be ready to support a phased implementation plan.
