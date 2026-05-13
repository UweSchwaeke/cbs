# Phase 2 — M1.1: subprocess + secret redaction + podman/buildah/skopeo + git wrappers + parse_version family

## Status

**Approved — finalized and ready for implementation.** Last audited at the v18
corpus pass (`reviews/002-20260513T0940-plan-cbscore-rust-port-design-v18.md`,
verdict commit `a806158`); zero findings across CRITICAL / MAJOR / MINOR /
SUGGESTION / OPEN QUESTION on the seq-002 phase plans. See `README.md` for the
dependency graph and the M0 / M1 / M2 milestone cuts.

## Progress

| #   | Commit                                                       | ~LOC | Status  |
| --- | ------------------------------------------------------------ | ---- | ------- |
| 1   | `cbscore: add utils::subprocess (SecureArg + async_run_cmd)` | ~500 | Pending |
| 2   | `cbscore: add utils::podman + utils::buildah wrappers`       | ~400 | Pending |
| 3   | `cbscore: add images::skopeo driver`                         | ~150 | Pending |
| 4   | `cbscore: add utils::git wrapper`                            | ~500 | Pending |
| 5   | `cbscore: add versions::utils parse_version family`          | ~230 | Pending |

**Estimate:** ~1780 LOC, 5 commits.

## Goal

Land the subprocess foundation and every subprocess-based subsystem wrapper in
`cbscore`. After Phase 2, every later subsystem (S3, Vault, runner, builder,
releases, images sign / sync) can be built on top of these wrappers. This is the
lowest layer of the M1 implementation; no public CLI surface yet (Phase 6 owns
that), no IO-bearing config or secrets manager (Phase 3 owns those).

End state: `cargo build --workspace` and `cargo test --workspace` pass;
`cbscore` exposes `utils::subprocess`, `utils::podman`, `utils::buildah`,
`utils::git`, `images::skopeo`, and `versions::utils` modules; the `cbsbuild`
binary still prints its placeholder string (CLI tree lands in Phase 6).

## Depends on

- Phase 1 — `cbscore-types` provides `CommandError`, `PodmanError`,
  `BuildahError`, `ImageDescriptorError`, `VersionError`, `MalformedVersion`,
  and the `logger` module that every Phase 2 wrapper uses for tracing targets.
- Design 002 — §Capability Mapping, §Error Taxonomy, §Subprocess & Secret
  Redaction (lines 939–1071), §Image Sign & Sync (§Skopeo driver, lines
  1077–1083), §Version Descriptors & Creation (lines 672–696).
- Design 001 — §Cargo Sketch (`cbscore` deps allow `regex`, `which`), §Lift-out
  invariants (`utils::subprocess` and `utils::git` are future `cbscommon-rs`
  candidates; Phase 2 keeps them self-contained for a mechanical future
  lift-out, but does NOT actually move them out).

## Out of scope

- Higher-level callers (runner, builder, S3, Vault, secrets manager, config IO)
  — all later phases.
- `cbsbuild` CLI surface (Phase 6).
- Image sign + image sync (`cbscore::images::sign`, `cbscore::images::sync`) —
  those wrap GPG / Vault transit signing and orchestration, both deferred to
  Phase 5 alongside the builder pipeline.
- Actual lift-out to `cbscommon-rs`. Design 001 §Lift-out invariants names
  `utils::subprocess` and `utils::git` as candidates, and Phase 2 honours the
  lift-out invariants (no cross-module deps beyond `cbscore-types::errors` + the
  logger), but the modules stay inside `cbscore` until a separate lift-out
  commit lands.
- Component-ref resolution via `git_ls_remote`. Phase 2 lands the raw wrapper
  (Commit 4) but does NOT land the resolution logic that `version_create_helper`
  calls (design 002 §Version creation lines 706–711, which composes
  `git_ls_remote` over a component list to resolve each ref to a SHA). That
  orchestrator lives in `cbscore::versions::create` alongside the
  `cbsbuild versions create` CLI surface, both of which land in Phase 6.

## Commit 1 — `utils::subprocess` (SecureArg + async_run_cmd)

