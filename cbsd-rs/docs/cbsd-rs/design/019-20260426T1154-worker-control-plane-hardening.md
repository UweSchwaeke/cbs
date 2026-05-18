# Worker Control Plane Hardening

| Field    | Value                                             |
| -------- | ------------------------------------------------- |
| Design   | 019 (sibling: `security-audit-remediation`)       |
| Date     | 2026-04-26                                        |
| Status   | Draft v11                                         |
| Packages | `cbsd-server`, `cbsd-worker`, `cbsd-proto`, `cbc` |

## Position in the Security Work

This design and the `security-audit-remediation` design (also at seq 019,
timestamp `20260514T1040`) are sibling deliverables of the cbsd-rs security
review effort. This document addresses the **worker control plane** trust
boundary specifically: lifecycle-message ownership, dispatch rollback,
descriptor validation, bounded log tailing, and reconnect ownership. The
`security-audit-remediation` design is the **natural progression** of this work,
covering the cross-cutting findings outside the worker control plane
(authentication, RBAC, CLI transport, worker-side input safety, resource limits,
token-material redaction), and additionally takes ownership of unresolved open
items from this design's v10 review (worker `accepted` phase reconnect,
liveness/dead-worker policy for receipt-aware `dispatched`, and
superseded-live-connection stop-work delivery). The two designs together
constitute the full security remediation plan; either without the other leaves
gaps.

