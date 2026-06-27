# 021 — Implementation Review: Metrics Protocol & Worker Collector (G1–G7)

**Type:** impl — adversarial review of the full G1–G7 metrics roadmap
**Branch:** `wip/cbsd-rs-metrics` **Commits reviewed:** `ea83c52` G1, `d79efa9`
G2a, `f3ce79e` G2b, `d8abc4f` G3, `8c26879` G4, `8cf7b23` G5, `0cf7711` G6,
`ef11fed` G7

**Design authority:** designs 021, 022, 023 **Plan authority:** plans 021, 022,
023

---

## Executive Summary

The core of this implementation is solid. The Prometheus recorder is installed
correctly with a GAUGE-only idle timeout, the wire protocol is well-specified
and serde-tested, the WorkerMessageTag parity machine was added as the design
required, the F6 duration guard is in place and tested, the queued_at re-stamp
on rollback prevents the 55-year wait sentinel, and the sampler uses `try_send`
with an AbortOnDrop guard on every exit path. These are the hard correctness
invariants and they all hold.

Two significant gaps remain. First, `cbsd_build_timeouts_total` and
`cbsd_sigkill_escalations_total` — explicitly listed in the G2a plan commit step
2 and in the 022/023 metric catalogs — are not emitted anywhere in the codebase,
and the "Timeouts & SIGKILLs" panel is absent from the
`build-duration-slos.json` dashboard. The plan did not document this as
intentionally deferred. Second, the G5 idle-prune test (B1 from design 022)
required by the plan — "without further pushes, advance past `stale_after` and
call `run_upkeep()` — worker gauges absent, server gauge persists" — is not
present; only the reconnect-continuity test was written. All other plan-required
tests are present.

Minor issues: the `cbsd_build_requeues_total{reason}` label vocabulary diverges
from the spec (`disconnect` → `reconnect_stale`, plus an undocumented `rejected`
value); `sysinfo` was pinned at 0.33 rather than the plan's ≈ 0.39, though the
required disk-IO fields (`total_read_bytes`, `total_written_bytes`) exist in
0.33 and the code compiles; and the "Build events" annotations panel in the
correlation dashboard is absent. These do not break runtime correctness.

---

## Critical Issues

### C1 — `cbsd_build_timeouts_total` and `cbsd_sigkill_escalations_total` not emitted

**Problem.** Plan 022 commit 2 (G2a) step 2 states explicitly: "Counters:
`cbsd_build_results_total` … at `build_finished`;
`cbsd_build_timeouts_total{arch}` on the build-timeout path;
`cbsd_sigkill_escalations_total` on the revoke/escalation path." Design 022
lists both in the counters table. Design 023 specifies a dashboard panel
"Timeouts & SIGKILLs" that graphs them. None of the three commitments are met:

- `grep -rn "build_timeouts_total|sigkill_escalations_total" cbsd-server/src/`
  returns no results.
- The `build-duration-slos.json` dashboard has three panels (Duration
  p50/p90/p99, Time-to-failure p50/p95, Duration heatmap) — the fourth
  ("Timeouts & SIGKILLs") is absent.

The SIGKILL escalation occurs in `cbsd-worker/src/build/executor.rs:246`
(`libc::kill(pgid, libc::SIGKILL)`). The server-side build-timeout path is the
liveness / dispatch ack-timer. Neither emits a counter.

**Impact.** Operators have no visibility into whether builds are timing out or
requiring hard kills. The plan progress tables mark G2a "Done" without this
counter having been delivered, which is inaccurate.

**Recommendation.** Emit `cbsd_build_timeouts_total{arch}` wherever the
server-side build-timeout code path finalises a build as timed-out (the liveness
grace-period expiry path in `ws/liveness.rs` / `dispatch.rs` dead-worker
resolution). Emit `cbsd_sigkill_escalations_total` from the SIGKILL branch of
`executor.rs:246`. Add the worker-side counter to the wire protocol
(`AppMetrics`) or emit it as a process-scoped counter on the server side if the
server orchestrates the escalation. Add the "Timeouts & SIGKILLs" panel to
`build-duration-slos.json`. Update the plan progress annotation.

---

## Significant Concerns

### S1 — G5 idle-prune (B1) test is missing

**Problem.** Plan 022 commit 5 (G5) lists a required test: "Without further
pushes, advancing past `stale_after` then calling `run_upkeep()`/`render()`
prunes the worker gauges, while a server-owned gauge re-set in between persists
(B1 — pruning is render/upkeep-driven)." Design 022 §Tests states the same
contract. The reconnect-continuity test
(`reconnect_same_worker_label_continues_one_series` in `metrics/worker.rs`) is
present, but the idle-prune test is not. No test calls `run_upkeep()` after a
synthetic elapsed timeout.