The foundation. Every other wrapper in this phase invokes `async_run_cmd`.

**Files:**

- `cbsd-rs/cbscore/src/utils/mod.rs` — new module file declaring
  `pub mod subprocess`. Later commits in this phase extend it with
  `pub mod podman`, `pub mod buildah`, `pub mod git`.
- `cbsd-rs/cbscore/src/utils/subprocess.rs` — `SecureArg` trait, `CmdArg` enum
  (`Plain(String)` / `Secure(Box<dyn SecureArg + Send + Sync>)`), concrete
  `Password` / `PasswordArg` / `SecureUrl` impls,
  `async_run_cmd(&[CmdArg], RunOpts)` returning
  `Result<RunOutcome, CommandError>`, `RunOpts` with timeout / cwd / extra_env /
  out_cb, and the `--pass[phrase]` sanitiser regex from design 002 lines
  1062–1071. Per design 002 §Subprocess & Secret Redaction.
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod utils;` declaration.

**Design constraints:**

- **Internal-timeout-only contract** per design 002 lines 1018–1031.
  `async_run_cmd` owns its timeout; the caller never sees a future-drop
  cancellation as `CommandError::Timeout`. Use `Child::start_kill()` on timeout,
  then `Child::wait().await` to reap, then return
  `CommandError::Timeout { after }` with whatever stdout / stderr was captured.
- **RAII drop guard** for child-process kill on outer cancellation (a
  `tokio::select!` branch losing, an awaited future being dropped). Reaping
  happens in the guard's `Drop`, best-effort.
- **Tracing emits redacted forms, never plaintext.** `CmdArg`'s `Debug` impl
  calls `SecureArg::redacted()` for the `Secure` variant (design 002 lines
  973–981). CLAUDE.md §Correctness Invariants item 5 is the hard rule.
- `reset_python_env` is **not ported** per design 002 §Open Questions resolution
  (lines 1386–1396) — the Rust binary has no venv-shadowing problem to solve.

**Testable:**

- Unit tests on `SecureArg::redacted()` for each impl (`Password` → `"****"`,
  `PasswordArg` → `"--passphrase=****"`, `SecureUrl` → template with `{user}` /
  `{pass}` placeholders redacted).
- `CmdArg`'s `Debug` impl emits the redacted form, not plaintext.
- Sanitiser regex round-trips against the design 002 line 1063–1071 examples
  (`--passphrase foo` two-token form, `--passphrase=foo` one-token form).
- `async_run_cmd` returns `Err(CommandError::Timeout { after })` when a
  `sleep 5` child is given a 100ms timeout; captured stdout / stderr up to the
  kill point are non-empty.
- `async_run_cmd` accumulates both stdout and stderr concurrently (use a fixture
  that interleaves both pipes).
- `async_run_cmd` calls the `out_cb` per line when supplied, otherwise
  accumulates into `RunOutcome.stdout` / `.stderr`.
- **RAII drop-guard smoke test (optional, `#[ignore]` if flaky in CI):** spawn
  `sleep 60` via `async_run_cmd` inside a `tokio::select!` with a 50ms timer;
  let the timer branch win and cancel the subprocess branch; capture the child
  PID from `RunOpts` or a test hook and verify
  `nix::sys::signal::kill(pid, None)` returns `Err(Errno::ESRCH)` (process
  gone). Verifies the outer-cancellation kill path the runner relies on for
  SIGTERM propagation (Phase 4).

## Commit 2 — `utils::podman` + `utils::buildah` wrappers

**Files:**

- `cbsd-rs/cbscore/src/utils/podman.rs` — port of `cbscore/utils/podman.py`.
  Free async functions: `podman_run(opts) -> Result<RunOutcome, PodmanError>`,
  `podman_stop(name, timeout) -> Result<(), PodmanError>`,
  `podman_pull(image_ref) -> Result<(), PodmanError>`,
  `podman_image_inspect(image_ref) -> Result<ImageMeta, PodmanError>`, etc.
