# 001 — Metrics & Observability for cbsd-rs

## Status

**Not selected — superseded by 002.** Proposal v1. Recommends a topology-
agnostic metrics architecture, stack, and phased work breakdown for the cbsd-rs
server and worker fleet. ADR-style: alternatives are discussed and one approach
is recommended per decision. Code-level instrumentation detail (exact metric
registration sites, label sets, histogram buckets) is deferred to a follow-up
design document and implementation plan.

The project adopted
[002](002-20260625T0839-metrics-observability-websocket-push.md) (outbound-only,
WebSocket-push) instead, on the assumption that workers are permanently
outbound-only and impractical to scrape. This document is retained as the
topology-agnostic alternative — the fallback to revisit only if workers ever
become reachable/scrapable.

## Problem

cbsd-rs has **no metrics today**. The only observability layer is structured
`tracing` logs to files (design doc 010). Logs answer "what happened to this
build?" but cannot answer the operational questions operators actually have:

- How deep is the queue right now, and is it draining?
- What is the build success rate, and how is it trending?
- How long do builds take — overall, per arch, per worker — and is that
  degrading?
- How often do builds get re-queued (the "retry" vector) or time out?
- Are periodic builds succeeding day over day?
- Is a worker host saturated (CPU/mem/disk/IO), and does that correlate with
  slow or failing builds?
- Is ccache effective, and is it filling the disk?

Answering these from logs requires ad-hoc grepping and offline aggregation —
slow, error-prone, and impossible to alert on. The project needs a proper
time-series metrics pipeline feeding Grafana dashboards.

This proposal addresses four questions:

1. Which metrics make sense for cbsd-rs (and what is missing from the initial
   wish list)?
2. How should each metric be collected (push vs. scrape), given a worker fleet
   whose network topology is not yet fixed?
3. What stack should store, query, and visualize the metrics?
4. What work is required in the cbsd-rs codebase to get there?

### Requested metric vectors

The initial wish list, for reference:

| Service | Requested metric                              |
| ------- | --------------------------------------------- |
| Server  | builds in the queue                           |
| Server  | build failures and successes over time        |
| Server  | build retries over time                       |
| Server  | periodic builds succeeded/failed per day      |
| Server  | time per build, over time                     |
| Server  | time per build, per worker, over time         |
| Server  | host statistics (mem, cpu, etc.)              |
| Worker  | build success/failure over time               |
| Worker  | time per build, over time                     |
| Worker  | average time to failed build, over time       |
| Worker  | ccache size/usage, over time                  |
| Worker  | host statistics (mem, cpu, disk, etc.)        |
| Worker  | correlating cpu/mem/disk/io usage with builds |

## Metric ownership model

The single most important design decision is **who owns the source of truth**
for each metric. cbsd-rs has an unusual property that makes this easy: the
server already observes the entire build lifecycle.

The server's in-memory queue (`cbsd-server/src/queue/mod.rs`) and the SQLite
`builds` table record every state transition with timestamps. From
`migrations/001_initial_schema.sql`, the `builds` row carries: `state`,
`priority`, `worker_id`, `trace_id`, `error`, and the four timestamps
`submitted_at`, `queued_at`, `started_at`, `finished_at`. The queue additionally
tracks live worker connection state (`cbsd-server/src/ws/liveness.rs`, the
`WorkerState` enum) and active builds per connection.

Consequently, metrics partition cleanly into two groups:

| Owner             | Holds source of truth for                                                                                                                           | Exposed by                                 |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| **Server**        | queue depth, build lifecycle/outcomes/retries, durations, per-worker timing, periodic outcomes, worker connection state, API request stats, DB pool | Server `/metrics`                          |
| **Worker / host** | host CPU/mem/disk/IO, container resource usage, ccache stats, build-subprocess facts the server cannot see                                          | node_exporter, cAdvisor, worker `/metrics` |

**Implication:** almost the entire wish list is server-owned and needs _zero
worker code_. The worker only emits what it alone can see. This avoids
double-counting (two services reporting the same build outcome) and keeps the
worker — currently a pure outbound WebSocket client with no listening port — as
simple as possible.

## Assessment of requested metrics

Each requested metric, with a verdict:

| Requested metric                      | Verdict            | Owner / how                                                                      |
| ------------------------------------- | ------------------ | -------------------------------------------------------------------------------- |
| builds in the queue                   | **Keep**           | Server gauge, labeled by priority lane and arch                                  |
| build success/failure over time       | **Keep**           | Server counter, labeled by result/arch                                           |
| build retries over time               | **Keep** (reframe) | Server counter on re-dispatch (`rollback_dispatch_to_queued`), labeled by reason |
| periodic builds succeeded/failed      | **Keep**           | Server counter, `periodic=true` label; "per day" is a Grafana query              |
| time per build, over time             | **Keep**           | Server histogram of `finished_at − started_at`                                   |
| time per build, per worker            | **Keep**           | Same histogram with a `worker` label (cardinality bounded by fleet size)         |
| server host statistics                | **Keep**           | node_exporter on the server host (no cbsd code)                                  |
| worker build success/failure          | **Derive**         | Server is the source of truth; worker-side would double-count. Don't emit.       |
| worker time per build                 | **Derive**         | Same. Server already times the build end-to-end.                                 |
| average time to failed build          | **Derive**         | Not a metric — a Grafana query over the duration histogram, `result=failure`     |
| ccache size/usage                     | **Keep**           | Worker gauge (only the worker can see `/cbs/ccache`)                             |
| worker host statistics                | **Keep**           | node_exporter on each worker host                                                |
| correlate cpu/mem/disk/io with builds | **Dashboard**      | Not a metric — overlay node_exporter graphs with a build-activity series         |

Three reframings are worth calling out:

- **"Retries"** in cbsd-rs are not user-initiated retries but **automatic
  re-dispatch**: a build returns to `QUEUED` via `rollback_dispatch_to_queued`
  when a worker dies mid-dispatch, an ack times out, or a
  `Stopping`/disconnected worker held a dispatched build (see the reconnection
  decision table in design doc 002). That is the signal worth counting, labeled
  by reason.
- **"Average time to failed build"** is a derived statistic, not a stored
  series. A single `cbsd_build_duration_seconds` histogram with a `result` label
  answers it (and p50/p95/p99, and success timing) from one source.
- **"Correlating CPU/mem/disk/io with builds"** is a _visualization_
  requirement, not a metric. It is satisfied by putting node_exporter series and
  a `cbsd_builds_active` series on the same Grafana panel, optionally with
  Grafana build annotations keyed by `trace_id`.

### Worker-side metrics: keep them minimal

Because the server is authoritative for build outcomes and timing, the worker
should **not** re-report them. Doing so invites drift (the two services
disagree) and double-counting in dashboards. The worker emits only:

- ccache statistics (size, hit ratio) — `/cbs/ccache` is worker-local.
- build-subprocess facts — exit-code distribution, output-spool bytes
  (`supervisor.rs` already tracks the 64 MiB spool budget).
- nothing about success/failure counts or durations — those come from the
  server.

## Proposed metric set

A concrete (non-exhaustive) catalog. Final names, labels, and histogram buckets
are deferred to the follow-up design doc; this fixes the shape and demonstrates
feasibility against existing code.

### Server — queue & build lifecycle

| Metric                           | Type      | Labels                       | Source                                |
| -------------------------------- | --------- | ---------------------------- | ------------------------------------- |
| `cbsd_builds_queued`             | gauge     | `priority`, `arch`           | queue lanes (`queue/mod.rs`)          |
| `cbsd_builds_active`             | gauge     | `worker`, `arch`             | `queue.active`                        |
| `cbsd_build_results_total`       | counter   | `result`, `arch`, `periodic` | `set_build_finished` (`db/builds.rs`) |
| `cbsd_build_requeues_total`      | counter   | `reason`                     | `rollback_dispatch_to_queued`         |
| `cbsd_build_duration_seconds`    | histogram | `result`, `arch`, `worker`   | `finished_at − started_at`            |
| `cbsd_build_queue_wait_seconds`  | histogram | `priority`, `arch`           | `dispatched_at − queued_at`           |
| `cbsd_dispatch_latency_seconds`  | histogram | `arch`                       | dispatch path (`ws/dispatch.rs`)      |
| `cbsd_build_timeouts_total`      | counter   | `arch`                       | build-timeout kills                   |
| `cbsd_sigkill_escalations_total` | counter   | —                            | SIGTERM→SIGKILL escalation            |

### Server — workers, connections, scheduler

| Metric                               | Type      | Labels          | Source                           |
| ------------------------------------ | --------- | --------------- | -------------------------------- |
| `cbsd_workers_connected`             | gauge     | `state`, `arch` | `WorkerState` (`ws/liveness.rs`) |
| `cbsd_worker_reconnects_total`       | counter   | `worker`        | connection migration path        |
| `cbsd_dispatch_ack_timeouts_total`   | counter   | —               | dispatch ack timer               |
| `cbsd_revoke_ack_timeouts_total`     | counter   | —               | revoke ack timer                 |
| `cbsd_periodic_fires_total`          | counter   | `result`        | scheduler (`scheduler`)          |
| `cbsd_periodic_schedule_lag_seconds` | histogram | —               | fire time − scheduled time       |

