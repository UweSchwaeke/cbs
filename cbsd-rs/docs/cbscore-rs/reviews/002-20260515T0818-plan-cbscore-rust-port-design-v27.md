# Plan Review v27 — Pre-Implementation Audit Pass 7 Closure Confirmation

**Review target:** seq-002 plan corpus (Phases 1–6)\
**Commit under review:** `f6c24f4`\
**Reviewer:** Staff Engineer (design-reviewer agent)\
**Date:** 2026-05-15

---

## §Scope

Focused confirmation review of the 19 pre-implementation audit pass-7 findings
(G1.1, G1.2, G1.4, G3.1, G3.3, G3.4, G4.1, G4.2, G4.3, G5.2, G5.4, G6.1, G6.2,
G6.3, G6.4, G7.1, G7.2, G7.3, G8.3) claimed closed in commit `f6c24f4`. Also
confirms no-drift on five structural invariants established by passes 1–6. A
`prettier --check` pass on all five edited files is included.

## §Method

For each finding, the closure text was located directly in the current plan file
at the relevant commit section. Quoted phrases are verified verbatim; line
references are recorded where the text lands. The no-drift checks read the live
plan corpus state — not git diff — and compare against the known-good baselines
recorded in the v26 review and project memory.

---

## §Closure Verification

### G1.1 — Phase 5 C1: concurrent-invocation as invoker responsibility

**Claimed change:** Phase 5 C1 §Design constraints adds "Concurrent invocation:
invoker responsibility" bullet.

**Verified.** Phase 5 C1 §Design constraints contains:

> **Concurrent invocation: invoker responsibility.** Each `cbsbuild build`
> invocation derives `config.paths.scratch` from its `--config` (or
> default-resolved config). cbsbuild does **not** acquire a lock on the scratch
> path; the binary assumes the invoker (operator or cbsd-worker) ensures no two
> concurrent invocations share the same scratch path for the same component.
> cbsd-worker enforces this by handling one build at a time (Phase 7 spec).

The bullet is present, the lock-omission and cbsd-worker one-at-a-time guarantee
are both stated. **Closed.**

---

### G1.2 — Phase 6 C5: reproducibility note rewritten; `--name <fixed>` for stable test names

**Claimed change:** Phase 6 C5 §Design constraints "Reproducibility" rewritten;
`gen_run_name` non-deterministic; `--name <fixed>` for stable test names.

**Verified.** Phase 6 C5 §Design constraints contains:

> **Reproducibility** — the structural-equivalence acceptance does not require
> determinism. `gen_run_name` uses `rand::rng()` non-deterministically; when the
> test needs a stable run-name (e.g., for log-file path assertions), it bypasses
> the random suffix via the `--name <fixed>` CLI flag. The earlier "fixed
> environment seeds" framing was dropped — RPM `BUILDTIME` / `BUILDHOST`
> non-determinism is captured by the structural-equivalence acceptance
> (criterion 2), not by seeded reproducibility.

Non-determinism acknowledged, `--name <fixed>` bypass path documented, stale
"fixed environment seeds" framing explicitly retired. **Closed.**

---

### G1.4 — Accepted as invoker responsibility; no code change

**Claimed change:** No plan text change required; accepted as invoker
responsibility matching Python TOCTOU semantic.

**Verified.** The v26 review confirmed the G1.4 "accepted" disposition. No plan
text change was required, and none was made. The commit message records the
acceptance rationale ("invoker responsibility (matches Python TOCTOU semantic;
operator-side concern)"). **Closed — by acceptance.**

---

### G3.1 — Phase 1 C2: `ComponentError::Yaml` source-chain termination note

**Claimed change:** Phase 1 C2 `ComponentError::Yaml` gets documented
`Error::source()` termination note.

**Verified.** Phase 1 C2 `ComponentError::Yaml` variant documentation now reads:

> `Error::source()` chain traversal **terminates** at this variant — the
> underlying `serde_saphyr::Error` is not preserved as a `#[source]` field
> because the parser type stays in the `cbscore` library crate per design 001
> line 366–370 (`cbscore-types` never depends on a format crate in
> `[dependencies]`). The lost-source trade-off is intentional:
> operator-actionable info survives in the stringified message; structured chain
> inspection (e.g. `eyre::Report::chain()`) sees only the cbscore-types-level
> error.

Chain termination is explicitly documented, the trade-off is acknowledged. The
`Display` text pin `"YAML parse error in {path}: {message}"` was also verified
present in the operator-facing Display block. **Closed.**

---

### G3.3 — Phase 3 C2: `VaultError::AuthFailed` explicit `#[source]`; `RequestFailed` explicit `#[from]`

**Claimed change:** `VaultError::AuthFailed` gets explicit `#[source]` on its
`source` field; `RequestFailed` gets explicit `#[from]`.