- `cbsd-rs/cbscore/src/utils/buildah.rs` — port of `cbscore/utils/buildah.py`.
  Free async functions: `buildah_from`, `buildah_commit`, `buildah_unmount`,
  etc.
- `cbsd-rs/cbscore/src/utils/mod.rs` — `pub mod podman; pub mod buildah;`.

**Design constraints:**

- Errors return `PodmanError { retcode, stderr }` / `BuildahError` as defined in
  `cbscore-types::utils::{podman, buildah}::errors` (Phase 1 Commit 2).
- All subprocess calls go through `cbscore::utils::subprocess::async_run_cmd`.
- `utils::buildah` does **not** depend on `utils::podman` (independent wrappers;
  the runner — Phase 4 — orchestrates them).
- Cidfile semantics match Python: `podman_run` accepts a `cidfile: Utf8PathBuf`
  option that podman writes; the runner reads it to recover the container ID for
  stop / inspect calls. Per design 002 §Runner Subsystem lines 754, 1033–1049.

**Testable:**

- Command construction tests: assemble each function's command-line via a test
  helper that captures the `&[CmdArg]` slice, assert tokens match expected (e.g.
  `podman_run` with `--cidfile /tmp/cid` and `--rm` and the supplied mounts).
- Error parsing: feed a known-bad podman stderr string into the error decoder,
  assert the right `PodmanError` variant is produced.
- `buildah_unmount` of an unmounted container produces a recoverable error
  (matches Python `BuildahError` variant).

## Commit 3 — `images::skopeo` driver

**Files:**

- `cbsd-rs/cbscore/src/images/mod.rs` — new module file declaring
  `pub mod skopeo`. Phase 5 later extends with `pub mod sign; pub mod sync;`.
