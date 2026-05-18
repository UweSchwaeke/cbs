# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v9                                                                     |
| Date           | 2026-04-26 22:39 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior review/security-review documents                 |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until worker-side reconnect state and stale-execution stop rules are added  |

## Summary

Draft v10 addresses the explicit v8 review findings on paper:

- acknowledged-but-not-started `dispatched` assignments now carry an
  `ActiveAssignmentReceipt` state and idle reconnect resolves `ReceivedByWorker`
  assignments to `queued`
- idle reconciliation candidate discovery is defined as a queue snapshot plus DB
  row filtering outside the queue lock
- same-worker migration now requires both persisted `worker_id` and active DB
  state freshness before migrating an active entry

The design is still not ready for implementation planning. The new reconnect
state machine relies on the worker reporting true local build state after a
websocket reconnect, but the design does not specify the worker-side state
ownership needed to make that true. The current worker keeps active build state
inside one websocket loop, has a TODO where `WorkerStatus(building)` would be
sent, always reports idle, and does not visibly kill the child process on
connection-loop drop. That makes the server's authoritative idle handling unsafe
to implement without additional worker design.

Implementation-only phase-review items are N/A because no implementation plan or
implementation commits are in scope.

## Findings

### High: worker reconnect state is not designed, but server policy depends on it

Draft v10 treats same-worker idle status as authoritative local-state loss. It
rolls `dispatched` + `ReceivedByWorker` back to `queued` even after superseding
a live same-worker connection, marks owned `started` assignments `failure`, and
marks owned `revoking` assignments `revoked`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:619`,
`:627`, `:633`). Those choices are only safe if the worker can truthfully report
whether it still has a local build after a websocket reconnect, or if it
guarantees that any unreported local build has already been stopped.

The current worker does not have that shape. It stores `active_build` as a local
variable inside `run_connection` (`cbsd-rs/cbsd-worker/src/ws/handler.rs:153`)
and always sends `WorkerStatus { state: Idle, build_id: None }` after `Welcome`
(`cbsd-rs/cbsd-worker/src/ws/handler.rs:135`). The code comment explicitly says
the `Building` report is still a TODO
(`cbsd-rs/cbsd-worker/src/ws/handler.rs:136`). The build executor is only killed
on an explicit `BuildRevoke` or worker shutdown notification
(`cbsd-rs/cbsd-worker/src/ws/handler.rs:389`, `:459`); `BuildExecutor` has
`kill()`, but no `Drop` or `kill_on_drop` contract in
`cbsd-rs/cbsd-worker/src/build/executor.rs`.

That means an implementation can follow the server-side v10 design and still
produce duplicate or zombie local execution:

1. the worker accepts or starts a build
2. the websocket loop ends unexpectedly
3. the worker reconnects and reports idle because active state was
   connection-local
4. the server requeues or fails the assignment according to v10
5. the old subprocess may continue locally without an authoritative server owner

Required design change: add an explicit worker-side reconnect state model. The
design needs to require one of these policies:

- active build ownership survives websocket reconnects outside the
  per-connection loop, allowing `WorkerStatus(building)` to be truthful; or
- the worker kills and awaits any active subprocess before it can reconnect and
  report idle.

The test expectations should cover worker connection loss during `dispatched`,
`started`, and `revoking`, including whether the worker reports building or
proves that the local process was stopped before an idle report.

### High: unauthorized execution messages do not stop stale local work

Draft v10 sends a reporter-directed `BuildRevoke` only for invalid
`worker_status(building)` claims
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:159`).
For other unauthorized build-scoped actions, including `build_started` and
`build_output`, the transition matrix says only "reply unauthorized" and no DB
or log mutation
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:478`,
`:479`).

That is safe for server state, but not for production execution. A late worker
message after dispatch-ack timeout is a concrete example: the server can requeue
the build, the stale worker can then send `build_started` or `build_output`, and
the design tells the server to send only `UnauthorizedBuildAction`. The worker
logs that response as an error and continues running
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:154`).
The same "local execution claim" rationale used for `worker_status(building)`
applies to `build_started` and `build_output`.

Current worker behavior reinforces the hazard: only `BuildRevoke` kills the
active subprocess during normal message handling
(`cbsd-rs/cbsd-worker/src/ws/handler.rs:389`). A non-fatal authorization error
alone is not a stop-work command.