**Impact.** The most important correctness property of the design — that a
silent worker's host gauges disappear and do not linger forever — has no
automated verification. The `metrics-exporter-prometheus` library's idle-timeout
machinery works as documented, but the implementation has no regression guard if
the `idle_timeout(GAUGE, …)` call is ever accidentally removed or misconfigured.

**Recommendation.** Add a test in `metrics/worker.rs` (or a new `metrics/mod.rs`
integration test) that: installs a local recorder with a short `idle_timeout`
(e.g. `Duration::from_millis(1)`), calls `record_worker_metrics`,
sleeps/advances past the timeout, calls `handle.run_upkeep()`, and asserts the
gauge series is absent in `render()` while a separately re-set gauge (simulating
the server-owned refresh) is still present.

### S2 — `cbsd_build_requeues_total{reason}` label values diverge from spec

**Problem.** Design 022 specifies
`cbsd_build_requeues_total{reason}(worker_dead/ack_timeout/disconnect)`. The
implementation emits four values:

- `"ack_timeout"` (`ws/dispatch.rs:288`) — matches
- `"worker_dead"` (`ws/handler.rs:1246`, `metrics/lifecycle.rs:108`) — matches
- `"rejected"` (`ws/dispatch.rs:594`) — not in the spec
- `"reconnect_stale"` (`ws/handler.rs:943`) — replaces the spec's `"disconnect"`

The `rejected` case (worker sent `BuildRejected`) is a legitimate requeue reason
absent from the design catalog. `reconnect_stale` is a more precise name than
`disconnect` and is arguably better, but neither deviation is documented.

**Impact.** Dashboard panels and alert rules written against the design-spec
label values (`disconnect`) will silently return no data. The dashboard JSON
references this metric; if it uses `reason="disconnect"`, it will be wrong.

**Recommendation.** Either update designs 021-023 to reflect the actual label
set, or add a sentence to the implementation notes documenting the divergence.
Verify the `build-queue-throughput.json` dashboard's re-dispatch panel uses the
values the implementation emits, not the spec's values.

### S3 — `run_sampler` loop termination is not exercised end-to-end

**Problem.** The plan (021 G6) requires: "Sampler task terminates when its
connection's `out_rx` is dropped." The test `closed_channel_signals_stop` in
`sampler.rs` only tests `send_or_count` returning `false` when the receiver is
dropped. It does not exercise `run_sampler` itself — the loop could
theoretically fail to call `break` in response to a `false` return without any
test catching it.

**Impact.** The sampler lifecycle contract is not fully regression-tested. If
`run_sampler`'s loop body changes (e.g. the `break` on `!send_or_count` is
accidentally converted to `continue`), no test fails.

**Recommendation.** Add a `#[tokio::test]` that drives `run_sampler` to
completion: create a bounded channel, spawn `run_sampler`, drop the receiver,
and assert the task completes within a short timeout. The supervisor dependency
can be satisfied by a minimal `Supervisor` instance with `spool_bytes()`
returning 0.

---

## Minor Observations

### M1 — sysinfo pinned at 0.33, plan says ≈ 0.39

Plan 021 §Implementation notes says "sysinfo version to pin at implementation (≈
0.39.x)". The implementation pins `sysinfo = "0.33"`. The required fields
(`total_read_bytes`, `total_written_bytes` on `DiskUsage`, `global_cpu_usage()`,
`load_average()`) are present in 0.33, and `Disks::refresh(true)` calls
`DiskRefreshKind::everything()` which includes IO usage. This is not a runtime
bug. The plan note should be updated to reflect the actual pinned version.

### M2 — "Build events" annotations absent from correlation dashboard

Design 023 specifies a "Build events" annotations entry for the Build ↔ Resource
Correlation dashboard:
`mark increase(cbsd_build_results_total {worker="$worker"}[1m]) > 0 on the resource panels`.
The dashboard's `"annotations": {"list": []}` is empty. The two data panels are
present; this is the only structural deviation in the dashboard set. The panel
is cosmetic (no counter is missing), but the design contract is not met.

### M3 — `Disks::new_with_refreshed_list()` in `HostSampler::new()` does not request IO usage explicitly

`HostSampler::new()` calls `Disks::new_with_refreshed_list()`. In sysinfo 0.33
this calls the default `DiskRefreshKind::everything()` on construction, so IO
counters are populated on the initial list. The subsequent `disks.refresh(true)`
also uses `everything()`. This is correct in 0.33. If a future sysinfo upgrade
changes the default `new_with_refreshed_list()` behaviour (e.g. to a minimal
kind), IO fields would silently zero. Using `Disks::new()` followed by
`refresh_specifics(true, DiskRefreshKind::everything().with_io_usage())` would
be more explicit and defensive.

