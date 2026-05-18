# Review: Worker Control Plane Hardening — Design Soundness

| Field    | Value                                                       |
| -------- | ----------------------------------------------------------- |
| Seq      | 019                                                         |
| Reviewed | `019-20260426T1154-worker-control-plane-hardening.md` (v11) |
| Against  | `019-20260514T1040-security-audit-remediation.md` (v8)      |
| Date     | 2026-05-16                                                  |
| Type     | Adversarial design soundness                                |
| Version  | v1                                                          |

## Mandate

Validate every WCP code claim against actual source files with exact line
citations. Check internal consistency across WCP decisions, invariants, and the
test list. Check cross-design conflicts between WCP v11 and audit-remediation
v8. Catalog unsound items that are not already remediated by audit-remediation
v8 as gaps for a future audit-remediation v9. Apply confidence scoring.

Audit-remediation v8 is treated as a reference for "already remediated" items.
Critique is directed at WCP v11 only.

## Methodology

Every WCP behavioral claim was verified by reading the source files listed
below. Code line numbers cited are from the current working tree on branch
`wip/cbsd-rs-security-review`.

Source files read in full:

- `cbsd-server/src/ws/handler.rs` (1005 lines)
- `cbsd-server/src/ws/dispatch.rs` (645 lines)
- `cbsd-server/src/ws/liveness.rs` (149 lines)
- `cbsd-server/src/queue/mod.rs` (343 lines)
- `cbsd-server/src/db/builds.rs` (371 lines)
- `cbsd-server/src/routes/builds.rs` (604 lines)
- `cbsd-server/src/logs/writer.rs` (173 lines)
- `cbsd-proto/src/ws.rs` (353 lines)
- `cbsd-worker/src/ws/handler.rs` (555 lines)
- `cbsd-worker/src/ws/connection.rs` (292 lines)

---

## Part I: Code-Claim Verification

### D1 — Build-scoped authorization

WCP claims all six build-scoped message types (worker_status(building),
build_accepted, build_started, build_output, build_finished, build_rejected) are
authorization-checked before any state mutation.

**Verified: partially false.**

- `handle_build_started` (`dispatch.rs:305–323`) accepts no `connection_id`
  parameter and performs no ownership check. The call site in
  `handler.rs:519–520` passes `build_id` only:
  `handle_build_started(state, build_id.0)`. Any worker that knows a valid
  `build_id` can advance the DB from `dispatched` to `started`.
- `handle_build_finished` (`dispatch.rs:333–384`) receives `connection_id` as a
  parameter but contains no assertion that the active entry's `connection_id`
  matches the caller. The check is absent entirely.
- `write_build_output` (`logs/writer.rs:62–100`) takes a `build_id` only; the
  call in `handler.rs:534–549` does not pass `connection_id`. Any authenticated
  worker can append output to any active build.
- `handle_build_accepted` (`dispatch.rs:283–299`) does not verify that the
  accepting connection owns the active entry.

The WCP D1 authorization invariant is stated in the design as implemented
behavior. It is not implemented.

**Gap status:** Wholly unimplemented. Not covered by audit-remediation v8 (which
adds new decisions D11–D13 on top of WCP and presumes D1 is in force). This is a
gap for v9.

### D2 — UnauthorizedBuildAction wire response

WCP claims unauthorized messages receive `UnauthorizedBuildAction`.

**Verified: message type does not exist in `cbsd-proto`.**

`cbsd-proto/src/ws.rs` (lines 1–353, `ServerMessage` enum): the variants are
`BuildNew`, `BuildRevoke`, `Welcome`, `Error`. There is no
`UnauthorizedBuildAction` variant, no `WorkerBuildAction` enum, and no
`UnauthorizedBuildReason` enum. The protocol described in WCP D3 is
forward-looking specification, not current code.

The `ServerMessage::Error` path is used instead (handler.rs line 901 area) for
connection-level errors, which WCP explicitly says must not be used for
build-scoped authorization failures.

