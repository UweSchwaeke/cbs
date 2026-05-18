# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v1                                                                     |
| Date           | 2026-04-26 12:28 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior security review docs                             |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until the design gaps below are resolved                                    |

## Summary

Design 019 correctly identifies the five security findings from the prior review
and makes the right top-level policy choices:

- active-build ownership is the authorization boundary for worker build messages
- unauthorized worker actions are rejected, logged, and ignored without closing
  the websocket
- empty component lists are invalid ingress data
- pre-delivery dispatch failures are dispatch failures, not build failures
- JSON log tailing must be memory-bounded

The design is not yet sufficient to hand to implementation unchanged. It leaves
several implementation-critical details ambiguous, and one rollback invariant is
incomplete relative to the current database fields.

## Findings

### High: dispatch rollback does not specify clearing DB assignment fields

Design 019 says a failed post-DB, pre-delivery dispatch rolls `builds.state`
back to `queued`, removes the active entry, removes the watcher, discards the
attempted `trace_id`, and creates a fresh `trace_id` on the next successful
dispatch
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:138`).

The current dispatch write stores both `trace_id` and `worker_id` in the
`builds` row when setting `state = 'dispatched'`
(`cbsd-rs/cbsd-server/src/db/builds.rs:262`). The generic state rollback helper
only changes `state` and maybe `error`; it does not clear `trace_id`,
`worker_id`, `started_at`, `finished_at`, or `build_report`
(`cbsd-rs/cbsd-server/src/db/builds.rs:223`).

For a pre-delivery failure, `started_at`, `finished_at`, and `build_report`
should normally still be `NULL`, but `trace_id` and `worker_id` are definitely
set before tarball packing and websocket delivery
(`cbsd-rs/cbsd-server/src/ws/dispatch.rs:103`). If the future plan implements
only `state = queued`, the database will expose a queued build with stale worker
and trace provenance, contradicting the design's "discarded trace_id" invariant.

Required design change: define a dedicated rollback DB operation and list the
exact columns it writes. At minimum, pre-delivery rollback must set
`state = 'queued'`, `worker_id = NULL`, `trace_id = NULL`, and `error = NULL` or
explicitly justify preserving `error`. The design should also say whether it
resets `started_at`, `finished_at`, and `build_report` defensively for corrupted
legacy rows.

### Medium: meaningful-state authorization is underspecified

D1 requires that each worker build message be valid for "a state where that
message type is meaningful"
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:63`),
but the design never defines the allowed state/message matrix.

That omission matters because the current handlers have materially different
effects by message type:

- `build_accepted` cancels the ack timer if the build is active
  (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:284`).
- `build_started` writes `state = 'started'` by build ID
  (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:305`).
- `build_finished` writes a terminal state, removes active state, finalizes
  logs, removes watchers, and dispatches more work
  (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:333`).
- `build_rejected` either fails the build for integrity errors or requeues it
  (`cbsd-rs/cbsd-server/src/ws/dispatch.rs:391`).
- `build_output` appends to the build log file
  (`cbsd-rs/cbsd-server/src/ws/handler.rs:521`).

The future implementation plan needs explicit answers for cases such as
duplicate `build_accepted`, `build_output` after `build_finished`,
`build_started` while DB state is already `revoking`, and
`build_finished(revoked)` before `build_accepted` after a revoke. Without a
matrix, implementers can satisfy "ownership" while still allowing stale or
out-of-order owned messages to corrupt state.

Required design change: add a table mapping each `WorkerMessage` build action to
permitted active state and DB state, side effects, unauthorized versus
invalid-transition response, and whether the action cancels timers or removes
active state.

### Medium: descriptor validation should be centralized and typed

D5 says REST build submission and periodic task creation/update reject empty
component arrays, and every listed component must be known to the server
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:150`).
That is the right policy, but the design does not say where the validation is
implemented or how the REST and periodic paths share it.

The current REST build path already iterates typed `BuildDescriptor.components`
and checks component names (`cbsd-rs/cbsd-server/src/routes/builds.rs:89`).
Periodic task creation/update stores a raw `serde_json::Value`, checks only that
it is an object, and currently uses a separate helper that only extracts
repository scopes from the `components` array
(`cbsd-rs/cbsd-server/src/routes/periodic.rs:175`,
`cbsd-rs/cbsd-server/src/routes/periodic.rs:579`).

If the design remains at this level, the implementation is likely to add a
second ad hoc periodic validator that partially duplicates the build submission
rules and can diverge when component semantics change.

Required design change: specify a single validation helper or module that
deserializes periodic descriptors into `BuildDescriptor` and applies the same
non-empty and known-component checks used by REST submission. The helper should
return route-appropriate `400` errors and be reused by REST submission, periodic
create/update, and any scheduler trigger path that can read legacy rows.

### Medium: bounded tailing lacks an implementation contract