### M4 — `cores_total` and `ram_total_mb` in `WorkerMessage::Hello` are hardcoded to 0

`ws/handler.rs:86-87` has `cores_total: 0, // TODO: populate from sysinfo` and
`ram_total_mb: 0, // TODO: populate from sysinfo`. The worker now has `sysinfo`
as a dependency (added in G6); these fields could be populated from
`host::global().sample()` or a one-shot `System::new_all()` at startup. This
pre-exists the metrics work but the metrics commit adds the exact dependency
that would enable fixing it.

### M5 — `record_http` label `status` is the full HTTP status code as a string

`cbsd_http_requests_total{route, method, status}` uses
`response.status().as_u16().to_string()` for the `status` label, making it a
3-digit numeric string (e.g. `"200"`). Design 022 specifies `status class` in
the table header. The dashboard's `sum by (status)` panel works either way, but
grouping by class (`"2xx"`) is conventional for cardinality discipline. With a
bounded fleet this is not a practical problem, but it is a minor deviation from
the design's phrasing.

### M6 — ccache `parse_print_stats` uses `split_whitespace`, not tab split

`parse_print_stats` splits on whitespace (`split_whitespace()`).
`ccache --print-stats` outputs `key\tvalue` tab-separated lines. For the current
format this works (whitespace includes tabs), but it would also match keys with
embedded spaces if any future ccache key gained a space. The design note says
"parse `key\tvalue` lines". A tab-explicit split (`line.split_once('\t')`) would
be more precisely matched to the documented format.

---

## Strengths

- **WorkerMessageTag parity machine.** The design identified this as a
  non-trivial correctness requirement ("not free, but cheap") and the
  implementation delivered it cleanly. The exhaustiveness machinery (enum +
  `from_message` match + `strum::EnumIter` + SI-18 payload cases) mirrors the
  pre-existing `ServerMessageTag` pattern exactly and is correct.

- **F6 duration guard.** The `record_build_finished` function correctly gates on
  `started_at.is_some() && finished >= started`. The guard is tested with both
  the success path (duration recorded) and the revoked-before-start path (no
  histogram sample). This is the subtle case the design isolated in G2a
  specifically.

- **queued_at re-stamp on rollback.** `rollback_active_to_queued` re-stamps
  `queued_at: chrono::Utc::now().timestamp()` at line 390 with an explanatory
  comment. This prevents the queue-wait histogram from recording multi-hour
  spurious values when a rolled-back build is later dispatched.

- **F8 worker-label invariant.** The `worker` label is stamped server-side from
  `registered_worker_id` in `ws/handler.rs:726-731`. The worker never sends its
  own identity label. This prevents label forgery and ensures
  reconnect-continuity without any cache.

- **AbortOnDrop covering all exit paths.** `_sampler_guard: Option<AbortOnDrop>`
  in `run_connection` ensures the sampler task is aborted on every return path
  without a separate `JoinHandle` management site at each return.

- **GAUGE-only idle timeout.** The recorder is installed with
  `idle_timeout(MetricKindMask::GAUGE, Some(stale_after))` and the gauge-refresh
  task calls `handle.run_upkeep()` each tick. The design's two invariants
  (server-owned gauges never idle out, decommissioned worker counters persist as
  flat series) are both structurally upheld.

- **ccache carry-forward.** The ccache refresh runs on `ccache_interval` but the
  carried value (`carried_ccache`) is attached to every push tick. This is the
  load-bearing property from design 021 §"App-metric sources" — without it,
  ccache gauges would idle-expire whenever
  `ccache_interval_secs > stale_after_secs`.

- **Disk IO monotonic.** `disk.usage().total_read_bytes` and
  `total_written_bytes` (cumulative-since-boot counters) are used, not the
  per-refresh deltas. The comment in `host.rs:89-91` explains why. This is the
  correctness requirement for server-side counter republication via
  `.absolute(v)`.

- **Validate invariants.** `check_metrics_invariants` enforces both
  `gauge_refresh_secs < stale_after_secs` and `bind != listen_addr` at startup,
  and is covered by three unit tests.

- **`try_send`-only push path.** `send_or_count` never calls `send().await`. The
  `Full` branch increments `push_drops_total` and continues; the `Closed` branch
  breaks. The critical property (metrics cannot back-pressure the build path) is
  upheld.

---

## Open Questions

