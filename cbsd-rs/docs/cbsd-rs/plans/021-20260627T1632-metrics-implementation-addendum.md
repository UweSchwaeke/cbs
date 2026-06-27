# Metrics Implementation Addendum — Divergences from Designs 021–023

**Scope:** records where the implemented metrics work (commits G1–G7 on
`wip/cbsd-rs-metrics`) intentionally diverges from the contract fixed in designs
021/022/023 and plans 021/022/023. The design documents are snapshots and are
deliberately left unedited (per the `seq-docs-convention`); this addendum is the
authoritative record of the deltas. It resolves the findings of the adversarial
implementation review
`021-20260627T1456-impl-metrics-protocol-and-worker-collector-v1.md`.

**Supersession:** the plan bodies are likewise frozen snapshots — only their
progress tables are updated in place. Where a plan's prose conflicts with what
shipped, this addendum supersedes it. In particular, plan 022's Commit-2 (G2a)
description still lists `cbsd_build_timeouts_total` and
`cbsd_sigkill_escalations_total` as G2a deliverables; their actual disposition
is the two D1 entries below (timeouts dropped; SIGKILL escalations delivered
worker-sourced in G6, not at the G2a server choke point).

---

## D1 — `cbsd_build_timeouts_total` is dropped (not emitted)

**Design/plan said:** design 022 catalogues `cbsd_build_timeouts_total{arch}` as
a server-owned counter sourced from the "build-timeout path", and plan 022 G2a
step 2 committed to emitting it.

**Reality:** there is no server-side build-timeout path. The build execution
timeout is enforced **inside the cbscore subprocess** via the
`CBS_BUILD_TIMEOUT` environment variable (`cbsd-worker/src/build/executor.rs`),
and it surfaces only as a generic `BuildFinished(failure)` on the wire. Neither
the server nor the Rust worker has a signal that distinguishes a timeout-failure
from any other failure, so the counter cannot be sourced without new plumbing (a
dedicated `BuildFinishedStatus`/`build_report` flag carried from the wrapper).

**Decision (option A):** drop the metric. It is not emitted, and the design-023
"Timeouts & SIGKILLs" dashboard panel is intentionally omitted
(`build-duration-slos.json` ships three panels, not four). If per-timeout
visibility becomes worthwhile, the follow-up is: classify timeout-failures in
the cbscore wrapper, surface the classification on the wire, and count it at the
server's terminal choke point (`db::builds::set_build_finished`).

## D1 — `cbsd_sigkill_escalations_total` is implemented (worker-sourced)

**Design/plan said:** design 022 catalogues `cbsd_sigkill_escalations_total` as
a server-owned counter from the "revoke/escalation path" with **no labels**;
plan 022 G2a step 2 committed to emitting it.

**Reality:** SIGTERM→SIGKILL escalation is a genuine event, but it happens
**worker-side**, in the worker's subprocess-termination logic
(`cbsd-worker/src/build/executor.rs`). The server never observes it, so the
design's server-owned, unlabelled placement is not realizable as specified.

**Resolution (now implemented):** delivered as a worker-sourced counter,
mirroring the other pushed app counters. A process-global `AtomicU64` is bumped
at the escalation branch in the executor, exposed on the wire through a new
`#[serde(default)] AppMetrics::sigkill_escalations_total` field, and re-exposed
server-side on ingest. Two **intentional deltas from the design**: the metric
keeps its design name `cbsd_sigkill_escalations_total` but now carries a
`worker` label (it is per-worker, like the other pushed series, rather than
unlabelled); and it counts escalation-**timer firings** — the executor sends the
SIGKILL unconditionally once the grace window elapses, so the count reflects how
often that path ran rather than a verified "process survived SIGTERM". The
design-023 dashboard panel is restored as a SIGKILL-only panel (timeouts remain
dropped per the entry above).

## D8 — `cbsd_build_requeues_total` reason vocabulary

**Design said:** design 022 lists the `reason` label values as `worker_dead` /
`ack_timeout` / `disconnect`.

**Implemented vocabulary:** `ack_timeout`, `worker_dead`, `reconnect_stale`,
`rejected` (see `cbsd-server/src/ws/dispatch.rs` and `ws/handler.rs`).
`reconnect_stale` supersedes the design's `disconnect` (it is the more precise
name for the stale-reconnect re-dispatch), and `rejected` is an added reason
covering a worker's explicit `BuildRejected` re-dispatch. The design snapshot is
left intact; this is the authoritative reason set. The design-023 "Re-dispatch
(retry) rate" panel uses `sum by (reason) (...)`, so it renders every value
without enumerating them and needs no change.

## D8 — Build-events annotation

Resolved in code: the design-023 correlation dashboard's "Build events"
annotation (`increase(cbsd_build_results_total{worker="$worker"}[1m]) > 0`) was
initially missing and has been added to `build-resource-correlation.json`. No
divergence remains.

## Scope note — G2a "Done" status

G2a (`cbsd-rs/server: record build outcomes and durations at finish`) delivered
its core surface — `cbsd_build_results_total{result,arch,periodic,worker}` and
the `cbsd_build_duration_seconds` histogram with the F6 guard. The two
additional counters its plan step listed are handled per the two D1 entries
above: `cbsd_build_timeouts_total` is dropped, and
`cbsd_sigkill_escalations_total` is implemented but as a worker-sourced counter
(it travels via the worker push and lands in G6), not at the server choke point
G2a touches. With those accounted for, the G2a "Done" marking reflects the
rescoped deliverable.
