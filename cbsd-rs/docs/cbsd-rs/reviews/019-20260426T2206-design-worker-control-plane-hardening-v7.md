# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v8                                                                     |
| Date           | 2026-04-26 22:06 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior review/security-review documents                 |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until same-worker idle reconnect resolution is made concrete                |

## Summary

Draft v8 materially addresses the v6 review blockers. It defines same-worker
connection migration before status handling, prevents superseded connection
cleanup from mutating migrated active entries, removes the unused
`InvalidAssignment` worker-facing reason, and specifies owned
`worker_status(building)` behavior while `revoking`.

The remaining blocker is narrower but still security-relevant for the worker
control plane: the design makes the newest same-worker connection win before
status handling, but the idle-status rules for an already `started` or
`revoking` assignment are delegated to "liveness/revoke rules" without saying
which connection owns the assignment, which monitor remains active, or what
state transition must happen. That can leave implementation authors with either
an orphaned active build on an idle connection or a live old connection that was
supposed to have been superseded.

Implementation-only phase-review items are N/A because no implementation plan or
implementation commits are in scope.

## Findings

### High: same-worker idle reconnect can orphan started/revoking assignments

Draft v8 says same-worker migration happens during the authenticated handshake,
before the server processes `worker_status`, and that the newest authenticated
connection wins
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:449`,
`:455`). The same section then allows active entries to migrate from the old
connection ID to the new connection ID, removes the old outbound sender, and
requires later cleanup of the superseded connection not to requeue, fail, or
revoke migrated active entries
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:465`,
`:472`).

That resolves the v6 cleanup race for `dispatched`, but the idle reconnect
policy remains incomplete for non-dispatch states. The matrix and reconnect
section say an idle same-worker reconnect with `started` or `revoking` state
follows existing liveness/revoke rules and grace-period policy
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:426`,
`:547`, `:552`). The design does not specify whether the active entry remains on
the live old connection until the idle report is resolved, migrates to the new
idle connection, starts a grace monitor for the abandoned work, immediately uses
the dead-worker resolver, or sends a normal revoke to either connection.

This is not just a missing implementation detail. In the current code,
`ActiveBuild` is owned by a `connection_id`
(`cbsd-rs/cbsd-server/src/queue/mod.rs:27`, `:36`), and the liveness monitor
resolves work by looking up active builds for that connection
(`cbsd-rs/cbsd-server/src/ws/liveness.rs:120`, `:147`;
`cbsd-rs/cbsd-server/src/ws/handler.rs:781`, `:805`, `:813`, `:821`). If v8 is
implemented literally by migrating active ownership during handshake and then
accepting a new idle connection, a `started` build can now be assigned to an
idle-but-connected websocket. The old connection was superseded, its sender was
removed, and its cleanup is forbidden from mutating the migrated build. The
ordinary liveness monitor no longer has a disconnected owner to expire, so the
assignment can remain active indefinitely.

The same ambiguity affects `revoking`: after migration to an idle new
connection, the design says the build follows revoke/liveness rules, but it does
not identify the command recipient or terminal fallback. A later implementation
could send a stateful revoke to a worker that just reported it has no local
build, wait forever for an ack from that connection, or wrongly let superseded
cleanup mark the migrated build terminal.

Required design change: define the exact same-worker idle reconnect state table
for `started` and `revoking`, including whether active ownership is migrated
before or after idle reconciliation, which connection receives any revoke, which
timer or grace monitor is authoritative, and which terminal/requeue operation is
used when the worker has confirmed local state loss. Add tests for live,
disconnected, dead, and absent previous-connection states for `started` and
`revoking`, not only for `dispatched`.

### Medium: migration's assignment-owner check lacks an implementation contract

The reconnect migration rule says active entries may migrate only when the
server-side assignment belongs to the authenticated registered worker ID, and
that the check uses server state rather than the reconnecting worker's claimed
`build_id`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:465`,
`:468`). The later `worker_status(building)` rule identifies `builds.worker_id`
as the persisted authority
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:478`,
`:485`).

The current in-memory active entry does not store the registered worker ID; it
stores only the active `connection_id`, trace, descriptor, priority, and ack
token (`cbsd-rs/cbsd-server/src/queue/mod.rs:27`, `:36`). The existing handshake
migration rewrites every active build on the old connection without checking
persisted `builds.worker_id` (`cbsd-rs/cbsd-server/src/ws/handler.rs:263`,
`:283`).

The design therefore requires an implementation choice that is not written down:
either add the registered worker ID to `ActiveBuild`, query the DB for each
active build during migration, or snapshot an assignment map elsewhere. That
choice affects lock ordering and the security invariant. A DB query while
holding the queue mutex must be deliberate; a migration based only on the old
connection ID preserves the current bug-shaped behavior for corrupted or
partially rolled-back rows.

Required design change: state the authoritative data source used by handshake
migration to prove `builds.worker_id == authenticated_worker_id`, and define the
lock/DB ordering expected for that check. If the intended answer is to extend
`ActiveBuild`, say so and add a test that an active entry with a mismatched
persisted `worker_id` is not migrated.

### Low: invalid stored periodic descriptors lack trigger-time disposition

D5 now centralizes descriptor validation and requires REST build submission,
periodic create/update, and scheduler trigger paths to use the same typed
validator
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:296`,
`:318`). That addresses the prior duplication and empty-component review
concerns for new ingress paths.

