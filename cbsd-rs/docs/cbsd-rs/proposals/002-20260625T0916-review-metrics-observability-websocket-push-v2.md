# 002 — Review: Metrics & Observability (WebSocket-push variant) — v2

## Status

Second-round adversarial design review of
`002-20260625T0839-metrics-observability-websocket-push.md` (now revised to v2
per its "Revision history"), against the v1 review
`002-20260625T0851-review-metrics-observability-websocket-push-v1.md` (findings
F1–F8). Review type: `design`. Verdict: **conditional go** — the revision
genuinely resolves the two load-bearing correctness gaps from v1 (F3 series-drop
race, F4 shared-channel backpressure) and corrects the three factual claims (F1,
F2, F5), which is real progress. But the fix for F1 introduced **one new
blocking gap**: the `Welcome.accepts_metrics` capability handshake the proposal
now relies on is not backward-compatible as described unless the field is
`#[serde(default)]`, and the proposal neither says so nor acknowledges that the
worker's Welcome parse is the strict (connection-dropping) path, not the lenient
one. With that corrected the design is implementable.

All findings re-verified against the source at review time. Citations are
`file:line`.

## Method

Code re-read and cross-checked for the v2 claims:

- `cbsd-proto/src/ws.rs` — `WorkerMessage` internal tagging
  (`#[serde(tag = "type")]`, line 129), `ServerMessage::Welcome` field set
  (lines 54–60), SI-18 unknown-field test (`cases()` "welcome", lines 684–692;
  `no_deny_unknown_fields_on_server_message`, line 717). No `#[serde(default)]`
  on any `Welcome` field; no `deny_unknown_fields`.
- `cbsd-server/src/ws/handler.rs` — `protocol_version != 2` hard reject (line
  141); unknown inbound message → `warn!` + continue (lines 434–440);
  `registered_worker_id` derived from `worker_row.id` (line 236), bound to API
  key at upgrade (line 68); reconnect/migration scan over `queue.workers` by
  `registered_worker_id` (lines 277–283); fresh `connection_id` per upgrade
  (line 83).
- `cbsd-server/src/ws/liveness.rs` — `WorkerState` carries
  `registered_worker_id` in `Connected`/`Disconnected`/`Stopping` (lines 25–44;
  accessor lines 57–73); grace monitor keyed by `connection_id` (lines 121–146).
- `cbsd-server/src/queue/mod.rs` — `workers` keyed by `ConnectionId` (verified
  via handler usage).
- `cbsd-server/src/ws/dispatch.rs` — `builds.worker_id` stamped from
  `ws.registered_worker_id().unwrap_or("unknown")` (line 88).
- `cbsd-worker/src/ws/handler.rs` — `OUTPUT_CHANNEL_CAPACITY = 64` (line 50);
  the outbound `mpsc::channel` is created **per connection** inside
  `run_connection` (line 91) and handed to the supervisor via `attach_transport`
  (line 92); worker's Welcome parse propagates a deser error and returns `Err`
  (lines 284–285, 325 path), i.e. the **strict** path.
- `cbsd-worker/src/build/supervisor.rs` — `Transport.outbound` (line 104);
  `on_output_message` **drops every message when no build is active** (lines
  270–275); `DEFAULT_SPOOL_CAP_BYTES = 64 MiB` (line 55).
- `cbsd-server/src/db/builds.rs` + `migrations/001_initial_schema.sql` —
  `started_at INTEGER` nullable (schema), `set_build_finished` stamps only
  `finished_at` (builds.rs:342), `rollback_dispatch_to_queued` sets
  `started_at = NULL` (builds.rs:266).
- `metrics` crate API for `Counter::absolute` (reasoned from the crate; no
  `metrics`/`sysinfo` dependency exists yet in either worker or server
  `Cargo.toml` — confirmed absent).

## Per-finding disposition (F1–F8)

