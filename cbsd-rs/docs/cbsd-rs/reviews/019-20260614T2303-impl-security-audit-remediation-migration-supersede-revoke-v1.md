# Impl Review — D13 Migration-Supersede Revoke (option A) — v1

**Verdict: DRAFT (in progress)**

Commit: `90610a60` — "cbsd-rs: revoke superseded same-worker connection on
migration". Closes audit-rem D13 (option A). Spans cbsd-proto + cbsd-server +
cbsd-worker + two `-v2` doc addenda. ~337 LOC.

## Scope reviewed

- Proto: `BuildRevoke.reason` + `BuildRevokeReason` enum + serde behavior
- Server: `send_migration_supersede_revokes` helper, migration-path wiring
- Worker: `BuildRevoke` destructure ignores reason
- Docs: design/plan `-v2` addenda
- Build/test/clippy/fmt + `.sqlx` churn

## Findings (in progress)

### Rationale verification (option A scope)

VERIFIED OK. `cbsd-worker/src/ws/connection.rs:147-188` confirms the rationale:

- `reconnect_loop` is a serial `loop`. Each iteration calls `connect()` then
  `run_connection(stream, ...)` (169-176), which OWNS the split stream and runs
  to completion (returns) before the loop iterates.
- The old `stream` is dropped when `run_connection` returns; a NEW connection is
  established only on the next iteration. Exactly one connection at a time.
- `supervisor.detach_transport().await` (183) preserves the subprocess/build
  state across the reconnect (build reconnects as Building) — confirms the
  "subprocess preserved" claim. Therefore the server's migration revoke, sent on
  the OLD (soon-removed) sender, is unreadable by the worker under same-process
  reconnect. The dropped worker-side drain/predicate would fire only under a
  duplicate-process misconfig. Option A is correctly scoped; not under-scoped.

### Server send correctness

VERIFIED OK. `cbsd-server/src/ws/dispatch.rs:658-688`
`send_migration_supersede_revokes`:

- Sends `BuildRevoke { reason: Some(MigrationSupersede) }` per build_id on the
  OLD `old_connection_id` sender (line 675-677).
- Reporter-directed ONLY: acquires `worker_senders` lock, does NOT touch
  pool/queue/ack-timer/log-watcher. Pure send.
- Best-effort: early `return` (no-op + info log) when the old sender is absent
  (line 667-673); empty `build_ids` short-circuits (663).
- Two dedicated tests: `migration_supersede_revokes_sent_on_old_connection`
  (1346) asserts one MigrationSupersede revoke per build, exact count;
  `..._is_noop_when_old_sender_absent` (1373) asserts no panic/hang.
- `send_build_revoke` (admin path) now labels `Some(Admin)` (718);
  `send_reporter_directed_revoke` labels `Some(UnauthorizedAction)` (643).

**FINDING F2 (D5, untested wiring — confidence ~85).** The handler migration
block (`handler.rs:273-353`) — capture-before-reassign (292/293) and
send-before-remove (338/347) — has NO test. The two new tests exercise
`send_migration_supersede_revokes` in isolation only; none of the 186 server
tests drive the handler block. Failure mode: a maintainer who reorders
`senders.remove(&old_cid)` (347) above the helper call (338) turns the feature
into a permanent no-op AND every test still passes, because the helper's "sender
gone → no-op" branch is itself a tested happy path. The send-before- remove
ordering is the load-bearing invariant of this commit and is currently
unguarded. Recommend a handler-level test that registers an old sender, drives
the migration, and asserts a `MigrationSupersede` revoke was observed on the old
sender before it was removed. Non-blocking (feature is best-effort and only
reachable under a misconfig), but it should be recorded, not scored as clean.

### Migration ordering / lock hazards

VERIFIED OK. `cbsd-server/src/ws/handler.rs:273-353`:

- `migrated_builds` captured in the SAME loop iteration that reassigns
  `ab.connection_id` old→new: `push(ab.build_id)` at 292 is BEFORE the
  reassignment at 293. No build_ids lost. (Captures the i64 build_id, not the
  connection.)
- Revoke sent at 338 BEFORE `senders.remove(&old_cid)` at 347. Correct order.
- Lock ordering: queue lock is taken at 276 and RELEASED at 327 (block scope
  ends) BEFORE the helper acquires `worker_senders` at 338. So queue and
  worker_senders are never nested here. The documented global order
  (worker_senders → queue, per `cleanup_worker`) is not violated because this
  path holds only one at a time. No inversion / deadlock.
- The migrated set is exactly the OLD connection's active builds; the new
  connection becomes the authoritative owner via the same reassignment (the
  existing migration semantics, unchanged).

