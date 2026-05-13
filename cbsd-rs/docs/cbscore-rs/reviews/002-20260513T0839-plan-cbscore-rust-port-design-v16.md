# Plan Review v16 — cbscore Rust port (plan 002)

**Scope:** Confirmation pass. Verifies that every v15 finding (N1, N2, N3, S1,
Q1) is correctly closed by commits `31ce251` (N1+N2+N3+Q1) and `86e5c89` (S1),
and that no new drift has been introduced into Phases 1–7 or the README.

**Files reviewed:**

- `cbsd-rs/docs/cbscore-rs/plans/README.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-07-worker-cutover.md` (Phase
  7 — primary focus)
- Phases 1–6 (regression recheck via `git diff 964a9ed 31ce251` and
  `git diff 964a9ed 86e5c89` — neither commit touched Phases 1–6)
- Live source: `container/ContainerFile.cbsd-rs`, `podman-compose.cbsd-rs.yaml`
- Prior review: `002-20260512T1020-plan-cbscore-rust-port-design-v15.md`

---

## 1. Scope

This is a confirmation-only pass over the two closure commits. No new design or
plan text was introduced outside Phase 7 and the README. The review is scoped to
the five v15 items; any finding not on that list is new and noted in §4 below.

---

## 2. Method

For each v15 finding:

1. Located the exact plan text that was added or changed by the closure commit.
2. Cross-checked against the design corpus (002 §Migration Strategy, §Rollback),
   the live `ContainerFile.cbsd-rs`, and `podman-compose.cbsd-rs.yaml` where the
   finding required live-file accuracy.
3. Verified prettier compliance: `prettier --check` on both edited files
   returned "All matched files use Prettier code style!" (prettier found at
   `/home/fnk/.nvm/versions/node/v25.8.1/bin/prettier`).
4. Verified Phases 1–6 are unaffected: `git diff 964a9ed 31ce251` and
   `git diff 964a9ed 86e5c89` show no changes outside
   `002-20260508T1558-07-worker-cutover.md` and `README.md`.

---

## 3. Closed Findings Confirmed

### V15-N1 — `config.rs` field disposition (closed by `31ce251`)

**Verified.** Phase 7 Commit 1 §Files now contains a
`cbsd-rs/cbsd-worker/src/config.rs` bullet with three explicit sub-bullets:

- `cbscore_wrapper_path` — **removed** from `WorkerConfig`,
  `ResolvedWorkerConfig`, the YAML key, and the resolution site. The removal
  rationale correctly cites the zero-warnings clippy policy (cbsd-rs/CLAUDE.md
  §Pre-Commit Checks) and the fact that no Rust code references the field after
  the executor rewrite.
- `cbscore_config_path` — **retained**; role change described as "path passed as
  `CBS_CONFIG_PATH` env var to the Python subprocess" → "path supplied to
  `cbscore::config::Config::load(path).await?`". No struct or YAML rename;
  operator config files need no edit. This matches design 002's `Config::load`
  surface exactly (the `async fn` qualifier was added by the v6 P3-S2 fix, which
  is cited inline).
- `sigkill_escalation_timeout_secs` — **removed**. The rationale correctly names
  the Phase 4 Commit 3 RAII guard / future-drop cancellation mechanism as the
  replacement, and states that a max-build-duration timeout, if wanted in
  future, lands as a distinct `RunOpts::max_build_secs` field under its own
  design review — a clean decision, not a hedge.

**Q1 operator-communication language** is present in the Commit 1 §Files block
(lines 131–135 of the current plan): "Operators with `cbscore-wrapper-path:` or
`sigkill-escalation-timeout-secs:` in their `worker.yaml` will have those keys
silently ignored by serde (`WorkerConfig` does not use
`#[serde(deny_unknown_fields)]`); this is the intended deprecation behaviour.
The M2 release changelog calls out the two retired keys so operators can clean
up at their convenience." This is precisely the language the v15 Q1 resolution
asked for.

**V15-N1: CLOSED. V15-Q1: CLOSED (settled as part of N1 text).**

---

### V15-N2 — Commit 2 §Files precision (closed by `31ce251`)

**Verified against live files.**

**Containerfile `FROM` target:** The plan now reads (paraphrased from the Commit
2 §Files block): "The live worker image is Alpine-based; it uses
`FROM python:3.13-alpine3.21 AS worker-base` and the final stage is
`FROM worker-base AS cbsd-rs-worker`. Post-M2: change
`FROM worker-base AS cbsd-rs-worker` to `FROM alpine:3.21 AS cbsd-rs-worker`. If
`worker-base` is not referenced by any other final stage after the change,
remove it." This matches the live `container/ContainerFile.cbsd-rs` exactly:
`FROM python:3.13-alpine3.21 AS worker-base` at line 46,
`FROM worker-base AS cbsd-rs-worker` at line 156.

