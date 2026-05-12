# Plan Review v15 — cbscore Rust port (plan 002)

**Scope:** Incremental pass over all seven Phase plans and the README. Primary
focus is verifying closure of v14 findings (N2, N3, S2, Q2) and identifying any
new issues introduced since v14. V14 M1 and Q1 are confirmed closed; they are
not re-raised.

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
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-07-worker-cutover.md`
  (Phase 7)
- Design corpus: 001, 002, 003, 004, 005 (all at current revision)
- Live source: `cbsd-rs/cbsd-worker/src/config.rs`,
  `cbsd-rs/cbsd-worker/src/build/executor.rs`,
  `cbsd-rs/cbsd-worker/src/ws/handler.rs`, `container/ContainerFile.cbsd-rs`,
  `podman-compose.cbsd-rs.yaml`
- Prior review: `002-20260512T1100-plan-cbscore-rust-port-design-v14.md`

---

## 1. Summary Assessment

The plan corpus is in good shape. The two substantive v14 findings that required
pre-implementation closure — M1 (subscriber layer underspecified) and Q1
(`build_report` path on the direct-dep path) — are correctly closed: the
`§Subscriber layer design` subsection now specifies `tracing_subscriber::Layer`,
`mpsc` channel forwarding, per-build span filtering, `future.instrument(span)`,
and explicit deletion of the old `stream_output`. `RunReport` now carries
`build_report: Option<serde_json::Value>`.

Three v14 minor findings (N2, N3, and the Containerfile apt/Alpine discrepancy
from the N3 checklist) remain open in the current plan text and carry forward as
v15 minors. One v14 suggestion (S2) and one open question (Q2) are also
unaddressed. No new blockers or major concerns were found. The plan corpus is
ready for implementation start; the three remaining minors are editorial and can
be corrected inline during Commit 1 and Commit 2 of Phase 7.

---

## 2. Closed Findings Confirmed

The following v14 findings are verified closed in the current plan text and are
not re-raised.

| ID     | Description                                       | Status |
| ------ | ------------------------------------------------- | ------ |
| V14-M1 | Subscriber layer underspecified                   | CLOSED |
| V14-Q1 | `build_report` on direct-dep path                 | CLOSED |
| V14-S1 | `future.instrument` not named                     | CLOSED |
| V9-N1  | Phase 4 C1 §Testable `MissingSchemaVersion` hedge | CLOSED |

V14-M1 is closed by the `§Subscriber layer design` subsection in Phase 7 Commit
1, which specifies `tracing_subscriber::Layer<S>`, `on_event` formatting into
`mpsc::Sender<String>`, per-build span filtering via `ctx.lookup_current()`, the
mandatory `future.instrument(span)` mechanism, and explicit `stream_output`
deletion. V14-Q1 is closed by the `build_report: Option<serde_json::Value>`
field now present in Phase 4 Commit 3's `RunReport` struct spec, with the source
documented as the in-container `BuildArtifactReport` JSON read after container
exit.

---

## 3. Blockers

None.

---

## 4. Major Concerns

None.

---

## 5. Minor Issues

### N1 — `cbscore_wrapper_path` / `cbscore_config_path` /

`sigkill_escalation_timeout_secs` absent from Phase 7 Commit 1 §Files (carries
forward from V14-N2)

**Location:** `002-20260508T1558-07-worker-cutover.md`, Commit 1 §Files.

**Design citation:** `cbsd-rs/CLAUDE.md` zero-warnings rule
(`cargo clippy --workspace` must pass); CLAUDE.md §Commit Granularity (each
commit must compile and pass tests).

**What:** Commit 1 §Files lists `cbsd-worker/src/builder.rs` (or
`build/executor.rs`) and `cbsd-worker/src/main.rs` but does not mention
`cbsd-worker/src/config.rs`. The live file has three fields specific to the
subprocess bridge that Commit 1 makes dead or repurposed:

- `cbscore_wrapper_path: Option<PathBuf>` — present in both `WorkerConfig` and
  `ResolvedWorkerConfig` (lines 73, 116, and their resolution sites at 217 and
  253). After Commit 1 removes the `Command::new("python")` block from
  `executor.rs`, this field is dead config that will produce a clippy warning or
  a dead-code lint under the codebase's zero-warnings policy.
- `cbscore_config_path: Option<PathBuf>` — still needed after Commit 1, but its
  role changes: pre-M2 it was passed as an env var to the Python subprocess;
  post-M2 it is the path fed directly to `cbscore::config::Config::load`. The
  plan is silent on this transition.
- `sigkill_escalation_timeout_secs: Option<u64>` — the subprocess path used this
  for SIGKILL escalation after SIGTERM. Post-M2 cancellation is via future-drop
  → Phase 2 Commit 1 RAII guard; the semantics of this field change. The plan
  does not state whether to retain it (as an outer `RunOpts::timeout` override)
  or remove it.

**Why it matters:** An implementer reading Commit 1's §Files has no guidance on
these three fields. Leaving `cbscore_wrapper_path` in place produces a clippy
failure that blocks the pre-commit check. Silently changing
`cbscore_config_path`'s role without documenting it risks a second reviewer (or
a future revert author) misunderstanding the field's post-M2 meaning.

**Resolution:** Add three bullet points to Commit 1 §Files under
`cbsd-worker/src/config.rs`:

1. `cbscore_wrapper_path` — **removed** from `WorkerConfig` and
   `ResolvedWorkerConfig` and their resolution sites in `config.rs`. (No Rust
   code references this after the `executor.rs` rewrite.)
2. `cbscore_config_path` — **retained**; role changes from "path passed as env
   var to Python subprocess" to "path supplied to
   `cbscore::config::Config::load`". No field rename; the YAML key
   (`cbscore-config-path`) stays the same so operator config files need no edit.
3. `sigkill_escalation_timeout_secs` — **disposition choice**: either (a) retain
   and map to `RunOpts::timeout`'s inner SIGKILL budget in Phase 4's RAII guard,
   or (b) remove now that future-drop is the cancellation mechanism. Pick one
   and state it. The plan must name the decision before Commit 1 lands; it
   affects the YAML surface.

Editorial; no re-review needed.

---

### N2 — Commit 2 compose volume and Containerfile `FROM` target still

imprecise (carries forward from V14-N3)

**Location:** `002-20260508T1558-07-worker-cutover.md`, Commit 2 §Files.

**Design citation:** Design 002 §Rollback; cbsd-rs/CLAUDE.md §Commit Granularity
("each commit must compile and pass tests").

**What:** Two residual imprecisions remain in Commit 2 §Files after partial v14
remediation:

**a) Compose volume.** The plan says "drop any `cbscore-wrapper.py`-related
volume mounts or env vars." The actual `podman-compose.cbsd-rs.yaml`
`worker-dev` service has the exact bind-mount
`./cbsd-rs/scripts:/opt/cbsd-rs:ro` (confirmed on disk, line 78). This specific
path is not named in the plan; the generic instruction is ambiguous about
whether related infrastructure volumes (e.g., a future `/opt/cbsd-rs` mount for
other scripts) should also be dropped.

**b) Containerfile `FROM` target.** The plan says "drop: The
`RUN apt install python3.13` (or equivalent) line." The actual
`container/ContainerFile.cbsd-rs` uses
`FROM python:3.13-alpine3.21 AS worker-base` (Alpine base image, no `apt`). The
`cbsd-rs-worker` final stage begins `FROM worker-base AS cbsd-rs-worker` (line
156). Removing Python means changing the `FROM` target for `cbsd-rs-worker` from
`worker-base` to a plain Alpine base (e.g., `FROM alpine:3.21`) and removing or
restructuring the `worker-base` stage — not removing a single `RUN apt install`
line. The `"or equivalent"` hedge in the plan partially covers this but an
implementer who reads the plan and then looks at the Containerfile will need to
reconcile the mismatch without guidance.

**Why it matters:** Commit 2 is a deletion commit with no new Rust code. Its
correctness is entirely in the precision of what it removes. An implementer left
to interpret "drop `RUN apt install python3.13` or equivalent" against an
Alpine-based Containerfile may drop the wrong thing, leave `worker-base` in
place with Python still present, or duplicate effort when the detail is
eventually clarified.

**Resolution:**

- In the compose file bullet, replace the generic instruction with: "Remove the
  `./cbsd-rs/scripts:/opt/cbsd-rs:ro` bind-mount from the `worker-dev` service."
- In the Containerfile bullet, replace the current instruction with: "The Python
  runtime comes from `FROM python:3.13-alpine3.21 AS worker-base` (not a
  `RUN apt/apk` line). Post-M2, change `FROM worker-base AS cbsd-rs-worker` to
  `FROM alpine:3.21 AS cbsd-rs-worker` (or equivalent lean Alpine base). If
  `worker-base` is not referenced by any other final stage after this change,
  remove it."

Editorial; no re-review needed.

---

### N3 — README commit-count estimate still "~25–30" after Phase 7 adds 3

commits

**Location:** `cbsd-rs/docs/cbscore-rs/plans/README.md`, §Implementation Status.

**Design citation:** Plans README §Conventions ("Update the progress table in
this README and the affected phase file after each commit lands").

**What:** The README reads "Total estimate: ~25–30 commits across 7 phases." The
actual count is Phase 1 (5) + Phase 2 (5) + Phase 3 (4) + Phase 4 (3) + Phase 5
(6) + Phase 6 (5) + Phase 7 (3) = 31 commits. This was flagged in v14 as
cosmetic and not a finding against Phase 7 itself; it remains unfixed.

**Resolution:** Update the README to "~25–31 commits across 7 phases." One-line
edit; no re-review needed.

---

## 6. Suggestions

### S1 — Prefer SHA-256 fixture over `git worktree` for M2 acceptance

reference (carries forward from V14-S2)

**Location:** `002-20260508T1558-07-worker-cutover.md`, Commit 3 §Design
constraints.

**What:** The M2 acceptance test specifies the reference RPM set as
"pre-recorded in a fixture or generated by checking out the pre-Commit-1 worker
code via `git worktree`." The `git worktree` approach requires building a second
worker binary at an exact prior commit, then running a build with matching
toolchain, image, and rpmbuild environment — an operationally fragile set of
constraints. The SHA-256 fixture approach (record digests during Phase 6 M1
acceptance, store in a test fixture file) is simpler and
environment-independent.

**Suggestion:** State the fixture approach as the default: "Reference RPM
digests are SHA-256 hashes recorded during the Phase 6 Commit 5 M1 acceptance
run and stored at `cbsd-rs/cbsd-worker/tests/fixtures/m2_reference_sha256.json`.
The `git worktree` approach is available as a fallback when no pre-recorded
fixture exists." Non-blocking; the current `"or"` phrasing is not wrong.

---

## 7. Open Questions

### Q1 — cbsd-worker YAML config schema migration for `cbscore-wrapper-path`

(carries forward from V14-Q2)

**Location:** `002-20260508T1558-07-worker-cutover.md`, Commit 1.

**What:** After Commit 1 removes `cbscore_wrapper_path` from `WorkerConfig`,
operators who have `cbscore-wrapper-path: /opt/cbsd-rs/cbscore-wrapper.py` in
their `worker.yaml` will silently have that key ignored by serde (confirmed:
`WorkerConfig` does not use `#[serde(deny_unknown_fields)]`). The plan does not
acknowledge this.

