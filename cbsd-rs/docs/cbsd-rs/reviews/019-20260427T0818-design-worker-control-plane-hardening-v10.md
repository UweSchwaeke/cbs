# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v11                                                                    |
| Date           | 2026-04-27 08:18 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior review/security-review documents                 |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until remaining reconnect/liveness edge cases are specified                 |

## Summary

Draft v11 addresses the explicit v9 review findings in the mainline cases:

- worker active-build state is now moved out of the websocket connection loop
- unauthorized `worker_status(building)`, `build_started`, and `build_output`
  now receive a reporter-directed `BuildRevoke`
- startup recovery is explicitly unchanged and does not reconstruct in-memory
  receipt state

The design is still not ready for implementation planning. The remaining gaps
are narrower than v9, but they are still on the worker control-plane boundary:
the accepted-but-not-started local phase is not included in reconnect truth
rules, liveness expiry for receipt-acknowledged `dispatched` assignments is not
specified, and superseded live same-worker connections can lose their outbound
sender without a concrete stop-work command.

Implementation-only phase-review items are N/A because no implementation plan or
implementation commits are in scope.

## Findings

### High: accepted-but-not-started worker state can be reported idle

V11 adds a process-level worker supervisor and lists `accepted` as a local
execution phase
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:493`).
However, the reconnect status rule sends `Building` only when the supervisor has
an active executor, an in-progress revoke, or a pending terminal result
(`:501`). It sends `Idle` when those three things are absent (`:506`).

That omits the explicitly modeled `accepted` phase. The current worker has a
real gap between `build_accepted` and subprocess spawn: `BuildAccepted` is sent
before `spawn_build` (`cbsd-rs/cbsd-worker/src/ws/handler.rs:284`, `:291`). If
the websocket drops after acceptance but before a process handle exists, a
literal implementation of v11 can report idle even though the worker still has
local assignment state and a component working directory. The server then
applies the `dispatched` idle rule and may roll back the build to `queued`
(`:712`), enabling redispatch while the original worker may still proceed unless
the implementation invents an unstated cleanup rule.

Required design change: make reconnect status depend on any non-terminal local
assignment phase, not only active executors, revokes, and pending terminal
results. Either `accepted` must report `WorkerStatus(building)` and be accepted
as proof of receipt, or the worker must kill/cleanup the accepted assignment
before it is allowed to report idle. Add explicit tests for disconnect after
`build_accepted` but before `build_started`.

### High: dead-worker handling for receipt-acknowledged `dispatched` builds is unspecified

D4 introduces `ActiveAssignmentReceipt` because `build_accepted` cancels the
dispatch-ack timer while the DB can remain `dispatched`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:325`,
`:338`). The idle reconnect table splits `dispatched` handling by
`AwaitingReceipt` versus `ReceivedByWorker` (`:712`). The design does not define
the liveness monitor or grace-expiry decision for the same two receipt states.
It only says unrelated idle reconnects leave other workers to the liveness
monitor (`:736`) and lists rollback cleanup for dispatch-ack timeout, transient
reject, and same-worker idle rollback (`:308`), not liveness expiry.

This matters because the current code already has an independent dead-worker
resolver: the grace-period task calls `handle_worker_dead`
(`cbsd-rs/cbsd-server/src/ws/liveness.rs:145`), and that resolver currently
handles active `dispatched` rows separately from idle reconnect
(`cbsd-rs/cbsd-server/src/ws/handler.rs:779`). V11 gives implementers no table
for whether `dispatched + AwaitingReceipt` and `dispatched + ReceivedByWorker`
are requeued, failed, revoked, or handled differently when the worker never
reconnects. Requeuing `ReceivedByWorker` after grace expiry can create duplicate
execution; failing `AwaitingReceipt` instead of requeueing can drop work.

Required design change: add a liveness/dead-worker resolution table that
includes `dispatched + AwaitingReceipt`, `dispatched + ReceivedByWorker`,
`started`, and `revoking`, including DB state, active-entry cleanup, watcher
cleanup, queued rollback column cleanup, and whether any terminal log
finalization occurs. Add tests for grace expiry after `build_new` delivery,
after `build_accepted`, after `build_started`, and while `revoking`.

### High: superseded live connections have no concrete stop-work path

