# 002 — Review: Metrics & Observability (WebSocket-push variant) — v3

## Status

Third-round adversarial design review of
`002-20260625T0839-metrics-observability-websocket-push.md` (now revised to v3
per its "Revision history"), against the v2 review
`002-20260625T0916-review-metrics-observability-websocket-push-v2.md` (which
carried forward F1–F8 and raised N1 BLOCKING, N2 MAJOR, N3/N4 MINOR; scored
50/100). Review type: `design`. Verdict: **go** — every v2 finding is genuinely
resolved against the actual source, the one blocking item (N1) is now specified
correctly with the right precedent and the right test, and no fix introduced a
new defect. The remaining open items are honestly scoped deferrals to the
follow-up design (work item 8), not correctness gaps.

All findings re-verified against the source at review time. Citations are
`file:line`.

## Method

Code re-read and cross-checked for the v3 claims:

- `cbsd-proto/src/ws.rs` — `ServerMessage::Welcome` field set: three required
  fields `protocol_version`, `connection_id`, `grace_period_secs`, **none**
  carrying `#[serde(default)]` (lines 55–60). `BuildRevoke.reason` carries
  `#[serde(default, skip_serializing_if = "Option::is_none")]` (lines 50–51) —
  the cited N1 precedent, confirmed verbatim. `WorkerMessage` and
  `ServerMessage` are internally tagged (`#[serde(tag = "type")]`, lines 23,
  129). No `#[serde(deny_unknown_fields)]` anywhere in `cbsd-proto`,
  `cbsd-server`, or `cbsd-worker` (grep clean); the SI-18 test actively asserts
  its absence (`no_deny_unknown_fields_on_server_message`, lines 717–760), and
  the "welcome" case injects `future_field` and asserts deser succeeds (lines
  683–692) — proving **unknown**-field tolerance, not **missing**-field
  tolerance.
- `cbsd-worker/src/ws/handler.rs` — `OUTPUT_CHANNEL_CAPACITY = 64` (line 50);
  outbound `mpsc::channel::<WorkerMessage>` created **per connection** inside
  `run_connection` (line 91); `out_tx.clone()` handed to the supervisor via
  `attach_transport` (line 92). `wait_for_welcome` parses Welcome on the
  **strict** path —
  `serde_json::from_str(&text).map_err(HandlerError::Deserialize)?` (line 285) —
  and destructures all three required fields (lines 288–292); any error returns
  `Err`, tearing down the connection. The post-handshake Welcome arm only
  `warn!`s and ignores (lines 189–191), but that is not the parse path N1
  concerns. `out_rx` is drained in the `select!` loop (lines 217–234): it
  special-cases only `BuildFinished` (for `retire`) and otherwise forwards any
  `WorkerMessage` verbatim via `send_msg`.
- `cbsd-worker/src/build/supervisor.rs` — `Transport { outbound: mpsc::Sender }`
  (lines 102–104); `attach_transport` simply stores a `Transport` from the
  caller-provided sender, replacing any prior one (lines 422–424);
  `on_output_message` **drops every message when `state.active` is `None`**
  ("dropping orphan output message", lines 270–275). Confirms the supervisor is
  build-scoped and would discard idle-time samples.
- `cbsd-server/src/ws/handler.rs` — `registered_worker_id` derived from
  `worker_row.id` (line 236); reconnect/migration scan over `queue.workers` by
  `registered_worker_id` with a different `connection_id` (line 278);
  `WorkerState::Connected { registered_worker_id, .. }` carried into the queue
  (lines 316, 700). Stable across reconnects, as claimed.
- `cbsd-server/src/ws/dispatch.rs` — `builds.worker_id` stamped from
  `ws.registered_worker_id().unwrap_or("unknown")` (line 88). The server-owned
  per-worker build metrics therefore key off the **same** value a server-stamped
  `worker` label would use — the F8 join is real, not asserted.

## Per-finding disposition (round-2 findings)

