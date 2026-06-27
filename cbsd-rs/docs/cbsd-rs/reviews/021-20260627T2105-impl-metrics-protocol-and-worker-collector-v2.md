# 021 — Implementation Review v2: Metrics Protocol & Worker Collector (G1–G7)

**Type:** impl — adversarial v2 re-review after v1 findings were addressed\
**Branch:** `wip/cbsd-rs-metrics`\
**Commits reviewed:** `cfc13ae~1..HEAD` (12 commits, post-rebase linear
history)\
**v1 review:**
`021-20260627T1456-impl-metrics-protocol-and-worker-collector-v1.md`\
**Addendum:** `021-20260627T1632-metrics-implementation-addendum.md`

**Design authority:** designs 021, 022, 023\
**Plan authority:** plans 021, 022, 023

---

## Executive Summary

All four v1 critical/significant findings (D1 × 2, D5 × 2) are genuinely
resolved — not papered over. `cbsd_sigkill_escalations_total` is emitted
end-to-end from the executor atomic through the wire to the server's
`counter!().absolute(v)` call; the B1 idle-prune test drives the real
`run_upkeep()` path with a timing wall that is purposely generous (220 ms for an
80 ms window); and `run_sampler_stops_when_transport_closes` drives the actual
`run_sampler` future, not just `send_or_count`. The "SIGKILL escalations"
dashboard panel is present in `build-duration-slos.json`. The "Build events"
annotation is present and correct in `build-resource-correlation.json`. The
sysinfo plan note now reads 0.33. The addendum is honest about the semantic
caveat on the SIGKILL counter (counts timer firings, not verified "process still
alive") and about why `cbsd_build_timeouts_total` was dropped.

One minor new concern surfaced: the plan 022 commit-2 prose still says it
"delivers … `cbsd_build_timeouts_total` … `cbsd_sigkill_escalations_total` on
the revoke/escalation path" (lines 104, 122–123), but neither counter lands in
that commit. The addendum's scope note names this rescoping, yet the plan body
contains a now-false description that a reader skimming 022 without reading the
addendum would find misleading. All 66 workspace tests pass and `cargo clippy`
is clean.

---

## v1 Finding Resolution

### D1 — `cbsd_build_timeouts_total` (option A: DROP)

**Verdict: genuinely resolved.**

No emit, no dashboard panel, no orphaned Rust reference. The only occurrences in
the repo are:

- frozen design snapshots (022, 023) — correctly left unedited per
  `seq-docs-convention`;
- the v1 review doc itself; and
- the addendum (which records the drop explicitly).

The addendum's rationale is sound: the build-execution timeout is enforced
inside the cbscore subprocess via `CBS_BUILD_TIMEOUT` (`executor.rs:150`). The
wire carries only `BuildFinished(Failure)` — no timeout flag — so there is
nothing to count server-side without new plumbing in the wrapper. Dropping the
metric is the correct call given the available signal; the addendum documents
the follow-up path if per-timeout visibility becomes worthwhile.

### D1 — `cbsd_sigkill_escalations_total` (IMPLEMENT, worker-sourced)

**Verdict: genuinely resolved. One residual semantic caveat, accurately
documented.**

Full chain verified:

1. **AtomicU64** — `SIGKILL_ESCALATIONS` in `cbsd-worker/src/metrics/app.rs:32`.
   `record_sigkill_escalation()` at line 72 does
   `fetch_add(1, Ordering::Relaxed)`. Tested by
   `sigkill_escalation_counter_increments` (passes).

2. **Increment site** — `executor.rs:248`,
   `crate::metrics::app::record_sigkill_escalation()` is called inside the
   `tokio::spawn` escalation task immediately before
   `libc::kill(pgid, SIGKILL)`. The `cancelled` AtomicBool swap at line 216
   ensures `kill()` is entered at most once per executor, so the escalation
   timer fires at most once per build. **No double-counting.**

3. **Wire field** — `AppMetrics::sigkill_escalations_total` in
   `cbsd-proto/src/ws.rs:252` with `#[serde(default)]`. Older workers that never
   send the field decode to 0. The SI-18 `worker_cases()` metrics payload
   (line 1172) omits `sigkill_escalations_total`, exercising exactly that
   backward-compat path. The `worker_message_metrics_round_trip` test (line 518)
   round-trips value 4 explicitly. Both tests pass.

4. **Sampler** — `cbsd-worker/src/metrics/sampler.rs:98`:
   `sigkill_escalations_total: app::sigkill_escalations()`. Included in every
   push tick.

5. **Server emit** — `cbsd-server/src/metrics/worker.rs:114`:
   `counter!("cbsd_sigkill_escalations_total", "worker" => ...).absolute(v)`.
   Test `snapshot_emits_gauges_and_counters_under_worker_label` asserts
   `cbsd_sigkill_escalations_total{worker="w-7"} 5` is present in the rendered
   output (passes).

6. **Dashboard panel** — "SIGKILL escalations" is the fourth panel in
   `build-duration-slos.json`, graphing
   `sum by (worker) (rate(cbsd_sigkill_escalations_total[1h]))`.

**Semantic caveat (accurately documented):** The escalation task has no check
whether the process already exited before firing. A process that exits cleanly
within 14 s of SIGTERM (below the 15 s default) will still trigger the timer,
call `record_sigkill_escalation()`, and send SIGKILL to the already-dead pgid
(`libc::kill` to a dead pgid returns `ESRCH` silently). The counter therefore
counts "grace-window elapsed" not "process required SIGKILL." The addendum names
this caveat at line 52–55. For operational purposes "grace window elapsed" is
still actionable — it means a build subprocess held on for more than
`sigkill_timeout` seconds after the initial SIGTERM. The semantic is acceptable
and documented.

**`worker` label delta from design:** The design specified an unlabelled
counter; the implementation adds a `worker` label. This is a sound
re-interpretation: the counter is per-worker (it rides the per-worker push
path), and the `worker` label makes it queryable per worker consistently with
all other pushed series. The addendum records this at lines 46–48.

### D5 — G5 idle-prune B1 test

**Verdict: genuinely resolved. Not a tautology; mild timing dependency is
managed.**

`idle_worker_gauges_prune_while_server_gauge_persists` in
`cbsd-server/src/metrics/worker.rs:250` installs a real
`PrometheusBuilder::new().idle_timeout(MetricKindMask::GAUGE, Some(Duration::from_millis(80)))`
recorder. The test:

1. Calls `record_worker_metrics("w-gone", …)` and sets a server gauge, then
   calls `handle.render()` to seed the recency-tracker baseline (line 267).
2. Sleeps 220 ms — 2.75 × the 80 ms idle window — giving a 140 ms margin above
   the window.
3. Touches only the server gauge (`cbsd_builds_active`) to keep its generation
   fresh (lines 271–275).
4. Calls `handle.run_upkeep()` followed by `handle.render()`.
5. Asserts the worker gauge series is absent and the server gauge is present.

This is a real B1 proof: the prune path (`run_upkeep` → idle-timeout sweep) is
exercised directly under the GAUGE-only idle-timeout mask that production
`install()` uses. The 140 ms timing margin is adequate for a CI-class test; a
2.75× headroom is not fragile in normal operation. The test would catch an
accidental removal of the `idle_timeout(…)` call or a mask regression to `ALL`.

### D5 — `run_sampler` loop termination test

**Verdict: genuinely resolved. Drives the real loop; regression-detects
`break`→`continue`.**

`run_sampler_stops_when_transport_closes` in
`cbsd-worker/src/metrics/sampler.rs:185` constructs a real `Supervisor`, calls
`run_sampler(tx, supervisor, Duration::from_millis(10), …)` as a spawned task,
drops the receiver before the first tick, and asserts the future completes
within a 5 s timeout. A `break`→`continue` regression would cause the task to
loop indefinitely on a closed channel (every `try_send` returns `Closed`,
re-entering the loop without sleeping on the closed channel does NOT tick the
interval again because `ticker.tick()` already fired) — actually, the loop would
spin on `ticker.tick()`, which does sleep. A `continue` would re-enter
`ticker.tick()`, sleep 10 ms, hit `try_send`, see `Closed`, and loop again
forever. The 5 s timeout catches this within 500 ticks. The test is a real
regression guard, not a tautology.

### D8 — `requeues_total` label vocabulary

**Verdict: resolved by addendum. Dashboard uses `sum by (reason)` — no
hard-coded label values.**

The addendum records the vocabulary delta (lines 57–68): `reconnect_stale`
replaces `disconnect`; `rejected` is an added value. The
`build-queue-throughput.json` "Re-dispatch (retry) rate" panel uses
`sum by (reason) (rate(cbsd_build_requeues_total[15m]))`, which renders every
value without enumerating them. No alert rules were present to audit.

### D8 — "Build events" annotation

**Verdict: resolved.**

`build-resource-correlation.json` `annotations.list[0]` contains:

```json
{
  "name": "Build events",
  "target": {
    "expr": "increase(cbsd_build_results_total{worker=\"$worker\"}[1m]) > 0"
  }
}
```

This matches the design 023 specification exactly.

### D11 — sysinfo version note

**Verdict: resolved.**

Plan 021 lines 134–135 now read: "sysinfo pinned at **0.33** during
implementation (the estimate here was ≈ 0.39.x; 0.33 was the resolved version
and carries the required disk-IO fields …)."

---

## New Findings

### N1 — Plan 022 commit-2 prose contradicts implemented scope (minor)

**Problem.** Plan 022 commit 2 (G2a) description at lines 104 and 122–123 still
says:

> "Delivers: build outcomes (success/failure/revoked), durations, **timeouts,
> and SIGKILL escalations** …\
> `cbsd_build_timeouts_total{arch}` on the build-timeout path;\
> `cbsd_sigkill_escalations_total` on the revoke/escalation path."

Neither counter is emitted in that commit. `cbsd_build_timeouts_total` was
dropped entirely. `cbsd_sigkill_escalations_total` landed in G6 (the worker
commit), worker-sourced, not in G2a. The addendum's scope note (lines 79–89)
acknowledges the rescoping, but the plan body itself still contains a now-false
description.

**Impact.** A reader diffing the G2a commit against the plan 022 spec will find
the plan claims two counters that are not present in the diff. This is a
documentation accuracy concern, not a runtime defect.

**Recommendation.** Either add a short strike-through or parenthetical in plan
022 commit 2's deliverables list pointing to the addendum, or accept the
addendum as the sole authoritative correction. If the project convention is that
plan bodies are also frozen snapshots, the addendum is sufficient — but that
convention should be noted in the plan header or the addendum itself, because
the addendum currently only says "design documents are snapshots and are
deliberately left unedited." Plans are not mentioned.

---

## Strengths (preserved from v1, re-confirmed)

All strengths noted in v1 remain correct and present. Specifically:

- **WorkerMessageTag parity machine** — `WorkerMessageTag::Metrics` arm is
  present and exhaustive in both `from_message` and `sentinel_for_worker_tag`.
- **F6 duration guard** — `record_build_finished` gates on
  `started_at.is_some() && finished >= started`. Guard test passes.
- **`queued_at` re-stamp on rollback** — `rollback_active_to_queued` sets
  `queued_at: Utc::now().timestamp()`.
- **F8 worker-label invariant** — label stamped server-side from
  `registered_worker_id`.
- **`AbortOnDrop` on all sampler exit paths** —
  `_sampler_guard: Option<AbortOnDrop>` in `run_connection`.
- **GAUGE-only idle timeout** — `idle_timeout(MetricKindMask::GAUGE, …)`.
- **ccache carry-forward** — `carried_ccache` attached to every push tick.
- **Disk IO monotonic** — `total_read_bytes` / `total_written_bytes`
  (since-boot), not per-refresh deltas.
- **`check_metrics_invariants`** — startup guard for
  `gauge_refresh_secs < stale_after_secs` and bind/listen split; tested.
- **`try_send`-only push path** — `send_or_count` never calls `send().await`.

**New strength:** The commit boundary between G5 (server ingest, `AppMetrics`
without `sigkill_escalations_total`) and G6 (worker collector + proto field
addition) is clean. G5 constructs `AppMetrics` with four fields; G6 adds the
fifth (`sigkill_escalations_total`) with `#[serde(default)]` and updates every
construction site. Each commit compiles and tests independently. This is a
correct layered split respecting the "proto-first, then implementation"
dependency order.

---

## Open Questions (resolved from v1)

All four v1 open questions are answered by the addendum:

1. The omission of `cbsd_build_timeouts_total` is intentional (option A drop).
   `cbsd_sigkill_escalations_total` is implemented worker-sourced.
2. `cbsd_build_requeues_total{reason="rejected"}` is added by the
   implementation; the addendum is the authoritative vocabulary record.
3. `status` label convention (3-digit vs class string) — not addressed in the
   addendum but was a v1 minor observation, not a critical finding.
4. The "Build events" annotation is present; no follow-up needed.

---

## Confidence Score

| Item                                                      | Points | Description                                                     |
| --------------------------------------------------------- | ------ | --------------------------------------------------------------- |
| Starting score                                            | 100    |                                                                 |
| D1: `cbsd_build_timeouts_total` not emitted               | 0      | Dropped with sound rationale; addendum documents decision       |
| D1: `cbsd_sigkill_escalations_total` not emitted          | 0      | Implemented worker-sourced; full chain verified                 |
| D1: "Timeouts & SIGKILLs" dashboard panel                 | 0      | Replaced by "SIGKILL escalations" panel per addendum            |
| D5: G5 idle-prune B1 test                                 | 0      | `idle_worker_gauges_prune_while_server_gauge_persists` passes   |
| D5: `run_sampler` loop termination                        | 0      | `run_sampler_stops_when_transport_closes` drives real loop      |
| D8: `requeues_total` label vocabulary                     | 0      | Addendum records divergence; dashboard uses `sum by (reason)`   |
| D8: Build-events annotation                               | 0      | Present and matches design 023 exactly                          |
| D11: sysinfo version note                                 | 0      | Plan 021 now says 0.33                                          |
| N1: Plan 022 commit-2 prose contradicts implemented scope | -5     | Minor doc inaccuracy; addendum covers it but plan body is stale |
| **Total**                                                 | **95** |                                                                 |

### Interpretation

Score 95 — Ready to merge with noted improvement. The sole remaining finding
(N1) is a documentation accuracy gap in plan 022's prose, not a code defect. The
addendum is the authoritative record and covers the divergence; the stale plan
prose is a minor reader-confusion risk, not a merge blocker.

---

## Verdict

**Approve with conditions.**

The implementation is correct, complete relative to the rescoped deliverable,
and all v1 block-merge findings are resolved. The single condition is N1: either
add a cross-reference in plan 022 commit 2's deliverables prose pointing to the
addendum for the counter rescoping, or explicitly document in the addendum that
plan bodies (like design docs) are frozen snapshots and the addendum supersedes
them. Either treatment is acceptable; the choice determines whether a future
reader needs both files to understand what G2a actually delivers.