### Wire compat & serde tests

VERIFIED OK. `cbsd-proto/src/ws.rs`:

- `reason: Option<BuildRevokeReason>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]` (lines 46-47).
- `BuildRevokeReason {Admin, MigrationSupersede, UnauthorizedAction}` with
  `#[serde(rename_all = "snake_case")]` (lines 104-115).
- 4 new tests cover the matrix: absent→None (462), None→omitted (476), Some
  round-trips (490), unknown VALUE rejected (508).
- SI-18 guard (`no_deny_unknown_fields_on_server_message`) still includes a
  `build_revoke` case with `future_field` (lines 669-676) and is driven by the
  compile-forced ServerMessageTag witness/sentinel. Adding the new field did not
  weaken the guard. Sentinel for BuildRevoke updated to `reason: None` (line
  626-629). Enum derives are idiomatic (Copy/Eq/serde).

**FINDING F1 (D11, stale doc-vs-code — confidence ~88).** The
`BuildRevokeReason` doc comment (`ws.rs:100-103`) and the `MigrationSupersede`
variant comment (`ws.rs:109-112`) describe DROPPED option-B behavior: "The
worker uses this to decide whether to discard or drain a locally-completed
build" and "The worker drains a pending terminal result instead of discarding
it." Under option A the worker does neither — `handler.rs:152-156` states "there
is no drain-vs-discard decision to make here" and discards `reason` via `..`.
This stale prose ships inside the very commit that rescopes to option A,
directly contradicting the worker it claims to describe. Recommend rewording to
reflect reality, e.g. "currently ignored by the single-connection worker;
reserved for a future multi-connection worker (design 019 v2 'option B')."
Cosmetic but misleading to the next reader; non-blocking.

### BuildRevoke sites repaired

VERIFIED OK. All construct sites carry a reason:

- `dispatch.rs:718` admin path → `Some(Admin)`
- `dispatch.rs:643` reporter-directed → `Some(UnauthorizedAction)`
- `dispatch.rs:677` migration → `Some(MigrationSupersede)`
- `main.rs:424` admin/drain HTTP route → `Some(Admin)`
- proto test sentinel (`ws.rs:628`) → `None`

Worker destructure (`cbsd-worker/src/ws/handler.rs:157`)
`BuildRevoke { build_id, .. }` discards `reason`; existing
`supervisor.on_build_revoke` behavior preserved (RevokingActive / NonActive /
Idle arms unchanged). No dangling site, no regression, no new worker behavior.

### Dropped worker side — reachable hole?

LEGITIMATELY DROPPABLE. Given the single-connection `reconnect_loop` (verified
above), the worker can never read a migration-supersede revoke under the
same-process reconnect the machinery was designed for — the revoke is sent on a
socket the worker has already dropped. It would fire only under a
duplicate-process misconfig (two processes sharing one worker id). Defense in
depth remains: subprocess preserved across reconnect (`detach_transport`), and
D12 dead-worker liveness is the net. No reachable hole. The design v2 "forward
note" correctly flags the dropped path as the thing to revisit IF the worker
ever becomes multi-connection. Worker ignoring `reason` is the right minimal
change.

### Docs (-v2 addenda)

VERIFIED OK. Both `019-20260614T2257-...-v2.md` (design + plan) are genuine
addenda: each opens with "This is an addendum, not a replacement", references
the original by filename in an `Amends` row, and does NOT reproduce the
originals. Content matches the code (scope, rationale, implemented/dropped
lists). seq-docs-convention satisfied: seq 019 shared with the design, correct
`YYYYMMDDTHHmm` timestamp, design has no `type`/`vN` (it is a design doc, not a
review), plan uses no sub. Originals untouched (left as historical snapshots).
Plan v2 correctly withdraws the original ~700 LOC single-commit size exception
now that the worker side is dropped (~250 LOC authored). Minor: the design v2
table separator row is wider than 79 cols, but that is exempt per docs CLAUDE.md
(separator rows must not be broken); prose wraps within 79.

### git-commits smell test

PASS (all five):

1. One-sentence purpose: "send a best-effort stop-work revoke to a superseded
   same-worker connection on migration, plus the BuildRevoke.reason field it
   needs." Yes.
2. Parent compiles: the change is additive (new optional field + new helper +
   one call site + reason labels); parent commit was already green.
3. Revertable: reverting drops the field, helper, call, and labels together; no
   unrelated functionality affected.
4. Testable: 6 new/changed tests (4 proto serde, 2 server helper) verify new
   behavior that did not exist before.