**Spot-check on `worker-base` consumers:** The live file also has
`FROM worker-base AS cbsd-rs-dev-worker` at line 216 (the development worker
image). The plan's conditional "if not referenced by any other final stage"
clause correctly handles this: `worker-base` will remain in place because the
dev-worker stage still needs it. An implementer reading the plan and then the
Containerfile can act without ambiguity.

**Compose volume:** The plan now names the exact bind-mount: "remove the
`./cbsd-rs/scripts:/opt/cbsd-rs:ro` bind-mount from the `worker-dev` service."
Confirmed present in `podman-compose.cbsd-rs.yaml` at line 78 under the
`worker-dev` service. The "Drop any `CBSCORE_*` or `PYTHONPATH` env vars on the
same service" instruction is also present; the live service has none, so nothing
extra to remove — but the instruction is harmless and correctly scoped.

**V15-N2: CLOSED.**

---

### V15-N3 — README commit-count estimate (closed by `31ce251`)

**Verified.** The README §Implementation Status now reads:

> **Total estimate:** ~25–31 commits across 7 phases.

Per-phase ranges from the phase progress tables: Phase 1 (4–5), Phase 2 (4–5),
Phase 3 (3–4), Phase 4 (2–3), Phase 5 (6), Phase 6 (4–5), Phase 7 (2–3). Low
sum: 4+4+3+2+6+4+2 = 25. High sum: 5+5+4+3+6+5+3 = 31. The "~25–31" bound is
arithmetically consistent with the per-phase ranges. The previous "~25–30" upper
bound was off by one.

**V15-N3: CLOSED.**

---

### V15-S1 — git worktree as the M2 reference method (closed by `86e5c89`)

**Verified.** Phase 7 Commit 3 §Files now contains a self-contained six-step
worktree procedure. Checking each S1 sub-criterion:

1. **Unambiguous and self-contained.** The six steps are numbered, each with a
   concrete command or action (`git worktree add`,
   `cargo build --release -p cbsd-worker`, drive reference build, drive
   candidate build, byte-compare, `git worktree remove`). No external knowledge
   is required; the env-var contract is declared inline and cross- referenced to
   the Phase 6 M1 acceptance test.

2. **Fixture fallback not retained.** The plan states only the worktree
   procedure; there is no "or fixture" alternative. The v15 S1 review text
   explained why worktree was selected over fixture (host-dependent drift
   problem: "if there is non-determinism in the RPM bytes, a worktree-based
   comparison still detects M2-specific divergence, whereas a fixture would
   freeze the wrong baseline"). That rationale is now recorded inline in the
   plan, not just in the review thread. The user's explicit choice is
   documented.

3. **Toolchain/image-consistency guarantee captured.** Step 2 uses "the
   worktree's `Cargo.lock` snapshot and the project's pinned Rust toolchain."
   Step 3 and step 4 both use the same `CBSCORE_TEST_BUILDER_IMAGE` env var. The
   closing sentence confirms: "Toolchain and builder image stay constant across
   the two runs because steps 2–4 execute back-to-back inside one `cargo test`
   invocation."

4. **Pre-Commit-1 reference build failure mode named.** Step 2 includes: "If the
   build fails (e.g., toolchain has aged off the host), the test fails with a
   clear 'pre-M2 reference build failed' message naming the missing toolchain or
   dep." The failure message text is explicit.

**V15-S1: CLOSED.**

---

## 4. Findings

None.

---

## 5. Phases 1–6 Regression Recheck

`git diff 964a9ed 31ce251` and `git diff 964a9ed 86e5c89` show no changes to any
Phase 1–6 plan file. The V14-M1 closure (subscriber layer subsection in Phase 7
Commit 1) and V14-Q1 closure (`RunReport.build_report` field in Phase 4
Commit 3) were verified closed in the v15 review and remain intact and
unmodified.

**Phases 1–6 are free of regression.**

---

## 6. Conventions

`prettier --check` passes on both edited files
(`002-20260508T1558-07-worker-cutover.md`, `README.md`). Three inline code spans
in the Phase 7 file exceed 79 characters (lines 99, 178, 195); these are
unbreakable inline code spans that prettier does not wrap, and they were present
before the closure commits. No convention violation.

---

## 7. Verdict

**Approve — v15 N1+N2+N3+S1+Q1 all closed; plan corpus implementation-ready.**

All five v15 findings are correctly and completely resolved. No new findings.
Phases 1–6 are unaffected. The plan corpus is ready to proceed to
implementation.

**Finding counts:** 0 blockers, 0 major concerns, 0 minors, 0 suggestions, 0
open questions.

| V15 ID | Description                                         | V16 Status |
| ------ | --------------------------------------------------- | ---------- |
| N1     | `config.rs` field disposition absent from C1 §Files | CLOSED     |
| N2     | Commit 2 §Files imprecise (volume + FROM target)    | CLOSED     |
| N3     | README total estimate "~25–30" (should be "~25–31") | CLOSED     |
| S1     | git worktree procedure unspecified                  | CLOSED     |
| Q1     | Operator YAML migration language missing            | CLOSED     |