| ID  | v1 severity | v2 disposition              | Evidence                                                     |
| --- | ----------- | --------------------------- | ------------------------------------------------------------ |
| F1  | MAJOR       | **Resolved, but regressed** | Compat correctly restated, but new handshake under-specified |
| F2  | MAJOR       | **Resolved**                | `registered_worker_id` is genuinely stable + join-correct    |
| F3  | MAJOR       | **Resolved**                | Freshness-primary + reconnect-guarded drop is implementable  |
| F4  | MAJOR       | **Partial**                 | Right policy; sender-ownership/idle-path mechanics glossed   |
| F5  | MINOR/MAJOR | **Resolved**                | `Counter::absolute()` + zero-window documented               |
| F6  | nit         | **Resolved**                | NULL/negative `started_at` guard now explicit                |
| F7  | MINOR       | **Resolved (deferred)**     | Pin + verify against pinned `sysinfo`; honest scope          |
| F8  | MINOR       | **Resolved**                | Server-stamped single-source `worker` label                  |

### F1 — Compatibility framing [Resolved] but the fix REGRESSED a new gap

**Resolved part.** The proposal now correctly states the server hard-rejects
`protocol_version != 2` (verified, `handler.rs:141`) and that the new message is
an **additive serde variant within protocol v2**, tolerated by the lenient
inbound parse (verified: unknown `WorkerMessage` → `warn!` + continue,
`handler.rs:434-440`; the internally-tagged enum at `ws.rs:129` dispatches an
unknown `type` to a serde error, which that arm swallows). This is exactly the
v1-required correction. Good.

**Regression (NEW, see N1 below).** To suppress per-push warn spam during a
rolling upgrade, v2 introduces a new gating handshake: the worker "only begins
pushing after the server advertises support via a new additive boolean field on
`ServerMessage::Welcome` (e.g. `accepts_metrics`)" and asserts "Additive fields
are backward/forward compatible (per design addendum SI-18 on unknown fields)."
That justification is only half-correct and the half it omits is load-bearing —
see **N1 [BLOCKING]**.

### F2 — Series identity [Resolved]

v2 keys the cache and `worker` label on `registered_worker_id` and explicitly
reconciles this with the design's "`worker_id` is display-only" rule by scoping
the departure to metric-series identity (continuity across reconnects) vs.
routing identity (per-connection isolation). Verified the underlying facts:

- `registered_worker_id` is derived from `worker_row.id`, bound to the API key
  at upgrade (`handler.rs:68,236`), and is the same value already migrated
  across reconnects (`handler.rs:277-283`). It is genuinely stable.
- The server already persists this same value into `builds.worker_id` at
  dispatch (`dispatch.rs:88`). So a server-stamped `worker` label sourced from
  `registered_worker_id` joins **exactly** with the server-owned per-worker
  build metrics, which key off `builds.worker_id`. The F8 join is real, not
  asserted.

The "must be present and unique per worker" precondition is correct and already
holds for authenticated workers (the upgrade path 403s an API key not bound to a
registered worker, `handler.rs:74-80`).

### F3 — Reconnect-safe staleness [Resolved]

v2 promotes the freshness window to the **primary** mechanism (export only while
`now − last_push < stale_after`) and makes the liveness-driven drop **secondary
and guarded**: a `Dead` transition evicts "only if no live connection currently
maps to that `registered_worker_id`." Both layers are implementable against the
actual code:

- The freshness window is connection-independent and inherently reconnect-safe
  (a live worker refreshes `last_push`), exactly as claimed.
- The reconnect-guard is feasible: `WorkerState::Connected` carries
  `registered_worker_id` (`liveness.rs:25-32`) and the handler already performs
  precisely this scan — "find a `workers` entry with this `registered_worker_id`
  that is `Connected`" — at migration time (`handler.rs:277-283`). The eviction
  can reuse that predicate. There is no missing data structure; the v1 "required
  but unspecified `connection_id → worker_id` join" now has a concrete, existing
  source.

This is the strongest of the revisions. One residual nit (minor): the doc says
the freshness window is "the backstop if the [liveness] check is ever wrong" —
but with freshness as primary, the guarded liveness drop is a pure latency
optimization and could be dropped entirely without correctness loss. Worth
saying so, so an implementer does not treat the secondary path as mandatory.

### F4 — Shared-channel backpressure [Partial]

The **policy** is now right and matches the code's constraints: `try_send`
drop-on-full over the single capacity-64 channel (`OUTPUT_CHANNEL_CAPACITY`,
verified `handler.rs:50`), no blocking `send().await`, bypass the spool, skip
when detached, count drops. All correct in principle.

