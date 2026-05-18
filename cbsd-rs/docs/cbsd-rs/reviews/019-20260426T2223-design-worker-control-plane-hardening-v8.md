# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v9                                                                     |
| Date           | 2026-04-26 22:23 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior review/security-review documents                 |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until acknowledged-dispatched idle reconnect is specified                   |

## Summary

Draft v9 addresses the explicit v7 review items: it defines terminal same-worker
idle handling for `started` and `revoking`, specifies a DB-backed two-phase
migration ownership check, and makes invalid stored periodic descriptors fatal
scheduler errors that disable the task.

The remaining blocker is in the `dispatched` reconnect path. The design now
relies on dispatch-ack timeout to resolve a live same-worker idle reconnect, but
the same design cancels that timer on valid receipt/execution messages while the
DB may still remain `dispatched`. That leaves a concrete orphan path for an
accepted but not started assignment after a live same-worker connection is
superseded by an idle reconnect.

Implementation-only phase-review items are N/A because no implementation plan or
implementation commits are in scope.

## Findings

### High: acknowledged dispatched reconnect can still orphan active work

D4 says any valid owned message proving receipt or execution cancels the
dispatch-ack timer before applying its own transition, including
`build_accepted`, `build_output`, `build_finished`, transient `build_rejected`,
and accepted reconnect `worker_status(building)`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:292`).
The transition matrix then keeps `build_accepted` in DB state `dispatched` and
says the build remains assigned
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:446`).

The idle reconnect rule says a same-worker idle reconnect for `dispatched` rolls
back only when the previous same-worker connection was absent, disconnected, or
dead. If the new idle connection superseded a live same-worker connection, the
server leaves the assignment in place and relies on dispatch-ack timeout,
liveness, or explicit revoke handling to resolve it
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:569`).

Those rules conflict for a normal sequence:

1. Worker receives `build_new`.
2. Worker sends `build_accepted`.
3. Server cancels the dispatch-ack timer but leaves DB state `dispatched`.
4. A new same-worker websocket authenticates and reports idle while the old
   connection is still live.
5. The new connection wins; the old sender is removed; superseded cleanup cannot
   mutate migrated active entries.
6. The idle rule leaves the `dispatched` assignment in place because the
   previous connection was live, but the dispatch-ack timer is already canceled
   and the old connection no longer has authoritative liveness.

The current code shape confirms why this is not just prose-level uncertainty.
`ActiveBuild` stores an `ack_cancel` token but no durable accepted/receipt state
(`cbsd-rs/cbsd-server/src/queue/mod.rs:27`), `build_accepted` only cancels that
timer (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:284`), and `builds` has no
accepted state between `dispatched` and `started`
(`cbsd-rs/cbsd-server/src/db/builds.rs:260`). Once the live old connection is
superseded, the normal liveness monitor resolves work by the old connection ID,
but the migrated active entry no longer belongs to that old connection
(`cbsd-rs/cbsd-server/src/ws/liveness.rs:120`;
`cbsd-rs/cbsd-server/src/ws/handler.rs:781`).

Required design change: split `dispatched` idle handling by delivery/receipt
state. The design needs to say what happens when the ack timer has already been
canceled but the DB is still `dispatched`, including active ownership, final
state or rollback, watcher cleanup, and whether a live superseded connection can
ever leave the assignment in place. Add tests for live same-worker idle
reconnect after `build_accepted` and after any other valid message that cancels
the ack timer without first moving DB state out of `dispatched`.

### Medium: idle reconciliation candidate discovery is still implicit

V9 says idle reconciliation is limited to builds whose persisted
`builds.worker_id` matches the authenticated registered worker ID
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:559`).
It also allows the previous same-worker connection state to be `absent`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:569`).