| ID            | v2 severity | v3 disposition | Evidence                                                           |
| ------------- | ----------- | -------------- | ------------------------------------------------------------------ |
| N1            | BLOCKING    | **Resolved**   | `#[serde(default)]` spelled out; SI-18 mis-citation dropped        |
| N2            | MAJOR       | **Resolved**   | Per-connection sampler task holding `out_tx.clone()`, bypasses sup |
| N3            | MINOR       | **Resolved**   | "detached" defined via per-connection task lifecycle               |
| N4            | MINOR       | **Resolved**   | Worker-scoped property + server-host cost class justified          |
| staleness fmt | MINOR       | **Resolved**   | Freshness window now sole mechanism; liveness drop = optional      |

### N1 — `Welcome.accepts_metrics` backward compatibility [Resolved]

The v3 proposal now states plainly that the new field **MUST be
`#[serde(default)]`** (lines 207–219) and gives the correct reasoning: SI-18
buys unknown-field tolerance, not missing-field tolerance, and an upgraded
worker deserializing a pre-upgrade server's `Welcome` (which omits the field)
would otherwise hit a hard serde error and fail to connect. Every load-bearing
fact in that paragraph is verified:

- The worker parses `Welcome` on the **strict** path. `wait_for_welcome` does
  `serde_json::from_str(&text).map_err(HandlerError::Deserialize)?`
  (`handler.rs:285`) and destructures all three current fields
  (`handler.rs:288-292`); a missing required field is a hard error that returns
  `Err` and tears down the connection. The proposal's claim "the worker parses
  `Welcome` on the strict path" is correct.
- The `Welcome` struct currently has **only required fields** (`ws.rs:55-60`) —
  none carry `#[serde(default)]` — so a missing `accepts_metrics` without the
  attribute genuinely **would** be a parse error.
- The cited precedent is exact: `ServerMessage::BuildRevoke.reason` carries
  `#[serde(default, skip_serializing_if = "Option::is_none")]` at `ws.rs:50-51`
  (the proposal cites `ws.rs:48-52`, which is the doc-comment + attribute +
  field span — accurate).
- The proposal **stops** citing SI-18 as the justification for the missing-field
  case (it correctly relegates SI-18 to unknown-field tolerance, lines 209–212)
  and **adds the required round-trip test** to work item 4
  ("old-server→new-worker missing-field ⇒ `false`", line 440).
- The degraded semantics are correct: missing ⇒ `false` ⇒ "server does not
  support metrics → stay silent," which is the desired no-op for a not-yet-
  upgraded server.

This is the exact fix v2's N1 required, with the exact precedent and the exact
test. Resolved.

One refinement worth noting (not a defect): the proposal says
`#[serde(default)]`, which for a bare `bool` defaults to `false` via `Default` —
correct. It does not need `Option<bool>`; a plain
`#[serde(default)] accepts_metrics: bool` is sufficient and is the simplest
form. The work item wording is consistent with that.

### N2 — Outbound-sender ownership & idle-path exclusion [Resolved]

v3 adds a dedicated section, "Sampler lifecycle & sender ownership" (lines
270–289), that pins exactly the mechanics v2 found soft. Verified:

- **Per-connection sender, confirmed.** `out_tx` is created fresh inside
  `run_connection` on every (re)connect (`handler.rs:91`) and cloned to the
  supervisor (`handler.rs:92`). The proposal cites `handler.rs:91-92` accurately
  and correctly describes the sampler's clone as "a sibling of the supervisor's
  clone, not routed through it."