- `cbsd-rs/cbscore/src/images/skopeo.rs` —
  `skopeo_image_exists(src: &ImageRef, opts: &SkopeoOpts) -> Result<bool, ImageDescriptorError>`
  and
  `skopeo_copy(src: &ImageRef, dst: &ImageRef, opts: &SkopeoOpts) -> Result<(), ImageDescriptorError>`
  free async functions. Per design 002 §Image Sign & Sync §Skopeo driver lines
  1077–1083.
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod images;`.

**Design constraints:**

- Subprocess via `utils::subprocess::async_run_cmd`.
- Errors return `ImageDescriptorError` from `cbscore-types::images::errors`.
- TLS / auth flags from `SkopeoOpts` (a small struct in this same module) use
  **per-side fields** because the underlying `skopeo copy` CLI distinguishes
  source-side and destination-side credentials and TLS-verify behaviour:

  ```rust
  pub struct SkopeoOpts {
      pub src_tls_verify: bool,
      pub dst_tls_verify: bool,
      pub src_creds:      Option<RegistryCreds>,
      pub dst_creds:      Option<RegistryCreds>,
  }
  ```

  The `RegistryCreds` type comes from Phase 1's secrets module. The implementer
  should cross-check `cbscore/images/skopeo.py` at commit time and confirm the
  Python wrapper exposes the same per-side semantics — if Python collapses them
  into a single boolean, decide whether to widen the API or match Python
  literally.

**Commit-size rationale:** ~150 LOC is below the 400-line sweet spot named in
`cbsd-rs/CLAUDE.md` §Commit Granularity. Kept as a standalone commit because it
introduces the `images/` module tree that Phase 5 will extend with
`images::sign` and `images::sync`; bundling skopeo into a different commit
(e.g., with `utils::buildah`) would conflate unrelated subsystem namespaces and
complicate review.

**Testable:**

- Command construction: `skopeo_copy` produces
  `skopeo copy --src-tls-verify=<bool> --dest-tls-verify=<bool> <src> <dst>`
  with each per-side flag mapped from the matching `SkopeoOpts` field; when
  `src_creds` / `dst_creds` are `Some`, the matching `--src-creds` /
  `--dest-creds` flags appear.
- `skopeo_image_exists` distinguishes a 0-exit (exists) from a
  non-zero-exit-with-known-stderr (does not exist) — the latter must return
  `Ok(false)`, not `Err`.

## Commit 4 — `utils::git` wrapper

**Files:**

- `cbsd-rs/cbscore/src/utils/git.rs` — port of `cbscore/utils/git.py` (~401 LoC
  of subprocess wrappers in Python). Free async functions: `git_ls_remote`,
  `git_clone`, `git_fetch`, `git_describe`, `git_switch`,
  `git_branch_show_current`, `git_rev_parse`, `repo_root` (Rust name for
  Python's `get_git_repo_root`; seq-004 Commit 2's `resolve_root` depends on
  this exact name), etc. Match the Python signatures.
- `cbsd-rs/cbscore/src/utils/git/errors.rs` — `GitError` enum (NOT in
  `cbscore-types`; design 001 §Lift-out invariants names `utils::git` as a
  future `cbscommon-rs` candidate, so its error type stays module-internal). The
  v1 plan-review confirmed this placement is correct.
- `cbsd-rs/cbscore/src/utils/mod.rs` — `pub mod git;`.

**Design constraints:**

- Subprocess via `utils::subprocess::async_run_cmd`. Uses the `git` binary
  `>= 2.23` per design 002 Capability Mapping line 195 and §Open Questions
  resolution lines 1349–1359 (set by `git switch` and
  `git branch --show-current`).
- **Lift-out invariant per design 001:** `utils::git` depends only on
  `cbscore-types::errors` + `cbscore::utils::subprocess` + the `cbscore::logger`
  re-export. No deps on `cbscore::config`, `cbscore::runner`, etc. A future move
  to `cbscommon-rs` is then a mechanical edit, not a rewrite.
- Sensitive args (auth tokens, SSH passphrases) go through `CmdArg::Secure` per
  the secret-redaction contract.

**Testable:**

- Command construction tests for each git operation: assert the command-line is
  `["git", "<subcommand>", "<arg>", …]` with the expected flag set per call.
- `git_ls_remote` against a stub response returns the parsed
  `HashMap<String, String>` (ref → SHA).
- `git_describe` failure on a non-tag commit produces the right `GitError`
  variant.
- Auth-bearing `git_clone` (HTTPS-with-token) redacts the token in any traced
  command line.

## Commit 5 — `versions::utils` parse_version family

Closes the drift acknowledged in Phase 1 §Out of scope: the parse-family
functions live in `cbscore::versions::utils`, not
`cbscore-types::versions::utils`, because their implementations depend on the
`regex` crate (allowed in `cbscore`, deliberately absent from `cbscore-types`).

**Commit-size rationale:** ~230 LOC sits at the lower end of the 400–800 sweet
spot. Kept as a standalone commit because the parse family is semantically
distinct from the subprocess wrappers in the preceding commits (pure string
parsing vs subprocess IO), introduces a new top-level `versions::` module under
`cbscore`, and lands behind the clean tag "closes the Phase 1 §Out of scope
drift" in `git log` — a review boundary that benefits from being explicit rather
than buried at the end of a larger commit. Bundling with Commit 4 (`utils::git`,
~500 LOC) would total ~730 LOC but conflate two unrelated subsystems.

**Files:**

- `cbsd-rs/cbscore/src/versions/mod.rs` — new module file declaring
  `pub mod utils`.
- `cbsd-rs/cbscore/src/versions/utils.rs` — `ParsedVersion` struct (prefix /
  major / minor / patch / suffix fields), `parse_version`, `get_version_type`,
  `get_major_version`, `get_minor_version`, `normalize_version`,
  `parse_component_refs`. All six function signatures match design 002 lines
  672–696 exactly.
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod versions;`.

**Design constraints:**

- Function signatures match design 002 exactly:
  - `parse_version(s: &str) -> Result<ParsedVersion, MalformedVersion>`
  - `get_version_type(name: &str) -> Result<VersionType, VersionError>`
  - `get_major_version(v: &str) -> Result<String, MalformedVersion>`
  - `get_minor_version(v: &str) -> Result<Option<String>, MalformedVersion>`
  - `normalize_version(v: &str) -> Result<String, MalformedVersion>`
  - `parse_component_refs(components: &[String]) -> Result<HashMap<String, String>, VersionError>`