Required design change: define stop-work behavior for unauthorized messages that
prove local execution is or was active. At minimum, stale or unauthorized
`build_started` and `build_output` should either receive a reporter-directed
`BuildRevoke` with no DB/active side effects, or the new
`UnauthorizedBuildAction` must explicitly instruct the worker to stop that local
build and the worker behavior must be specified and tested.

### Medium: receipt-state recovery conflicts with existing startup recovery

D4 adds in-memory `ActiveAssignmentReceipt` and says it is not persisted; queue
recovery must therefore treat any recovered `dispatched` active assignment
without in-memory receipt as `AwaitingReceipt`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:307`,
`:328`). This is not reconciled with the existing startup recovery contract.

The current implementation fails all `dispatched` and `started` builds on server
restart (`cbsd-rs/cbsd-server/src/queue/recovery.rs:25`, `:35`). The prior
architecture design says the same: startup recovery marks `dispatched` and
`started` as `failure` because no active worker connection exists
(`cbsd-rs/docs/cbsd-rs/design/002-20260313T1800-cbsd-rust-port-design.md:877`).
There is no persisted connection ID from which to reconstruct `queue.active`,
and v10's reconnect authorization still requires `queue.active[build_id]` to
exist before a `worker_status(building)` resume is valid
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:540`).

Required design change: state whether design 019 intentionally changes startup
recovery. If not, remove or narrow the "queue recovery" receipt-state sentence.
If yes, define how active assignments are reconstructed after process restart,
how `connection_id` is represented before a worker reconnects, and how this
coexists with the existing "fail in-flight builds on restart" design.

## Prior Review Coverage

The v8 review findings were checked against Draft v10:

| v8 finding                                                      | Status                  | Notes                                                                                                                                                               |
| --------------------------------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Acknowledged `dispatched` idle reconnect can orphan active work | Addressed with new risk | V10 adds `ActiveAssignmentReceipt` and resolves `ReceivedByWorker` idle reconnect to `queued`. This exposes the missing worker-side state contract described above. |
| Idle reconciliation candidate discovery is still implicit       | Addressed               | V10 snapshots `queue.active`, queries DB outside the queue lock, filters by persisted `worker_id` and active DB state, then reconciles still-active entries.        |
| Two-phase migration does not require DB state freshness         | Addressed               | V10 requires persisted `worker_id` and DB state to match `dispatched`, `started`, or `revoking` before migration, and removes stale active entries otherwise.       |

Earlier review findings carried forward into v8 were also checked:

| Earlier finding                                           | Status           | Notes                                                                                                                                                   |
| --------------------------------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dispatch rollback column resets                           | Addressed        | D4 lists the reset columns and requires a dedicated rollback DB operation.                                                                              |
| Meaningful-state authorization matrix                     | Mostly addressed | The matrix covers server-side state transitions, but stale local execution after unauthorized `build_started`/`build_output` lacks stop-work semantics. |
| Descriptor validation centralization                      | Addressed        | D5 requires one typed validator for REST, periodic create/update, and scheduler trigger paths.                                                          |
| Bounded log tailing contract                              | Addressed        | D7 selects reverse block scanning, a 1,000-line cap, a 4 MiB budget, full-line UTF-8 output, and no exact `total_lines`.                                |
| Enum-based unauthorized action/reason                     | Addressed        | `WorkerBuildAction` and coarse `NotAssigned` are specified.                                                                                             |
| Protocol version decision                                 | Addressed        | D3 keeps protocol version 2 as a pre-production correction.                                                                                             |
| Reconnect ownership gap                                   | Mostly addressed | Server-side ownership checks are now concrete. Worker-side reconnect state is still missing.                                                            |
| `cbc` log-tail contract                                   | Addressed        | `cbc` is in package scope, default tail count is 50, and exact `total_lines` is no longer required.                                                     |
| UTF-8/full-line truncation                                | Addressed        | D7 defines both behaviors.                                                                                                                              |
| `worker_status(building)` unauthorized response conflict  | Addressed        | Invalid building reports receive `UnauthorizedBuildAction` followed by reporter-directed `BuildRevoke`.                                                 |
| `worker_status(idle)` listed under build-id authorization | Addressed        | D1 separates idle reconciliation from build-scoped authorization.                                                                                       |
| Dispatch-ack timer cancellation for start/resume paths    | Addressed        | D4 cancels timers for all valid owned receipt/execution messages.                                                                                       |
| Reporter-directed stray revoke side effects               | Addressed        | D2 forbids DB, timer, watcher, active-queue, and assigned-worker side effects.                                                                          |
| Queued rollback cleanup only for pre-delivery failures    | Addressed        | D4 applies cleanup to all assigned-build requeue paths.                                                                                                 |
| Coarse unauthorized reason mapping                        | Addressed        | All syntactically valid authorization failures map to `NotAssigned`; details stay server-only.                                                          |