5. No dead code: the helper has a caller (handler.rs:338); the enum variants are
   all constructed (Admin, MigrationSupersede, UnauthorizedAction) and the field
   is read by tests. No `#[allow(dead_code)]`.

Spanning 3 crates + 2 docs is the _correct_ shape per git-commits: a wire-field
addition plus its sole server producer plus the consumer (worker) repaired
atomically — splitting by layer would leave a non-compiling or dead-code
intermediate (proto field with no producer). ~337 LOC diff (incl. tests + docs);
authored non-test/non-doc LOC ~250, within the 400–800 budget after the worker
side was dropped. Coherent, atomic, well-scoped. Message follows Ceph style
(`cbsd-rs:` prefix, why-focused body, one Co-authored-by, Signed-off-by).

### Build / test / clippy / fmt

VERIFIED (independently re-run, not trusted):

- `cargo test -p cbsd-proto -p cbsd-server -p cbsd-worker` (SQLX_OFFLINE=true):
  proto **29/0**, server **186/0**, worker **54/0**, doctests 0/0. Matches the
  implementer's reported counts exactly.
- `cargo clippy ... --all-targets -- -D warnings`: clean (Finished, no
  warnings).
- `cargo fmt --all --check`: clean (no diff).
- `.sqlx` churn: none (`git status` empty for the cache). This commit adds no
  sqlx queries, so that is correct.

## Confidence table

| Item                                         | Points | Description                                                                                                                              |
| -------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Starting score                               | 100    |                                                                                                                                          |
| F2 — D5: handler migration wiring untested   | -15    | Send-before-remove ordering (handler.rs 338/347) has no test; helper tested only in isolation; a reorder regression would pass all tests |
| F1 — D11: stale proto doc describes option B | -5     | `ws.rs` enum/variant comments say the worker drains/discards by reason; the worker ignores it (option A)                                 |
| **Total**                                    | **80** |                                                                                                                                          |

Notes on criteria considered and cleared:

- **D1 (deferred work):** the dropped worker-side machinery is a deliberate,
  documented descope (design/plan v2, option A), not unfinished work — it is
  unreachable under the single-connection worker. Not a D1 deduction.
- **D2/D6 (dup/dead code):** the new helper reuses the existing
  `send_text_to_connection`-style pattern but has a distinct contract (per-build
  loop, best-effort no-op log); not a dedup candidate. Helper has a live caller;
  enum variants all constructed. None.
- **D5 (untested path):** the helper itself has 2 dedicated tests, BUT its
  load-bearing wiring in the handler (capture-before-reassign, send-before-
  remove) is unguarded — see F2.
- **D7 (security):** reporter-directed only, no state mutation; the
  duplicate-process edge (old socket reads revoke, emits BuildFinished) is
  rejected by the existing WCP ownership check — benign. None.
- **D9/D10:** best-effort send logs both outcomes (sent / sender-gone);
  CLAUDE.md conventions (line wrap, error handling, commit form) observed. None.
- **D11 (docs):** the new doc prose is mostly accurate, but the proto enum
  comments still describe the dropped worker drain/discard — see F1.
- **D12 (commit boundary):** passes the five-point smell test (above). None.

## Verdict

**GO (merge-ready) with two non-blocking follow-ups.** Confidence **80/100**.

The implementation correctly delivers audit-rem D13 option A. The core
adversarial probe — whether option A is under-scoped — resolves in the
implementation's favor: `reconnect_loop` is verifiably one-connection-at-a-time
(`run_connection` owns and drops the stream before reconnecting), so the worker
cannot read a revoke on the old socket, and the dropped worker-side drain would
fire only under a duplicate-process misconfig. The server send is correctly
ordered (build_ids captured before old→new reassignment; revoke sent before
old-sender removal), reporter-directed only, best-effort, and free of
lock-inversion (queue lock released before worker_senders is taken). The proto
field is wire-compatible in both directions and the SI-18 guard still protects
it. All BuildRevoke sites are repaired with sensible reasons; the worker ignores
`reason` with no behavior change. Tests (29/186/54), clippy, and fmt are clean
with no `.sqlx` churn. The two `-v2` docs are genuine, accurate addenda. Commit
boundary is coherent and revertable.

Two issues keep this off a clean score but do not block merge, since the feature
is best-effort and only reachable under a duplicate-process misconfig:

- **F1 (D11, action before merge preferred):** reword the stale `ws.rs`
  `BuildRevokeReason` / `MigrationSupersede` comments — they describe the
  dropped option-B worker drain/discard, contradicting the option-A worker in
  the same commit.
- **F2 (D5, follow-up):** add a handler-level test for the send-before-remove
  ordering; today a reorder regression would silently turn the feature into a
  permanent no-op with all tests still green.