**Gap status:** Wire type entirely absent. Not remediated by v8. Gap for v9.

### D3 — Protocol extension (UnauthorizedBuildAction variant)

Same as D2. WCP D3 specifies the wire type addition as a design decision that
must be implemented. The `cbsd-proto/src/ws.rs` file contains no trace of it.
The design correctly marks this as forward-looking ("the protocol gains…"), so
this is an accurate statement of intent, not a false current-state claim — but
it confirms that the D3 implementation is zero-percent complete.

**Gap status:** Zero implementation. Gap for v9.

### D4 — Dispatch rollback

WCP claims rollback clears: state → queued, worker_id → NULL, trace_id → NULL,
error → NULL, started_at → NULL, finished_at → NULL, build_report → NULL, plus
removes the active entry and log watcher.

**Verified: incomplete.**

`db/builds.rs` provides only `update_build_state(pool, id, new_state, error)`
(line 221–240), which updates `state` and `error` via `COALESCE(?, error)`. No
function clears `worker_id`, `trace_id`, `started_at`, `finished_at`, or
`build_report`.

`requeue_active_build` in `handler.rs` (lines 877–908) calls
`update_build_state(..., "queued", None)`. This leaves stale `worker_id` and
`trace_id` in the DB row, violating WCP SI-6 and SI-13.

The dispatch-ack timeout path in `dispatch.rs` (lines 255–270) also calls
`update_build_state("queued", ...)` directly, producing the same incomplete
rollback.

