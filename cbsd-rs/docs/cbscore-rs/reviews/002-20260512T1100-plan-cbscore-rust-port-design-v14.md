# Plan Review v14 — cbscore Rust port (plan 002)

**Scope:** Comprehensive pass covering all seven Phase plans plus the README.
Primary focus is Phase 7 (first review). Sanity recheck of Phases 1–6 for
regressions introduced by Phase 7's addition.

**Files reviewed:**

- `cbsd-rs/docs/cbscore-rs/plans/README.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md` (Phase 1)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-02-subprocess-and-shell-tools.md`
  (Phase 2)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md`
  (Phase 3)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-04-runner.md` (Phase 4)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-05-builder-and-releases.md`
  (Phase 5)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-06-cbsbuild-cli.md` (Phase 6)
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-07-worker-cutover.md` (Phase
  7 — new)
- Design corpus: 001, 002, 003, 004, 005 (all at current revision)
- Live source: `cbsd-rs/cbsd-worker/src/` (build/executor.rs, build/output.rs,
  build/mod.rs, ws/handler.rs, config.rs), `cbsd-rs/scripts/cbscore-wrapper.py`,
  `container/ContainerFile.cbsd-rs`, `podman-compose.cbsd-rs.yaml`

---

## 1. Summary Assessment

Phase 7 is a well-scoped, clearly motivated first draft. It correctly identifies
what must be deleted (the Python bridge), what must be added (direct Cargo dep),
and what must not change (the WebSocket protocol). The three-commit sequence is
clean and independently revertable. However, two concrete implementation gaps
need closure before work starts: (1) the custom subscriber layer for converting
in-process `tracing` events into `BuildOutput` WebSocket messages is described
in high-level terms but is not specified well enough for an implementer to write
it without design-level guesswork; and (2) four config fields and one
operator-facing YAML key that the subprocess bridge currently depends on
(`cbscore_wrapper_path`, `cbscore_config_path`,
`sigkill_escalation_timeout_secs` from `ResolvedWorkerConfig`, plus the compose
mount) are not explicitly addressed as retirement or re-mapping items in the
plan. One minor issue (plan file path reference for the historical cbsd-rs plan)
also needs a one-word fix.

Neither gap is a blocker in the strong sense — the design is fundamentally sound
and the reviewer is not asking for a redesign — but gap (1) is a major concern
that must be resolved before Commit 1 lands. **Phase 7 is ready to proceed to
implementation start with the major concern resolved; the minor issue can be
fixed inline during Commit 1.**

---

## 2. Phases 1–6 Regression Recheck

All v13 findings remain closed. Phase 7's addition introduces no text that
contradicts or invalidates any prior decision. Specific cross-phase touchpoints
verified:

- **Phase 4 §Vault wrapper security note** mentions "Phase 7 context" in its
  per-call-auth rationale. Phase 7's existence does not change Phase 4's
  security analysis — the no-token-caching argument is an M1 posture, and Phase
  7 does not introduce a long-lived Vault client. CONSISTENT.
- **Phase 1** does not need updating for the cbsd-worker Cargo dep. The dep is
  added by Phase 7 to `cbsd-worker/Cargo.toml`, not to `cbscore/Cargo.toml` or
  `cbscore-types/Cargo.toml`. Phase 1 is unaffected. CONSISTENT.
- **README running totals:** Phase 1 (5) + Phase 2 (5) + Phase 3 (4) + Phase 4
  (3) + Phase 5 (6) + Phase 6 (5) + Phase 7 (3) = 31 commits. README estimate
  "~25–30 commits across 7 phases" is now one commit over the upper bound. This
  is a cosmetic discrepancy; the estimate pre-dates Phase 7 and is not a finding
  against the plan itself. Worth updating to "~25–31 commits" when Phase 7
  lands, but not blocking.

**Phases 1–6 free of regression.**

---

## 3. Blockers

None.

---

## 4. Major Concerns

### M1 — Custom subscriber layer underspecified

**What:** Phase 7 Commit 1 §Design constraints says: "Post-M2, cbscore's
`tracing` emissions are captured by a custom subscriber layer that converts them
into `build_output` messages directly — no pipe parsing, no line buffering
races." The plan also says: "Build outputs streamed via tokio channels, not
stdout pipes." But the plan gives no further specification of how this
subscriber layer is built or how it integrates with the existing `output.rs`
batching machinery.