This is likely acceptable — silent-ignore is the right behaviour for a
deprecated field — but the plan should explicitly state: "Operators with
`cbscore-wrapper-path:` in their `worker.yaml` need not edit the file; serde
silently ignores the unknown key. A one-line note in the M2 release changelog is
sufficient." Or, if a clippy/config-validation pass is wanted, add a
`deny_unknown_fields` comment so the decision is explicit.

The question is not blocking Phase 7 implementation; it is an operator
communication decision.

---

## 8. Phases 1–6 Regression Recheck

No new issues found in Phases 1–6. All prior findings from v1 through v13 remain
closed. Phase 7's additions to Phase 4's `RunReport` struct spec (the
`build_report` field) are consistent with Phase 4 Commit 3's existing field
layout and add no new ambiguity.

**Phases 1–6 are free of regression.**

---

## 9. Subscriber Layer Clarification (Non-Finding)

The §Subscriber layer design in Phase 7 Commit 1 says the layer is "installed
per-build (not globally)." The actual design described — a
`tracing_subscriber::Layer<S>` whose `on_event` routes by `build_id` span field,
installed once in the registry and driven by `runner::run(...).instrument(span)`
— is a globally-registered layer that routes per-build via span context. The
"not globally" phrase refers to the layer's routing scope (it forwards only the
active build's events to the WebSocket channel), not to the registry
installation mechanism.

