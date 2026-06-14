# Impl Review: keep build dispatched on accepted-phase reconnect (D11)

- Review type: impl
- Seq: 019
- Commit: 1013cfe9 "cbsd-rs: keep build dispatched on accepted-phase reconnect"
- Branch: wip/cbsd-rs-security-review
- Verdict: GO (confidence 100)
- Reviewer: adversarial phase-review

## Scope

Server fix (`ws/dispatch.rs`, `ws/handler.rs`) + worker regression test
(`build/supervisor.rs`). Closes audit-rem D11.

## Verification status (filled incrementally)

- [x] Bug claim CONFIRMED: `rollback_dispatch_to_queued` is `WHERE id = ?` only
      (db/builds.rs:259) — no state guard, re-queues regardless of DB state. Old
      `attach_connection_and_start` never cancelled `ack_cancel`. Double-exec
      via stale ack timer was reachable.
- [x] Fix CONFIRMED: `attach_connection_and_mark_received` (dispatch.rs:411)
      sets connection_id + `ack_cancel.cancel()` + receipt=ReceivedByWorker, no
      DB write, all under one queue lock acquisition.
- [x] G10 CONFIRMED: `authorize_lifecycle_message` (dispatch.rs:565) keys on
      `active_build_for_connection_mut(build_id, connection_id)`; helper sets
      the new connection_id so later real build_started is authorized.
- [x] Regression CONFIRMED OK: `set_build_finished` (builds.rs:334) is
      unconditional UPDATE → terminalizes from `dispatched`, no stranding.
      `started` arm untouched. idle-reconcile
      `dispatched+ReceivedByWorker →     RollbackToQueued` (handler.rs:732) is
      the intended D3 reader.
- [x] Concurrency #4: see findings — verified inline await, no drop point.
- [x] Tests: worker accepted-phase uses BuildPhase::Accepted; server test
      asserts {conn_id, receipt, ack cancelled, DB dispatched, not started}.
- [x] Hygiene: no dangling attach_connection_and_start refs (grep clean).

## Bug claim — VERIFIED SOUND

The double-execution gap is real and reachable pre-commit:

- `db::builds::rollback_dispatch_to_queued` (db/builds.rs:259) is an
  unconditional `UPDATE builds SET state = 'queued' ... WHERE id = ?` — no state
  guard. It re-queues regardless of whether the row has since advanced to
  `started`.
- The old `attach_connection_and_start` set `connection_id` + called
  `set_build_started`, but never cancelled `ab.ack_cancel`. The ack timer is
  cancelled only in `handle_build_accepted` and `rollback_active_to_queued`.
- Sequence: dispatch arms the ack timer; worker accepts but `build_accepted` is
  lost in a disconnect; worker reconnects reporting `Building`; old code
  implicitly drove `dispatched -> started` without cancelling the timer; the
  stale timer fires and `rollback_active_to_queued` re-queues the now-`started`
  build, redispatching it to a second worker → double execution.

The fix removes the implicit start, cancels the ack timer, marks the receipt
`ReceivedByWorker`, and keeps `dispatched`. Correct and minimal.

## Verified-correct interactions

- **G10 ownership-at-started preserved.** `authorize_lifecycle_message`
  (dispatch.rs:565) authorizes a later real `build_started` by
  `active_build_for_connection_mut(build_id, connection_id)`. The helper sets
  `ab.connection_id = <new connection>`, so the worker's real `build_started`
  from the reconnected connection is authorized and advances SM-S to `started`.
  Setting connection_id is load-bearing and correctly preserved.
- **No stranding.** `set_build_finished` (db/builds.rs:334) is an unconditional
  UPDATE; `handle_build_finished` (dispatch.rs:439) terminalizes directly from
  `dispatched` if `build_started` is lost. No state-machine dead end.
- **Idle-reconcile reader is intended.** `idle_reconcile_decision`
  (handler.rs:736) maps `dispatched + ReceivedByWorker -> RollbackToQueued`
  regardless of prev-liveness. A worker that later reports `Idle` for this build
  is no longer running it, so re-queue is correct D3 recovery. The new receipt
  write plugs into this reader cleanly.
- **`started` reconnect arm untouched** (handler.rs:1002) — sets connection_id
  only; ack timer was already cancelled at original accept.
- **Worker reports Building for Accepted phase.** `take_reconnect_messages`
  (supervisor.rs:327) keys off `state.active.is_some()`, reporting `Building`
  for any active build (Accepted included). The worker needed only a test; the
  divergence from the worker-only plan scope is correct (user option C).