This matters because the current `build/output.rs` is architecturally tied to a
`ChildStdout` pipe: `stream_output(stdout: ChildStdout, …)` reads
`AsyncBufReadExt::lines()`, batches them, and emits `WorkerMessage::BuildOutput`
via an `mpsc::Sender<WorkerMessage>`. The existing batch-flush logic (50 lines
or 200 ms, whichever comes first) and the structured `{"type":"result"}`
terminator detection in `output.rs` are specific to the subprocess protocol.

**Why it matters:** An implementer reading the plan as written will need to
independently answer at least three architectural questions with no guidance:

1. **What type backs the subscriber layer?** The plan mentions `tracing::Span`
   inheritance and `tracing-subscriber`'s span-aware formatter but does not
   specify whether to use `tracing_subscriber::Layer` (the correct answer), a
   channel-backed `tracing-appender` subscriber, or something else.
   `tracing_subscriber::Layer` with a custom `on_event` impl is the right tool,
   but the plan leaves this implicit.

2. **How does the layer forward log lines into the existing `BuildOutput`
   batching machinery?** The pre-M2 path uses
   `output::stream_output(stdout, build_id, &sender)` which drives the
   `mpsc::Sender<WorkerMessage>` that `handler.rs` already manages. The post-M2
   path needs to install the custom layer _before_ calling `runner::run` and
   tear it down _after_ the future completes. The plan does not describe whether
   the layer holds a clone of the `mpsc::Sender<WorkerMessage>` directly, or
   goes through an intermediary (e.g., a `tokio::sync::mpsc` of raw log lines
   fed into a separate batching task that matches the existing 50-line / 200ms
   contract). Either is reasonable; neither is specified.

3. **What happens to `build/output.rs` after Commit 1?** The current file is
   tightly coupled to the subprocess path. After Commit 1 removes the subprocess
   call, `output::stream_output` becomes dead code. The plan does not say
   whether `output.rs` is deleted, gutted, or refactored into a shared batching
   utility that both paths could theoretically use. Without guidance, an
   implementer may either leave dead code (clippy warning, compile failure under
   the zero-warnings rule) or delete `output.rs` entirely and re-implement
   batching inline — both paths land awkwardly.

**The build_report gap:** Pre-M2, the worker parses the wrapper's structured
`{"type":"result", "build_report": …}` JSON terminator line to extract the
`build_report` value sent in `WorkerMessage::BuildFinished`. Post-M2, there is
no such terminator. The plan says to "forward the resulting `RunReport`… into
the existing WebSocket `build_output` / `build_finished` message flow," but
`RunReport` (Phase 4 Commit 3) does not include a `build_report` field in the
shape that `cbsd-proto::ws::WorkerMessage::BuildFinished` expects (that field is
`Option<serde_json::Value>` per `ws.rs`). The plan must specify where the
`build_report` value comes from on the direct-dep path, or explicitly note that
`BuildFinished.build_report` becomes `None` in M2 (which would be a semantic
change to the field's completeness, even if the protocol shape is technically
unchanged).

**Concrete direction:** Add a §Subscriber layer design subsection to Commit 1.
It should specify:

- Install a `tracing_subscriber::Layer` impl with a `on_event` handler that
  formats the log event into a `String` (using the existing log formatter logic
  from `cbscore`'s tracing setup) and sends it down a
  `tokio::sync::mpsc::Sender<String>`.
- A lightweight batching task (replacing `output::stream_output`) reads from the
  channel, batches up to 50 lines or 200ms, and emits
  `WorkerMessage::BuildOutput` via the existing `mpsc::Sender<WorkerMessage>`
  that `handler.rs` already uses. This preserves the existing batch contract
  byte-for-byte.
- The layer is installed via `tracing_subscriber::registry().with(layer)` on a
  per-build basis using `tracing::dispatcher::with_default` or
  `subscriber.set_default()` so it does not interfere with the worker's own
  tracing setup (the worker logs its own spans to the file/console handler, not
  to the WebSocket channel).
- State that `output.rs` is replaced by the new batching task; the file can be
  deleted or gutted in Commit 1 rather than left as dead code.
- Resolve the `build_report` question: either `RunReport` carries enough
  information to populate `BuildFinished.build_report`, or the field becomes
  `None` at M2 (document this as a known semantic delta, not a protocol change).

---

## 5. Minor Issues

### N1 — Commit 2 file path for the historical cbsd-rs plan is wrong

**What:** Commit 2 §Files lists:

> `cbsd-rs/docs/cbsd-rs/plans/007-20260318T0725-cbscore-wrapper.md` — add a
> §Status update noting "retired in M2 / cbscore-rs Phase 7 Commit 2".

The actual file is at
`cbsd-rs/docs/cbsd-rs/plans/007-20260318T0725-cbscore-wrapper.md` (confirmed on
disk). The path in the plan is correct _as a relative path from the repo root_,
but note that the docs skill routes cbsd-rs plans to `cbsd-rs/docs/cbsd-rs/` —
the path the plan cites is this exact location. So the path is right.

However, the plan says "this is the cbsd-rs plan that originally introduced the
Python bridge", which is accurate. The direction to "add a §Status update" is
the right approach; **do not delete the historical plan**. The one word to fix:
the plan says to add the status note to
`cbsd-rs/docs/cbsd-rs/plans/007-20260318T0725-cbscore-wrapper.md`, but that file
lives in the cbsd-rs plan sequence, not the cbscore-rs plan sequence. The
cross-tree annotation is appropriate — just be explicit in the commit message
that the plan under `cbsd-rs/docs/cbsd-rs/` is being updated, not a file under
`cbsd-rs/docs/cbscore-rs/`.

(On closer inspection the path is correct; this is editorial, not a path error.
Noting for completeness only.)

### N2 — `cbscore_wrapper_path` and `cbscore_config_path` config fields not mentioned

**What:** `cbsd-worker/src/config.rs` has two fields in both `WorkerConfig` and
`ResolvedWorkerConfig` that are specific to the subprocess bridge:
`cbscore_wrapper_path` and `cbscore_config_path`. After Commit 1, both fields
become dead config — the wrapper path is no longer resolved, and the cbscore
config path is now passed directly to `cbscore::config::Config::load` (not as an
env var to a subprocess). Commit 1 §Files does not mention these fields.

**Why it matters:** Leaving dead config fields in a struct violates the
codebase's single-responsibility convention and will produce
`#[allow(dead_code)]` markers or clippy warnings under the zero-warnings rule.
More importantly, `cbscore_config_path` transitions from "path to pass as env
var to Python" to "path to load directly via `Config::load`" — it is still
needed but its role changes. The plan should state explicitly: (a)
`cbscore_wrapper_path` is removed from both `WorkerConfig` and
`ResolvedWorkerConfig`; (b) `cbscore_config_path` is retained but its usage
changes from env-var pass-through to direct `Config::load` call; (c)
`sigkill_escalation_timeout_secs` was a subprocess process-group escalation
timeout — its meaning changes with the direct-dep path (the cancellation
mechanism is now future-drop → runner RAII guard, not SIGKILL). The plan should
note whether this field is retained, repurposed, or removed.

**Resolution:** Add three bullet points to Commit 1 §Files under
`cbsd-worker/src/config.rs`: one for `cbscore_wrapper_path` (removed), one for
`cbscore_config_path` (retained, role changes), and one for
`sigkill_escalation_timeout_secs` (disposition: retained for the runner's
internal timeout budget, or removed if Phase 4's `RunOpts::timeout` subsumes it
— pick one and state it). No re-review needed; this is implementer guidance.

### N3 — Compose file Python volume mount not listed in Commit 2

**What:** Commit 2 §Files lists `podman-compose.cbsd-rs.yaml` and says to "drop
any `cbscore-wrapper.py`-related volume mounts or env vars." The actual compose
file has a `worker-dev` service volume:

```yaml
# cbscore-wrapper.py — mounted to the same path as the prod image so
# worker.yaml.example works without modification in dev mode
- ./cbsd-rs/scripts:/opt/cbsd-rs:ro
```

And the `ContainerFile.cbsd-rs` `cbsd-rs-worker` production stage has:

```dockerfile
COPY cbsd-rs/scripts/cbscore-wrapper.py /opt/cbsd-rs/cbscore-wrapper.py
```

These two specific paths (`/opt/cbsd-rs:ro` volume, the COPY line) are not named
in the plan. The plan's prose direction covers them in principle but does not
give the implementer the exact lines to remove. Given that the plan holds other
files to a standard of naming exact line content (e.g., "drop the
`RUN apt install python3.13` line"), naming these two specific removal targets
is consistent with that standard.

**Resolution:** In Commit 2 §Files, add the exact path
`./cbsd-rs/scripts:/opt/cbsd-rs:ro` to the compose file bullet, and add the
exact COPY line
`COPY cbsd-rs/scripts/cbscore-wrapper.py /opt/cbsd-rs/cbscore-wrapper.py` to the
Containerfile bullet. Editorial only.

---

## 6. Suggestions

### S1 — Commit 1 §Testable: name the `tracing::dispatcher` mechanism for scope

The trace_id propagation test described in Commit 1 §Testable (verifying
`trace_id` appears on `cbscore::runner::*` log lines from within the worker
process) is correct and necessary. However, the test description assumes the
implementer knows how to install the custom subscriber for a single build's
duration without setting it as the global default. This is non-obvious in
tokio's multi-threaded runtime: `tracing::dispatcher::with_default` is
synchronous and thread-local, which does not compose cleanly with `await` points
inside `runner::run`. The correct mechanism on tokio is to instrument the
`runner::run` future with a span (`future.instrument(span)`) and have the
subscriber layer filter on that span's context — not to use `with_default`.

Add one sentence to §Testable: "The trace_id propagation mechanism under tokio
is `future.instrument(span)` on the `runner::run` call, not
`dispatcher::with_default` (which is thread-local and does not propagate across
await points in a multi-threaded executor)." This prevents a subtle bug where
the trace_id appears on synchronous log lines but silently drops off on lines
emitted after an await point in `cbscore::runner::run`.

### S2 — Commit 3 §Design constraints: clarify the reference RPM source

The byte-equality parity test requires a reference RPM set produced by the
pre-M2 (subprocess-driven) path. The plan says the reference can be
"pre-recorded in a fixture or generated by checking out the pre-Commit-1 worker
code via `git worktree`". The git-worktree approach is technically correct but
operationally fragile: the worktree approach compiles a second worker binary,
which requires matching the build environment exactly (Rust toolchain version,
linker flags, cbscore Python installation). The fixture approach (record the
RPMs once during Phase 6 M1 acceptance and store SHA-256 digests as the
reference) is simpler and less environment-sensitive.

**Suggestion:** Specify the fixture approach as the default: "Reference RPM
digests are SHA-256 hashes recorded during the Phase 6 M1 acceptance test run
and stored in the test fixture directory (e.g.,
`cbsd-rs/cbsd-worker/tests/fixtures/m2_reference_sha256.json`). The test reads
the fixture file and asserts each produced RPM's SHA-256 matches. The
`git worktree` approach is available as a fallback for environments where the M1
reference fixture was not recorded." Non-blocking; the current "or" phrasing is
not wrong, but the ambiguity may cost an implementer a day of debugging why the
worktree-compiled binary produces slightly different RPM timestamps.

---

## 7. Open Questions

### Q1 — `build_report` on the direct-dep path

As noted in M1 above: does `RunReport` (Phase 4 Commit 3) carry enough
information to populate `BuildFinished.build_report: Option<serde_json::Value>`?
The pre-M2 path extracts `build_report` from the wrapper's final structured JSON
line. The post-M2 path has no such terminator. The plan says to "forward the
resulting `RunReport`" but does not map `RunReport`'s fields to
`BuildFinished.build_report`. If `RunReport` already carries a `build_report`
field (not explicitly stated in Phase 4's Commit 3 spec), this is resolved. If
not, the implementer must either (a) add a `build_report` field to `RunReport`
in a cbscore-side change, or (b) accept `None` in M2 with the field populated in
a future pass. Either answer is acceptable; the plan should state which.

### Q2 — cbsd-worker config schema version bump

`cbsd-worker/src/config.rs`'s `WorkerConfig` struct is loaded from operator
YAML. After Commit 1 removes `cbscore_wrapper_path` and transitions
`cbscore_config_path`'s semantics, operators who have
`cbscore-wrapper-path: /opt/cbsd-rs/cbscore-wrapper.py` in their `worker.yaml`
will pass an unknown key to serde, which (assuming
`#[serde(deny_unknown_fields)]` is absent — it is absent in the current struct)
will silently be ignored. This is probably acceptable since the struct does not
deny unknown fields. But if the team wants to warn operators that
`cbscore-wrapper-path` is now obsolete, a one-time migration note in the release
changelog (not in the plan) is sufficient. The plan should acknowledge this
config-evolution consequence explicitly, even if the decision is "do nothing —
serde ignores the unknown key."

---

## 8. Phase 7 Specific Checklist

**Commit 1 — design alignment**

- M2 surface `cbscore::runner::run` cited with correct path (Phase 4 Commit 3).
  PASS.
- `Config::load` cited as `cbscore::config::Config` (Phase 3 Commit 4). PASS.
- `read_descriptor` cited as `cbscore::versions::desc::read_descriptor` (Phase 4
  Commit 1). PASS.
- `SecretsMgr::load_files` (Phase 3 Commit 3). PASS.
- `version_create_helper` cited as Phase 6 Commit 2. PASS.
- Design 002 §M2 surface quote: "runner, Config, version_create_helper,
  VersionDescriptor, errors". All five items accounted for in the plan's
  implementation bullet list. PASS.
- trace_id invariant: plan correctly notes the shift from env-var (subprocess)
  to `tracing::Span` (in-process). Mechanism has the `instrument()` gap noted in
  S1. PARTIAL PASS (see S1).

**Commit 1 — custom subscriber layer**

- Subscriber layer existence acknowledged; architectural details insufficient
  for implementation without guesswork. See M1. FAIL (major concern).
- `build_report` path on direct-dep path unspecified. See Q1. OPEN.

**Commit 2 — retirement checklist**

- `cbscore-wrapper.py` deletion: `cbsd-rs/scripts/cbscore-wrapper.py`. PASS
  (file confirmed on disk).
- Containerfile stage `cbsd-rs-worker`: Python install is in `worker-base`
  (`FROM python:3.13-alpine3.21 AS worker-base`) which is shared with dev
  images. Plan says "drop the `RUN apt install python3.13` line" but the actual
  Containerfile uses Alpine + `FROM python:3.13-alpine3.21` base image (not
  `apt`). The directive is correct in intent but wrong in the specific command
  name. An implementer should drop the `worker-base` stage reference from the
  `cbsd-rs-worker` final stage (changing `FROM worker-base AS cbsd-rs-worker` to
  `FROM alpine:3.21 AS cbsd-rs-worker` or similar minimal base), not a single
  `apt install` line. This is a concrete implementation detail the plan gets
  wrong by assuming `apt`. Minor issue — see N3 for the compose side.
- `podman-compose.cbsd-rs.yaml` compose file:
  `./cbsd-rs/scripts:/opt/cbsd-rs:ro` volume mount on `worker-dev`. Listed
  generically; specific path not named. See N3. PARTIAL PASS.
- Historical plan
  `cbsd-rs/docs/cbsd-rs/plans/007-20260318T0725-cbscore-wrapper.md` status
  annotation. Correct approach (annotate, do not delete). PASS.

**Commit 2 — Containerfile apt/Alpine discrepancy**

The plan says to drop "The `RUN apt install python3.13` (or equivalent) line."
The actual `ContainerFile.cbsd-rs` does not use `apt` — the Python runtime comes
from the `FROM python:3.13-alpine3.21 AS worker-base` base image. Removing
Python means removing the `worker-base` stage entirely and changing
`FROM worker-base AS cbsd-rs-worker` to a plain
`FROM alpine:3.21 AS cbsd-rs-worker` (with any non-Python deps that were in
`worker-base` re-added as needed). The `uv` install and cbscore wheel install in
`worker-base` are also gone. The plan's "or equivalent" hedge partially covers
this, but an implementer who reads the plan carefully and then looks at the
Containerfile will need to reconcile the mismatch. This is a minor calibration
issue, not a blocker.

**Resolution:** Revise Commit 2 §Files to note: "The Python runtime in the
worker image comes from `FROM python:3.13-alpine3.21 AS worker-base` (the
`worker-base` stage) — not from a `RUN apk add` or `RUN apt install` line.
Post-M2, the `cbsd-rs-worker` final stage changes its `FROM` target from
`worker-base` to a lean Alpine base (or `FROM rust-builder` output directly),
and the `worker-base` stage can be removed if no other final stage depends on
it."

**Commit 3 — acceptance criteria**

- Four acceptance criteria map correctly onto design 002 §M2 lines 1293–1301.
  PASS.
- Byte-equality test technique: "pre-recorded fixture or git worktree" — see S2
  for the recommendation to prefer fixtures. PASS (S2 is a suggestion, not a
  finding).
- `#[ignore]` default with env-var unlock consistent with Phase 6 Commit 5
  pattern. PASS.
- README and plan progress table updated in same commit as the test. PASS.

**Cross-phase consistency**

- Phase 7 §Depends on cites five functions; all verified at their correct module
  paths in their respective phase plans. PASS.
- Vagueness about "wherever the current subprocess invocation lives" in Commit 1
  §Files: the plan says `cbsd-rs/cbsd-worker/src/builder.rs` (or wherever). The
  actual file is `cbsd-rs/cbsd-worker/src/build/executor.rs` (confirmed on
  disk). The plan's parenthetical hedge is appropriate for a plan (the file name
  could change before Phase 7 is implemented), but the plan could helpfully name
  the current file for orientation. This is a suggestion, not a finding.

**Rollback story**

- Commits 1 and 2 are independently revertable. The compilation invariant after
  Commit 2's wrapper deletion is sound: Commit 1 already removed the subprocess
  spawn code that referenced `cbscore-wrapper.py`; Commit 2's deletion of the
  file therefore cannot break compilation (no Rust code references the file path
  at compile time — it was a runtime path string). PASS.
- Design 002 §Rollback cited correctly. PASS.

**WebSocket protocol invariance**

- All `WorkerMessage` variants emitted by the current worker (`BuildStarted`,
  `BuildOutput`, `BuildFinished`, `BuildAccepted`, `BuildRejected`,
  `WorkerStatus`, `WorkerStopping`, `Hello`) can be filled on the direct-dep
  path with the same field shapes. `cbsd-proto::ws` is untouched. PASS — with
  the `build_report` caveat noted in Q1.

**Commit granularity**

- Commit 1 ~450 LOC: in the sweet spot. PASS.
- Commit 2 ~100 LOC: below the 400-line floor; rationale present and convincing
  (deletion-only). PASS.
- Commit 3 ~250 LOC: below the floor; rationale present and convincing (M2
  acceptance gate is a distinct review concern from the cleanup). PASS.
- Total ~800 LOC across 3 commits matches plan header. PASS.
- README estimate: 31 actual commits vs. "~25–30" — one over the upper bound.
  Not a finding against Phase 7; the estimate dates from before Phase 7 was
  drafted. Update the README when Phase 7 lands.

**Out-of-scope clarity**

- M3, Python cbscore deletion, wire-format changes, cbsd-server changes: all
  correctly deferred and documented. PASS.
- `cbscore_config_path` config field transition not addressed. See N2.

---

## 9. Verdict

Phase 7 is **conditionally ready for implementation start**. One major concern
(M1: custom subscriber layer underspecified) must be resolved before Commit 1 is
coded — not before implementation starts, but before the implementer writes the
`tracing` integration code. In practice this means adding the §Subscriber layer
design subsection to the plan and resolving Q1 (`build_report` on the direct-dep
path) before the first `cargo build` of Commit 1. Three minor issues (N2, N3,
and the Containerfile `apt`/Alpine discrepancy noted in the Commit 2 checklist)
can be corrected inline during implementation without re-review. Two suggestions
(S1, S2) are optional.

**New findings by severity:** 0 blockers, 1 major, 3 minors, 2 suggestions, 2
open questions.

**Collectively, the cbscore-rs plan corpus (Phases 1–7) is ready for
implementation start.** Phase 7's major concern is scoped to a single design
decision within Commit 1 (how the subscriber layer forwards events to the WS
channel) and does not affect the soundness of the three-commit sequence or the
rollback story. The implementer can begin Phase 1 through Phase 6 immediately;
Phase 7 should resolve M1 before writing Commit 1's tracing integration code.
