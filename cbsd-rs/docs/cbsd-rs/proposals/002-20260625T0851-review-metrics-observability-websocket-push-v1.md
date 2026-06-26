# 002 — Review: Metrics & Observability (WebSocket-push variant) — v1

## Status

Adversarial design review of
`002-20260625T0839-metrics-observability-websocket-push.md` (proposal v1), with
`001-20260625T0740-cbsd-rs-metrics-observability.md` read as context. Review
type: `design`. Verdict: **conditional go** — the core decision (push
worker/host metrics over the existing WebSocket, aggregate and re-expose on the
server) is sound and well-matched to the codebase, but three claims are
inaccurate against the code and must be corrected, and two correctness
mechanisms (drop-on-disconnect, shared-channel ordering) are under-specified in
ways that will produce wrong dashboards or dropped samples if implemented as
written.

All findings were verified against the actual source. Citations are `file:line`
against the tree at review time.

## Method

Code read and cross-checked:

- `cbsd-proto/src/ws.rs` — `WorkerMessage`/`ServerMessage`, `Hello`/`Welcome`,
  `protocol_version`, SI-18 unknown-field tolerance.
- `cbsd-server/src/ws/handler.rs` — upgrade/identity, version check, inbound
  parse-and-dispatch, grace/dead handling.
- `cbsd-server/src/ws/liveness.rs` — `WorkerState`, grace-period monitor.
- `cbsd-server/src/ws/dispatch.rs` — what is written to `builds.worker_id`.
- `cbsd-server/src/queue/mod.rs` — worker map keyed by `connection_id`.
- `cbsd-server/src/db/builds.rs`, `migrations/001_initial_schema.sql` (and
  `003`, `005`, `007`) — build timestamps, `worker_id`,
  `rollback_dispatch_to_queued`, `state`.
- `cbsd-server/src/db/mod.rs` — 4-connection pool / dispatch-mutex invariant.
- `cbsd-worker/src/build/supervisor.rs`, `cbsd-worker/src/ws/handler.rs`,
  `cbsd-worker/src/build/executor.rs` — outbound-only worker, spool budget,
  single outbound channel.
- `docs/cbsd-rs/design/README.md` — worker-identity model.

## Findings

Severity tags: **[BLOCKING]** must be resolved before implementation;
**[MAJOR]** correctness/feasibility gap that must be addressed in the follow-up
design (work item 8); **[MINOR]** nit or clarification.

### F1 — Protocol-version framing is inaccurate; the real compat mechanism is something else [MAJOR]

The proposal repeatedly justifies additive safety via version negotiation:
"gated by the existing `protocol_version` negotiated in `Hello`/`Welcome`. A
server that predates the variant ignores it; a worker that predates it simply
never sends one" (002 lines 134–136), and work item 4 lists "version gating"
(002 line 249).

The code does **not** negotiate a version range. The server requires the version
to be _exactly_ 2 and otherwise closes the connection:

- `cbsd-server/src/ws/handler.rs:141` —
  `if protocol_version != 2 { ... server supports 2 ... return; }`.

There is no `min_version`/`max_version` acceptance window on the server ingress
path; the `min_version`/`max_version` fields in `ServerMessage::Error`
(`ws.rs:63-67`) are only populated on the _reject_ path and carry the single
value `2` (handler.rs:123, 153). Consequences:

- You **cannot** "gate" the new variant behind a bumped `protocol_version`:
  bumping it to 3 hard-rejects every un-upgraded peer in the fleet. The variant
  must ship _within_ protocol v2 as a purely additive serde variant.
- What actually makes it safe is unrelated to version numbers: it is (a) serde's
  tagged-enum dispatch plus (b) the server's tolerant inbound parse, which logs
  and continues on an unrecognized message rather than dropping the connection —
  `handler.rs:434-440` (the `Err(e)` arm only `warn!`s; the receive loop keeps
  running). An old server receiving a `metrics` frame logs
  `failed to parse worker message` once per push and otherwise behaves normally.