The seq number for both designs was 018 and 020 in earlier drafts, respectively.
Both were renamed to seq 019 (this file's timestamp preserved) so the security
work shares a single sequence number.

## Revision History

- **v11 (2026-04-27)** — addresses the v9 design review
  (`019-20260426T2239-design-worker-control-plane-hardening-v9.md`). Adds the
  worker-side active-build state model required for truthful reconnect status,
  extends reporter-directed stop-work revokes to unauthorized execution
  messages, and reconciles in-memory receipt state with the existing
  fail-in-flight startup recovery policy.
- **v10 (2026-04-26)** — addresses the v8 design review
  (`019-20260426T2223-design-worker-control-plane-hardening-v8.md`). Adds
  explicit assignment receipt state for delivered-but-not-started builds,
  resolves acknowledged `dispatched` idle reconnects to `queued`, defines
  idle-reconciliation candidate discovery, and requires active DB state
  freshness during two-phase migration.
- **v9 (2026-04-26)** — addresses the v7 design review
  (`019-20260426T2206-design-worker-control-plane-hardening-v7.md`). Defines
  authoritative idle reconnect resolution for `started` and `revoking`
  assignments, specifies DB-backed two-phase same-worker migration checks, and
  makes invalid stored periodic descriptors fatal-disable at trigger time.
- **v8 (2026-04-26)** — addresses the v6 design review
  (`019-20260426T2151-design-worker-control-plane-hardening-v6.md`). Defines
  same-worker reconnect migration, constrains idle rollback when a live
  same-worker connection was superseded, specifies owned `revoking` reconnect
  handling, and removes the unused `InvalidAssignment` worker-facing reason.
- **v7 (2026-04-26)** — addresses the v5 design review
  (`019-20260426T1625-design-worker-control-plane-hardening-v5.md`). Defines
  dispatch-ack timer cancellation for all valid receipt/start paths, separates
  reporter-directed stray revokes from state-mutating assignment revokes,
  applies queued rollback cleanup to all assigned-build requeue paths, and maps
  coarse unauthorized reasons.
- **v6 (2026-04-26)** — addresses the v4 design review
  (`019-20260426T1614-design-worker-control-plane-hardening-v4.md`). Splits
  build-scoped authorization from idle reconnect reconciliation and makes
  invalid `worker_status(building)` claims send both `UnauthorizedBuildAction`
  and `BuildRevoke`.
- **v5 (2026-04-26)** — addresses the v3 design review
  (`019-20260426T1602-design-worker-control-plane-hardening-v3.md`). Defines
  `worker_status(idle)` reconciliation as limited to the authenticated worker's
  own persisted assignment and changes worker-facing unauthorized reasons to
  coarse values, with detailed internal reasons kept in server logs only.
- **v4 (2026-04-26)** — addresses the v2 design review
  (`019-20260426T1547-design-worker-control-plane-hardening-v2.md`). Extends
  active-build ownership to reconnect `worker_status`, defines mismatched
  reconnect handling as warning + revoke-to-reporter, adds `cbc` to the affected
  packages, clarifies tail request-vs-response terminology, sets the client tail
  default to 50 lines, and defines UTF-8/full-line truncation behavior.
- **v3 (2026-04-26)** — resolves the remaining open decisions: use enum-based
  unauthorized-action fields, keep protocol version 2, reduce JSON tail requests
  to 1,000 lines, and implement bounded reverse block scanning with a 4 MiB
  budget and truncation metadata.
- **v2 (2026-04-26)** — addresses the v1 design review
  (`019-20260426T1228-design-worker-control-plane-hardening-v1.md`). Clarifies
  dispatch rollback columns, adds an owned-message transition matrix,
  centralizes descriptor validation, documents current tail behavior with
  alternatives, and adds protocol message-shape context.
- **v1 (2026-04-26)** — initial draft from the security review findings.

## Problem

The worker websocket is an authenticated control plane, but the server currently
trusts worker-supplied `build_id` values too broadly after the websocket upgrade
succeeds. Any registered worker API key can send lifecycle or output messages
for a build assigned to another worker connection.

The related review is `cbsd-rs/docs/000-20264026T1104-security-review.md`.

Security issues identified:

1. Worker lifecycle messages are authorized at connection level but not at
   active-build ownership level.
2. Worker output messages can append log data to arbitrary build IDs.
3. Invalid build descriptors with empty component lists can enter the queue and
   leave dispatch state stuck if component packing fails.
4. Dispatch failures after the DB state changes to `dispatched` do not share a
   complete rollback model.
5. The log-tail endpoint applies a line cap only after reading the full log file
   into memory.

## Goals

- Treat each active build assignment as an authorization boundary.
- Make every worker-originated build message prove that the sending connection
  owns the referenced active build.
- Give workers an explicit non-fatal authorization failure response so operators
  can diagnose stale or malicious worker behavior.
- Give stale local execution an explicit stop-work command, not only a rejected
  state mutation.
- Keep dispatch state rollback-safe until the worker has actually received the
  build assignment.
- Make worker reconnect status truthful by moving active-build execution state
  outside the websocket connection loop.
- Reject invalid descriptors before they can affect queue or DB state.
- Bound memory use for log-tail reads.

## Non-Goals

- Redesigning worker registration or worker API-key issuance.
- Replacing websocket transport or changing the basic protocol handshake.
- Implementing a full per-worker RBAC model.
- Treating component-packaging failures as build failures. A build that has not
  reached a worker has not started.
- Changing build log storage layout.

## Decisions

### D1: Worker build messages are server-authorized

Build-scoped worker messages are authorized only after authentication and any
same-worker reconnect migration have completed. They are authorized only if all
of the following are true:

- `queue.active` contains the message's `build_id`.
- The active entry's `connection_id` equals the websocket connection that sent
  the message.
- The build is in a state where that message type is meaningful.

This build-scoped authorization check applies to:

- `worker_status` when it reports `Building { build_id }`
- `build_accepted`
- `build_started`
- `build_output`
- `build_finished`
- `build_rejected`

The authorization check is server-side and based on server state only. Worker
claims are never sufficient to establish build ownership.

Same-worker reconnect migration is the only non-dispatch path that may rewrite
an active entry's `connection_id`. Ordinary lifecycle, output, reject, finish,
and status handlers do not use worker claims to migrate active ownership.

`worker_status(idle)` has no message-local `build_id`, so it is not authorized
through the build-scoped check above. It is handled as reconnect reconciliation
for the authenticated registered worker's own persisted assignments. An idle
status from a worker with no owned active assignment is success/no-op and does
not emit `UnauthorizedBuildAction`.

### D2: Unauthorized worker build messages are rejected, logged, and ignored

When a worker sends a build-scoped message it is not authorized to perform, the
server does not mutate build state, does not append output, and does not close
the websocket solely because of that mismatch.

The server records a security warning containing at least:

- registered worker ID
- worker name
- websocket connection ID
- message type
- referenced build ID
- whether the build was unknown, inactive, or owned by another connection

The response to the worker is non-fatal and explicit: the worker receives a
server message that says the attempted action is not authorized for that build.
The response must avoid disclosing another worker's identity or internal
connection ID. The worker logs that response as an error and continues running.

Some unauthorized messages also prove or strongly imply stale local execution.
When the reporting worker sends unauthorized `worker_status(building)`,
`build_started`, or `build_output`, the server sends `UnauthorizedBuildAction`
followed by `BuildRevoke { build_id }` to that reporting connection. The first
message tells the worker the reported action was not authorized; the second is
the stop-work command for the local build. The server still logs the event as a
security warning and does not move active ownership.

That `BuildRevoke` is a reporter-directed stop command, not the normal
state-mutating assignment revoke. It is sent only to the reporting connection.
It must not set the real assigned build to `revoking`, cancel the real
assignment's timers, remove a log watcher, remove or rewrite `queue.active`, or
send anything to the assigned worker.

Unauthorized `build_accepted`, `build_rejected`, and `build_finished` do not get
the additional reporter-directed revoke by default. `build_accepted` does not
prove local execution started. `build_rejected` says the worker did not run the
build. `build_finished` says the local execution has already ended. Those
messages still receive `UnauthorizedBuildAction`, are logged as security
warnings, and have no DB, queue, watcher, or log-output side effects.

### D3: Add a non-fatal authorization error to the worker protocol

`ServerMessage::Error` is reserved for connection or protocol failures that end
the handshake or close the connection. Unauthorized build actions need a
different wire shape because the selected behavior is to keep the worker
connected.

Current websocket messages are composed as serde-tagged Rust enums in
`cbsd-proto/src/ws.rs`:

```rust
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage { ... }

#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerMessage { ... }
```

On the wire, each message is JSON with a `"type"` discriminator. For example,
`ServerMessage::BuildRevoke { build_id }` serializes as a JSON object whose type
is `"build_revoke"`. Variant names and enum field types therefore define the
protocol contract.

The protocol gains a non-fatal server-to-worker message with these semantics:

```rust
UnauthorizedBuildAction {
    build_id: BuildId,
    action: ...,
    reason: ...,
}
```

The message uses enums for both `action` and `reason`:

```rust
#[serde(rename_all = "snake_case")]
pub enum WorkerBuildAction {
    WorkerStatus,
    BuildAccepted,
    BuildStarted,
    BuildOutput,
    BuildFinished,
    BuildRejected,
}

#[serde(rename_all = "snake_case")]
pub enum UnauthorizedBuildReason {
    NotAssigned,
}
```

This matches the existing enum-driven protocol style, makes typos compile-time
errors, gives tests concrete variants to assert, and keeps security-sensitive
control-plane outcomes documented in `cbsd-proto`. The worker renders these enum
values into human-readable error logs.

Worker-facing reasons are intentionally coarse. The worker protocol must not let
an authenticated worker distinguish unknown build IDs, inactive builds, builds
owned by another worker, wrong connection IDs, wrong worker identity, or invalid
internal DB state. Detailed reasons remain server-only structured log fields.

The worker-facing mapping is:

- `NotAssigned`: every syntactically valid build-scoped message that fails
  active-build authorization, including unknown, inactive, terminal, differently
  owned, wrong-connection, and wrong-worker cases.

There is intentionally no second worker-facing reason until a concrete,
non-leaking wire case exists. Malformed protocol messages that cannot identify a
build are handled by protocol validation and server logs, not by
`UnauthorizedBuildAction`.

Backward compatibility is not a constraint for this feature because the Rust
worker/server deployment is not yet production. The server and worker changes
may land together without a compatibility shim for older workers.

The protocol version remains `2`. This is treated as an in-flight pre-production
protocol correction rather than a versioned compatibility break.

### D4: Dispatch remains tentative until the assignment is delivered

A build is not considered started, accepted, or failed merely because the server
selected a worker and wrote `dispatched` to SQLite. Until `build_new` and its
component tarball are successfully handed to the worker websocket sender, the
assignment is tentative.

If any post-DB, pre-delivery step fails, the build rolls back to `queued`.
Examples include:

- component directory missing
- component tarball packing failure
- missing websocket sender
- failure to enqueue either the JSON `build_new` frame or the binary tarball
  frame

Rollback semantics:

- `builds.state` returns to `queued`.
- `builds.worker_id` is cleared to `NULL`.
- `builds.trace_id` is cleared to `NULL`.
- `builds.error` is cleared to `NULL`.
- `builds.started_at`, `builds.finished_at`, and `builds.build_report` are
  cleared to `NULL` defensively. They should normally be `NULL` for a
  pre-delivery failure, but clearing them prevents legacy or corrupted rows from
  exposing stale build provenance after rollback.
- the active-build entry is removed
- the log watcher created for that dispatch attempt is removed
- the attempted `trace_id` is discarded
- the next successful dispatch creates a fresh `trace_id`
- the original build descriptor and priority are preserved

This requires a dedicated rollback DB operation, not the generic state-update
helper. The rollback operation owns the exact column reset list above.

This records the decision that a pre-delivery failure is a dispatch failure, not
a build failure.

The same rollback cleanup applies to every assigned-build path that returns a
build to `queued` before terminal completion. This includes dispatch-ack
timeout, transient `build_rejected`, and same-worker idle reconnect rollback
from `dispatched`. Returning to `queued` means the build is unassigned and must
not retain stale attempt provenance from the abandoned assignment. If operators
need retry diagnostics, those belong in structured logs or a future retry
history table, not in the current queued build row.

Dispatch-ack timers measure whether the assignment was delivered to and acted on
by the assigned worker. Any valid owned message that proves receipt or local
execution cancels the dispatch-ack timer before applying its own state
transition. This includes `build_accepted`, `build_started` from `dispatched`,
`build_output`, `build_finished(failure)`, `build_finished(revoked)`, transient
`build_rejected`, and accepted reconnect `worker_status(building)` when it
resumes or implicitly accepts a `dispatched` assignment. Unauthorized messages
never cancel timers for the real assignment.

Because SQLite currently has no separate `accepted` state between `dispatched`
and `started`, active assignments must carry an in-memory receipt state:

```rust
enum ActiveAssignmentReceipt {
    AwaitingReceipt,
    ReceivedByWorker,
}
```

`AwaitingReceipt` means the dispatch-ack timer is still authoritative.
`ReceivedByWorker` means an owned worker message proved that the worker received
or acted on the assignment while the DB row may still be `dispatched`.
`build_accepted` sets this state to `ReceivedByWorker` and cancels the
dispatch-ack timer without changing DB state. Owned `build_started` and
`build_output` from `dispatched` also set this state, cancel the timer, and move
DB state to `started` before recording execution-side effects. Owned
`build_finished(failure)`, `build_finished(revoked)`, and transient
`build_rejected` from `dispatched` set this state only long enough to cancel the
timer before applying their terminal or rollback transition.

This receipt state is live server-process state only. Design 019 does not change
startup recovery: after a server process restart there are no trusted websocket
connections and no trusted in-memory active assignments to recover. Startup
recovery continues to fail `dispatched` and `started` builds with a
server-restarted error, mark `revoking` builds `revoked`, finalize their logs,
and re-enqueue only rows that were already `queued`. If a worker reconnects
after server restart and reports `Building { build_id }` for one of those now
terminal rows, the normal unauthorized-building path applies:
`UnauthorizedBuildAction` followed by reporter-directed `BuildRevoke`.

### D5: Build descriptors must contain at least one valid component

REST build submission and periodic task creation/update reject descriptors whose
`components` array is empty. Every listed component must already be known to the
server.

This validation belongs in a single shared descriptor-validation module, not in
separate ad hoc route code. The validator deserializes input into
`BuildDescriptor` and applies the same checks for all ingress paths:

- `components` must be non-empty.
- every component name must be known to the server's component registry.
- repository scope checks remain tied to the typed component list.
- route handlers convert validation failures into route-appropriate `400`
  responses.

REST build submission, periodic task creation/update, and scheduler trigger
paths all use this helper. Periodic rows stored as raw JSON are deserialized
through the same typed validator before they can be accepted or triggered.

At scheduler trigger time, invalid stored periodic descriptors are permanent
task errors. If a legacy or corrupted row fails JSON parsing, the shared typed
validator, non-empty component validation, known-component validation, channel
resolution, or scope checks, the scheduler must:

- not enqueue a build
- log the validation failure with the periodic task ID
- return a fatal trigger error
- disable the periodic task and persist the validation message in `last_error`

Those rows are not retried with backoff because descriptor validity will not
repair itself without user or operator action. Re-enabling the task requires a
valid update through the normal periodic task update path.

The dispatch path still treats an empty component list as an invariant violation
and rolls back to `queued` if encountered from legacy or corrupted data.

### D6: Log output is accepted only for the assigned active build

`build_output` follows the same ownership rule as lifecycle messages. Output is
written only when the sending connection owns the active build.

The server also treats output as append-only data for an active assignment:

- output for unknown, inactive, terminal, or differently owned builds is
  rejected with `UnauthorizedBuildAction` followed by reporter-directed
  `BuildRevoke`
- output is not allowed to create logs for arbitrary build IDs
- output sequence numbers remain advisory for SSE resume behavior, not proof of
  authority

This keeps log provenance tied to the worker that received the build.

### D7: Tail responses must be memory-bounded

The JSON tail endpoint reduces its server-side line-count cap from 10,000 to
1,000 lines and no longer reads the full log into memory before applying it. The
`cbc logs tail` client keeps a line-count argument, defaults it to 50, and the
server clamps any request above 1,000 lines to 1,000.

Current behavior:

1. The handler authorizes access to the build.
2. It builds `{log_dir}/builds/{id}.log`.
3. It reads the entire file into a `String`.
4. It splits all lines into a `Vec<&str>`.
5. It slices the final `n` lines, where `n` is currently capped at 10,000.

The line-count request and the response's current `total_lines` field are
different concepts:

- Requesting "the last 50 lines" does not defeat the bounded-memory goal. The
  server can satisfy that by reading backwards from the end of the file.
- Returning an exact "this file contains N total lines" value does defeat the
  bounded-memory goal for large files, because exact counting requires scanning
  the whole file.

The new response therefore does not promise exact total file line counts. `cbc`
must stop requiring the current `total_lines` response field and instead render
`returned`, `requested`, and `truncated`.

The selected design is reverse block scanning with a fixed byte budget and
truncation:

- `MAX_TAIL_LINES = 1_000`.
- `MAX_TAIL_BYTES = 4 MiB`.
- The handler seeks from the end of the log file and reads backwards in
  fixed-size blocks until it has enough newline-delimited records for the
  requested line count or reaches `MAX_TAIL_BYTES`.
- The response returns the newest complete lines that fit in the budget.
- If the requested tail cannot be represented within the budget, the response
  succeeds with `"truncated": true` and includes only the suffix that fit.
- If scanning begins in the middle of a line, the partial leading line is
  dropped. The server returns only full newline-delimited lines.
- If a single line exceeds the byte budget, the endpoint cannot return a full
  line within budget. It returns no partial line, sets `"truncated": true`, and
  includes a warning/detail field indicating that the newest line exceeded the
  tail byte budget.
- If the retained byte window starts in the middle of a multibyte UTF-8 code
  point, the partial leading code point is dropped before decoding. The response
  contains valid UTF-8 only.
- `total_lines` is no longer exact for large files because exact counting would
  require scanning the whole log. The response omits exact `total_lines`.

The JSON response shape becomes:

```json
{
  "build_id": 123,
  "lines": ["..."],
  "returned": 1000,
  "requested": 1000,
  "truncated": false,
  "bytes_scanned": 1048576,
  "max_tail_bytes": 4194304,
  "detail": null
}
```

`truncated` means the endpoint returned the newest available suffix within the
memory budget, not the full requested tail. Clients that need complete logs use
the streaming full-log endpoint.

This decision affects only the JSON tail endpoint. Full-log download remains a
streaming endpoint.

## Worker-Side Active Build State

The server reconnect rules depend on the worker reporting local build state
truthfully. The worker therefore must not keep active build ownership only as a
local variable inside one websocket connection loop.

The worker owns build execution in a process-level active-build supervisor that
outlives websocket reconnects. The websocket loop is a transport client for that
supervisor, not the owner of the subprocess. The supervisor tracks at least:

- build ID
- local execution phase: `accepted`, `started`, `revoking`, or
  `terminal-pending-report`
- executor/process handle
- component working directory
- pending terminal result, if the process completed while disconnected
- a bounded local output spool for output produced while disconnected

Reconnect status is derived from the supervisor:

- If the supervisor has an active executor, an in-progress revoke, or a terminal
  result that has not been reported to the server, the worker sends
  `WorkerStatus { state: Building, build_id }` after `Welcome`.
- The worker sends `WorkerStatus { state: Idle, build_id: None }` only when the
  supervisor has no active executor, no in-progress revoke, and no pending
  terminal result.
- If the worker cannot determine whether an active subprocess still exists, it
  must stop and await any possible child process, clean up local active state,
  and only then report `Idle`.
- A websocket receive/send error does not by itself kill the build. The
  supervisor keeps the subprocess and local assignment state until the worker
  receives a `BuildRevoke`, the process exits, or local worker shutdown stops
  it.

When a disconnected build finishes before the worker has a usable websocket, the
supervisor keeps the terminal result as `terminal-pending-report`. On the next
successful websocket connection, the worker first reports
`WorkerStatus(building)` for that build, then sends the pending output and
`build_finished` result in order. This preserves the invariant that the worker
never reports idle while it still has unreported local state for a build.

Output produced while disconnected must not cause unbounded memory growth. The
worker uses a per-build local spool file, not an unbounded in-memory queue, for
output that cannot currently be sent to the server. The default spool budget is
64 MiB per active build. If the spool cannot be written or the budget is
exceeded, the worker kills and awaits the local build, records
`worker disconnected output spool exceeded` or the concrete I/O error as the
failure reason, and reports `build_finished(failure)` when it reconnects. The
worker must not continue an unbounded local build while reporting idle or
silently dropping all evidence of local execution.

`BuildRevoke` is the only stop-work command. `UnauthorizedBuildAction` is a
diagnostic rejection; when it is followed by `BuildRevoke`, the worker kills and
awaits the matching local build and reports `build_finished(revoked)` if the
connection remains usable. If the matching local state is
`terminal-pending-report`, the worker discards the pending terminal result,
clears local state, and reports `build_finished(revoked)` if the connection
remains usable. If the worker receives `UnauthorizedBuildAction` for the active
build without a following revoke, it logs an error and keeps its local state
unchanged unless a later server message or local shutdown changes that state.

## Worker Message Transition Matrix

Owned messages from the assigned worker are allowed to be idempotent where that
matches normal behavior. If the server would normally acknowledge a message with
success, it should still do so for a benign duplicate. Otherwise benign
duplicates are logged at debug or info level and ignored without a security
warning.

Unauthorized build-scoped messages are messages for an unknown, inactive,
terminal, or differently owned build. Those receive the non-fatal
`UnauthorizedBuildAction` response and are logged as security warnings. Invalid
`worker_status(building)`, `build_started`, and `build_output` claims
additionally receive `BuildRevoke { build_id }` after the unauthorized-action
response so the worker stops any stray local execution.

| Worker action             | Valid active/DB state                                                                                                                                                                                                               | Valid side effect                                                                                                                                                                                                                                                                                                                                                 | Benign duplicate / stale owned behavior                                                                                                                                       | Unauthorized behavior                                                                                                                            |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `worker_status(building)` | after reconnect migration, authenticated registered worker ID matches `builds.worker_id`; active entry exists for the same build and current connection; DB state is `dispatched`, `started`, or `revoking`                         | `dispatched`: mark `ReceivedByWorker`, cancel dispatch-ack timer, and implicit accept may move DB to `started`; `started`: resume on the current connection; `revoking`: keep state `revoking` and send the normal stateful revoke to the current assigned connection                                                                                             | duplicate status from the same assigned worker is success/no-op or resume; duplicate while `revoking` may re-send the normal revoke                                           | log security warning; send `UnauthorizedBuildAction` and then reporter-directed `BuildRevoke` for that `build_id`; do not rewrite `queue.active` |
| `worker_status(idle)`     | reconciliation is limited to active DB rows whose persisted `builds.worker_id` matches the authenticated registered worker ID; stale active entries whose DB row is queued, terminal, or missing are cleaned up without DB mutation | for that same worker's own assignment only: `dispatched` + `AwaitingReceipt` rolls back to `queued` only when the previous same-worker connection was absent, disconnected, or dead; `dispatched` + `ReceivedByWorker` rolls back to `queued` even if the previous same-worker connection was live; `started` is marked `failure`; `revoking` is marked `revoked` | duplicate idle for a worker with no owned active assignment is success/no-op; idle after superseding a live same-worker connection is logged before authoritative resolution  | must not requeue, fail, revoke, or otherwise mutate builds owned by other workers                                                                |
| `build_accepted`          | active entry exists for sender; DB `dispatched`                                                                                                                                                                                     | mark `ReceivedByWorker`; cancel dispatch-ack timer; build remains assigned                                                                                                                                                                                                                                                                                        | duplicate accept for same assigned build is success/no-op                                                                                                                     | reply unauthorized; no timer/state mutation                                                                                                      |
| `build_started`           | active entry exists for sender; DB `dispatched` or `started`                                                                                                                                                                        | mark `ReceivedByWorker`; cancel dispatch-ack timer if still armed; set DB state to `started` when not already started                                                                                                                                                                                                                                             | duplicate start while already `started` is success/no-op                                                                                                                      | reply unauthorized and send reporter-directed `BuildRevoke`; no DB mutation                                                                      |
| `build_output`            | active entry exists for sender; DB `dispatched`, `started`, or `revoking`                                                                                                                                                           | cancel dispatch-ack timer if still armed; if DB is `dispatched`, mark `ReceivedByWorker` and move DB to `started`; append output and update log size                                                                                                                                                                                                              | duplicate sequence numbers are accepted as append-only output unless a future log protocol adds stronger sequencing                                                           | reply unauthorized and send reporter-directed `BuildRevoke`; no file or DB log mutation                                                          |
| `build_finished(success)` | active entry exists for sender; DB `started` or `revoking`                                                                                                                                                                          | cancel dispatch-ack timer if still armed; set terminal `success`; store report; finalize log; remove active entry/watcher                                                                                                                                                                                                                                         | duplicate after terminal state is ignored as stale owned completion if ownership can still be established from the active entry; otherwise terminal/inactive handling applies | reply unauthorized; no DB mutation                                                                                                               |
| `build_finished(failure)` | active entry exists for sender; DB `dispatched`, `started`, or `revoking`                                                                                                                                                           | cancel dispatch-ack timer if still armed; set terminal `failure`; finalize log; remove active entry/watcher                                                                                                                                                                                                                                                       | duplicate after terminal state is ignored as stale owned completion if ownership can still be established from the active entry; otherwise terminal/inactive handling applies | reply unauthorized; no DB mutation                                                                                                               |
| `build_finished(revoked)` | active entry exists for sender; DB `dispatched`, `started`, or `revoking`                                                                                                                                                           | cancel dispatch-ack timer if still armed; set terminal `revoked`; finalize log; remove active entry/watcher                                                                                                                                                                                                                                                       | duplicate after terminal state is ignored as stale owned completion if ownership can still be established from the active entry; otherwise terminal/inactive handling applies | reply unauthorized; no DB mutation                                                                                                               |
| `build_rejected`          | active entry exists for sender; DB `dispatched` and build not accepted/started                                                                                                                                                      | cancel dispatch-ack timer if still armed; integrity rejection fails the build; transient rejection rolls back to `queued` using the rollback cleanup operation and removes active entry/watcher                                                                                                                                                                   | duplicate reject after rollback/terminal state is stale and ignored only if still attributable to the same assignment; otherwise terminal/inactive handling applies           | reply unauthorized; no DB or queue mutation                                                                                                      |

Terminal or inactive messages usually cannot prove current ownership because the
active entry has already been removed. They therefore follow the unauthorized
response path unless the implementation preserves enough recently completed
assignment context to classify the message as a benign stale duplicate. This
design does not require such a recent-completion cache.

## Reconnect Ownership

`WorkerMessage::WorkerStatus` is part of the same trust boundary as lifecycle
and output messages. A reconnecting worker may report that it is still building
a `build_id`, but that report is only a claim until the server validates it.

### Same-worker connection migration

The server permits at most one active websocket connection per registered worker
ID. When a new websocket authenticates as a registered worker ID that already
has a connection entry, the newest authenticated connection wins.

Same-worker migration happens during the authenticated handshake, before the
server processes `worker_status` or any other worker build message from the new
connection:

1. The server identifies the old connection, if any, for the same registered
   worker ID.
2. The server records whether the old connection was live, disconnected, dead,
   or absent. This recorded state is later used by idle reconnect
   reconciliation.
3. Under the same synchronization boundary used for `queue.workers` and
   `queue.active`, the server replaces the worker entry with the new connection.
4. The server snapshots candidate active build IDs that are currently assigned
   to the old connection, then releases the queue lock before consulting the
   database.
5. The server queries persisted build assignment state for those candidates. The
   authoritative ownership check is
   `builds.worker_id == authenticated_registered_worker_id`, and the persisted
   DB state must still be one of `dispatched`, `started`, or `revoking`.
6. The server reacquires the queue lock and migrates only active entries that
   are still assigned to the old connection and whose persisted `worker_id` and
   DB state matched the authenticated registered worker ID and an active DB
   state. Entries that changed connection are left untouched. Entries whose DB
   row is no longer active (`queued`, terminal, or missing) are stale active
   entries: the server logs a warning, removes the active entry, removes any log
   watcher, and does not mutate DB state or enqueue another copy.
7. The old connection's outbound sender is removed. Any later messages from the
   superseded connection cannot pass active-build authorization and its eventual
   cleanup must not requeue, fail, or revoke active entries now owned by the new
   connection.

This migration is the only reconnect path that may replace an active entry's
`connection_id`. After migration, all ordinary build-scoped messages, including
`worker_status(building)`, use the normal active-entry `connection_id` check.
The implementation must not hold the queue lock across DB I/O for the
persisted-owner and active-state checks.

A `worker_status(building)` resume is accepted only when:

- the websocket is authenticated as a registered worker
- the authenticated registered worker ID matches `builds.worker_id`
- `queue.active[build_id]` exists
- after same-worker migration, the active entry is assigned to the current
  connection
- the DB state is `dispatched`, `started`, or `revoking`

Accepted `worker_status(building)` behavior by DB state:

- `dispatched`: cancel the dispatch-ack timer and treat the report as proof that
  the worker received the assignment. The server may implicitly accept and move
  DB state to `started`.
- `started`: resume on the current connection without changing build result
  state.
- `revoking`: keep DB state `revoking` and send the normal stateful revoke to
  the current assigned connection. This is not the reporter-directed stray
  revoke path.

If a worker reports `Building { build_id }` for a build that is not a valid
assignment for the reporting worker, the server logs a security warning, sends
`UnauthorizedBuildAction`, then sends `BuildRevoke { build_id }` to the
reporting worker. This applies to unknown/not-found builds, queued or otherwise
not-yet-assigned builds, terminal builds, inactive active entries, wrong
connection IDs, wrong registered worker IDs, and builds assigned to another
worker. The revoke is sent directly to the reporting connection and has no DB,
timer, watcher, or active-queue side effects. The active assignment is not moved
to the reporting connection.

This handles the plausible race where a worker lost connectivity, started a
build, and the server later dispatched the build elsewhere: the server keeps the
assignment that is recorded in its state, tells the unauthorized reporter that
the reported building status is not assigned to it, and asks it to stop the
stray build.

The warning must include enough structured data to validate the race claim:

- authenticated registered worker ID and name
- reporting connection ID
- reported build ID
- persisted `builds.worker_id`
- whether `queue.active` had an entry for the build
- current DB state

The server response must not disclose the other worker's identity to the
reporting worker.

The same reporter-directed stop-work rule applies when a worker sends
unauthorized `build_started` or `build_output`. Those messages prove that local
execution has started or is producing output, so the server must tell the
reporting connection to stop the local build even though the server ignores the
state or output mutation. This stop command has the same no-side-effect
requirements as the invalid `worker_status(building)` revoke.

### Idle reconnect

An idle reconnect is also worker-originated lifecycle input. It cannot be used
as a general signal that other workers' active builds are stale.

`worker_status(idle)` reconciliation is limited to active assignments whose
persisted `builds.worker_id` matches the authenticated registered worker ID and
whose DB state is `dispatched`, `started`, or `revoking`. It must not requeue,
fail, revoke, or otherwise mutate builds assigned to other registered workers.
Those decisions remain owned by that worker's own reconnect/status messages,
explicit revoke handling, or the liveness grace-period monitor.

Idle reconciliation discovers candidates using the same two-phase lock/DB
pattern as same-worker migration:

1. Under the queue lock, snapshot all `queue.active` build IDs and connection
   IDs. This covers live, disconnected, dead, absent, and already-migrated old
   connection states because discovery does not depend on a worker map entry for
   the old connection.
2. Release the queue lock and query the DB rows for those build IDs.
3. Keep only rows whose `builds.worker_id` equals the authenticated registered
   worker ID and whose state is `dispatched`, `started`, or `revoking`.
4. Reacquire the queue lock and apply the idle-resolution table only to entries
   that are still active and whose DB row is still in the same active state. If
   the DB row became `queued`, terminal, or missing, remove the stale active
   entry and watcher, log the mismatch, and do not mutate DB state.

If the same worker reconnects as idle while the server still records an active
assignment for that worker, the server uses the previous connection state
recorded during same-worker migration:

- `dispatched`: the assignment may roll back to `queued` only when the previous
  same-worker connection was absent, disconnected, or dead before this reconnect
  and the active assignment receipt is `AwaitingReceipt`. If the receipt is
  `ReceivedByWorker`, the server rolls the build back to `queued` even when the
  previous same-worker connection was live, because the newest authenticated
  connection is authoritative and the build has not reached `started`.
  Superseding a live same-worker connection in this path is logged as an
  operational/security warning.
- `started`: the server treats the new connection's idle status as authoritative
  local-state loss for that registered worker and marks the build `failure` with
  a clear reason such as `worker reported idle after reconnect`. If the previous
  same-worker connection was live, the server also logs an operational/security
  warning that a live same-worker connection was superseded before failure
  resolution.
- `revoking`: the server treats the new connection's idle status as confirmation
  that the worker no longer has local work to stop and marks the build
  `revoked`. If the previous same-worker connection was live, the server logs
  the same superseded-live-connection warning before terminal resolution.

These `started` and `revoking` idle resolutions remove the active entry, finish
the log, and remove the log watcher. They do not send a reporter-directed stray
revoke because the reporting connection is the authenticated owner after
same-worker migration.

If an unrelated worker reconnects idle while another worker is disconnected or
dead, the idle report does not accelerate requeue/failure for the other worker's
builds. The liveness monitor remains responsible for those transitions.

## Authorization Model

The websocket upgrade authenticates worker identity. Active-build ownership
authorizes build-scoped actions.

Authentication answers:

> Is this a registered worker connection?

Active-build authorization answers:

> Is this specific worker connection currently assigned to this specific build?

Both answers are required. A valid worker API key is not a cluster-wide build
mutation capability.

## State Invariants

The following invariants define the corrected behavior:

1. A build ID can appear in `queue.active` at most once.
2. A worker build message can mutate state only through the active entry that
   names the sending `connection_id`.
3. No worker message can move a build owned by another connection.
4. No worker message can append logs for a build owned by another connection.
5. A dispatch attempt that fails before delivery leaves no active entry and no
   stale watcher.
6. A dispatch rollback clears persisted `worker_id`, `trace_id`, `error`,
   `started_at`, `finished_at`, and `build_report`.
7. A reconnecting worker cannot take over an active build unless its
   authenticated registered worker ID matches the persisted build assignment.
8. An idle reconnect cannot mutate builds assigned to other registered workers.
9. Worker-facing unauthorized reasons do not reveal build existence or
   assignment state.
10. A redispatched build receives a new `trace_id`.
11. Build submission and periodic tasks accept only descriptors with at least
    one known component.
12. Log tail memory use is bounded to 4 MiB, independently of total log size.
13. Any build in `queued` state is unassigned and has no stale assignment
    provenance in `worker_id` or `trace_id`.
14. Dispatch-ack timers are delivery timers only; any valid owned message that
    proves worker receipt or execution cancels the timer before later state
    mutation.
15. There is at most one active websocket connection per registered worker ID;
    newest authenticated same-worker reconnect wins.
16. Same-worker reconnect migration is the only reconnect path that may replace
    an active entry's `connection_id`, and superseded connection cleanup cannot
    mutate active entries migrated to the new connection.
17. Idle reconnect cannot roll back a live-superseded `dispatched` assignment
    while its receipt state is `AwaitingReceipt`; once receipt state is
    `ReceivedByWorker`, idle reconnect rolls it back to `queued` because the
    build has not reached `started`.
18. Same-worker reconnect migration proves ownership from persisted
    `builds.worker_id` and active DB state without holding the queue lock across
    DB I/O.
19. Same-worker idle reconnect resolves `started` to `failure` and `revoking` to
    `revoked`, so migrated active assignments cannot remain orphaned on an idle
    connection.
20. Invalid stored periodic descriptors are fatal scheduler trigger errors: they
    do not enqueue builds or retry with backoff, and they disable the task with
    `last_error`.
21. Active assignments distinguish `AwaitingReceipt` from `ReceivedByWorker` so
    acknowledged-but-not-started `dispatched` assignments cannot lose their
    resolver after the dispatch-ack timer is canceled.
22. Idle reconciliation discovers candidate assignments from `queue.active` plus
    persisted DB rows; it never depends on the old worker connection still being
    present in `queue.workers`.
23. Worker active-build state outlives websocket connections. A worker cannot
    report idle while it has an active executor, in-progress revoke, or pending
    terminal result for a build.
24. Unauthorized local-execution evidence (`worker_status(building)`,
    `build_started`, or `build_output`) receives a reporter-directed stop-work
    revoke in addition to `UnauthorizedBuildAction`.
25. Server startup recovery is unchanged by `ActiveAssignmentReceipt`: after
    process restart, in-flight builds are failed or revoked rather than
    reconstructed from stale in-memory receipt state.

## Observability

Unauthorized worker build messages are security-relevant, not ordinary protocol
noise. They should be logged at warning level with structured fields, using a
message that can be filtered by operators.

Worker-facing unauthorized reasons are coarse (`NotAssigned`). Server logs keep
detailed internal reason fields so operators can diagnose whether the root cause
was an unknown build, inactive build, wrong connection, wrong worker identity,
invalid DB state, or reconnect race.

Recommended server log fields:

- `event = "worker_unauthorized_build_action"`
- `worker_id`
- `worker_name`
- `connection_id`
- `build_id`
- `action`
- `reason` (coarse worker-facing reason)
- `internal_reason` (server-only detail)
- for reconnect mismatches: `assigned_worker_id`, `reported_worker_id`,
  `db_state`, and whether an active queue entry existed

Recommended worker log fields:

- `event = "server_rejected_worker_action"`
- `build_id`
- `action`
- `reason` (coarse worker-facing reason)

The worker log message is an error because the worker attempted an action the
server rejected as unauthorized.

## Test Expectations

The design requires test coverage for behavior, not just helper functions:

- two connected workers where worker B attempts to send each build-scoped
  message for worker A's active build
- unauthorized output does not create or append a log file
- unauthorized lifecycle messages do not alter DB state or active queue state
- unauthorized messages produce the non-fatal server response
- reconnect `worker_status(building)` for another worker's build does not move
  active ownership and sends `UnauthorizedBuildAction` followed by revoke to the
  reporting worker
- reconnect `worker_status(building)` for unknown/not-found, queued, terminal,
  wrong-worker, and wrong-connection builds uses the same unauthorized-then-
  revoke response contract
- reporter-directed reconnect revokes do not set DB state to `revoking`, cancel
  the real assignment's timers, remove watchers, or rewrite `queue.active`
- unauthorized `build_started` and `build_output` for unknown, queued, terminal,
  wrong-worker, wrong-connection, or reassigned builds receive
  `UnauthorizedBuildAction` followed by reporter-directed `BuildRevoke`
- reporter-directed revokes for unauthorized `build_started` and `build_output`
  do not mutate DB state, active ownership, timers, watchers, or log files
- same-worker reconnect migration happens during handshake before
  `worker_status` handling, moves only assignments for the authenticated
  registered worker ID using DB-backed persisted `builds.worker_id` and active
  DB state checks, avoids holding the queue lock across DB I/O, and leaves
  ordinary messages subject to the current `connection_id` ownership check
- same-worker reconnect migration does not migrate active entries whose
  persisted `worker_id` differs from the authenticated registered worker ID or
  whose DB state is queued, terminal, or missing
- stale active entries found during same-worker migration are removed from
  `queue.active` and `log_watchers` without mutating DB state or enqueueing
  another copy
- cleanup of a superseded same-worker connection cannot requeue, fail, revoke,
  or otherwise mutate active entries migrated to the newer connection
- `build_accepted` marks the active assignment `ReceivedByWorker` and cancels
  the dispatch-ack timer while DB remains `dispatched`
- owned `build_output` from `dispatched` marks the assignment
  `ReceivedByWorker`, cancels the dispatch-ack timer, and moves DB state to
  `started` before appending output
- `build_started` from `dispatched` marks the active assignment
  `ReceivedByWorker` and cancels the dispatch-ack timer before the timer can
  requeue the started build
- reconnect `worker_status(building)` that resumes or implicitly starts a
  `dispatched` assignment cancels the dispatch-ack timer
- reconnect `worker_status(building)` for an owned `revoking` assignment keeps
  DB state `revoking` and sends the normal stateful revoke to the current
  assigned connection
- valid owned `build_output`, `build_finished(failure)`,
  `build_finished(revoked)`, and transient `build_rejected` cancel the
  dispatch-ack timer if it is still armed
- reconnect `worker_status(idle)` cannot requeue, fail, revoke, or otherwise
  mutate another worker's active build
- reconnect `worker_status(idle)` can only reconcile assignments whose persisted
  `builds.worker_id` matches the authenticated registered worker ID
- reconnect `worker_status(idle)` discovers candidates by snapshotting
  `queue.active`, querying DB rows outside the queue lock, and reconciling only
  still-active rows in `dispatched`, `started`, or `revoking`
- reconnect `worker_status(idle)` rolls back `dispatched` + `AwaitingReceipt`
  only when the previous same-worker connection was absent, disconnected, or
  dead before this reconnect
- reconnect `worker_status(idle)` rolls back `dispatched` + `ReceivedByWorker`
  even after superseding a live same-worker connection
- stale active entries found during idle reconciliation are removed from
  `queue.active` and `log_watchers` without mutating DB state
- reconnect `worker_status(idle)` for an owned `started` assignment marks the
  build `failure`, finishes the log, removes the active entry, and removes the
  log watcher
- reconnect `worker_status(idle)` for an owned `revoking` assignment marks the
  build `revoked`, finishes the log, removes the active entry, and removes the
  log watcher
- reconnect `worker_status(idle)` from a worker with no owned active assignment
  is success/no-op and does not emit `UnauthorizedBuildAction`
- worker active-build state is owned outside the websocket connection loop
- worker reconnect reports `Building { build_id }` when a local executor,
  in-progress revoke, or pending terminal result exists
- worker reconnect reports `Idle` only after there is no active executor,
  in-progress revoke, or pending terminal result
- worker kills and awaits any uncertain local child process before reporting
  `Idle`
- worker preserves pending terminal results across reconnect and reports
  `WorkerStatus(building)` before sending the pending `build_finished`
- disconnected output buffering is bounded; overflow stops the local build and
  produces a failure result rather than unbounded memory growth or silent idle
- disconnected output uses a per-build local spool with the 64 MiB default
  budget; spool I/O failure or budget overflow kills the build and produces
  `build_finished(failure)`
- worker receiving reporter-directed `BuildRevoke` after
  `UnauthorizedBuildAction` kills the matching local build and reports revoked
  if the connection remains usable
- worker receiving reporter-directed `BuildRevoke` for `terminal-pending-report`
  discards the pending terminal result, clears local state, and reports revoked
  if the connection remains usable
- worker-facing unauthorized responses map authorization failures to
  `NotAssigned` while server logs include detailed internal reasons
- component-less REST build submissions are rejected
- component-less periodic descriptors are rejected
- invalid stored periodic descriptors encountered by the scheduler are fatal: no
  build is enqueued, the failure is logged, the task is disabled, and
  `last_error` is persisted
- post-DB dispatch tarball failure rolls the build back to `queued`
- post-DB dispatch rollback clears persisted `worker_id`, `trace_id`, and
  defensive stale columns
- dispatch-ack timeout, transient `build_rejected`, and same-worker idle
  rollback from `dispatched` use the same queued rollback cleanup operation
- startup recovery still fails `dispatched` and `started` builds, marks
  `revoking` builds revoked, finalizes logs, and does not reconstruct
  `ActiveAssignmentReceipt`
- worker reconnect after server startup recovery for a now-terminal build gets
  `UnauthorizedBuildAction` followed by reporter-directed `BuildRevoke`
- REST, periodic create/update, and scheduler trigger paths use the same typed
  descriptor validator
- JSON tail handles large logs without reading the full file into memory
- JSON tail caps requests at 1,000 lines and reports truncation when the newest
  suffix exceeds 4 MiB
- JSON tail returns only full valid UTF-8 lines, dropping partial leading lines
  and partial leading UTF-8 code points
- `cbc logs tail` defaults to 50 requested lines, tolerates the new response
  shape, and no longer requires exact `total_lines`

## Open Questions

None. The policy choices through v11 are resolved in this design.
