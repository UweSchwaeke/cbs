# 002 — Metrics & Observability for cbsd-rs (WebSocket-push variant)

## Status

**Accepted (v3).** Selected over
[001](001-20260625T0740-cbsd-rs-metrics-observability.md) as the approach to
implement, on the assumption that **workers are permanently outbound-only and
impractical for any external scraper or per-host agent to reach**. Where 001
stayed topology-agnostic (worker `/metrics` + a per-host `remote_write` agent),
this variant commits to a single transport: the worker pushes **all** its
metrics — application and host — over its existing authenticated WebSocket
connection, and the server is the sole Prometheus scrape target. Detailed
instrumentation/schema design follows in a separate `design/` document (this
proposal's work item 8).

This document supersedes 001 for any deployment that adopts the outbound-only
assumption. It does not re-derive the parts that are unchanged (the metric
ownership model, the server-owned metric set, and most of the stack); those are
referenced from 001 and only summarized here. The architectural divergence —
worker transport and server aggregation — is treated in full.

### Revision history

**v3** incorporates the round-2 review
(`002-…-review-metrics-observability-websocket-push-v2.md`). Concrete changes:

- **Welcome compat (N1, was blocking).** The `accepts_metrics` field **must be
  `#[serde(default)]`** — SI-18 covers unknown fields, not _missing_ ones, so an
  upgraded worker would otherwise fail to parse a pre-upgrade server's `Welcome`
  and not connect at all. Now spelled out, with the in-tree precedent
  (`BuildRevoke.reason`, `cbsd-proto/src/ws.rs:48-52`) and a required round-trip
  test. See
  [Worker → server metrics protocol](#worker--server-metrics-protocol).
- **Sampler sender ownership & idle path (N2/N3).** Adds
  [Sampler lifecycle & sender ownership](#sampler-lifecycle--sender-ownership-n2):
  the sampler is a **per-connection task** holding its own `out_tx.clone()`
  (`ws/handler.rs:91-92`), bypassing the supervisor — whose `send_or_spool`
  drops messages when idle (`build/supervisor.rs:269-275`), which would
  otherwise lose idle-worker host stats.
- **Staleness framing (N-minor).** The freshness window is now stated as the
  sole mechanism needed for correctness; liveness-driven eviction is an
  **optional latency optimization**, not a co-equal layer.
- **node_exporter / "zero daemons" tension (N4).** Clarifies the property is
  about _worker_ hosts; the reachable server host is a different cost class.
- **Round-3 minors folded in (NF1, NF2).** `accepts_metrics` is specified as a
  bare `#[serde(default)] bool` (not `Option<bool>`); the `try_send` drop
  counter is given an exposition path — it rides the next `Metrics` push so
  sustained drops are visible rather than silent.

**v2** incorporates the adversarial review of v1
(`002-…-review-metrics-observability-websocket-push-v1.md`). Concrete changes:

- **Protocol compatibility (F1).** Corrects the false "version negotiation"
  claim: the server hard-rejects any `protocol_version != 2`
  (`cbsd-server/src/ws/handler.rs:141`). The new message is an **additive serde
  variant within protocol v2**, and the worker only pushes after the server
  advertises support via an additive `Welcome` capability flag — see
  [Worker → server metrics protocol](#worker--server-metrics-protocol).
- **Metric series identity (F2).** Makes explicit that the stable series key is
  `registered_worker_id` (stable across reconnects), **not** the per-connection
  `connection_id`; reconciles this with the design's "`worker_id` is
  display-only" routing identity.
- **Reconnect-safe staleness (F3, promoted from deferred).** Specifies a
  freshness-timestamp as the primary staleness mechanism and a reconnect-guarded
  drop predicate so a stale old connection cannot delete a live reconnected
  worker's series.
- **Shared-channel backpressure (F4, promoted from deferred).** Specifies
  `try_send` with drop-on-full over the single capacity-64 outbound channel
  (`handler.rs:50`), and that metrics bypass the build-output spool.
- **Counter republish (F5).** Server uses `Counter::absolute()`; documents the
  one-interval post-restart zero window.
- **Server-owned histogram guard (F6).** Duration histogram must skip builds
  with NULL/negative `started_at`.
- **Host-metric coverage / label join (F7, F8).** `sysinfo` coverage to be
  verified against a pinned version; the `worker` label value is stamped
  server-side from one source so pushed and server-owned series join.

## Problem & guiding constraint

The motivation is identical to 001: cbsd-rs has no metrics, only structured logs
(design doc 010), and operators need queue depth, build outcomes/
durations/retries, periodic-build health, ccache usage, and host utilization
correlated with build activity, in Grafana.

The new, binding constraint:

> Worker hosts are outbound-only and difficult-to-impractical to reach. We
> cannot rely on anything scraping a worker, and we want **zero extra daemons to
> deploy or maintain on worker hosts**.

This rules out, for workers:

- Direct Prometheus scrape of a worker `/metrics` (needs inbound reach).
- node_exporter / cAdvisor on worker hosts (they expose scrape endpoints; even
  fronted by a remote_write agent, they are extra daemons to install and babysit
  on hosts we can barely access).
- A per-host remote_write agent (Grafana Alloy / Prometheus agent) — one more
  thing to deploy and keep running where access is hard.

What remains available is the one channel cbsd-rs already operates reliably: the
worker→server WebSocket. It is outbound-initiated, authenticated,
auto-reconnecting (exponential backoff, `cbsd-worker/src/ws/connection.rs`), and
already multiplexes high-volume build output. Adding small, periodic metrics
frames to it is negligible incremental load and requires **nothing new deployed
on the worker host**.

## Decision

**Push all worker and host metrics over the existing WebSocket; aggregate and
re-expose them on the server; scrape only the server.**

```
worker host (outbound-only, no extra daemons)        reachable
┌───────────────────────────────────────────┐
│  cbsd-worker                               │
│   ├─ in-proc host sampler (sysinfo)        │
│   │    cpu / mem / disk / io               │
│   ├─ app metrics (ccache, subprocess,      │   WorkerMessage::Metrics
│   │    spool)                              │   (periodic, over existing WS)
│   └─ WS client ───────────────────────────┼──────────────┐
└───────────────────────────────────────────┘              │
                                                            ▼
                                          ┌──────────────────────────────┐
                                          │ cbsd-server                  │
                                          │  ├─ server-owned metrics      │
                                          │  │   (queue, builds, …)       │
                                          │  └─ per-worker metrics cache  │   scrape
                                          │      (latest pushed snapshot, │ ◄───────── Prometheus
                                          │       labeled by worker)      │
                                          │     GET /metrics              │
                                          └──────────────────────────────┘
```

Consequences:

- **One scrape target.** Prometheus only ever talks to the server, which is
  reachable and co-located with the monitoring stack. No service discovery, no
  per-worker networking.
- **No per-host infrastructure.** The worker binary is fully self-contained;
  nothing else runs on a worker host for metrics.
- **Correlation gets easier, not harder.** Build activity (server-owned) and
  host stats (pushed) land in the same Prometheus, both keyed by the same
  `worker` label, so per-worker "CPU vs. active builds" overlays are natural.

The cost we accept (vs. 001): protocol additions in `cbsd-proto`, an
aggregation/staleness layer in the server, and a small in-process host-metric
collector instead of node_exporter's breadth. These are detailed and justified
below.

## What is unchanged from 001

The following carry over verbatim; see 001 for the full treatment:

- **Metric ownership model.** The server already owns the entire build lifecycle
  (queue in `queue/mod.rs`; `builds` table timestamps
  `submitted_at / queued_at / started_at / finished_at`; `WorkerState` in
  `ws/liveness.rs`). All build-outcome, duration, queue, dispatch, periodic,
  requeue, HTTP-RED, and DB-pool metrics remain **server-owned** and are emitted
  exactly as in 001 — no worker involvement, no double-counting.
- **Server-owned metric set.** The catalog in 001 §"Proposed metric set"
  (`cbsd_builds_queued`, `cbsd_build_results_total`,
  `cbsd_build_duration_seconds`, `cbsd_build_requeues_total`,
  `cbsd_workers_connected`, `cbsd_db_pool_connections`, the RED metrics, etc.)
  is adopted unchanged.
- **Server exporter mechanics.** `GET /metrics` on the axum router
  (`cbsd-server/src/app.rs`) via the `metrics` facade +
  `metrics-exporter-prometheus`, with startup gauge resync from the `builds`
  table.
- **Reframings.** Worker "build success/failure" and "time per build" are
  **derived server-side**; "average time to failed build" is a Grafana query;
  build/resource "correlation" is a dashboard concern.
- **Duration-histogram guard (F6).** `cbsd_build_duration_seconds` is
  `finished_at − started_at`, but `started_at` is NULL for builds revoked or
  failed before a worker started them (`set_build_finished` always stamps
  `finished_at`; `started_at` may be absent). The observation is recorded **only
  when `started_at` is present and `finished_at ≥ started_at`** — never emitting
  a negative or garbage duration. This refines the 001 metric.
- **Stack core.** Prometheus + Grafana.

The remainder of this document covers only what differs: how worker/host metrics
travel and how the server republishes them.

## Worker → server metrics protocol

Add one additive variant to the `WorkerMessage` enum in `cbsd-proto/src/ws.rs`
(tagged JSON, `type: "metrics"`), pushed periodically worker→server:

- **Shape:** a typed snapshot, not opaque text. A `host` sub-struct (CPU busy %,
  load, memory used/total/available, swap, per-mount disk used/total, disk IO
  read/write bytes, optionally net IO) plus an `app` section (ccache size bytes,
  ccache hit ratio, output-spool bytes, and build-subprocess exit counts). Typed
  keeps the wire contract reviewable and versioned, consistent with the rest of
  `cbsd-proto`.
- **Counter semantics:** counter-like fields (e.g. subprocess-exit counts) are
  sent as **cumulative since worker process start**. The server republishes the
  value via `Counter::absolute()` (the `metrics` facade is increment-only; naive
  `increment` would double-count), under a `worker` label whose **value is
  stamped server-side** from the connection's `registered_worker_id` — the
  worker never sends its own label value, so pushed series join cleanly with the
  server-owned per-worker metrics and cannot be spoofed (F8). Prometheus handles
  the reset on worker restart via `rate()`. Gauges (CPU, memory, ccache size)
  are point-in-time. Note a one-interval **zero window** after a _server_
  restart: the worker keeps counting but the server cache is empty until the
  next push; `rate()`/`increase()` absorb it.
- **Compatibility (no version bump).** The server **hard-rejects** any
  `protocol_version != 2` (`cbsd-server/src/ws/handler.rs:141`), so bumping the
  version to gate this would break the entire fleet. Instead the message is an
  **additive serde variant within protocol v2**. Two safety nets: (1) an unknown
  inbound message only logs a `warn!` and is skipped, never dropping the
  connection (`handler.rs:434-440`); (2) to avoid one warn per push against a
  not-yet-upgraded server during a rolling upgrade, the worker **only begins
  pushing after the server advertises support** via a new additive boolean field
  on `ServerMessage::Welcome` (e.g. `accepts_metrics`).
  - **The new field MUST be `#[serde(default)]` (F1/N1).** SI-18 only buys
    _unknown-field tolerance_ (a new field an old peer ignores); it does **not**
    cover a _missing_ field. An upgraded worker deserializing a pre-upgrade
    server's `Welcome` — which omits `accepts_metrics` — would hit a hard serde
    error and **fail to connect** (the worker parses `Welcome` on the strict
    path), which is worse than the warn spam the flag was meant to avoid.
    `#[serde(default)]` makes the missing field deserialize to `false`, i.e.
    "server does not support metrics → stay silent," which is exactly the
    desired degraded behavior. This is the established pattern in this codebase:
    `ServerMessage::BuildRevoke.reason` already uses
    `#[serde(default, skip_serializing_if = "Option::is_none")]` for the same
    additive-field reason (`cbsd-proto/src/ws.rs:48-52`). Use the field exactly
    as **`#[serde(default)] accepts_metrics: bool`** — a bare `bool`, _not_
    `Option<bool>`: the precedent is `Option` only because a build-revoke reason
    is genuinely tri-state, whereas "supports metrics" is a plain false/true and
    `#[serde(default)]` already supplies the absent ⇒ `false` semantics. A
    round-trip test for old-server→new-worker (field absent ⇒ `false`) is
    required.
  - With `#[serde(default)]` in place: an old worker omits the field; an old
    server never sets it and the new worker reads `false` and stays silent. Net:
    zero breaking change, zero warn spam, no version bump.
- **No spooling on disconnect.** Unlike build output (which the supervisor
  spools, `cbsd-worker/src/build/supervisor.rs`), metrics are point-in-time and
  are **not** buffered while disconnected. The sampler's lifecycle is tied to
  the connection (see
  [Sampler lifecycle & sender ownership](#sampler-lifecycle--sender-ownership-n2)):
  between connections there is no live sampler, so "detached" simply means no
  task is sending; on reconnect a fresh sampler is spawned. The server ages out
  the stale snapshot (below). This keeps the supervisor's spool budget reserved
  for build output.

### Cadence

The worker samples and pushes on a configurable interval
(`metrics.push-interval`, default ~15 s to match a typical scrape interval).
Expensive sources are sampled less often internally: `ccache -s` on a slower
cadence (e.g. 60 s) to avoid per-push cost, with the last value carried in each
snapshot. Payloads are small JSON frames — orders of magnitude below build-
output volume already flowing on the same socket.

### Shared-channel backpressure (F4)

All worker→server frames funnel through a single bounded `mpsc` channel of
capacity 64 (`OUTPUT_CHANNEL_CAPACITY`, `cbsd-worker/src/ws/handler.rs:50`) —
metrics frames share it with build output. The design must not let a metrics
push stall build progress, and must not let a backlog of build output starve
metrics into staleness. Rules:

- **Push with `try_send`, drop on full.** The sampler uses non-blocking
  `try_send`; if the channel is full (a burst of build-output batches is
  draining), the snapshot is **dropped**, not awaited. Blocking (`send().await`)
  is forbidden here — it would back-pressure the sampler and, transitively,
  build handling. Dropping one sample is harmless: another arrives next interval
  and the server's freshness window (below) tolerates the gap. Drops are counted
  in a cumulative local counter; since the worker exposes no endpoint, that
  counter **rides the next successful `Metrics` push** as an ordinary `app`
  field (e.g. `metrics_push_drops_total`) and the server republishes it like any
  other worker counter (NF2) — so sustained drops are visible in Grafana rather
  than silent. The field is pinned in the work-item-8 snapshot schema.
- **Bypass the supervisor entirely.** Metrics never enter the supervisor's spool
  or its `send_or_spool` path. That path drops any message when there is no
  active build (`cbsd-worker/src/build/supervisor.rs:269-275`, "dropping orphan
  output message"), so routing metrics through the supervisor would silently
  lose **exactly the idle-worker host stats we most want** (an idle worker is
  the common case). The sampler holds the outbound sender directly (next
  section).
- **Head-of-line tolerance.** A metrics frame can still queue behind a build-
  output batch already in the channel; at capacity 64 and coarse cadence this
  adds at most sub-second latency, absorbed by the freshness window. If strict
  isolation is ever required, the escalation is a dedicated control lane (a
  second channel or a WS message-priority split) — noted, not adopted now.

### Sampler lifecycle & sender ownership (N2)

The outbound sender `out_tx` is **per-connection**: it is created fresh inside
`run_connection` on every (re)connect and cloned to the supervisor
(`cbsd-worker/src/ws/handler.rs:91-92`). The metrics sampler must therefore be
re-wired to the new sender on each reconnect, and must not capture a stale
sender. Concretely:

- The sampler runs as a **per-connection task spawned inside `run_connection`**,
  holding its own `out_tx.clone()` taken at connection setup — a sibling of the
  supervisor's clone, not routed through it.
- The task's lifetime equals the connection's: it is cancelled (or naturally
  exits on `try_send` returning `Closed`) when the connection drops, alongside
  the existing transport teardown. There is **no long-lived sampler holding a
  swappable sender** — avoiding the "stale `out_tx` after reconnect" bug class.
- Because the sender is independent of the supervisor, the sampler emits on its
  interval **whether or not a build is active**, which is precisely what makes
  idle-host metrics flow. This is the mechanism behind "skip when detached":
  detached = the connection (and thus the task) is gone; reconnect spawns a new
  one.

## Server aggregation & exposition

The server keeps a **per-worker latest-snapshot cache** and renders it into the
Prometheus exposition alongside the server-owned metrics on the same `/metrics`
endpoint. This makes the server a narrow, purpose-built push-gateway for worker
metrics.

### Series identity: `registered_worker_id`, not `connection_id` (F2)

The cache and the exported `worker` label are keyed by
**`registered_worker_id`** — the worker's configured identity, which is **stable
across reconnects** (asserted at hello and reused on reconnection:
`cbsd-server/src/ws/dispatch.rs`, `ws/handler.rs`). It is deliberately **not**
keyed by `connection_id`, the fresh per-connection UUID minted on every upgrade
(`ws/handler.rs`), because a metric time series must stay continuous when a
worker drops and reconnects.

This is a conscious, scoped departure from the design's rule that "`worker_id`
is display-only; the canonical identity is the per-connection handle" (design
README "Decided Questions"). That rule governs **routing and build ownership**,
where per-connection isolation is required. **Metric-series identity has the
opposite requirement** — it must survive reconnects — so it uses the stable id.
Precondition: `registered_worker_id` must be present and unique per worker
(already true for authenticated workers); the follow-up design states this as an
invariant.

### Staleness & the reconnect race (F3)

A dead or stalled worker must not linger in `/metrics` reporting its last CPU
forever — and, critically, cleanup must not delete a **live** worker's series.
Note the hazard: connection liveness is tracked **per `connection_id`**
(`queue/mod.rs` `workers: HashMap<ConnectionId, WorkerState>`), but series are
keyed per `registered_worker_id`. On reconnect a worker holds a **new**
`connection_id` while its **old** connection is still in the grace period; when
the old connection finally transitions to `Dead` (`ws/liveness.rs`
`handle_worker_dead`), a naïve "drop this worker's series" would erase the live
reconnected worker's data.

Resolution — a single sufficient mechanism plus one optional optimization:

- **Freshness window (the mechanism).** Each cached snapshot carries a monotonic
  `last_push` instant. A series is exported only while
  `now − last_push < stale_after` (default ≈ 3× `push-interval`). This **alone
  is sufficient** for correctness: it ages out crashed, stalled, or disconnected
  workers without touching the liveness machinery, and is inherently
  reconnect-safe (a live worker keeps refreshing `last_push`, a gone one stops).
  The implementation can ship with only this.
- **Liveness-driven eviction (optional latency optimization, guarded).** A
  `Dead` transition _may_ evict immediately so a crashed worker disappears in
  seconds rather than after `stale_after`. This is **not required for
  correctness** — it only shortens the disappearance latency. If adopted, it
  **must** be guarded: evict only if no live connection currently maps to that
  `registered_worker_id` (consult the live connection set / a per-`worker_id`
  refcount of active connections), otherwise the reconnect race erases a live
  worker. Given the freshness window already bounds staleness, this optimization
  is optional and should be added only if the `stale_after` latency proves too
  coarse in practice.

### Continuity across restarts

On **server** restart the cache is empty until workers re-push (one-interval
gap; counters re-published via `Counter::absolute()`). On **worker** restart,
cumulative counters reset to zero — a normal Prometheus counter reset handled by
`rate()`.

### Labeling & cardinality

Pushed metrics are namespaced (`cbsd_worker_*`, `cbsd_worker_host_*`) and carry
the single server-stamped `worker` label (§ counter semantics); the server adds
no per-build or per-user labels (those belong in logs, keyed by `trace_id`),
keeping cardinality bounded by fleet size.

## Worker host-metric collection (in-process)

With node_exporter off the table for workers, the worker collects host stats
itself via the `sysinfo` crate (new worker dependency, **version pinned** at
implementation), covering the vectors asked for: CPU busy %, load average,
memory (used/total/available, swap), and per-mount disk usage for the volumes
that matter (`/cbs/scratch`, `/cbs/ccache`, container storage). Per-mount
selection and disk/network IO counters are **to be verified against the pinned
`sysinfo` version** during the follow-up design (its per-device IO surface
varies by release); where `sysinfo` is insufficient the fallback is reading
`/proc` directly (`/proc/diskstats`, `/proc/net/dev`). Coverage gaps that remain
are documented, not silently dropped.

This is a deliberately **small subset** of node_exporter — only what the
dashboards consume — accepted as the price of zero per-host daemons. The same
collector is library code that the server binary can reuse if a daemon-free
server is later desired (see below).

## Server host metrics

The server **is** reachable and co-located with Prometheus, so its host stats do
not face the worker constraint. Note the scope of the "zero daemons" property
(N4): it is specifically about **worker hosts** — the hard-to-reach, possibly
operator-foreign machines where every extra daemon is a maintenance liability.
The server host is reachable, operator-managed, and already runs the monitoring
stack beside it, so one well-understood sidecar there is a different and
acceptable cost class. Recommended: run **node_exporter as a sidecar on the
server host** (comprehensive, standard, scraped directly) — the one place
node_exporter still fits cleanly. Alternative, for operators who prefer no extra
daemon even here: have the server expose the same in-process `sysinfo` host
metrics on its own `/metrics`. Either is fine; node_exporter is recommended
because the server host is the easy case where its breadth is free.

## Stack

| Concern                 | Choice                                        | Notes                                                |
| ----------------------- | --------------------------------------------- | ---------------------------------------------------- |
| Storage / TSDB          | Prometheus                                    | Scrapes only the server                              |
| Visualization           | Grafana                                       | Dashboards, alerting, annotations (as code)          |
| Server instrumentation  | `metrics` + `metrics-exporter-prometheus`     | Same as 001                                          |
| Worker host metrics     | `sysinfo` crate (in-process)                  | Pushed over WS; no node_exporter on workers          |
| Server host metrics     | node_exporter sidecar (server host only)      | Reachable; or reuse in-process collector             |
| Worker→server transport | existing WebSocket (`WorkerMessage::Metrics`) | No agents, no exporters, no inbound ports on workers |

There are **no agents, exporters, or inbound ports on worker hosts** — the
defining property of this variant.

## Dashboards

Unchanged in intent from 001 (queue, build outcomes/SLOs, per-worker timing,
periodic, fleet health, host utilization). One concrete improvement: because
host stats and build activity now share Prometheus and the `worker` label, the
"host utilization overlaid with `cbsd_builds_active`" correlation panel is a
direct per-worker join rather than a cross-source overlay.

## Configuration & deployment changes

- **Worker config** (`config/worker.example.yaml`): new `metrics:` section —
  `enable` flag and `push-interval` (kebab-case per design doc 005). No bind
  address (the worker exposes no endpoint).
- **Server config** (`config/server.example.yaml`): `metrics:` section as in 001
  (enable, optional separate bind), plus aggregation knobs if needed
  (stale-after).
- **`podman-compose.cbsd-rs.yaml`**: add `prometheus` and `grafana`; add
  node_exporter **on the server host only**; provision the Grafana datasource
  and dashboards. Worker service is unchanged — no sidecars added.
- **Worker image / hosts**: no change. Nothing new to install on worker hosts.

## Work breakdown

Rough effort: S ≈ ≤½ day, M ≈ 1–2 days.

| #   | Work item                                                                                                                                                                                                                                                             | Size |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---- |
| 1   | **Server exporter foundation** — `metrics`/`metrics-exporter-prometheus`, `/metrics` route, recorder, config (same as 001 item 1)                                                                                                                                     | S    |
| 2   | **Server-owned instrumentation** — build/queue/dispatch/worker-state/periodic/requeue + startup gauge resync (same as 001 item 2)                                                                                                                                     | M    |
| 3   | **HTTP RED + DB pool** (same as 001 item 3)                                                                                                                                                                                                                           | S    |
| 4   | **Protocol** — additive `WorkerMessage::Metrics` variant + additive `#[serde(default)] Welcome.accepts_metrics` flag (no version bump) in `cbsd-proto/src/ws.rs`; round-trip tests incl. old-server→new-worker missing-field ⇒ `false`, and unknown-variant tolerance | S    |
| 5   | **Worker collector** — `sysinfo` host sampler + app metrics (ccache, subprocess, spool); per-connection sampler task holding `out_tx.clone()` (bypasses supervisor); push gated on `accepts_metrics`; `try_send` drop-on-full                                         | M    |
| 6   | **Server aggregation** — per-worker cache keyed by `registered_worker_id`; freshness-window staleness + reconnect-guarded liveness drop; `Counter::absolute()` republish; server-stamped `worker` label                                                               | M    |
| 7   | **Stack & compose** — prometheus, grafana, server-host node_exporter; provisioned dashboards                                                                                                                                                                          | M    |
| 8   | **Follow-up design doc + plan** — finalize snapshot schema, label sets, buckets, staleness policy before implementation                                                                                                                                               | S    |

Items 1–3 are identical to 001 and deliver most of the wish list (server-owned
metrics) before any worker/protocol work. The push pipeline is items 4–6.

## Risks & open questions

- **Server becomes a metrics aggregator.** Modest new responsibility and memory
  (one small snapshot per connected worker). Bounded by fleet size.
- **Staleness correctness.** Resolved in
  [Staleness & the reconnect race](#staleness--the-reconnect-race-f3): a
  freshness window is the primary mechanism and the liveness-driven drop is
  guarded against the reconnect race. Residual risk is only mis-tuning
  `stale_after`.
- **Counter-reset semantics.** Republishing worker cumulative counters uses
  `Counter::absolute()` and relies on `rate()` tolerating resets; document
  chosen metric types so dashboard authors use `rate()`/`increase()` correctly.
- **Host-metric coverage gap.** `sysinfo` is narrower than node_exporter (e.g.
  per-device IO, filesystem internals). Scope to the dashboard's needs; document
  what is intentionally not collected. If breadth is later required on a worker,
  that worker would need node_exporter — contradicting the constraint, so treat
  as out of scope.
- **WS contention.** Metrics share the single capacity-64 outbound channel with
  build output. Resolved in
  [Shared-channel backpressure](#shared-channel-backpressure-f4): `try_send`
  drop-on-full prevents the push from stalling build handling, and the freshness
  window absorbs dropped samples. Residual risk is only sustained drops under a
  pathologically chatty build, which the drop counter surfaces.
- **Protocol evolution.** The typed snapshot couples metric schema into
  `cbsd-proto`; adding a host/app field is a proto change on both sides. Accept
  for type-safety; if churn becomes painful, a generic `gauges`/`counters` map
  field can absorb additions without enum changes (noted, not adopted now).

## Non-goals

Same as 001: no Loki/log aggregation, no distributed tracing, no alert rules in
this proposal, and no exact metric-registration code (deferred to work item 8).

## Alternatives summary

| Decision             | Options (this variant in **bold**)                                             | Why                                                                |
| -------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| Worker transport     | direct scrape · per-host remote_write agent · **push over existing WebSocket** | Only the WS needs no inbound reach and no per-host daemon          |
| Worker host stats    | node_exporter+agent · **in-process `sysinfo`, pushed**                         | Zero daemons on hard-to-reach hosts; accept narrower coverage      |
| Snapshot wire format | opaque exposition text · **typed snapshot**                                    | Reviewable, versioned, consistent with `cbsd-proto`                |
| Disconnect handling  | spool & replay · **skip & drop-stale**                                         | Metrics are point-in-time; reuse liveness; preserve spool for logs |
| Server host stats    | in-process sysinfo · **node_exporter sidecar (server only)**                   | Server is reachable; node_exporter's breadth is free there         |
| Build-outcome source | worker-reported · **server-derived** (unchanged from 001)                      | Single source of truth; no double-counting                         |

### When to revisit

If worker hosts ever become reachable/manageable (co-located, in-cluster with
service discovery), the 001 approach (node_exporter + remote_write agent)
becomes preferable for its richer host coverage, and the WS push for host stats
can be retired while keeping the cbsd-specific app metrics on whichever path is
simpler.