**Verified.** Phase 3 C2 `VaultError` variant spec:

> `AuthFailed { method: &'static str, #[source] source: vaultrs::error::ClientError }`
> — … Explicit `#[source]` annotation (not relying on thiserror's auto-detection
> of the field name `source`) ensures `Error::source()` chain traversal reaches
> the underlying ClientError unambiguously.

> `RequestFailed { #[from] source: vaultrs::error::ClientError }` — … `#[from]`
> triggers the automatic `From<ClientError>` impl and marks the field as
> `#[source]` automatically (thiserror convention).

Both annotations are explicit in the spec, with rationale for each choice.
**Closed.**

---

### G3.4 — Phase 3 C1: `S3Error` variant structure pinned (`Head`/`List`/`Put`/`Other`)

**Claimed change:** `S3Error` variant structure pinned with per-operation
variants plus `Other` catch-all.

**Verified.** Phase 3 C1 §Files contains:

> - `Head { #[from] source: SdkError<HeadObjectError> }` — used by
>   `check_release_exists`.
> - `List { #[from] source: SdkError<ListObjectsV2Error> }` — used by
>   `check_released_components`.
> - `Put { #[from] source: SdkError<PutObjectError> }` — used by
>   `release_desc_upload`, `release_upload_components`, `s3_upload_rpms`.
> - `Other { #[from] source: aws_sdk_s3::Error }` — catch-all for the aggregate
>   error type when an operation surfaces an error not covered by its
>   per-operation type.

All four named variants are present with their `#[from]` annotations and
consumer callsites identified. **Closed.**

---

### G4.1 — Phase 4 C3: `user_args` unvalidated by design

**Claimed change:** Phase 4 C3 pins `user_args` passthrough as
unvalidated-by-design.

**Verified.** Phase 4 C3 §Design constraints:

> **`user_args` passthrough is unvalidated.** `RunOpts::user_args: Vec<String>`
> is passed verbatim to the in-container `cbsbuild runner build` invocation …
> The host runner performs no flag-shape validation. Unrecognised flags fail at
> the in-container clap parser, surface as `RunnerError::Command(...)`, and
> propagate as a build failure. This is the intended escape-hatch behaviour:
> `user_args` is the operator's vehicle for flags cbscore-rs doesn't model, and
> cbsbuild trusts the operator not to pass garbage.

Design intent explicitly stated; error path identified. **Closed.**

---

### G4.2 — Phase 4 C3: `$PATH` as operator responsibility

**Claimed change:** Phase 4 C3 pins in-container `$PATH` as operator
responsibility.

**Verified.** Phase 4 C3 §Design constraints:

> **In-container `$PATH` is the operator's responsibility.** cbsbuild inherits
> `$PATH` from the operator-supplied builder image (the `distro` field's image).
> Operators must use an image whose `$PATH` resolves `rpmbuild`, `gpg`, `git`,
> `buildah`. cbscore-rs does not enforce or check this; tools missing from
> `$PATH` surface as standard "command not found" subprocess errors at first
> use.

Responsibility boundary is stated, tools list is enumerated, failure mode
identified. **Closed.**

---

### G4.3 — Phase 4 C3: `--workdir /runner` on podman invocation

**Claimed change:** Phase 4 C3 pins `--workdir /runner` on the podman
invocation.

**Verified.** Phase 4 C3, the paragraph following the mount table:

> The `--workdir /runner` flag is set on the same podman invocation to pin the
> in-container working directory; the in-container CLI uses absolute paths for
> config/secrets/descriptor everywhere so `--workdir` is for determinism, not
> correctness.

Flag is pinned, cosmetic/determinism rationale is present. **Closed.**

---

### G5.2 — Phase 6 C1: `#[tokio::main(flavor = "multi_thread")]`

**Claimed change:** Phase 6 C1 pins `#[tokio::main(flavor = "multi_thread")]` on
the `main` fn.

**Verified.** Phase 6 C1 §Files, `cbsd-rs/cbsbuild/src/main.rs`:

> Tokio multi-thread runtime entry point declared as
> `#[tokio::main(flavor = "multi_thread")]` on the `main` fn (default worker
> thread count = num_cpus, accepted as the standard pattern; the macro awaits
> the runtime to idle before returning, which gives spawned background tasks a
> chance to finish on normal exit).

The flavor annotation is explicit and the rationale is provided. **Closed.**

---

### G5.4 — Phase 6 C1: `tracing-appender::non_blocking` WorkerGuard in `main`

**Claimed change:** Phase 6 C1 pins `tracing-appender::non_blocking` WorkerGuard
held in `let _guard = ...` binding; Drop at main exit flushes the background
log-writer thread.

**Verified.** Phase 6 C1 §Files:

> `tracing-appender::non_blocking` returns a `WorkerGuard` that `main` holds in
> a `let _guard = ...` binding so its `Drop` (at `main`'s return) flushes the
> background log-writer thread before the process exits — guarantees the last
> log lines reach disk on any exit path.

Binding pattern is pinned, flush-on-exit guarantee is documented. **Closed.**

---

### G6.1 — Phase 5 C2: warn call pinned to structured fields (`path = %path`)

**Claimed change:** Phase 5 C2 warn call pinned to structured fields, not
message interpolation.

**Verified.** Phase 5 C2 §Design constraints, the per-file error logging block:

```rust
tracing::warn!(
    target: TARGET_CORE_COMPONENT,
    path = %path,
    "component file parse failed: {}", err,
);
```

The `path = %path` structured field is explicit; the comment "`path` is a
structured field (not interpolated into the message string) so log parsers /
filters can extract it" is present. **Closed.**

---

### G6.2 — Phase 4 C3: `RunOpts` adds `trace_id: Option<Uuid>`; runner top-level span carries `trace_id`

**Claimed change:** Phase 4 C3 `RunOpts` adds `trace_id: Option<Uuid>`; runner's
top-level span carries `trace_id` as a structured field.

**Verified.** Phase 4 C3 §Files:

> `pub struct RunOpts { pub timeout: Duration, pub user_args: Vec<String>, pub image_ref: String, pub trace_id: Option<Uuid>, … }`
> where `trace_id` carries the cross-process correlation UUID populated by the
> cbsd-worker (Phase 7) when invoking the runner; `None` for standalone
> `cbsbuild` invocations. The runner's top-level `tracing::span!` carries
> `trace_id` as a structured field — populated from `RunOpts::trace_id`
> (rendered as the UUID string when `Some`, or the literal `"none"` when
> `None`). Consistent field-name policy across standalone CLI and worker
> contexts; downstream subscribers can filter by `trace_id` either way.

Field type, default value, rendering rules, and cross-context consistency policy
are all present. **Closed.**

---

### G6.3 — Phase 1 C2: `logger.rs` canonical span-field names

**Claimed change:** Phase 1 C2 logger.rs adds canonical span-field names:
`trace_id`, `build_id`, `component`, `stage`, `path`.

**Verified.** Phase 1 C2 §Files, `logger.rs`, immediately following the target
enumeration:

> **Canonical span-field names (corpus-wide):** alongside the target
> enumeration, `logger.rs` declares the canonical structured-field names every
> subsystem uses on spans and events:
>
> - `trace_id` (UUID string; populated by `RunOpts::trace_id` in Phase 4 — real
>   UUID when supplied by cbsd-worker, literal `"none"` for standalone CLI
>   invocations).
> - `build_id` (UUID string; populated by Phase 7's subscriber-layer wrapper
>   span when a cbsd-worker is driving the build).
> - `component` (component `name:` string; on per-component spans inside the
>   builder pipeline).
> - `stage` (one of `"prepare"`, `"rpmbuild"`, `"containers"`, `"signing"`,
>   `"upload"`; on builder stage spans).
> - `path` (file path on per-file warnings / errors, e.g. the component loader's
>   WARN events).
>
> Subsystems use these field names exactly; deviations (`buildId`, `traceID`,
> `comp`, …) are not permitted.

All five canonical field names are present with type, source, and
permitted-value notes. The allowlist of prohibited alternatives is explicit.
**Closed.**

---

### G6.4 — Phase 3 C1+C2: `#[tracing::instrument]` on every public async function

**Claimed change:** Phase 3 C1 and C2 pin
`#[tracing::instrument(level = "debug", target = "...", skip(...))]` on every
public async function.

**Verified.** Phase 3 C1 §Design constraints:

> **Tracing instrumentation.** Every public async function is wrapped in
> `#[tracing::instrument(level = "debug", target = "cbscore::utils::s3", skip(body, rpm_paths))]`
> (or the appropriate target / skip-list per function — `skip` excludes large
> buffers and credential-bearing args from automatic span field capture).
> Captures the operation duration and arg shape in the trace timeline so
> operators can see slow S3 ops without adding manual span scaffolding.

Phase 3 C2 §Design constraints:

> **Tracing instrumentation.** Every public async function is wrapped in
> `#[tracing::instrument(level = "debug", target = "cbscore::utils::vault", skip(config))]`
> (`skip(config)` excludes the `VaultConfig` arg from automatic span field
> capture — it carries credential-bearing fields). Captures KV-read /
> AppRole-login / transit-sign duration in the trace timeline so slow Vault ops
> surface without manual span scaffolding.

Both C1 and C2 pin the attribute with explicit `level`, `target`, and `skip`
specifications. The `skip(config)` rationale (credential-bearing) is present.
**Closed.**

---

### G7.1 — Phase 1 C1: Cargo.lock committed at workspace root

**Claimed change:** Phase 1 C1 explicitly notes Cargo.lock committed at
workspace root.

**Verified.** Phase 1 C1 §Files, `cbsd-rs/Cargo.toml`:

> `Cargo.lock` is committed at the workspace root, matching the existing cbsd-rs
> convention — required for reproducible binary builds and for the
> `SQLX_OFFLINE` CI cache pattern used elsewhere in cbsd-rs.

Explicit statement with rationale. **Closed.**

---

### G7.2 — Phase 1 C1: `edition = "2024"` workspace-wide

**Claimed change:** Phase 1 C1 pins `[workspace.package]` `edition = "2024"`;
each new crate inherits via `edition.workspace = true`.

**Verified.** Phase 1 C1 §Files:

> Set `[workspace.package]` block with `version = "0.1.0"` … **and**
> `edition = "2024"` (workspace-wide). Every member crate inherits via
> `version.workspace = true` and `edition.workspace = true`.

Edition pin and inheritance pattern both stated. **Closed.**

---

### G7.3 — Phase 1 C1: `[workspace.lints]` policy

**Claimed change:** Phase 1 C1 pins `[workspace.lints.rust]` with
`missing_docs = warn` and `[workspace.lints.clippy]` with `all = warn`; each
crate inherits via `[lints] workspace = true`.

**Verified.** Phase 1 C1 §Files:

> Add `[workspace.lints.rust]` with `missing_docs = "warn"` and
> `[workspace.lints.clippy]` with `all = { level = "warn", priority = -1 }` —
> centralises the lint policy so no per-crate `#![warn(missing_docs)]` attribute
> can be accidentally omitted. Each new crate inherits via
> `[lints] workspace = true`.

Both tables are named with their exact keys; the inheritance mechanism and the
motivation (prevent accidental omission) are documented. **Closed.**

---

### G8.3 — Phase 4 C3: in-container log file path explained in mount-table section

**Claimed change:** Phase 4 C3 mount-table section adds explanation of the
in-container log file path (`/runner/logs/cbs-build.log` in container writable
layer; operator-add-your-own-mount for persistence).

**Verified.** Phase 4 C3, immediately following the mount table:

> **In-container log file (`/runner/logs/cbs-build.log`).** Design 002 §Logging
> routes the in-container `cbsbuild`'s log output via
> `config.logging.log_file = "/runner/logs/cbs-build.log"` (set by the host
> runner on `new_config` before writing the config tempfile). This path lives in
> the container's writable layer — **no host mount** — and is not persisted
> after the container exits. Operators wanting log persistence add their own
> host-to-container mount via custom config (e.g., bind-mount a host log dir
> onto `/runner/logs`); cbscore-rs does not enforce this. … This matches Python
> cbscore behaviour.

Exact path, storage location (writable layer), persistence policy, and
operator-side escape hatch are all documented. **Closed.**

---

## §No-Drift Check

Five structural invariants from passes 1–6 were spot-checked against the live
plan corpus state.

| Invariant                                                                                             | Expected                                               | Observed                                                                                                                                              | Status |
| ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| Phase 5 commit count                                                                                  | 7 commits                                              | `grep -c "^## Commit"` → 7                                                                                                                            | PASS   |
| Phase 3 C3 Secrets uses `HashMap`                                                                     | Four per-family `HashMap<String, *>` fields            | Lines 286–288 confirm `HashMap<String, GitCreds>`, `HashMap<String, StorageCreds>`, `HashMap<String, SigningCreds>`, `HashMap<String, RegistryCreds>` | PASS   |
| Phase 1 C2 logger.rs target enumeration (22 targets, G6.3 adds canonical field names to same section) | 22 named targets; canonical field block appended after | Grep count: 22 `"cbscore..."` literals (lines 187–212); canonical field block at lines 219–237                                                        | PASS   |
| `cbscore-rs/CLAUDE.md` has 6 correctness invariants                                                   | Items 1–6 listed under §Correctness Invariants         | Lines 294/304/309/315/325/329 confirm all six numbered items                                                                                          | PASS   |
| `cbscore-types` zero-IO discipline preserved                                                          | §Goal says "zero-IO"; no IO deps in `[dependencies]`   | §Goal line 28: "Land a zero-IO `cbscore-types` crate"; Commit 1 §Files explicitly excludes `serde_json`/`serde_saphyr` from `[dependencies]`          | PASS   |

---

## §Prettier Check

All five edited files pass `prettier --check`:

```
prettier --check \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-04-runner.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-05-builder-and-releases.md \
  cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-06-cbsbuild-cli.md

All matched files use Prettier code style!
```

---

## §Findings

None. No new findings were surfaced during this review.

---

## §Verdict

> **Approve — G1+G3+G4+G5+G6+G7+G8 (19 findings) closed; pre-impl audit pass 7
> fully resolved; design corpus + plan corpus ready for Phase 1 implementation
> start.**
