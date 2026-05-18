# Design Review: Worker Control Plane Hardening

| Field          | Value                                                                             |
| -------------- | --------------------------------------------------------------------------------- |
| Review         | 019 design v2                                                                     |
| Date           | 2026-04-26 15:47 UTC                                                              |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` |
| Plan           | N/A                                                                               |
| Scope          | Uncommitted design 019 and prior security review docs                             |
| Reviewer       | Codex                                                                             |
| Recommendation | No-go until the reconnect ownership gap is resolved                               |

## Summary

Design 019 v3 addresses most of the prior v1 review findings:

- dispatch rollback now lists the DB columns that must be cleared and requires a
  dedicated rollback operation
- descriptor validation is centralized around typed `BuildDescriptor` validation
  for REST, periodic create/update, and scheduler trigger paths
- JSON tailing now has a selected reverse-scan strategy with a 1,000-line cap, 4
  MiB scan budget, and truncation metadata
- unauthorized action and reason fields are enum-based
- protocol version 2 is retained as an explicit pre-production correction

The design is still not implementation-ready because it omits a build-scoped
reconnect message that can currently transfer ownership of an active build to
the wrong worker connection. There is also an API/client contract gap for the
changed log-tail response.

## Findings

### High: reconnect `worker_status` can still become a build takeover path

D1 says server-side active-build ownership is required for worker build messages
and lists `build_accepted`, `build_started`, `build_output`, `build_finished`,
and `build_rejected`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:68`).
It does not include `worker_status`, even though `WorkerMessage::WorkerStatus`
is a worker-supplied build-scoped message on reconnect
(`cbsd-rs/cbsd-proto/src/ws.rs:77`).

The omission matters because the current reconnect handler accepts
`WorkerStatus { state: Building, build_id: Some(...) }`, reads only the build's
DB state, and for `dispatched` or `started` rewrites the active entry's
`connection_id` to the reconnecting socket
(`cbsd-rs/cbsd-server/src/ws/handler.rs:637`,
`cbsd-rs/cbsd-server/src/ws/handler.rs:656`,
`cbsd-rs/cbsd-server/src/ws/handler.rs:668`). The caller has the authenticated
`registered_worker_id`, but it is not passed into `handle_worker_status`
(`cbsd-rs/cbsd-server/src/ws/handler.rs:596`).

That leaves the same class of trust-boundary problem through a different
message: worker B can claim to be building worker A's dispatched/started build,
cause `queue.active[build_id].connection_id` to point at B's connection, and
then satisfy D1's future connection-based ownership checks for later lifecycle
or output messages. The persisted `builds.worker_id` is exposed by the DB record
(`cbsd-rs/cbsd-server/src/db/builds.rs:122`), but the design does not require
the reconnect path to compare it with the authenticated worker ID before
reassigning active ownership.

Required design change: include reconnect `worker_status` in the ownership
model. The design should say that a building reconnect may resume an active
assignment only when the authenticated registered worker ID matches the
persisted `builds.worker_id` and the active assignment being resumed. A mismatch
must not rewrite `queue.active`; it should be logged as a security warning and
handled with an explicit non-fatal rejection or revoke behavior that does not
disclose another worker's identity.

### Medium: log-tail response change does not include the `cbc` client contract