- `get_version_type` is the sixth function on this list. Design 001 §Downstream
  Consumers line 65 names it as one of the `cbscore-types` symbols imported by
  external `cbc`; since its implementation calls `parse_version` (and therefore
  requires `regex`), it travels with the parse family into
  `cbscore::versions::utils` rather than staying in `cbscore-types`. This is
  part of the same Phase 1 §Out of scope drift; the design-002 follow-up edit
  covers all six functions uniformly.
- Regex pattern is the Python verbatim per design 002 lines 698–700.
  `parse_component_refs` matches `^([\w_-]+)@([\d\w_./-]+)$` per design 002
  line 700.
- `ParsedVersion` lives in `cbscore::versions::utils`, alongside
  `parse_version`. Not in `cbscore-types` — the type is the parser's result,
  used by `cbscore`-internal callers (patch walker in Phase 5, title generator)
  and by the `cbsbuild` `versions` subcommand (Phase 6, which depends on
  `cbscore`). No external Python-consumer dependency forces `cbscore-types`
  placement.
- Errors: `MalformedVersion` and `VersionError` variants are from
  `cbscore-types::versions::errors` (Phase 1 Commit 2).

**Testable:**

- Regex matching: `parse_version("ces-v19.2.3")` →
  `Ok(ParsedVersion { prefix: "ces", major: 19, minor: 2, patch: Some(3), suffix: None })`;
  `parse_version("ces-v19.2.3-dev.1")` ditto with suffix; `parse_version("99")`
  → `Err(MalformedVersion)`; `parse_version("0193e1a8-7c2e-7000-…")` →
  `Err(MalformedVersion)` (UUIDv7 reject; see design 005).
- `get_version_type("ces-v19.2.3-dev.1")` → `Ok(VersionType::Dev)`;
  `get_version_type("ces-v19.2.3")` → `Ok(VersionType::Release)` (no suffix);
  `get_version_type("ces-v19.2.3-test.1")` → `Ok(VersionType::Test)`.
- `get_major_version("ces-v19.2.3-dev.1")` → `"19"`.
- `get_minor_version("ces-v19.2.3-dev.1")` → `Some("19.2.3")`;
  `get_minor_version("ces-v19.2")` → `Ok(None)` (patch missing).
- `normalize_version` canonicalises a parsed shape back to
  `<prefix>-v<major>.<minor>[.<patch>][-<suffix>]`.
- `parse_component_refs(&["ceph@master", "el9@v1.0"])` →
  `{"ceph" → "master", "el9" → "v1.0"}`.

## End-of-phase acceptance

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace`, `cargo fmt --all --check` all pass.
- `cbscore` library exposes `utils::subprocess`, `utils::podman`,
  `utils::buildah`, `utils::git`, `images::skopeo`, `versions::utils`.
- The Phase 1 §Out of scope drift (parse_version family in `cbscore` not
  `cbscore-types`) is closed by Commit 5.
- **Lift-out invariants (design 001):** `utils::subprocess` and `utils::git`
  depend only on `cbscore-types::errors` + `cbscore::logger`
  - (for `utils::git`) `cbscore::utils::subprocess`. Verified by a module-level
    import check, NOT by `cargo tree`. `cargo tree` reports crate-level
    transitive deps and would produce false positives from Phase 3 onward (once
    `cbscore` itself depends on `aws-sdk-s3`, `vaultrs`, etc. for other
    modules). The enforcing check is:

  ```bash
  grep -nE 'use crate::(config|runner|builder|releases|images)' \
      cbsd-rs/cbscore/src/utils/subprocess.rs \
      cbsd-rs/cbscore/src/utils/git.rs \
      cbsd-rs/cbscore/src/utils/git/errors.rs
  ```

  Expected: zero matches. Any match means the lift-out invariant is broken and a
  future move to `cbscommon-rs` would require a non-trivial edit. The grep
  targets the precise constraint the invariant expresses (no cross-module `use`
  statements into the named subtrees) and is cheap to run as a pre-commit or CI
  check.