Same-worker migration declares the newest authenticated connection the winner
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:585`)
and says the old connection's outbound sender is removed (`:615`). If the new
connection reports idle, the server may roll back `dispatched`, mark `started`
as `failure`, or mark `revoking` as `revoked` while only logging that a live
same-worker connection was superseded (`:712`, `:720`, `:726`).

That leaves the old live connection's local subprocess ambiguous. Removing the
old sender matches the current architecture's control path
(`cbsd-rs/cbsd-server/src/ws/handler.rs:313`), but it also removes the obvious
way to send a reporter-directed `BuildRevoke` to the old connection. V11 says
later messages from the superseded connection fail authorization (`:615`), and
unauthorized execution messages should get revoke (`:555`), but it does not say
how that revoke is delivered after the old sender is removed or whether the
server proactively sends a stop command before removal.

Required design change: define the old-live-connection shutdown protocol during
same-worker migration. Options include sending `BuildRevoke` for migrated active
work before removing the sender, closing the old websocket with a reason that
the worker treats as stop-and-cleanup, or keeping a per-connection response path
for later unauthorized old-connection messages. The design must state which path
is used and require tests where a live superseded connection still has an
accepted/started subprocess.

## Prior Review Coverage

The v9 review findings were checked against Draft v11:

| v9 finding                                | Status              | Notes                                                                                                                                             |
| ----------------------------------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worker reconnect state missing            | Partially addressed | V11 adds a process-level supervisor and spool, but the `accepted` phase is not included in the reconnect `Building`/`Idle` decision.              |
| Unauthorized execution lacks stop command | Addressed           | V11 sends `UnauthorizedBuildAction` plus reporter-directed `BuildRevoke` for unauthorized `worker_status(building)`, `build_started`, and output. |
| Startup recovery receipt-state conflict   | Addressed           | D4 now says startup recovery is unchanged and fails/revokes in-flight rows rather than reconstructing receipt state.                              |
| Missing worker-side reconnect tests       | Partially addressed | Tests are listed for executor/revoke/pending terminal reconnect, but not accepted-before-start reconnect.                                         |
| Missing stale-execution stop tests        | Addressed           | V11 lists unauthorized `build_started`/`build_output` stop-work tests and reporter-directed no-side-effect tests.                                 |

Earlier review findings carried forward into v9 were also checked:

| Earlier finding                                           | Status             | Notes                                                                                                                          |
| --------------------------------------------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| Dispatch rollback column resets                           | Addressed          | D4 lists the reset columns and requires a dedicated rollback DB operation.                                                     |
| Meaningful-state authorization matrix                     | Mostly addressed   | The matrix is concrete for message handlers, but liveness expiry for receipt states remains outside the matrix.                |
| Descriptor validation centralization                      | Addressed          | D5 requires one typed validator for REST, periodic create/update, and scheduler trigger paths.                                 |
| Bounded log tailing contract                              | Addressed          | D7 defines reverse block scanning, a 1,000-line cap, a 4 MiB budget, full-line UTF-8 output, and no exact `total_lines`.       |
| Enum-based unauthorized action/reason                     | Addressed          | `WorkerBuildAction` and coarse `NotAssigned` are specified.                                                                    |
| Protocol version decision                                 | Addressed          | D3 keeps protocol version 2 as a pre-production correction.                                                                    |
| Reconnect ownership gap                                   | Mostly addressed   | DB-backed migration and idle ownership are specified, but old-live-connection stop behavior is still ambiguous.                |
| `cbc` log-tail contract                                   | Addressed          | `cbc` is in package scope, default tail count is 50, and exact `total_lines` is no longer required.                            |
| UTF-8/full-line truncation                                | Addressed          | D7 defines both behaviors.                                                                                                     |
| `worker_status(building)` unauthorized response conflict  | Addressed          | Invalid building reports receive `UnauthorizedBuildAction` followed by reporter-directed `BuildRevoke`.                        |
| `worker_status(idle)` listed under build-id authorization | Addressed          | D1 separates idle reconciliation from build-scoped authorization.                                                              |
| Dispatch-ack timer cancellation for start/resume paths    | Addressed          | D4 cancels timers for all valid owned receipt/execution messages.                                                              |
| Reporter-directed stray revoke side effects               | Addressed on paper | D2 forbids DB, timer, watcher, active-queue, and assigned-worker side effects; delivery to superseded live connections is not. |
| Queued rollback cleanup only for pre-delivery failures    | Addressed          | D4 applies cleanup to assigned-build requeue paths, though liveness expiry still needs explicit state handling.                |
| Coarse unauthorized reason mapping                        | Addressed          | All syntactically valid authorization failures map to `NotAssigned`; details stay server-only.                                 |

The original security review findings are represented:

| Security review finding                   | Status in design                 | Notes                                                                                                               |
| ----------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Cross-worker lifecycle spoofing           | Mostly addressed                 | Active ownership checks are specified; remaining risk is stale execution on accepted/liveness/live-supersede edges. |
| Cross-worker log output spoofing          | Addressed                        | D6 applies active-build ownership to output and forbids arbitrary log creation.                                     |
| Empty component lists can strand dispatch | Addressed                        | D5 rejects empty descriptors and keeps dispatch rollback as an invariant guard.                                     |
| Post-DB dispatch failures lack rollback   | Addressed for pre-delivery paths | D4 defines pre-delivery rollback and queued rollback cleanup.                                                       |
| JSON tail reads full log into memory      | Addressed                        | D7 defines bounded reverse tailing.                                                                                 |

## Deferred Or Ambiguous Items

No item is explicitly deferred by the design; it says open questions are
resolved. Remaining ambiguous or missing items:

- reconnect behavior for the worker's local `accepted` phase
- liveness/grace-expiry resolution for `dispatched + AwaitingReceipt` and
  `dispatched + ReceivedByWorker`
- stop-work delivery to a superseded live same-worker connection after migration
- tests for accepted-phase reconnect, receipt-state liveness expiry, and live
  superseded connections

## Phase-Review N/A Items

No implementation plan or implementation commits are in scope. Therefore these
phase-review checks are N/A:

- Plan item implementation status.
- Commit-by-commit build/test hygiene.
- Duplicated implementation code introduced by the phase.
- Completed implementation test coverage.

## Confidence Score

Design-phase scoring starts at 100. Implementation-only criteria with no plan or
code in scope are N/A unless the design itself creates the defect.

| Item                                           | Points | Description                                                                                                                     |
| ---------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------- |
| Starting score                                 | 100    |                                                                                                                                 |
| D7: accepted phase can report idle             | -20    | `accepted` is tracked as a local phase but omitted from reconnect `Building`/`Idle` rules, risking redispatch of accepted work. |
| D7: receipt-state liveness expiry unspecified  | -20    | The design splits idle reconnect by receipt state but gives no dead-worker/liveness table for acknowledged `dispatched` builds. |
| D7: superseded live connection lacks stop path | -20    | Same-worker migration removes the old sender while leaving no concrete way to stop an old live subprocess.                      |
| D11: missing accepted-phase reconnect tests    | -5     | Test expectations do not cover disconnect after `build_accepted` before `build_started`.                                        |
| D11: missing receipt-state liveness tests      | -5     | Test expectations do not cover grace expiry for `AwaitingReceipt` versus `ReceivedByWorker` dispatched assignments.             |
| D11: missing superseded-live stop tests        | -5     | Test expectations do not prove that a live superseded connection with local work is stopped or cannot continue stale execution. |
| **Total**                                      | **25** |                                                                                                                                 |

Interpretation: 25/100. Draft v11 fixes the explicit v9 mainline gaps, but the
remaining reconnect and liveness holes are still security-relevant enough to
block implementation planning.

## Go / No-Go

No-go for implementation planning as-is.

Required before planning:

1. Define worker reconnect behavior for `accepted` local assignments, including
   whether they report `Building` or are cleaned up before `Idle`.
2. Add a dead-worker/liveness resolution table for receipt-aware active
   assignments.
3. Define how superseded live same-worker connections receive a stop-work signal
   or are otherwise prevented from continuing stale local execution.
4. Add tests for the three edge cases above.

## Final Summary

Top findings ordered by severity:

1. High: the worker's `accepted` phase is modeled but omitted from reconnect
   status rules, allowing accepted local state to be treated as idle.
2. High: the design has no liveness/dead-worker policy for
   `dispatched + ReceivedByWorker`, so acknowledged work can be requeued,
   failed, or orphaned depending on implementation choice.
3. High: same-worker migration removes the old live connection sender without a
   concrete stop-work path for an old subprocess.

Confidence score: 25/100, with deductions shown above. Recommendation: no-go
until these reconnect and liveness edge cases are made concrete.