D7 changes the JSON tail response by omitting `total_lines` from the example
shape and replacing it with `requested`, `truncated`, `bytes_scanned`, and
`max_tail_bytes`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:286`).
The design's package list includes only `cbsd-server`, `cbsd-worker`, and
`cbsd-proto`, but the existing Rust `cbc` client deserializes `total_lines` as a
required field and prints it (`cbsd-rs/cbc/src/logs.rs:76`,
`cbsd-rs/cbc/src/logs.rs:122`).

If the future implementation follows the design literally in `cbsd-server` only,
`cbc logs tail` will fail to deserialize every successful tail response. That is
avoidable implementation risk rather than a policy question.

Required design change: either add `cbc` to the affected packages and specify
the CLI response update, or preserve a compatible `total_lines` field with
documented semantics such as `null`, `approximate_total_lines`, or exact only
when the scan reaches the start of file.

### Low: long-line truncation needs a UTF-8 boundary rule

D7 says that if a single line exceeds the 4 MiB budget, the response includes
"the suffix of that line that fits" and sets `truncated: true`
(`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md:280`).
Because the response is JSON strings, the implementation must decide what
happens when the byte suffix starts in the middle of a multi-byte UTF-8
sequence.

The current log writer receives Rust `String` lines, so log files are expected
to be UTF-8, but reverse byte scanning can still cut a valid file at an invalid
character boundary. Leaving this undefined can lead to ad hoc lossy conversion,
panic-prone `from_utf8` use, or inconsistent client output.

Required design change: specify the decoding rule for truncated byte prefixes.
For example, drop partial leading code points before UTF-8 decoding, or use
lossy decoding and document replacement-character behavior.

## Prior Review Findings

The previous design review findings were checked against v3:

| Prior finding                               | Status           | Notes                                                                                                             |
| ------------------------------------------- | ---------------- | ----------------------------------------------------------------------------------------------------------------- |
| Rollback DB columns unspecified             | Addressed        | D4 now lists `state`, `worker_id`, `trace_id`, `error`, timestamps, and `build_report`.                           |
| Message/state matrix missing                | Mostly addressed | The lifecycle/output matrix is present; reconnect `worker_status` remains missing.                                |
| Descriptor validation not centralized       | Addressed        | D5 requires a shared typed validator across REST, periodic, and scheduler paths.                                  |
| Bounded tailing lacks budget/algorithm      | Mostly addressed | Reverse scanning, 1,000-line cap, 4 MiB budget, and truncation are specified; UTF-8 truncation remains ambiguous. |
| Unauthorized action/reason stringly typed   | Addressed        | D3 uses `WorkerBuildAction` and `UnauthorizedBuildReason` enums.                                                  |
| Protocol compatibility decision conditional | Addressed        | D3 explicitly keeps protocol version 2.                                                                           |

## Deferred Or Ambiguous Items

- `worker_status` reconnect ownership is not covered by the active-build
  authorization boundary.
- The unauthorized-action enum set does not include a reconnect/status action or
  reason for "authenticated worker does not match assigned worker".
- The tail response changes omit the current `cbc` logs-tail client contract.
- Tail truncation does not define UTF-8 boundary behavior for over-budget single
  lines.

## Commit Boundary Notes

No implementation commits exist. For the current uncommitted documentation work,
design 019, the v1 review, and this v2 review are logically related to the
worker-control-plane hardening thread and can be committed together or as
reviewable documentation commits. They should remain separate from the broad
unrelated Markdown churn already present elsewhere in the working tree.

## Top Findings

1. High: reconnect `worker_status` is a build-scoped takeover path unless the
   design requires authenticated worker ID checks before active ownership can be
   reassigned.
2. Medium: the new tail response shape can break `cbc logs tail` unless the
   design includes the CLI update or preserves a compatible total-lines field.
3. Low: the 4 MiB long-line truncation rule needs an explicit UTF-8 boundary
   policy.

## Confidence Score

| Item                                  | Points | Description                                                                                       |
| ------------------------------------- | ------ | ------------------------------------------------------------------------------------------------- |
| Starting score                        | 100    |                                                                                                   |
| D7: reconnect ownership gap           | -20    | `worker_status` can currently reassign active ownership without proving the registered worker ID. |
| D1: reconnect authorization deferred  | -20    | A build-scoped worker message is omitted from the design's ownership and transition decisions.    |
| D11: tail client contract missing     | -5     | The design changes server response shape without documenting the required `cbc` update.           |
| D3: UTF-8 truncation rule unspecified | -5     | Reverse byte scanning over a long line lacks a decoding/boundary contract.                        |
| **Total**                             | **50** |                                                                                                   |

Interpretation: 50/100. The design is much closer than v1, but it still has a
security-significant reconnect gap and a couple of implementation-readiness
holes.

## Go / No-Go

No-go for implementation planning until the reconnect `worker_status` ownership
model is added. After that, the tail client contract and UTF-8 truncation rule
should be resolved before the plan is written so implementation does not have to
make API decisions during coding.