The design does not say how the server discovers the owned active assignment in
that absent/dead case. In the current code, `ActiveBuild` is keyed by build ID
and stores only `connection_id`, not registered worker ID
(`cbsd-rs/cbsd-server/src/queue/mod.rs:27`), while `WorkerState::Dead` carries
no registered worker ID (`cbsd-rs/cbsd-server/src/ws/liveness.rs:23`). If the
worker map no longer has a live or disconnected entry for the old connection, an
implementation must choose between scanning `queue.active` and querying each
build, querying SQLite for active rows by `worker_id`, or extending the active
entry. That choice affects lock ordering and which source is authoritative.

Required design change: define the candidate-discovery algorithm for
`worker_status(idle)` before applying the `dispatched`/`started`/`revoking`
table, including lock/DB ordering and the exact DB states considered active.

### Medium: two-phase migration does not require DB state freshness

The same-worker migration algorithm snapshots active build IDs under the queue
lock, releases the lock, queries persisted assignment state, then reacquires the
queue lock and migrates entries whose `builds.worker_id` matches the
authenticated registered worker ID
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:484`).

That fixes the v7 owner-check gap, but the migration predicate is still written
only in terms of persisted `worker_id`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:487`).
The design does not require the implementation to re-check that the persisted DB
state is still one of `dispatched`, `started`, or `revoking` when the queue lock
is reacquired. A concurrent finish/requeue path could leave a stale active entry
briefly present while the DB row is already terminal or queued. Migrating that
entry to the new connection would violate the meaningful-state rule and can
block dispatch eligibility or feed later idle reconciliation with a non-active
DB state.

Required design change: make migration conditional on both current queue
ownership and current active DB state, and say what to do with active entries
whose DB row is queued or terminal when migration reacquires the lock.

## Prior Review Coverage

The v7 review findings were checked against Draft v9:

| v7 finding                                                             | Status                      | Notes                                                                                                                                                                               |
| ---------------------------------------------------------------------- | --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Same-worker idle reconnect can orphan `started`/`revoking` assignments | Addressed with residual gap | V9 now marks `started` as `failure` and `revoking` as `revoked`. A related orphan remains for acknowledged `dispatched` assignments after the ack timer has been canceled.          |
| Migration's assignment-owner check lacks an implementation contract    | Mostly addressed            | V9 specifies DB-backed persisted `builds.worker_id` checks and no queue lock across DB I/O. It still needs active DB state freshness during migration.                              |
| Invalid stored periodic descriptors lack trigger-time disposition      | Addressed                   | D5 makes invalid stored descriptors fatal, non-retried, disables the task, and persists `last_error`. Existing scheduler code already has a `Fatal` path with `disable_with_error`. |

Earlier review findings carried forward into v7 were also checked:

| Earlier finding                                           | Status                        | Notes                                                                                                                                              |
| --------------------------------------------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dispatch rollback column resets                           | Addressed                     | D4 lists the reset columns and requires a dedicated rollback DB operation.                                                                         |
| Meaningful-state authorization matrix                     | Mostly addressed              | The matrix covers message/state validity, but migration still needs to require active DB states when applying a stale DB snapshot.                 |
| Descriptor validation centralization                      | Addressed                     | D5 requires one typed validator for REST, periodic create/update, and scheduler trigger paths.                                                     |
| Bounded log tailing contract                              | Addressed                     | D7 selects reverse block scanning, a 1,000-line cap, a 4 MiB budget, full-line UTF-8 output, and no exact `total_lines`.                           |
| Enum-based unauthorized action/reason                     | Addressed                     | `WorkerBuildAction` and coarse `NotAssigned` are specified.                                                                                        |
| Protocol version decision                                 | Addressed                     | D3 keeps protocol version 2 as an in-flight pre-production correction.                                                                             |
| Reconnect ownership gap                                   | Mostly addressed              | Cross-worker and same-worker ownership checks are much stronger; idle candidate discovery and acknowledged-dispatched resolution remain ambiguous. |
| `cbc` log-tail contract                                   | Addressed                     | `cbc` is in scope, default tail count is 50, and exact `total_lines` is removed.                                                                   |
| UTF-8/full-line truncation                                | Addressed                     | D7 defines both behaviors.                                                                                                                         |
| `worker_status(building)` unauthorized response conflict  | Addressed                     | Invalid building reports receive `UnauthorizedBuildAction` followed by reporter-directed `BuildRevoke`.                                            |
| `worker_status(idle)` listed under build-id authorization | Addressed                     | D1 separates idle reconciliation from build-scoped authorization.                                                                                  |
| Dispatch-ack timer cancellation for start/resume paths    | Addressed but exposed new gap | D4 cancels timers broadly; the missing piece is what resolves `dispatched` after the timer is already canceled.                                    |
| Reporter-directed stray revoke side effects               | Addressed                     | D2 forbids DB, timer, watcher, active-queue, and assigned-worker side effects.                                                                     |
| Queued rollback cleanup only for pre-delivery failures    | Addressed                     | D4 applies cleanup to all assigned-build requeue paths.                                                                                            |
| Coarse unauthorized reason mapping                        | Addressed                     | All syntactically valid authorization failures map to `NotAssigned`; details stay server-only.                                                     |