The original security review findings are represented:

| Security review finding                   | Status in design                       | Notes                                                                                                                                  |
| ----------------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Cross-worker lifecycle spoofing           | Addressed with residual execution risk | D1/D2 authorize build-scoped messages through active ownership. The remaining issue is stopping stale local execution after rejection. |
| Cross-worker log output spoofing          | Addressed                              | D6 applies active-build ownership to output and forbids arbitrary log creation.                                                        |
| Empty component lists can strand dispatch | Addressed                              | D5 rejects empty descriptors before queue/DB state and keeps dispatch rollback as an invariant guard.                                  |
| Post-DB dispatch failures lack rollback   | Addressed                              | D4 defines pre-delivery and assigned-requeue rollback cleanup.                                                                         |
| JSON tail reads full log into memory      | Addressed                              | D7 defines bounded reverse tailing.                                                                                                    |

## Deferred Or Ambiguous Items

No item is explicitly deferred by the design; it says open questions are
resolved. Remaining ambiguous or missing items:

- worker-side active-build ownership across websocket reconnects
- worker behavior when reporting idle after losing connection while a subprocess
  may still exist
- stop-work semantics for unauthorized `build_started` and `build_output`
- whether design 019 changes startup recovery for `dispatched` assignments

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

| Item                                          | Points | Description                                                                                                                                                                                            |
| --------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Starting score                                | 100    |                                                                                                                                                                                                        |
| D7: worker reconnect state missing            | -20    | Server idle-reconnect policy depends on truthful worker local-state reporting, but the design does not specify how worker active build state survives reconnect or is stopped before idle.             |
| D7: unauthorized execution lacks stop command | -20    | Unauthorized `build_started` and `build_output` get only `UnauthorizedBuildAction`, leaving stale local execution running after timeout/requeue races.                                                 |
| D11: startup recovery receipt-state conflict  | -5     | D4 says queue recovery treats recovered `dispatched` assignments as `AwaitingReceipt`, conflicting with existing startup recovery that fails in-flight builds and has no active connection to recover. |
| D11: missing worker-side reconnect tests      | -5     | Test expectations do not require worker reconnect tests proving active state is retained or subprocesses are killed before idle reports.                                                               |
| D11: missing stale-execution stop tests       | -5     | Test expectations do not cover unauthorized `build_started`/`build_output` after timeout or reassignment and whether the worker stops local execution.                                                 |
| **Total**                                     | **45** |                                                                                                                                                                                                        |

Interpretation: 45/100. Draft v10 resolves the specific v8 server-side gaps, but
it still leaves worker-side reconnect and stale-execution behavior
underdesigned. Those are central to this control-plane hardening work.

## Go / No-Go

No-go for implementation planning as-is.

Required before planning:

1. Define the worker-side active-build state model across websocket reconnects,
   including whether state survives reconnect or subprocesses are stopped before
   an idle report.
2. Add stop-work semantics for unauthorized messages that prove local execution,
   especially `build_started` and `build_output`.
3. Reconcile the new receipt-state language with existing startup recovery.
4. Add explicit tests for worker reconnect truthfulness and stale local
   execution stop behavior.

## Final Summary

Top findings ordered by severity:

1. High: server reconnect policy depends on worker local-state truthfulness, but
   the design does not specify how the worker preserves or stops active builds
   across websocket reconnects.
2. High: unauthorized `build_started`/`build_output` responses do not stop stale
   local execution after timeout or reassignment races.
3. Medium: the new receipt-state recovery sentence conflicts with the existing
   startup recovery model for `dispatched` and `started` builds.

Confidence score: 45/100, with deductions shown above. Recommendation: no-go
until the worker-side reconnect and stale-execution contracts are made concrete.