- **Idle-path exclusion, confirmed.** The supervisor's `on_output_message` drops
  every message when no build is active (`supervisor.rs:270-275`, "dropping
  orphan output message"). The proposal cites `supervisor.rs:269-275` and
  correctly concludes that routing metrics through the supervisor "would
  silently lose exactly the idle-worker host stats we most want." The sampler
  therefore must hold its own `out_tx.clone()` and bypass the supervisor — which
  is what the section specifies.
- **Soundness of a sibling sender, confirmed.** Nothing in the outbound drain
  assumes the supervisor is the sole sender. `out_rx.recv()` in the `select!`
  loop (`handler.rs:217-234`) forwards any `WorkerMessage` via `send_msg` and
  only special-cases `BuildFinished` (to trigger `retire`). A `Metrics` variant
  injected by a sibling `out_tx.clone()` is simply serialized and sent on the
  wire, interleaved with build output at the channel's arrival order. There is
  no ordering invariant a metrics frame could violate (build-output ordering is
  enforced within the supervisor's own clone; metrics carry no build identity).
  So the design's chosen injection point is genuinely sound.

The "no long-lived sampler holding a swappable sender" framing (lines 283–284)
directly closes the "stale `out_tx` after reconnect" bug class v2 flagged: the
task lifetime equals the connection's, so there is no swap to get wrong.
Resolved.

### N3 — "skip when detached" sampler lifecycle [Resolved]

v3 defines "detached" concretely (lines 285–289 and 226–231): the sampler is a
per-connection task; "detached = the connection (and thus the task) is gone;
reconnect spawns a new one." The task exits naturally on `try_send` returning
`Closed` or is cancelled with the connection. This is the cleaner of the two
implementations v2 floated (per-connection spawn vs. swappable `Option<Sender>`)
and removes the interpretation gap. Resolved.

### N4 — node_exporter vs. "zero daemons" tension [Resolved]

v3's "Server host metrics" section (lines 381–394) now explicitly scopes the
"zero daemons" property to **worker** hosts and labels the reachable,
operator-managed server host a "different and acceptable cost class," giving the
one-line justification v2 asked for ("the server host is the easy case where its
breadth is free"). The revision history item (lines 40–42) records the same. The
in-process `sysinfo` alternative remains offered for operators who want zero
daemons even on the server. Internally consistent and honest. Resolved.

### Staleness framing [Resolved]

v3's "Staleness & the reconnect race" section (lines 318–347) now states the
freshness window is "the mechanism" and "**alone is sufficient** for
correctness… The implementation can ship with only this," while the
liveness-driven eviction is demoted to an "optional latency optimization,
guarded" that "is **not required for correctness**." This is exactly the v2
F3-nit ask: an implementer will no longer treat the secondary path as mandatory.
The guard itself remains correct — evict only if no live connection maps to that
`registered_worker_id`, reusing the existing migration-scan predicate
(`handler.rs:278`). Resolved.

## F1–F8 regression check

No regression. Spot-confirmed against source:

- **F1 (compat framing).** Still correct: hard-reject of `protocol_version != 2`
  and additive serde variant within v2; the unknown-inbound `warn!`+continue net
  is unchanged. The N1 fix strengthens F1 rather than regressing it.
- **F2 (series identity).** `registered_worker_id` from `worker_row.id`
  (`handler.rs:236`), stable across the reconnect scan (`handler.rs:278`).
  Diagram and §"Series identity" still key on it. Intact.
- **F3 (reconnect-safe staleness).** Strengthened by the staleness reframing
  above; the reconnect-guard predicate still exists in code. Intact.
- **F4 (shared-channel backpressure).** `try_send` drop-on-full over the
  capacity-64 channel (`handler.rs:50`), bypassing the spool. The N2 section
  reinforces this. Intact.
- **F5 (counter republish).** `Counter::absolute()` + documented zero window
  still present (lines 188–197, 350–354). Intact.
- **F6 (histogram NULL guard).** "only when `started_at` is present and
  `finished_at ≥ started_at`" still present (lines 166–170). Intact.
- **F7 (`sysinfo` coverage).** Pinned-version + `/proc` fallback + documented
  gaps still present (lines 363–374). Intact.
- **F8 (single-source `worker` label).** Server-stamped from
  `registered_worker_id`, joining `builds.worker_id` (`dispatch.rs:88`).
  Diagram/text still describe one server-side source; the worker never sends a
  label value. Intact and anti-spoofing property preserved.

## New findings (this round)

No blocking or major findings. Two minor observations, neither requiring a
change before the follow-up design:

### NF1 — `accepts_metrics` field type left implicit [MINOR]

The proposal says "additive boolean field … e.g. `accepts_metrics`" and "MUST be
`#[serde(default)]`" but does not state whether it is a bare `bool` (default
`false` via `Default`) or `Option<bool>`. A bare `#[serde(default)] bool` is the
correct and simplest choice and is consistent with the prose ("missing ⇒
`false`"); the BuildRevoke precedent happens to use `Option` only because its
semantics need three states. Worth one explicit word in work item 4 so the
implementer does not reach for `Option<bool>` by pattern-matching the cited
precedent. Cosmetic; does not affect correctness.

### NF2 — drop counter has no defined exposition path [MINOR]

The backpressure section says dropped samples "are counted in a server-resync-
safe local counter for visibility" (line 256). But the whole premise of this
variant is that workers expose no endpoint and only push over WS. A worker-
local drop counter is therefore only observable if it is itself carried in the
next `Metrics` push (as an `app`-section counter) — which is sensible but
unstated. If the channel is saturated badly enough to drop samples, the drop
count rides the same saturated channel, so it is best-effort by construction.
Note this in work item 8 so the counter lands in the snapshot schema rather than
in a phantom local-only sink. Minor; the data still surfaces eventually once the
burst clears.

## Strengths (including genuine improvements over v2)

- **The one blocking item is genuinely closed, with proof.** N1's
  `#[serde(default)]` requirement, the strict-parse rationale, the exact in-tree
  precedent (`BuildRevoke.reason`, `ws.rs:50-51`), the dropped SI-18
  mis-citation, and the explicit missing-field round-trip test are all present
  and all verified against the code. This is the difference between v2's
  conditional-go and v3's go.
- **N2/N3 are resolved at the design level, not deferred.** The new "Sampler
  lifecycle & sender ownership" section pins the per-connection task, the
  sibling `out_tx.clone()`, the supervisor bypass, and the detach semantics —
  and every cited line (`handler.rs:91-92`, `supervisor.rs:269-275`) checks out.
  The injection point is provably sound against the outbound drain.
- **The staleness reframing removes a real implementer trap.** Stating the
  freshness window as sufficient-alone, with liveness eviction explicitly
  optional, prevents someone from shipping the guarded-eviction race as if it
  were mandatory.
- **End-to-end label join re-confirmed.** Server-stamped `worker` label ↔
  `registered_worker_id` ↔ `builds.worker_id` (`dispatch.rs:88`) is a real,
  verified join, and the server-stamping anti-spoofing property holds.
- **The central architecture remains sound** and well-matched to the codebase
  (push over the existing authenticated, reconnecting, outbound-only WS; server
  is sole scrape target; server owns all build/queue metrics), exactly as the
  prior rounds found.
- **Revision history is specific and traceable** to each round-2 finding, which
  made adjudication fast.

## Confidence score

Scored per the confidence-scoring criteria, treating "the change" as the v3
design: deductions apply to design correctness, internal consistency, claims
contradicted or unsupported by code, and gaps deferred without a concrete plan.

| Item                                                                    | Points | Description                                                              |
| ----------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------ |
| Starting score                                                          | 100    |                                                                          |
| D11: `accepts_metrics` field type (bool vs Option) left implicit (NF1)  | -5     | Implementer could mis-pattern-match the `Option` precedent; cosmetic     |
| D9: drop-counter exposition path unstated under no-endpoint model (NF2) | -5     | Counter only surfaces via the next push; should be pinned in work item 8 |
| **Total**                                                               | **90** |                                                                          |

**Score: 90 / 100 — ready to proceed.** The design is implementable as written;
the two residual minors are work-item-8 schema notes, not correctness gaps.

**Delta vs v2: 50 → 90 (+40).** The jump reflects that the single blocking item
that capped v2 (N1, −20) is fully and correctly resolved with verified precedent
and a mandated test; the MAJOR sender-ownership/idle-path gap (N2, −15) is now
specified at the design level rather than deferred; and the three MINOR items
(N3, N4, F3-framing, −15 combined in v2) are each addressed in prose. No fix
introduced a new defect, and no F1–F8 item regressed. The two new minors are
genuinely small (−5 each), leaving the design in the "ready to merge" band.

## Recommended (non-blocking) before implementation

1. **NF1 [MINOR]** — state `accepts_metrics` as a bare `#[serde(default)] bool`
   (not `Option<bool>`) in work item 4, so the implementer does not copy the
   `Option` shape of the `BuildRevoke.reason` precedent unnecessarily.
2. **NF2 [MINOR]** — in work item 8, place the local drop counter in the pushed
   snapshot's `app` section so it is actually observable under the no-endpoint
   model.