D7 requires the JSON tail endpoint to read only enough data to produce the
requested tail within an explicit memory budget
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:176`).
The current bug is real: `logs_tail` reads the full file into a `String`, then
collects all lines before slicing
(`cbsd-rs/cbsd-server/src/routes/builds.rs:475`,
`cbsd-rs/cbsd-server/src/routes/builds.rs:495`).

The design does not define the budget, the status code, or the algorithmic
contract. "Read only enough file data" can mean reverse block scanning, a
maximum readable byte window, reuse of the live seq offset index, or maintaining
a persistent line index. These choices have different behavior for long lines,
UTF-8 boundaries, exact `total_lines`, and completed logs whose in-memory index
has been dropped.

Required design change: pick the tailing strategy. At minimum, define
`MAX_TAIL_BYTES`, whether a single line longer than the budget is truncated or
rejected, the error status/body for budget overflow, and whether `total_lines`
remains exact or becomes omitted/approximate.

### Low: UnauthorizedBuildAction uses stringly typed action/reason fields

D3 defines the new server-to-worker message as:

```rust
UnauthorizedBuildAction {
    build_id: BuildId,
    action: String,
    reason: String,
}
```

(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:103`)

The action values are supposed to be stable snake-case names, but using an
unconstrained `String` makes typos part of the wire behavior and makes tests
weaker. This also duplicates information already embodied by the worker message
variant names in `cbsd-proto` (`cbsd-rs/cbsd-proto/src/ws.rs:63`).

Recommended design change: define a small serializable enum such as
`WorkerBuildAction` with `serde(rename_all = "snake_case")`, and use a compact
reason enum or reason code plus optional human message. The worker log can still
render a generic string without exposing internal worker identity.

### Low: compatibility language is too conditional for a design

D3 says older workers ignore unknown server messages "only if their receive loop
already tolerates them" and otherwise the worker and server changes must land
together
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:117`).

The current worker receive loop does tolerate unknown server message variants:
serde parsing fails, it logs a warning, and continues
(`cbsd-rs/cbsd-worker/src/ws/handler.rs:178`). The current protocol version is
already 2 for both handshake and server welcome
(`cbsd-rs/cbsd-proto/src/ws.rs:40`, `cbsd-rs/cbsd-proto/src/ws.rs:64`).

Required design change: replace the conditional text with the verified
compatibility decision. Either declare this a backward-compatible protocol v2
extension because current v2 workers ignore unknown server messages, or define a
protocol v3 bump. Leaving both options open forces the implementation plan to
make a design decision.

## Deferred Or Ambiguous Items

- No explicit DB rollback operation or exact column reset list is defined.
- No state/message transition matrix is defined for owned but stale or invalid
  worker messages.
- No centralized descriptor validation API is named for REST, periodic, and
  scheduler/legacy paths.
- No bounded tail algorithm, byte budget, long-line behavior, or HTTP error
  contract is defined.
- No exact wire type is defined for unauthorized actions beyond string fields.
- Compatibility is left conditional even though the current worker behavior can
  be verified.

## Commit Boundary Notes

No implementation commits exist. For the current uncommitted doc changes, design
019 is a coherent standalone design document and should be committed separately
from the broad unrelated Markdown churn visible elsewhere in the working tree.
The prior security review can be a separate documentation commit because it is
the evidence source that motivates design 019.

## Top Findings

1. High: dispatch rollback does not specify clearing persisted `trace_id` and
   `worker_id`, so a queued build can retain stale assignment provenance.
2. Medium: the design requires meaningful message states but does not define the
   worker message/state transition matrix.
3. Medium: descriptor validation is not centralized across typed REST submission
   and raw JSON periodic descriptors.
4. Medium: bounded log tailing lacks a byte budget, algorithm, long-line
   behavior, and HTTP error contract.
5. Low: the new unauthorized-action protocol message is stringly typed and the
   compatibility decision is still conditional.

## Confidence Score

| Item                                           | Points | Description                                                                                         |
| ---------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------- |
| Starting score                                 | 100    |                                                                                                     |
| D7: rollback leaves stale DB assignment fields | -20    | Design says trace is discarded but does not specify clearing persisted `trace_id`/`worker_id`.      |
| D1: state/message matrix deferred              | -20    | Meaningful states are required by design but not defined for implementation.                        |
| D2: descriptor validation likely duplicated    | -15    | REST and periodic currently use different descriptor shapes; design does not centralize validation. |
| D3: tailing budget/algorithm unspecified       | -5     | Bounded tail requirement lacks concrete data structure and memory budget choices.                   |
| D3: stringly typed unauthorized action         | -5     | Stable protocol action names are represented as unconstrained strings.                              |
| D11: compatibility decision not documented     | -5     | Design leaves protocol compatibility conditional despite verifiable current behavior.               |
| **Total**                                      | **30** |                                                                                                     |

Interpretation: 30/100. The design addresses the right security areas, but it is
not implementation-ready because several important invariants would still have
to be designed during planning or coding.

## Go / No-Go

No-go for implementation planning as-is.

Required actions before proceeding:

1. Define exact rollback DB semantics, including clearing persisted assignment
   fields.
2. Add the worker build-message state/transition matrix.
3. Specify centralized typed descriptor validation for REST, periodic, and
   legacy scheduler paths.
4. Choose the bounded tailing algorithm and public error contract.
5. Replace conditional protocol compatibility language with a concrete decision.