The original security review findings are represented:

| Security review finding                   | Status in design                   | Notes                                                                                                                                                    |
| ----------------------------------------- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Cross-worker lifecycle spoofing           | Addressed with reconnect residuals | D1/D2 authorize build-scoped messages through active ownership. The remaining risk is same-worker reconnect state resolution, not cross-worker spoofing. |
| Cross-worker log output spoofing          | Addressed                          | D6 applies active-build ownership to output and forbids arbitrary log creation.                                                                          |
| Empty component lists can strand dispatch | Addressed                          | D5 rejects empty descriptors before queue/DB state and keeps dispatch rollback as an invariant guard.                                                    |
| Post-DB dispatch failures lack rollback   | Addressed                          | D4 defines pre-delivery and assigned-requeue rollback cleanup.                                                                                           |
| JSON tail reads full log into memory      | Addressed                          | D7 defines bounded reverse tailing.                                                                                                                      |

## Deferred Or Ambiguous Items

No item is explicitly deferred by the design; it says open questions are
resolved. Remaining ambiguous items:

- `dispatched` same-worker idle reconnect after dispatch-ack cancellation.
- Candidate discovery for idle reconciliation when the old worker entry is
  absent or dead.
- Active DB state freshness during two-phase same-worker migration.

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

| Item                                                       | Points | Description                                                                                                                                                                             |
| ---------------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Starting score                                             | 100    |                                                                                                                                                                                         |
| D7: acknowledged dispatched idle reconnect can orphan work | -20    | `build_accepted` can cancel the dispatch-ack timer while DB remains `dispatched`; a live same-worker idle reconnect then leaves the assignment in place with no authoritative resolver. |
| D11: idle candidate discovery missing                      | -5     | The design limits idle reconciliation by persisted `worker_id` but does not define how to find owned active assignments when the old worker entry is absent or dead.                    |
| D11: migration state freshness missing                     | -5     | The DB-backed migration predicate checks persisted `worker_id` but does not require active DB state freshness when reacquiring the queue lock.                                          |
| **Total**                                                  | **70** |                                                                                                                                                                                         |

Interpretation: 70/100. The design fixes the explicit v7 blockers but still has
a high-risk reconnect state gap that should be resolved before implementation
planning.

## Go / No-Go

No-go for implementation planning as-is.

Required before planning:

1. Define same-worker idle behavior for acknowledged-but-not-started
   `dispatched` assignments after the dispatch-ack timer has been canceled.
2. Define the idle reconciliation candidate-discovery algorithm and lock/DB
   ordering.
3. Require active DB state freshness during two-phase same-worker migration.

## Final Summary

Top findings ordered by severity:

1. High: acknowledged `dispatched` assignments can still be orphaned by a live
   same-worker idle reconnect after the dispatch-ack timer is canceled.
2. Medium: idle reconciliation does not define how to discover same-worker
   active assignments when the previous connection is absent or dead.
3. Medium: two-phase migration checks persisted worker ownership but not active
   DB state freshness.

Confidence score: 70/100, with deductions shown above. Recommendation: no-go
until the acknowledged-dispatched reconnect path is made concrete.