## Concurrency claim #4 — VERIFIED (within commit scope)

`handle_build_new` (worker ws/handler.rs:371) runs `send(BuildAccepted)` →
`spawn_build().await` → `register_accepted().await` as sequential inline awaits,
and is itself `.await`ed inline inside the per-connection message loop
(ws/handler.rs:132). `take_reconnect_messages` runs once at the top of a fresh
connection (ws/handler.rs:93), before that loop; reconnection is an outer loop.
A new connection's drain therefore cannot interleave with an in-flight
`handle_build_new` on the prior connection.

There is a pre-existing spawn-race window (subprocess spawned at line 465,
`active` not set until line 489) in which the supervisor reports `Idle`. That
routes to the `(Idle, _)` arm / idle-reconcile, NOT this commit's `dispatched`
arm, so it cannot corrupt the code added here. It is orthogonal and pre-existing
(this commit does not touch `handle_build_new`). Recorded as a NOTE, not scored
against this change.

## Tests

- Worker `accepted_phase_reports_building_on_reconnect` (supervisor.rs:830)
  forces `BuildPhase::Accepted` via `force_active` and asserts a single
  `WorkerStatus(Building, Some(42))`. Exercises the Accepted window D11 is
  about. (PASS)
- Server
  `attach_connection_and_mark_received_keeps_dispatched_and_marks_receipt`
  (dispatch.rs:1209) asserts connection_id swapped to new-conn,
  `receipt == ReceivedByWorker`, `ack.is_cancelled()`, DB stays `dispatched`,
  `started_at` is none. Pins exactly the D11 contract. (PASS)

## Verification run (this reviewer)

- `SQLX_OFFLINE=true cargo test -p cbsd-server` → 175 passed, 0 failed.
- `SQLX_OFFLINE=true cargo test -p cbsd-worker` → 54 passed, 0 failed.
- `SQLX_OFFLINE=true cargo clippy -p cbsd-server -p cbsd-worker --all-targets -- -D warnings`
  → clean.
- `cargo fmt --all --check` → clean.

Implementer's reported 175/0, 54/54, clean clippy+fmt all confirmed.

## Commit hygiene (git-commits smell test)

- One-sentence purpose: yes — "keep an accepted-phase reconnect dispatched and
  cancel its ack timer to prevent double execution."
- Compiles alone: yes (tests + clippy pass at this SHA).
- No dead code: `attach_connection_and_start` and its test removed; grep finds
  zero residual references. New helper has a caller (handler.rs:995).
- Revertable: yes — self-contained, no cross-commit coupling.
- Component prefix `cbsd-rs:` — appropriate; the change spans server + worker.
- Single commit (server fix + worker test + helper removal) — coherent; the
  worker test pins the invariant the server fix relies on.

## Minor findings

- **F1 (D4, low, 26):** worker test `force_active(BuildPhase::Accepted)` sets
  `executor: None`, but in production the Accepted phase always carries a
  spawned executor (`register_accepted` runs after `spawn_build`). The test's
  assertion target (`take_reconnect_messages` keys off `active.is_some()`, never
  reads `executor`) is unaffected, so the test value holds — but the synthetic
  state diverges from the real invariant. Below the 80 report threshold;
  documented for completeness, no action required.

No findings at or above the confidence threshold (80). No blocking issues.

## Confidence score

| Item            | Points  | Description                                                                                                                                                                                 |
| --------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --- |
| Starting score  | 100     |                                                                                                                                                                                             |
| (no deductions) | 0       | Bug claim sound; fix minimal and correct; G10 preserved; no stranding; idle-reconcile reader intended; tests pin the contract; clippy/fmt/tests clean; single coherent commit, no dead code |     |
| **Total**       | **100** |                                                                                                                                                                                             |

Sub-threshold note F1 (D4-class, conf 26) is recorded above but not deducted:
the test's assertion target does not depend on the diverging field, so it is a
cosmetic fidelity gap, not a correctness or quality defect in the change.

## Verdict

**GO.** The commit correctly closes audit-rem D11. The double-execution bug it
targets is real (unguarded `rollback_dispatch_to_queued` + uncancelled ack timer
on the old reconnect path), and the fix is the minimal correct change: cancel
the ack timer, mark `ReceivedByWorker`, keep `dispatched`, drop the implicit
start. Ownership-at-started (WCP G10), terminalization-from-dispatched, and
idle-reconcile semantics are all preserved and verified. Tests pin both the
server contract and the worker reporting invariant. Build, tests, clippy, and
fmt are clean. No blocking or above-threshold findings.