### Server — API (RED) & storage

| Metric                               | Type      | Labels                      | Source                  |
| ------------------------------------ | --------- | --------------------------- | ----------------------- |
| `cbsd_http_requests_total`           | counter   | `route`, `method`, `status` | axum metrics layer      |
| `cbsd_http_request_duration_seconds` | histogram | `route`, `method`           | axum metrics layer      |
| `cbsd_db_pool_connections`           | gauge     | `state` (in_use/idle)       | sqlx pool (`db/mod.rs`) |

The DB pool gauge is high-value: `max_connections = 4` is a documented
correctness constraint (CLAUDE.md invariant #2 — pool exhaustion can deadlock
the dispatch mutex). Surfacing pool saturation turns a latent deadlock risk into
an observable, alertable signal.

### Worker — application metrics

| Metric                                    | Type    | Labels | Source                           |
| ----------------------------------------- | ------- | ------ | -------------------------------- |
| `cbsd_worker_ccache_bytes`                | gauge   | —      | `ccache -s` / stat `/cbs/ccache` |
| `cbsd_worker_ccache_hit_ratio`            | gauge   | —      | `ccache -s`                      |
| `cbsd_worker_build_subprocess_exit_total` | counter | `code` | executor exit-code classify      |
| `cbsd_worker_spool_bytes`                 | gauge   | —      | output spool (`supervisor.rs`)   |

### Host & container (sidecars, no cbsd code)

- **node_exporter** — CPU, memory, disk, filesystem fill, IO, file descriptors,
  load — on every server and worker host.
- **cAdvisor** (optional) — per-container/per-build resource usage, enabling
  attribution of CPU/mem/IO to individual build containers.

### Additions beyond the wish list

Explicitly proposed because they are cheap to emit and operationally critical:
`cbsd_build_queue_wait_seconds` (SLO: how long builds wait),
`cbsd_dispatch_latency_seconds`, `cbsd_db_pool_connections` (deadlock early
warning), the ack-timeout counters, the RED HTTP metrics, and the
timeout/SIGKILL counters (build pathology that logs alone bury).

## Collection architecture

### Server: native Prometheus exporter

Add a `GET /metrics` route to the existing axum router in
`cbsd-server/src/app.rs`, as a sibling to the existing `/health` route, using
the `metrics` facade plus `metrics-exporter-prometheus` recorder. All
server-owned metrics register against the process-global recorder; the route
renders the Prometheus text exposition format.

The endpoint may bind on the main listener (path-gated, internal-only) or on a
separate metrics address; the follow-up design will pick based on the auth
posture (see Risks). A startup **gauge resync** reads counts from the `builds`
table so gauges (queued/active) are correct immediately after a restart rather
than starting at zero.

_Alternative considered — the `prometheus` crate directly:_ more boilerplate
(manual registry, manual handle threading) and no ecosystem of ready middleware.
Rejected in favor of the `metrics` facade, which has an off-the-shelf axum/tower
layer for the RED metrics.

### Worker: local exposition + outbound collection agent

This is the real decision, and it is shaped by the constraint that **worker
topology is not yet fixed** — workers may be co-located with the server or run
remotely behind NAT/firewalls, making only outbound connections (design doc 002:
"Workers are pure clients. Outbound connection only — no listening port"). Any
solution must work in both cases.

**Recommended:** the worker exposes a `/metrics` endpoint bound to localhost (a
minimal tokio HTTP listener — the worker has no HTTP server today). A per-host
**collection agent** (Grafana Alloy, or Prometheus in agent mode) scrapes the
local worker `/metrics`, node_exporter, and cAdvisor, then `remote_write`s
outbound to the central Prometheus.

```
worker host                                     central
┌─────────────────────────────────────┐
│  cbsd-worker  ──► :PORT/metrics  ◄─┐ │
│  node_exporter ──► :9100/metrics ◄─┤ │   remote_write
│  cAdvisor      ──► :8080/metrics ◄─┤ │   (outbound)     ┌────────────┐
│                                    │ │ ───────────────► │ Prometheus │
│  agent (Alloy / Prom-agent) ───────┘ │                  └────────────┘
└─────────────────────────────────────┘
```

Why this approach:

- **Topology-agnostic.** The agent connects _out_ to Prometheus, exactly like
  the worker connects out to the server. No inbound reachability required, so it
  works for remote/NAT'd workers.
- **Reuses node_exporter/cAdvisor fully** (the chosen host-stats approach) on
  the same path as app metrics — one uniform pipeline.
- **No protocol changes.** cbsd-proto and the WebSocket handler are untouched.
- **Degrades gracefully.** For a simple co-located deployment (e.g.
  podman-compose dev), the central Prometheus can scrape the worker and
  exporters directly and the agent is omitted entirely.

_Alternative A — push worker metrics over the existing WebSocket._ The worker
already holds an authenticated WS connection; it could send periodic metric
frames that the server re-exposes on its own `/metrics`, labeled by `worker_id`.
Avoids any inbound port and any agent. **Rejected as primary:** it requires new
`cbsd-proto` message types and server-side aggregation/cardin- ality management,
couples app metrics to a bespoke protocol, and still leaves host stats
(node_exporter) needing a separate transport — so it does not actually remove
the agent for the host-stats case. Worth revisiting only if deploying any
per-host agent is deemed unacceptable.

_Alternative B — Prometheus scrapes each worker `/metrics` directly_ with
service discovery. Idiomatic Prometheus, but assumes Prometheus can reach every
worker inbound. That breaks for remote/NAT'd workers and contradicts the
"outbound only" worker property. **Rejected as the sole approach;** it survives
only as the co-located degenerate case of the recommended design.

## Stack

| Concern                | Choice                                    | Rationale                                                       |
| ---------------------- | ----------------------------------------- | --------------------------------------------------------------- |
| Metrics storage / TSDB | Prometheus                                | De-facto standard; pull + `remote_write`; native Grafana source |
| Visualization          | Grafana                                   | Dashboards, alerting, annotations; provisioned as code          |
| Host metrics           | node_exporter                             | Standard, battle-tested; zero cbsd code                         |
| Container metrics      | cAdvisor (optional)                       | Per-build container CPU/mem/IO attribution                      |
| Per-host collection    | Grafana Alloy / Prometheus agent          | Outbound `remote_write`; topology-agnostic                      |
| Server instrumentation | `metrics` + `metrics-exporter-prometheus` | Facade + recorder; off-the-shelf axum RED layer                 |

_Why Prometheus pull/remote_write over alternatives:_ an OTLP/OpenTelemetry
collector pipeline is more capable but heavier and unjustified for a
metrics-only goal; push-to-InfluxDB inverts the model without benefit here and
loses the node_exporter/cAdvisor ecosystem. Prometheus' `remote_write` gives the
one thing the mixed topology needs — outbound delivery — without leaving the
Prometheus ecosystem. Logs (Loki) and trace correlation are deliberately out of
scope for this proposal (see Non-goals).

**Cardinality budget.** The only unbounded-looking labels are `worker` and
`route`. The worker fleet is small and bounded, and routes are a fixed
enumeration, so per-worker histograms and RED metrics stay well within
Prometheus' comfortable range. No per-build or per-user labels are proposed
(those belong in logs, keyed by `trace_id`).

## Dashboards

Grafana dashboards, provisioned as JSON (config-as-code), one row per concern:

- **Queue overview** — `cbsd_builds_queued` by priority/arch; queue-wait
  p50/p95; active builds.