This is not a design flaw. The worker handles one build at a time (confirmed:
`ws/handler.rs` line 153 — "Active build state — only one build at a time"), so
there is no concurrent-build routing collision. The phrasing is mildly confusing
but correct. No plan change required; noting here for implementer orientation.

---

## 10. Verdict

**The plan corpus is ready for implementation start.**

No new blockers or major concerns. Three minor issues (N1, N2, N3) are editorial
and can be corrected inline during Phase 7 Commit 1 and Commit 2 without
re-review. One suggestion (S1) is optional. One open question (Q1) is an
operator-communication decision that does not block implementation.

**New findings by severity:** 0 blockers, 0 major concerns, 3 minors (N1–N3), 1
suggestion (S1), 1 open question (Q1).

**Carry-forward status:**

| V14 ID | Description                          | V15 Status   |
| ------ | ------------------------------------ | ------------ |
| M1     | Subscriber layer underspecified      | CLOSED       |
| Q1     | `build_report` on direct-dep path    | CLOSED       |
| S1     | `future.instrument` not named        | CLOSED       |
| N2     | Config fields not in Commit 1 §Files | CARRIES → N1 |
| N3     | Compose/ContainerFile imprecise      | CARRIES → N2 |
| S2     | SHA-256 fixture vs worktree          | CARRIES → S1 |
| Q2     | Worker config schema unknown-key     | CARRIES → Q1 |