1. Is the omission of `cbsd_build_timeouts_total` and
   `cbsd_sigkill_escalations_total` intentional (deferred to a follow-up) or an
   oversight? If intentional, the plan progress tables should say so rather than
   marking G2a "Done" without qualification.

2. Should `cbsd_build_requeues_total{reason="rejected"}` be added to the design
   catalog? It covers a real requeue cause (worker sent `BuildRejected`) that
   the spec omitted.

3. Should the `status` label on `cbsd_http_requests_total` be a 3-digit code
   (current) or a class string (`2xx`, `4xx`, `5xx`) per the conventional
   Prometheus RED pattern?

4. Is the build-events annotation panel for the correlation dashboard planned
   for a follow-up, or intentionally dropped?

---

## Confidence Score

| Item                                                                  | Points                   | Description                                               |
| --------------------------------------------------------------------- | ------------------------ | --------------------------------------------------------- |
| Starting score                                                        | 100                      |                                                           |
| D1: `cbsd_build_timeouts_total` not emitted                           | -20                      | Plan G2a step 2 explicitly lists this; not delivered      |
| D1: `cbsd_sigkill_escalations_total` not emitted                      | -20                      | Plan G2a step 2 explicitly lists this; not delivered      |
| D1: "Timeouts & SIGKILLs" panel absent from dashboard                 | -20                      | Design 023 panel contract; G7 did not deliver it          |
| D5: G5 idle-prune B1 test missing                                     | -15                      | Plan 022 G5 test contract; not written                    |
| D5: `run_sampler` loop termination not exercised end-to-end           | -15                      | Plan 021 G6 test contract partially satisfied only        |
| D8: `requeues_total` label values diverge from spec                   | -5                       | `disconnect` → `reconnect_stale`; `rejected` undocumented |
| D8: "Build events" annotation absent from correlation dashboard       | -5                       | Design 023 contract; not delivered                        |
| D11: sysinfo pinned at 0.33, plan says ≈ 0.39 — plan note not updated | -5                       | Minor doc drift; no runtime impact                        |
| **Total**                                                             | **-5** (floor 0) → **0** | Score: **0**                                              |

Wait — recalculating correctly:

| Item                                                           | Points                 | Description                                                             |
| -------------------------------------------------------------- | ---------------------- | ----------------------------------------------------------------------- |
| Starting score                                                 | 100                    |                                                                         |
| D1: `cbsd_build_timeouts_total` not emitted                    | -20                    | Explicitly committed to in plan G2a step 2                              |
| D1: `cbsd_sigkill_escalations_total` not emitted               | -20                    | Explicitly committed to in plan G2a step 2                              |
| D1: "Timeouts & SIGKILLs" dashboard panel absent               | -20                    | Design 023 panel; G7 did not deliver it                                 |
| D5: G5 idle-prune B1 test missing                              | -15                    | Required by plan 022 G5 test contract                                   |
| D5: `run_sampler` loop exit path not end-to-end tested         | -15                    | Plan 021 G6: "task terminates when out_rx is dropped" only half-covered |
| D8: `requeues_total` label values diverge from spec            | -5                     | `disconnect` → `reconnect_stale`; `rejected` undocumented               |
| D8: Build events annotations absent from correlation dashboard | -5                     | Design 023 panel contract not met                                       |
| D11: sysinfo version plan note not updated                     | -5                     | Plan says ≈ 0.39, pinned at 0.33                                        |
| **Total**                                                      | **-105** → floor **0** | Final score: **0**                                                      |

Recalculation confirms: -105 deductions against a base of 100, floored at 0. The
total weight of the three missing D1 items (-60) alone exceeds the buffer.
Despite strong structural correctness in the delivered work, the undelivered
metric set (two counters + one dashboard panel) that the plan explicitly
committed to, combined with two plan- required tests that were not written, puts
the score below the floor.

**Final score: 0 / 100**

> **Interpretation:** Major rework needed. Block merge until the missing
> counters are emitted, the B1 prune test and sampler-loop teardown test are
> written, and either the dashboard panel is added or the omission is explicitly
> documented in the plan as a known gap.

---

## Verdict

**Revise and re-review.**

The implementation is structurally sound and the hardest correctness invariants
are all upheld. The block is the undocumented omission of two metrics that the
plan explicitly committed to delivering in G2a, the missing dashboard panel, and
two absent plan-required tests. These are discrete, scoped additions rather than
architectural rework. Once `cbsd_build_timeouts_total`,
`cbsd_sigkill_escalations_total`, and the "Timeouts & SIGKILLs" panel are added
(or their omission is explicitly documented as a known deferred item in the
plan), and the B1 idle-prune test and `run_sampler` teardown test are written,
this is ready to merge.