What is still glossed (downgraded from v1's BLOCKING to a remaining MAJOR, see
**N2**): the proposal says "the periodic sampler holds the connection's outbound
sender directly." In the code, that sender (`out_tx`) is created **inside
`run_connection` per connection** (`handler.rs:91`) and is moved into the
supervisor via `attach_transport` (`handler.rs:92`); it is not exposed to any
standalone task. Two consequences the proposal does not address:

1. **Re-plumbing on every reconnect.** A fresh `out_tx` is minted per
   connection. A long-lived sampler task must be re-handed the new sender on
   each reconnect (and have it cleared on disconnect so "skip when detached"
   actually holds). This is a per-connection wiring step that does not exist
   today; "holds the outbound sender directly" hides it.
2. **The supervisor path cannot carry idle-worker metrics.** The proposal wants
   host CPU/mem pushed continuously — including when the worker is idle. But the
   supervisor's `on_output_message` **drops any message when `state.active` is
   `None`** (`supervisor.rs:270-275`). So metrics must **not** route through the
   supervisor at all; they must use a direct clone of `out_tx`. The proposal's
   "bypass the output spool" wording is consistent with this, but it never
   states the stronger, necessary fact: the sampler must be wired to the raw
   per-connection channel independently of the supervisor, because the
   supervisor is build-scoped and would discard idle-time samples. An
   implementer following the prose literally could plausibly hang the sampler
   off the supervisor and silently lose all idle-worker host metrics.

Not a correctness landmine the way v1's was (the design will work once wired
correctly), but the ownership/lifecycle of the sender across reconnects and the
idle-path exclusion are exactly the mechanics F4 asked to be pinned, and they
are still soft. Hence Partial.

### F5 — Counter republish [Resolved]

v2 mandates `Counter::absolute()` (correct: the `metrics` facade's `Counter`
exposes `absolute(value: u64)` to set an absolute value, distinct from
increment-only `increment`; a naive `increment` per push would double-count) and
documents the one-interval zero window after a server restart, absorbed by
`rate()`/`increase()`. Both the API claim and the semantics are accurate. The
"`Counter::absolute()` republish" is now also surfaced in the Risks section and
work item 6. Resolved.

### F6 — Duration histogram NULL guard [Resolved]

v2 states the observation is recorded "only when `started_at` is present and
`finished_at ≥ started_at`." Premise fully verified: schema has
`started_at INTEGER` nullable with no default (`001_initial_schema.sql`);
`set_build_finished` stamps only `finished_at` and never `started_at`
(`builds.rs:342`); `rollback_dispatch_to_queued` clears `started_at = NULL`
(`builds.rs:266`). So a revoked/failed-before-start build genuinely has
`finished_at` set and `started_at` NULL, and the guard is exactly what is
needed. Resolved.

### F7 — `sysinfo` coverage [Resolved as deferred]

v2 commits to pinning the `sysinfo` version, verifying per-mount and per-device
IO surface against that pinned version during the follow-up design, and falling
back to `/proc` (`/proc/diskstats`, `/proc/net/dev`) where insufficient, with
remaining gaps documented. No `sysinfo` dependency exists yet (confirmed absent
from the worker `Cargo.toml`), so this is the honest treatment v1 asked for.
Acceptable as a scoped deferral.

### F8 — Single source for the `worker` label [Resolved]