The dedicated rollback DB operation WCP D4 requires ("not the generic
state-update helper") does not exist.

**Gap status:** Not in code; not remediated by v8 (which presumes the rollback
semantics of D4 are in force for its D12 table). Gap for v9.

### D5 — Descriptor validation (empty components)

WCP claims REST submission rejects empty component lists before build insertion.

**Verified: false.**

`routes/builds.rs` line 89–97 iterates `body.descriptor.components` but checks
only whether each listed component name is known to the registry. If
`components` is empty, the loop body never executes and the build proceeds to
insertion. There is no explicit guard returning HTTP 400 for an empty list.

The WCP D5 claim ("REST build submission and periodic task creation/update
reject descriptors whose `components` array is empty") is not implemented.

**Gap status:** Not in code. Gap for v9.

### D6 — Log output ownership check

WCP claims `build_output` is rejected unless the sending connection owns the
active build.

**Verified: false.**

`logs/writer.rs` `write_build_output` (lines 62–100) is keyed on `build_id` only
(path: `builds_dir.join(format!("{build_id}.log"))`). The call in
`handler.rs:534–549` does not pass `connection_id`. Any authenticated worker can
append output to any `build_id`.

**Gap status:** Not implemented. Gap for v9.

### D7 — Bounded log tail

WCP states:

- MAX_TAIL_LINES changed from 10,000 to 1,000.
- MAX_TAIL_BYTES = 4 MiB.
- Implementation uses reverse block scanning (not full file read).
- Client default changed to 50 lines.

**Verified: partially false.**

`routes/builds.rs:437`: `const MAX_TAIL_LINES: u32 = 10_000;` — the cap is still
10,000, not 1,000. WCP D7 states the reduction to 1,000 as the implemented
decision; it has not been applied.

`routes/builds.rs:432`: `fn default_tail_n() -> u32 { 30 }` — default is 30, not
50 as WCP D7 specifies.

`routes/builds.rs:475–505`: the handler calls
`tokio::fs::read_to_string(path).await?` then `.lines().collect()`. This is a
full file read into memory before slicing. The reverse block scanning described
in WCP D7 is not implemented.

Three of the four D7 code claims are false. The one accurate claim is that
MAX_TAIL_BYTES is not yet present (it cannot be, given no reverse scan exists).

**Gap status:** Not implemented. Gap for v9.

---

## Part II: Internal Consistency

### WCP D4 vs. SI-6/SI-13 alignment

SI-6 requires rollback to clear persisted `worker_id`, `trace_id`, etc. SI-13
requires any build in `queued` state to be unassigned with no stale assignment
provenance. D4 specifies the dedicated rollback operation. All three are
internally consistent with each other — the problem is that none of the three
are implemented in code (see D4 finding above).

Internal consistency: **pass** (the design is self-consistent; implementation
diverges).

### WCP D1 vs. SI-2/SI-3/SI-4 alignment

SI-2 ("only through the active entry that names the sending connection_id"),
SI-3, SI-4 directly derive from D1. No inconsistencies between design sections.
Implementation diverges from all four.

### WCP D4 dispatch-ack timer cancellation scope

WCP lists which events cancel the dispatch-ack timer (D4, near the
`ActiveAssignmentReceipt` section). The list includes `build_output`,
`build_finished(failure/revoked)`, `transient build_rejected`, and
`accepted reconnect worker_status(building)`. The transition matrix
(end-of-design table) repeats this consistently. No internal contradictions
found.

### WCP idle-reconnect table vs. SI-22 candidate discovery

SI-22 states idle reconciliation "discovers candidate assignments from
`queue.active` plus persisted DB rows; it never depends on the old worker
connection still being present in `queue.workers`." The idle reconnect section
(page 7 of the design, "Idle reconnect" subsection) specifies a four-step
lock/DB/re-lock pattern consistent with SI-22. The design is internally
consistent on this point.

### WCP worker-supervisor model vs. current code

The Worker-Side Active Build State section (design page 9) specifies that the
supervisor outlives the websocket connection loop and holds the subprocess
handle, spool, and terminal result.

`cbsd-worker/src/ws/handler.rs:154`:
`let mut active_build: Option<ActiveBuild> = None;` — the active build is a
local variable inside `run_connection`. It does not outlive the websocket
connection loop.

`cbsd-worker/src/ws/handler.rs:136–148`: a TODO comment explicitly acknowledges
this and confirms the worker ALWAYS sends `WorkerStatus { state: Idle }` on
reconnect regardless of local state. This directly contradicts the supervisor
model described in WCP.

`ActiveBuild` struct (`worker/src/ws/handler.rs:35–39`):
`{ build_id, executor, component_dir }` — no `phase` field, no
`terminal-pending-report` slot, no output spool pointer. The struct does not
reflect the supervisor model.

**Gap status:** Worker supervisor model is design-only. Not remediated by v8
(D12 and D13 presume supervisor existence for their correct operation). Gap for
v9.

### WCP `dispatched`+idle+`AwaitingReceipt`+live-previous-connection behavior (SI-17 vs. idle table)

SI-17: "Idle reconnect cannot roll back a live-superseded `dispatched`
assignment while its receipt state is `AwaitingReceipt`."

Idle reconnect section (line 730–737 of WCP): "the assignment may roll back to
`queued` only when the previous same-worker connection was absent, disconnected,
or dead before this reconnect and the active assignment receipt is
`AwaitingReceipt`."

These are consistent: `AwaitingReceipt` + live previous = blocked rollback. No
contradiction.

---

## Part III: Cross-Design Conflicts (WCP v11 vs. Audit-Remediation v8)

### Conflict C1 — D13 step order vs. WCP migration step 7

Audit-remediation D13 (lines 1097–1109) specifies that the server sends
`BuildRevoke` to the old connection **before** removing its outbound sender, and
that the revoke happens synchronously during migration step 2 (before the WCP
migration steps run).

WCP v11 migration step 7 reads: "The old connection's outbound sender is
removed."

Reading both together: D13 inserts a send step before WCP step 3 (the
queue-lock-guarded swap), which means it occurs before step 7 (sender removal).
The WCP migration is extended, not contradicted. No genuine conflict.

### Conflict C2 — terminal-pending-report revoke semantics

WCP v11 "Worker-Side Active Build State" (near the `BuildRevoke` paragraph): "If
the matching local state is `terminal-pending-report`, the worker discards the
pending terminal result, clears local state, and reports
`build_finished(revoked)` if the connection remains usable." (Generic
`BuildRevoke` rule: discard.)

Audit-remediation D13 (lines 1138–1153): For a migration-context
`BuildRevoke { reason: MigrationSupersede }`, the supervisor "drains the pending
terminal result first, then treats the migration revoke as a no-op for that
build" — reporting the real outcome on the new connection. This deviation from
WCP is "scoped specifically to migration-driven revokes."

These are not in conflict: D13 creates a named carve-out, not a contradiction.
The WCP generic rule remains authoritative for admin and `UnauthorizedAction`
revokes. The `BuildRevokeReason` enum distinguishes the cases at the wire level
(D13 lines 1165–1177).

**Verdict: No conflict. D13 extends WCP with an explicit, scoped deviation.**

### Conflict C3 — D12 receipt-state liveness vs. WCP server-restart invariant

WCP SI-25: "Server startup recovery is unchanged by `ActiveAssignmentReceipt`:
after process restart, in-flight builds are failed or revoked rather than
reconstructed."

Audit-remediation D12 (lines 1057–1065): "The receipt state lives in process
memory only (per WCP v11 invariant 25). After a server restart, no
`ReceivedByWorker` rows exist; startup recovery uses the existing fail-in-flight
policy."

Consistent. D12 explicitly defers to SI-25.

**Verdict: No conflict.**

### Conflict C4 — D11 accepted-phase inclusion in `worker_status(building)` reconnect

WCP v11 reconnect section (lines 644–651): accepted `worker_status(building)`
valid states are "DB state is `dispatched`, `started`, or `revoking`." The
`accepted` in-memory phase maps onto `dispatched` DB state.

Audit-remediation D11 (title: "accepted phase included in reconnect-Building
rule"): confirms the accepted phase is handled via the `dispatched` DB state row
in the WCP Building reconnect table. Adds the explicit test for
`dispatched + AwaitingReceipt` reconnect- Building.

No conflict. D11 clarifies WCP's implicit coverage without overriding it.

---

## Part IV: Gap Catalog (Unimplemented; not remediated by v8)

The following gaps require implementation work before WCP semantics take effect.
They are candidates for a future audit-remediation v9 design that maps them to
implementation tasks.

| Gap | WCP Reference         | Description                                               |
| --- | --------------------- | --------------------------------------------------------- | ----------------------------------- |
| G1  | D1, D2, D3, SI-2–4    | Build-scoped authorization not implemented for any        |
|     |                       | lifecycle message type; `UnauthorizedBuildAction` wire    |
|     |                       | type absent from `cbsd-proto`                             |
| G2  | D1, D6, SI-4          | `build_output` ownership check missing; any worker can    |
|     |                       | append to any `build_id`                                  |
| G3  | D4, SI-6, SI-13       | Dedicated rollback DB operation does not exist;           |
|     |                       | `update_build_state("queued")` leaves stale `worker_id`   |
|     |                       | and `trace_id` in every rollback path                     |
| G4  | D5                    | Empty `components` array not rejected by REST submission  |
|     |                       | or periodic task paths                                    |
| G5  | D7                    | MAX_TAIL_LINES still 10,000 (not 1,000); default still    |
|     |                       | 30 (not 50); full file read still used (no reverse        |
|     |                       | block scan); MAX_TAIL_BYTES guard absent                  |
| G6  | Worker supervisor,    | Worker active-build state lives inside the WS connection  |
|     | SI-23, SI-24          | loop (local variable); TODO confirms always-Idle on       |
|     |                       | reconnect; supervisor model entirely unimplemented        |
| G7  | D4 migration, SI-18   | Same-worker migration is blind: no DB-backed two-phase    |
|     |                       | ownership check (handler.rs:265–308 iterates and          |
|     |                       | rewrites `connection_id` without consulting               |
|     |                       | `builds.worker_id` or verifying active DB state)          |
| G8  | D1, SI-8, idle table  | `worker_status(idle)` handler (handler.rs:717–766)        |
|     |                       | filters by `ab.connection_id != connection_id` with       |
|     |                       | `Disconnected                                             | Dead` state — any worker can affect |
|     |                       | another disconnected worker's builds; no                  |
|     |                       | `builds.worker_id` check performed                        |
| G9  | D4, ActiveAssignment  | `ActiveAssignmentReceipt` field absent from `ActiveBuild` |
|     | Receipt, SI-21        | struct (queue/mod.rs:27–37); receipt state entirely       |
|     |                       | untracked                                                 |
| G10 | D4, dispatch ordering | `handle_build_started` call in handler.rs:519 comes       |
|     | (lock+DB→WS)          | before the connection_id is set into the queue entry      |
|     |                       | (handler.rs:657–667), inverting the WCP-specified lock    |
|     |                       | ordering for `dispatched→started`                         |

---

## Part V: Confidence Score

Starting score: 100

| Item | Deduction | Finding                                               |
| ---- | --------- | ----------------------------------------------------- |
| D1   | -5        | D7 default-lines claim incorrect (30, not 50)         |
| D2   | -5        | D7 MAX_TAIL_LINES claim incorrect (10,000, not 1,000) |
| D3   | -5        | Worker supervisor model presented as specified but    |
|      |           | labeled "TODO" in code without that caveat in design  |
| D4   | -5        | Migration blind-swap vs. two-phase spec: gap G7 is    |
|      |           | a correctness claim stated as current behavior        |
| D5   | -5        | Empty-components rejection stated as current          |
|      |           | behavior (D5) when it is not enforced in code         |
| D6   | -5        | SI-8 isolation claim contradicted by code (G8)        |
| D7   | -5        | `ActiveAssignmentReceipt` presented as a defined      |
|      |           | struct but absent from queue/mod.rs                   |
| D8   | -5        | Rollback semantics (D4/SI-6) stated as implemented    |
|      |           | but the dedicated rollback function does not exist    |
| D9   | -5        | Authorization checks for lifecycle messages stated    |
|      |           | as operative (D1/D2) but absent from code             |
| D10  | -5        | D6 ownership enforcement stated as operative but      |
|      |           | absent from both writer.rs and handler.rs             |

**Total deductions: -50**

**Final score: 50 / 100**

Score interpretation: Significant issues. The design specification is internally
consistent and cross-design consistent with audit-remediation v8, but it
overstates implementation status throughout. Ten distinct code-claim mismatches
were verified against source. The design is correct as a specification; it is
not correct as a description of current code.

---

## Part VI: Summary Verdict

**Go/No-Go for implementation: Conditional Go with required v9 scope.**

The WCP v11 design is internally consistent and correctly extends
audit-remediation v8 without contradiction. It is a sound specification. It is
not, however, an accurate description of current code. The confidence score of
50 reflects that the majority of WCP's behavioral claims are forward-looking
design intent rather than implemented invariants.

Audit-remediation v8 adds D11–D13 on top of WCP and implicitly assumes G1, G3,
G6, G7, G8, and G9 are in force. They are not. A v9 design must scope those gaps
explicitly before D11–D13 implementation begins, or risk D12 and D13 landing on
a server that does not enforce the ownership model they presuppose.

**Required actions before proceeding:**

1. Acknowledge in WCP or audit-remediation v9 that gaps G1–G10 are
   implementation work, not currently live. The design should not present them
   as current behavior.
2. A v9 design must explicitly scope G1–G10 into implementation tasks with
   defined commit boundaries.
3. The `ActiveAssignmentReceipt` enum (G9), the dedicated rollback function
   (G3), and the build-scoped authorization check (G1) are load-bearing for D11
   and D12 correctness and must land before those decisions are implemented.
4. The worker supervisor model (G6) is load-bearing for D13 and must land before
   D13 is implemented.