However, the scheduler trigger path is not a route handler, so it cannot return
the route-appropriate `400` response described for REST paths
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:309`,
`:314`). Existing trigger code classifies malformed stored JSON as fatal
(`cbsd-rs/cbsd-server/src/scheduler/trigger.rs:75`, `:77`), while other trigger
failures feed retry/disable behavior later in the scheduler. The design does not
say what happens when a legacy or corrupted periodic row fails the new
non-empty/known-component validator at trigger time.

Required design change: define whether invalid stored periodic descriptors are
skipped, marked fatal and disabled, retried with backoff, or only logged. The
important security property is already stated: they must not enqueue builds. The
operational disposition should still be explicit to prevent retry loops or
silent scheduler noise.

## Prior Review Coverage

The v6 review findings were checked against Draft v8:

| v6 finding                                                                                    | Status           | Notes                                                                                                                                                                 |
| --------------------------------------------------------------------------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `worker_status(building)` has conflicting connection ownership rules                          | Addressed        | D1 now says authorization happens after same-worker migration, and the reconnect section defines migration before `worker_status` handling.                           |
| Idle reconnect can roll back a same-worker assignment without a connection-state precondition | Mostly addressed | `dispatched` rollback is now limited to absent, disconnected, or dead previous same-worker connections. `started`/`revoking` idle disposition remains underspecified. |
| Owned `worker_status(building)` while `revoking` is missing                                   | Addressed        | The matrix and reconnect section now keep DB state `revoking` and send the normal stateful revoke to the current assigned connection.                                 |
| `InvalidAssignment` remains abstract and untested                                             | Addressed        | The worker-facing enum now contains only `NotAssigned`; malformed messages are handled by protocol validation and server logs.                                        |

Earlier review findings carried forward into v6 were also checked:

| Earlier finding                                           | Status           | Notes                                                                                                                                                                          |
| --------------------------------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Dispatch rollback column resets                           | Addressed        | D4 lists the cleared columns and requires a dedicated rollback DB operation.                                                                                                   |
| Meaningful-state authorization matrix                     | Mostly addressed | The matrix covers lifecycle/output states. Same-worker idle handling for `started`/`revoking` still delegates too much to unspecified liveness/revoke behavior.                |
| Descriptor validation centralization                      | Mostly addressed | D5 requires one typed validator for REST, periodic create/update, and scheduler trigger paths; scheduler trigger-time disposition for invalid stored rows remains unspecified. |
| Bounded log tailing contract                              | Addressed        | D7 selects reverse block scanning, a 1,000-line cap, a 4 MiB byte budget, full-line output, valid UTF-8, and no exact `total_lines`.                                           |
| Enum-based unauthorized action/reason                     | Addressed        | `WorkerBuildAction` and a single coarse `NotAssigned` reason are specified.                                                                                                    |
| Protocol version decision                                 | Addressed        | D3 keeps protocol version 2 as a pre-production correction.                                                                                                                    |
| Reconnect ownership gap                                   | Mostly addressed | Cross-worker and building reconnect authorization are covered; handshake migration's authoritative assignment-owner check needs an implementation contract.                    |
| `cbc` log-tail contract                                   | Addressed        | `cbc` is in package scope, the client default is 50, and exact `total_lines` is no longer required.                                                                            |
| UTF-8/full-line truncation                                | Addressed        | D7 defines both behaviors.                                                                                                                                                     |
| `worker_status(building)` unauthorized response conflict  | Addressed        | Invalid building reports consistently receive `UnauthorizedBuildAction` followed by reporter-directed `BuildRevoke`.                                                           |
| `worker_status(idle)` listed under build-id authorization | Addressed        | D1 separates idle reconciliation from build-scoped authorization.                                                                                                              |
| Dispatch-ack timer cancellation for start/resume paths    | Addressed        | D4 says all valid owned receipt/execution messages cancel the dispatch-ack timer before later transitions.                                                                     |
| Reporter-directed stray revoke side effects               | Addressed        | D2 forbids DB, timer, watcher, active-queue, and assigned-worker side effects.                                                                                                 |
| Queued rollback cleanup only for pre-delivery failures    | Addressed        | D4 applies the same cleanup to dispatch-ack timeout, transient rejection, and same-worker idle rollback from `dispatched`.                                                     |
| Coarse unauthorized reason mapping                        | Addressed        | All syntactically valid authorization failures map to `NotAssigned`; detailed reasons stay server-only.                                                                        |

The original security review findings are represented:

| Security review finding                   | Status in design                       | Notes                                                                                                                                        |
| ----------------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Cross-worker lifecycle spoofing           | Addressed with residual reconnect risk | D1/D2 authorize all build-scoped messages through active ownership. Same-worker idle reconnect still needs concrete non-dispatch resolution. |
| Cross-worker log output spoofing          | Addressed                              | D6 applies the same active-build ownership rule to output and forbids arbitrary log creation.                                                |
| Empty component lists can strand dispatch | Addressed                              | D5 rejects empty descriptors before queue/DB state and keeps dispatch rollback as an invariant guard.                                        |
| Post-DB dispatch failures lack rollback   | Addressed                              | D4 defines pre-delivery and assigned-requeue rollback cleanup.                                                                               |
| JSON tail reads full log into memory      | Addressed                              | D7 defines bounded reverse tailing.                                                                                                          |

## Deferred Or Ambiguous Items

No item is explicitly deferred by the design; it says open questions are
resolved. Remaining ambiguous items:

- Same-worker idle reconnect for `started` and `revoking` assignments needs an
  exact owner/timer/revoke/terminal-state table.
- Handshake migration needs a concrete authoritative data source and lock/DB
  ordering for the persisted assignment-owner check.
- Scheduler-triggered invalid periodic descriptors need an explicit operational
  disposition.

## Phase-Review N/A Items

No implementation plan or implementation commits are in scope. Therefore the
following phase-review checks are N/A for this review:

- Plan item implementation status.
- Commit-by-commit build/test hygiene.
- Duplicated implementation code introduced by the phase.
- Completed implementation test coverage.

## Confidence Score

Design-phase scoring starts at 100. Implementation-only criteria with no plan or
code in scope are N/A unless the design itself creates the defect.

| Item                                                  | Points | Description                                                                                                                                                                                |
| ----------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Starting score                                        | 100    |                                                                                                                                                                                            |
| D7: same-worker idle reconnect can orphan active work | -20    | `started` and `revoking` idle reconnects are delegated to liveness/revoke rules without defining ownership, timers, recipients, or terminal behavior after handshake migration.            |
| D11: migration owner-check contract missing           | -5     | The design requires migration only for assignments owned by the same registered worker but does not state whether that proof comes from DB, `ActiveBuild`, or another synchronized source. |
| D11: invalid periodic trigger disposition missing     | -5     | The shared descriptor validator covers scheduler triggers, but the design does not define how invalid legacy/corrupted periodic rows are handled operationally.                            |
| **Total**                                             | **70** |                                                                                                                                                                                            |

Interpretation: 70/100. The design has addressed the main prior security
blockers, but one reconnect-state ambiguity is still significant enough to block
implementation planning.

## Go / No-Go

No-go for implementation planning as-is.

Required before planning:

1. Define the exact same-worker idle reconnect behavior for `started` and
   `revoking`, including active ownership, liveness/revoke authority,
   recipients, and final state transitions.
2. Define the authoritative assignment-owner data source and lock/DB ordering
   used during handshake migration.
3. Define trigger-time handling for invalid stored periodic descriptors.

## Final Summary

Top findings ordered by severity:

1. High: same-worker idle reconnect can orphan `started` or `revoking`
   assignments after handshake migration.
2. Medium: same-worker migration's persisted assignment-owner check lacks an
   implementation contract.
3. Low: invalid stored periodic descriptors have no explicit scheduler
   disposition.

Confidence score: 70/100, with deductions shown above. Recommendation: no-go
until the reconnect-state ambiguity is resolved.