v2 stamps the `worker` label value **server-side** from the connection's
`registered_worker_id`; the worker never sends its own label value. This both
fixes the join (same source as `builds.worker_id`, `dispatch.rs:88`) and adds a
genuine anti-spoofing property (a worker cannot claim another worker's series).
Good improvement over v1, which only asked for label-value alignment.

## New findings (this round)

### N1 — `Welcome.accepts_metrics` is not backward-compatible as described [BLOCKING]

The F1 fix leans entirely on a new capability flag: an additive boolean
`accepts_metrics` on `ServerMessage::Welcome`, with "an old server never sets it
and the worker stays silent," justified "per design addendum SI-18 on unknown
fields." This conflates two different serde behaviours:

- **Unknown (extra) fields** — covered by the absence of `deny_unknown_fields`.
  The `Welcome` type has no `deny_unknown_fields`, and the SI-18 test proves it
  (`cases()` injects `future_field` into the "welcome" payload and asserts deser
  succeeds, `ws.rs:684-692,746-755`). So an **old worker** parsing a **new
  server's** Welcome (with the extra `accepts_metrics`) is fine.
- **Missing fields** — NOT covered by anything SI-18 says. The dangerous
  direction is a **new worker** parsing an **old server's** Welcome, which
  **lacks** `accepts_metrics`. serde treats a missing field as a hard
  deserialization error **unless** the field has `#[serde(default)]` (or is
  `Option` with default). None of the existing `Welcome` fields carry
  `#[serde(default)]` (`ws.rs:55-60`), and the proposal does not say the new
  field must.

This matters because the worker parses Welcome on the **strict** path, not the
lenient one: `wait_for_welcome` does
`serde_json::from_str(&text).map_err(HandlerError::Deserialize)?`
(`handler.rs:284-285`) and any error returns `Err`, which tears the connection
down and triggers reconnect/backoff. So if `accepts_metrics` is added
**without** `#[serde(default)]`, a freshly upgraded worker talking to a
not-yet-upgraded server would fail to deserialize Welcome and **fail to connect
at all** — a far worse rolling-upgrade outcome than the warn spam the flag was
meant to avoid, and the exact reverse-direction breakage the variant approach
was chosen to prevent.

Required fix: the proposal must specify `accepts_metrics` as `#[serde(default)]`
(defaulting to `false`), and should stop citing SI-18 as the justification —
SI-18 is about unknown fields; missing-field tolerance is a separate
`#[serde(default)]` requirement. It should also add a round-trip test for
"old-server Welcome (field absent) → new worker deserializes to
`accepts_metrics = false`," mirroring the existing
`worker_message_hello_arm64_alias` / `build_revoke_absent_reason_*` precedents
(`ws.rs:296,469`). Work item 4 already lists "round-trip + unknown-variant
tolerance tests" but must add the **missing-field-defaults** test explicitly.

### N2 — Outbound-sender ownership and idle-path exclusion under-specified [MAJOR]

See F4 above. The two concrete mechanics — re-handing the per-connection
`out_tx` to the sampler on each reconnect (and clearing it on detach), and
keeping the sampler off the supervisor so idle-time host metrics are not dropped
by `on_output_message`'s no-active-build drop (`supervisor.rs:270-275`) — are
not stated. The follow-up design (work item 8) must specify where the sampler
task lives, how it acquires/loses the outbound sender across reconnects, and
that it never routes through the supervisor.

### N3 — "skip when detached" needs a defined sampler lifecycle [MINOR]

The proposal says metrics are skipped "when detached" and the worker "resumes on
reconnect." Given the sender is per-connection, "detached" must be defined as
"the sampler's currently-held sender has been dropped/closed." The natural
implementation (sampler holds an `Option<Sender>` swapped on each
connect/disconnect, or the sampler is spawned per-connection and dies with it)
should be named, so "skip when detached" is not left to interpretation. Minor,
but it is the same soft spot as N2 viewed from the lifecycle side.

### N4 — node_exporter-on-server contradicts the "zero extra daemons" framing it is contrasted against [MINOR]

The doc's defining property is "zero extra daemons on worker hosts," which it
honours. But the Server-host-metrics section recommends a node_exporter sidecar
on the server host, while the same section and the stack table also offer the
in-process `sysinfo` collector "for symmetry and zero extra daemons anywhere."
This is internally consistent (the constraint is worker-scoped) but the
"recommended: node_exporter" default quietly reintroduces a daemon the rest of
the document is at pains to eliminate, and the reusable in-process collector is
described as already being library code the server "can reuse." Recommending the
daemon as the default, when the daemon-free path is claimed to be nearly free,
is a mild tension worth a one-line justification (it is: the server host is the
easy case). Not blocking; flagged for honesty.

## Strengths (including genuine improvements over v1)

- **F3 and F4 were the two load-bearing gaps in v1 and both are materially
  better.** F3 is now fully implementable against an existing predicate
  (`handler.rs:277-283`); F4's policy is correct for the real capacity-64 single
  channel. This is the substance the v1 verdict conditioned "go" on.
- **F2/F8 reconciliation is correct and verified end-to-end**, not asserted: the
  server-stamped `worker` label and `builds.worker_id` share one source
  (`dispatch.rs:88` ↔ `registered_worker_id`), so the per-build/host join is
  real, and server-stamping adds a spoofing guard v1 did not ask for.
- **F5 and F6 are now precise and code-accurate** — `Counter::absolute()` is the
  right API, and the `started_at`-NULL guard matches the actual schema and
  `set_build_finished` behaviour.
- The central architecture (push over the existing authenticated, reconnecting,
  outbound WS; server is sole scrape target; server owns all build/queue
  metrics) remains sound and well-matched to the codebase, exactly as v1 found.
- The revision history is specific and traceable to each v1 finding, which made
  this round materially easier to adjudicate — a good practice.

## Confidence score

Scored per the confidence-scoring criteria, treating "the change" as the v2
design: deductions apply to design correctness, internal consistency, claims
contradicted or unsupported by code, and gaps deferred without a concrete plan.

| Item                                                                                     | Points | Description                                                                                    |
| ---------------------------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------- |
| Starting score                                                                           | 100    |                                                                                                |
| D8: `Welcome.accepts_metrics` missing-field compat unsupported; SI-18 mis-cited (N1)     | -20    | Needs `#[serde(default)]`; worker parses Welcome on strict path (`handler.rs:284-285`)         |
| D1: outbound-sender ownership across reconnect + idle-path exclusion unspecified (F4/N2) | -15    | `out_tx` is per-connection (`handler.rs:91`); supervisor drops idle msgs (`supervisor.rs:270`) |
| D8: "skip when detached" lacks a defined sampler lifecycle (N3)                          | -5     | "detached" undefined given per-connection sender                                               |
| D10: node_exporter default reintroduces a daemon the doc otherwise eliminates (N4)       | -5     | internal tension; needs one-line justification                                                 |
| D11: F3 secondary liveness-drop presented as backstop, not optional optimization         | -5     | with freshness primary, the guarded drop is pure latency optimization                          |
| **Total**                                                                                | **50** |                                                                                                |

**Score: 50 / 100 — significant issues; address N1 before proceeding.**

**Delta vs v1: 30 → 50 (+20).** The +20 reflects that the two MAJOR mechanism
gaps that dominated v1 (F3 -20, F4 -20) are resolved/partial (F3 fully off the
board; F4 reduced from -20 to -15), and the three factual-claim corrections
(F1/F2/F5, -5 each in v1) plus the F6 nit and F7/F8 are all cleared. The score
does not climb higher because the F1 fix introduced a new **blocking**
serde-compatibility gap (N1, -20) that, if implemented as written, breaks the
very rolling-upgrade case the variant approach exists to protect — so the raw
band stays at the "significant issues" boundary even though the architecture is
now closer to ready than v1.

Interpretation note: unlike v1, the residual weight is concentrated in **one**
genuinely blocking item (N1) that is a small, well-scoped fix
(`#[serde(default)]` + one round-trip test + drop the SI-18 citation). With N1
corrected and N2 pinned in the work-item-8 follow-up, this is a clear go.

## Required changes before implementation (ordered)

1. **N1 [BLOCKING]** — specify `Welcome.accepts_metrics` as `#[serde(default)]`
   (default `false`); stop citing SI-18 (unknown-field) as the basis — the
   requirement is missing-field tolerance. Add an "old-server Welcome, field
   absent → `accepts_metrics = false`" round-trip test to work item 4.
2. **N2/F4 [MAJOR]** — pin the sampler's location and how it acquires/releases
   the per-connection `out_tx` across reconnects, and state explicitly that it
   does **not** route through the supervisor (which drops messages while idle).
3. **N3 [MINOR]** — define "detached" in terms of the sampler's sender
   lifecycle.
4. **N4 [MINOR]** — add a one-line justification for the node_exporter default
   vs. the "zero daemons anywhere" alternative.
5. **F3 nit [MINOR]** — note the guarded liveness drop is an optional latency
   optimization once the freshness window is primary.