This is a correct-but-noisy outcome, not the clean "ignores it" the proposal
implies. The follow-up design must (1) state that the variant is additive within
v2, not version-gated, and (2) decide whether per-push warn spam on an old
server during a rolling upgrade is acceptable or whether the worker should
suppress metrics until it has seen evidence the server supports them (there is
no such handshake today, so this likely means "accept the warn spam for the
upgrade window"). The SI-18 machinery in `ws.rs` covers unknown _fields_ on
`ServerMessage`, not unknown _variants_ on `WorkerMessage`, so it does not
protect this case — the tolerant parse at handler.rs:434 does.

### F2 — "Stable `worker_id`" is true but collides with the documented identity model [MAJOR]

The proposal keys all pushed series on a "stable `worker_id`" and asserts
"`connection_id` is per-connection and not used as a label" (002 lines 168–172).
Against the code:

- The value persisted to `builds.worker_id` at dispatch is the worker's
  registration id, which **is** stable across reconnects:
  `cbsd-server/src/ws/dispatch.rs:88` —
  `ws.registered_worker_id().unwrap_or("unknown")`, derived from `worker_row.id`
  at upgrade (`handler.rs:236`, bound to the API key at `handler.rs:68`). So the
  proposal's stability claim is _correct_.
- However, the authoritative design says the opposite about which id is
  canonical: `docs/cbsd-rs/design/README.md:252` — "Worker identity.
  Server-assigned connection handle (UUID), not `worker_id` string. `worker_id`
  is display-only." A fresh `connection_id` is minted on every upgrade
  (`handler.rs:83`).

So the proposal builds a long-lived, externally-visible Prometheus label
namespace on an identifier the design explicitly calls "display-only." That is
not automatically wrong, but it promotes a display string to a stable metric key
without acknowledging the tension. The follow-up design must either (a) justify
elevating `registered_worker_id` to a label-grade stable key (and note that its
uniqueness/immutability guarantees now matter for metrics, not just display), or
(b) define an explicit, stable `worker` label sourced from worker registration
rather than reusing the display-only string by name. Silently relabeling
"display-only" as "stable series key" is the kind of drift that bites later.

### F3 — "Drop stale series on disconnect via `WorkerState`" has a reconnect race [MAJOR]

The proposal reuses the liveness state machine to drop a worker's pushed series
when it goes `Disconnected past grace` / `Dead` (002 lines 161–167). The
mechanism does not line up cleanly with how liveness is keyed:

- `WorkerState` lives in `BuildQueue.workers`, keyed by **`connection_id`**, not
  by `worker_id`: `cbsd-server/src/queue/mod.rs:91` ("keyed by server-assigned
  connection UUID"), and the grace monitor looks the worker up by
  `connection_id` (`liveness.rs:121-126`, `:142`).
- Pushed series are keyed by `worker_id` (F2). There is therefore a required but
  unspecified `connection_id → worker_id` join to translate a liveness
  transition into a series drop.
- The race: on reconnect the worker gets a **new** `connection_id`
  (`handler.rs:83`) and is registered fresh, while the **old** `connection_id`
  can still be `Disconnected` inside its grace window and will later fire
  `start_grace_period_monitor` → `Dead` → `handle_worker_dead`
  (`liveness.rs:142-146`). If the drop-series logic triggers on the old
  connection's `Dead` transition and keys the drop by `worker_id`, it will
  **delete the series of the already-reconnected, live worker** — exactly the
  same `worker_id` — producing a metrics gap or a flapping series for a healthy
  worker.

The proposal's own "continuity across reconnects" paragraph (002 lines 168–173)
assumes one continuous series per `worker_id`, which is precisely what the
connection-keyed liveness drop can break. The follow-up design must specify the
drop predicate as: drop a `worker_id`'s series only when **no live connection**
currently maps to that `worker_id` (i.e. ref-count connections per `worker_id`,
or check the current connection map at drop time), not merely when "a"
connection for that worker reached `Dead`. The "freshness timestamp guard"
mentioned as optional (002 line 166) is closer to a correct primary mechanism
than the liveness reuse and should probably be mandatory, since it is
independent of the connection-id bookkeeping.

### F4 — Shared outbound channel: head-of-line/backpressure interaction is dismissed too quickly [MAJOR]

The proposal treats WS contention as negligible because payloads are tiny (002
lines 149–150, 273–275). The concern is not payload size; it is the single
bounded outbound channel and its backpressure semantics:

- All worker→server messages funnel through one `mpsc::Sender<WorkerMessage>` of
  capacity 64: `cbsd-worker/src/ws/handler.rs:48`
  (`OUTPUT_CHANNEL_CAPACITY = 64`) and `:91` (the channel), drained by a single
  writer loop.
- During a noisy build, `BuildOutput` frames (batched every 200 ms or 50 lines,
  `ws.rs:161-167`) can fill that channel. A periodic metrics push contends for
  the same 64 slots. Depending on how the push is implemented: a blocking
  `send().await` stalls the sampler (and whatever task drives it) behind build
  output; a `try_send` drops the metrics frame silently under load — and load is
  exactly when host/CPU metrics are most interesting.
- The supervisor's disconnect path additionally diverts messages to the 64 MiB
  on-disk spool (`supervisor.rs:55`, `DEFAULT_SPOOL_CAP_BYTES`). The proposal
  correctly says metrics must **not** be spooled (002 lines 137–141), but does
  not say _how_ the push avoids the spool path while build output uses it, given
  they share the `Transport.outbound` sender (`supervisor.rs:102-104`). If
  metrics go through the supervisor's send helper they risk being spooled or
  counted against the spool budget; if they bypass it they need a separate,
  explicitly-specified send path.

The follow-up design must specify: (1) whether the metrics push uses `try_send`
with documented drop-on-full semantics (acceptable for point-in-time gauges, but
say so), (2) that it bypasses the build-output spool entirely, and (3) that it
is skipped when `Transport` is detached (consistent with "no spooling on
disconnect"). "Tiny and periodic" does not by itself resolve any of these.

### F5 — Republishing worker cumulative counters needs absolute-set semantics; name the mechanism [MINOR/MAJOR]

The proposal sends counter-like fields as "cumulative since worker process
start" and has the server "republish the value verbatim," relying on `rate()` to
absorb resets (002 lines 129–133, 265–267). This is the right model, but it has
an implementation constraint the proposal does not call out: the chosen
`metrics` facade (adopted from 001 §"Server exporter mechanics", 002 lines
107–109) exposes counters as **increment-only** (`counter!(...).increment(n)`).
Republishing an externally-owned cumulative value requires _setting_ an absolute
value, i.e. `Counter::absolute(v)`, which exists but is a different API and is
easy to get wrong (a naive `increment(v)` each push double-counts
catastrophically). On server restart the recorder resets to zero and the first
post-restart push jumps the series from 0 to the worker's current cumulative —
fine for `rate()`/`increase()`, but note the "startup gauge resync from the
`builds` table" (002 lines 107–109) does **not** apply to pushed counters, so
there is a one-interval window where pushed counters read 0 after a server
restart. The follow-up design should explicitly mandate `Counter::absolute()`
(or a server-side delta tracker) and document the post-restart zero window.

### F6 — Server-owned metric set is well-grounded in the code [STRENGTH, with one nit]

The server-owned half (inherited from 001) checks out against the schema and
queue:

- The four lifecycle timestamps exist exactly as cited:
  `migrations/001_initial_schema.sql:99-102`
  (`submitted_at/queued_at/started_at/finished_at`), plus `worker_id`,
  `trace_id`, `state` (`:93-97`).
- `rollback_dispatch_to_queued` exists and is the right re-dispatch signal for
  `cbsd_build_requeues_total` (`db/builds.rs:259-276`).
- The 4-connection pool / dispatch-mutex deadlock invariant the pool gauge
  targets is real and documented in code: `cbsd-server/src/db/mod.rs:38-52`
  (`max_connections(4)`), matching CLAUDE.md invariant #2. The
  `cbsd_db_pool_connections` gauge is genuinely high-value.

Nit: 001's metric table maps `cbsd_build_results_total` to `set_build_finished`
and `cbsd_build_duration_seconds` to `finished_at − started_at`.
`set_build_finished` (`db/builds.rs:334-353`) sets `finished_at = unixepoch()`
but a revoked/failed build may have `started_at = NULL` (it never reached
`started` — see `rollback_dispatch_to_queued` clearing `started_at`, and `state`
allowing `revoked` without `started`). The duration histogram must guard against
`NULL`/negative `started_at` or it will emit garbage for non-started terminal
builds. Worth a line in the follow-up.

### F7 — `sysinfo` per-mount disk + IO coverage is asserted, not verified [MINOR]

The proposal commits `sysinfo` for CPU/load/memory/swap and per-mount disk plus
disk/network IO, "falling back to reading `/proc`" (002 lines 181–188).
`sysinfo`'s API surface for per-device IO counters and per-mount usage varies by
version and platform and has churned across releases; load average is Unix-only.
None of this is verified here (no `sysinfo` dependency exists yet — confirmed
absent from the worker `Cargo.toml`). The follow-up design should pin a
`sysinfo` version and confirm the specific calls for per-mount usage and
per-device IO exist in it, rather than assuming, and state the `/proc` fallback
as a concrete plan if they do not.

### F8 — Minor consistency / documentation nits [MINOR]

- 001's round-trip test for `Welcome` uses `protocol_version: 1` (`ws.rs:257`)
  while `Hello` uses `2` and the server enforces `2` (`handler.rs:141`).
  Pre-existing and not introduced by 002, but since 002 leans on
  `protocol_version` semantics, the follow-up should not cite the `Welcome`
  field as evidence of negotiation — it is unvalidated on the worker beyond a
  backoff-ceiling check.
- 002 says host metrics and build activity "share the `worker` label" for a
  "direct per-build join" (002 lines 219–225); server-owned per-worker series
  are labeled from `registered_worker_id` (F2) and pushed series must use the
  identical label _value_ for the join to work. Make the single source of the
  `worker` label value explicit so the two halves actually align.

## Strengths

- The central decision is correct for the stated constraint. The worker is
  genuinely outbound-only (no `TcpListener`/HTTP server in `cbsd-worker`), the
  WS is already authenticated, reconnecting, and multiplexes high-volume output,
  and the supervisor already outlives connections — so adding a periodic push is
  a natural, low-infrastructure fit. "Zero extra daemons on worker hosts" is a
  real, well-defended property.
- Keeping all build-outcome/duration/queue metrics server-owned (single source
  of truth, no double-counting) is correct and matches where the data actually
  lives (queue + `builds` table).
- Typed snapshot over opaque exposition text is the right call for a crate
  (`cbsd-proto`) that already enforces wire types and rolling-upgrade discipline
  (SI-18).
- The "no spooling for metrics, preserve the spool budget for build output"
  instinct is correct (the 64 MiB budget is real, `supervisor.rs:55`); it just
  needs the send-path mechanics of F4 to actually deliver it.
- Scoping `sysinfo` to a deliberate subset and naming node_exporter for the
  reachable server host is a pragmatic, honest trade.

## Confidence score

Scored per the confidence-scoring criteria, treating "the change" as the design
proposal: deductions apply to design correctness, internal consistency, claims
contradicted by code, and gaps deferred without a plan.

| Item                                                                                       | Points | Description                                                                                                         |
| ------------------------------------------------------------------------------------------ | ------ | ------------------------------------------------------------------------------------------------------------------- |
| Starting score                                                                             | 100    |                                                                                                                     |
| D8: protocol-version "negotiation/gating" contradicts hard `!= 2` reject (F1)              | -5     | `handler.rs:141`; real mechanism is tolerant parse, mis-described                                                   |
| D8: "stable `worker_id`" label vs design's "display-only" identity (F2)                    | -5     | `design/README.md:252` vs proposal's stable-key assumption                                                          |
| D1: drop-on-disconnect predicate unspecified and racy across reconnect (F3)                | -20    | liveness keyed by `connection_id` (`queue/mod.rs:91`), series keyed by `worker_id`; can drop a live worker's series |
| D1: shared-channel ordering / drop / spool-bypass for the push left unspecified (F4)       | -20    | single `mpsc` cap-64 (`ws/handler.rs:48,91`); backpressure & spool interaction not designed                         |
| D8: counter republish needs `Counter::absolute`, not stated; post-restart zero window (F5) | -5     | `metrics` facade is increment-only; naive increment double-counts                                                   |
| D3: duration histogram unguarded against NULL/negative `started_at` (F6 nit)               | -5     | `set_build_finished` sets `finished_at`; `started_at` may be NULL for revoked/failed                                |
| D11: `sysinfo` per-mount/IO coverage asserted without verifying the API (F7)               | -5     | version unpinned; per-device IO/per-mount calls unconfirmed                                                         |
| D11: `worker`-label single-source-of-value not made explicit for the join (F8)             | -5     | server-owned vs pushed label values must match exactly                                                              |
| **Total**                                                                                  | **30** |                                                                                                                     |

**Score: 30 / 100 — significant issues; address before proceeding.**

Interpretation note: the raw score lands in the "major rework" band, but the
weight is concentrated in two deferrable mechanism gaps (F3, F4) that the
proposal itself routes to work item 8 ("finalize ... staleness policy before
implementation"). The _architecture_ is sound; the score reflects that the two
load-bearing correctness mechanisms (series-drop and shared-channel delivery)
are currently described in a way that, if implemented literally, yields wrong
dashboards or dropped samples — and that three factual claims about the code
(F1, F2, F5) need correction. With F3 and F4 specified concretely and F1/F2/F5
corrected, this is a go.

## Required changes before implementation (ordered)

1. **F3** — define the series-drop predicate as "no live connection maps to this
   `worker_id`," not "a connection for this worker reached `Dead`"; make the
   freshness-timestamp guard the primary, connection-independent mechanism.
2. **F4** — specify the push send-path: `try_send` with documented drop-on-full,
   explicit spool bypass, skip-when-detached.
3. **F1** — restate compatibility as "additive variant within protocol v2,
   tolerated by the server's lenient parse," not version-gating; decide the
   rolling-upgrade warn-spam posture.
4. **F2** — justify or replace the `worker` label source; reconcile with the
   "display-only" identity in `design/README.md`.
5. **F5** — mandate `Counter::absolute()` (or server-side delta tracking);
   document the post-server-restart zero window.
6. **F6/F7/F8** — duration NULL guard; pin and verify `sysinfo`; single source
   for the `worker` label value.