- **Build outcomes & SLOs** — success rate, `cbsd_build_results_total` rate by
  result; duration p50/p95/p99 (overall and `result=failure` → "time to failed
  build"); requeue and timeout rates.
- **Per-worker** — duration and throughput by `worker`; active builds per
  worker; reconnects.
- **Periodic builds** — daily success/failure from `cbsd_periodic_fires_total`;
  schedule lag.
- **Fleet health** — `cbsd_workers_connected` by state; ack timeouts; DB pool
  saturation; HTTP RED.
- **Host utilization** — node_exporter CPU/mem/disk/IO per host, **overlaid with
  `cbsd_builds_active`** and build annotations to satisfy the build/resource
  correlation requirement.

## Configuration & deployment changes

- **Server config** (`config/server.example.yaml`): new `metrics:` section
  (enable flag, optional separate bind address) — kebab-case keys per design
  doc 005.
- **Worker config** (`config/worker.example.yaml`): new `metrics:` section
  (enable flag, localhost bind address/port for the agent to scrape).
- **`podman-compose.cbsd-rs.yaml`**: add `prometheus`, `grafana`,
  `node_exporter`, and (optional) `cadvisor` services; provision the Grafana
  datasource and dashboards; in the co-located dev compose, point Prometheus
  directly at the server and worker `/metrics` (no agent needed).
- **Remote workers**: document the per-host agent (Alloy / Prometheus agent)
  with a `remote_write` URL to the central Prometheus; no inbound ports.
- **`container/ContainerFile.cbsd-rs`**: no change required for the server
  exporter; worker image may add node_exporter/agent or run them as separate
  containers per host.

## Work breakdown

Rough effort: S ≈ ≤½ day, M ≈ 1–2 days.

| #   | Work item                                                                                                                                                                                                                                                            | Size |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| 1   | **Server exporter foundation** — add `metrics`/`metrics-exporter-prometheus`, `/metrics` route, recorder wiring, config `metrics:` section                                                                                                                           | S    |
| 2   | **Server instrumentation** — emit lifecycle/queue/dispatch/worker-state/periodic/requeue metrics at existing transition points (`queue/mod.rs`, `ws/dispatch.rs`, `ws/handler.rs`, `ws/liveness.rs`, `db/builds.rs`, scheduler) + startup gauge resync from `builds` | M    |
| 3   | **HTTP RED + DB pool** — axum/tower metrics layer beside the existing `TraceLayer`; sqlx pool gauges                                                                                                                                                                 | S    |
| 4   | **Worker exporter** — minimal localhost HTTP `/metrics` listener; ccache, subprocess-exit, spool metrics; config `metrics:` section                                                                                                                                  | M    |
| 5   | **Sidecars & compose** — node_exporter, cAdvisor, prometheus, grafana in compose; remote_write/agent path for remote workers                                                                                                                                         | M    |
| 6   | **Grafana dashboards** — provisioned JSON for the dashboard set above                                                                                                                                                                                                | M    |
| 7   | **Follow-up design doc + plan** — finalize metric names/labels/buckets and exact code sites before implementation                                                                                                                                                    | S    |

Suggested sequencing: items 1–3 deliver immediate operational value
(server-owned metrics cover most of the wish list) and can ship before the
worker-side and sidecar work in 4–6.

## Risks & open questions

- **`/metrics` exposure & auth.** Prometheus endpoints are conventionally
  unauthenticated but must not be internet-reachable. Decide between
  internal-network-only binding vs. a token/allowlist. The existing URI logging
  policy (CLAUDE.md invariant #8) already keeps secrets out of logs; `/metrics`
  carries no secrets but does reveal operational shape.
- **Cardinality.** `worker`-labeled histograms are safe only while the fleet is
  bounded. If workers ever become ephemeral/autoscaled, revisit (drop the label
  or aggregate server-side).
- **Gauge resync correctness.** Restart must rebuild queued/active gauges from
  the `builds` table to avoid under-counting; counters reset to zero on restart
  by design (Prometheus handles counter resets via `rate()`).
- **ccache stat cost.** `ccache -s` is cheap but not free; sample on an interval
  (e.g. 30–60 s), not per build.
- **Agent operational overhead.** The recommended design adds a per-host agent
  for remote workers. Acceptable for production fleets; omitted for co-located
  dev. If even one agent is unacceptable, reconsider Alternative A
  (push-over-WS) for app metrics only.

## Non-goals

- **Log aggregation (Loki) and distributed tracing.** Out of scope; the logging
  design (doc 010) already covers structured file logs. A future proposal can
  ship logs to Loki and correlate with metrics via `trace_id`.
- **Alerting rules.** Dashboards first; alert rules are a follow-up once
  baselines are observed.
- **Exact metric registration code.** Deferred to the follow-up design doc and
  implementation plan (work item 7).

## Alternatives summary

| Decision                | Options                                                       | Verdict                                                                                     |
| ----------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Worker metric transport | push-over-WS · direct scrape · **local + agent remote_write** | local expose + agent remote_write (topology-agnostic; degrades to direct scrape co-located) |
| Host stats              | in-process (`sysinfo`) · **node_exporter/cAdvisor**           | reuse node_exporter/cAdvisor (no reinvented host code)                                      |
| Server instrumentation  | `prometheus` crate · **`metrics` facade**                     | `metrics` + `metrics-exporter-prometheus`                                                   |
| Build outcome source    | worker-reported · **server-derived**                          | server is single source of truth (no double-counting)                                       |
| Storage/viz             | OTLP collector · push-to-InfluxDB · **Prometheus + Grafana**  | Prometheus + Grafana (outbound remote_write, ecosystem)                                     |
