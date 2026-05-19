# Phase 5 — M1.4: Builder pipeline + releases + image sign/sync

## Status

**Approved — finalized and ready for implementation.** Last audited at the v23
corpus pass (`reviews/002-20260513T1356-plan-cbscore-rust-port-design-v23.md`,
verdict commit `cd22cb8`); zero findings across CRITICAL / MAJOR / MINOR /
SUGGESTION / OPEN QUESTION on the seq-002 phase plans. Three pre-implementation
audit passes (closed in `6cc553f`, `2d6062c`, `1a88722`) plus follow-up MN
closures (`72852a8`) cleared 25 substantive findings across the design and plan
corpus. See `README.md` for the dependency graph and the M0 / M1 / M2 milestone
cuts.

## Progress

| #   | Commit                                                                 | ~LOC | Status  |
| --- | ---------------------------------------------------------------------- | ---- | ------- |
| 1   | `cbscore: add builder::prepare stage (sources + repo resolution)`      | ~550 | Done    |
| 2   | `cbscore: add core::component module (load_components IO)`             | ~200 | Done    |
| 3   | `cbscore: add builder::rpmbuild stage (per-component RPM builds)`      | ~500 | Done    |
| 4   | `cbscore: add containers module + images::sync (container production)` | ~550 | Pending |
| 5   | `cbscore: add builder::signing + images::signing (GPG + transit)`      | ~500 | Pending |
| 6   | `cbscore: add builder::upload + releases module (S3 publish)`          | ~600 | Pending |
| 7   | `cbscore: add builder::run_build orchestrator + report assembly`       | ~400 | Pending |

**Estimate:** ~3300 LOC, 7 commits.

## Goal

Land the in-container build pipeline on top of Phase 2 (subprocess + shell-tool
wrappers), Phase 3 (S3 + Vault + secrets + config IO), and the wire-format types
from Phase 1. After Phase 5, `cbscore` exposes `builder::run_build` as the
public orchestrator that the in-container `cbsbuild runner build` CLI (Phase 6)
calls, plus the `releases::{s3, utils}`, `images::{signing, sync}` (skopeo
already landed in Phase 2), and `containers::{build, component, repos}` modules
that the pipeline uses internally.

End state: `cargo build --workspace` and `cargo test --workspace` pass;
`cbscore::builder::run_build(desc, config, secrets, opts)` returns
`Result<BuildArtifactReport, BuilderError>` after executing the pipeline
(prepare → rpmbuild → containers::build → signing → upload); integration tests
gated on real `rpmbuild` / `gpg` / S3 / podman endpoints pass when reachable
(otherwise `#[ignore]`); the `cbsbuild` binary still prints its placeholder
string (CLI tree lands in Phase 6).

## Depends on

- Phase 1 — `cbscore-types` provides `BuilderError`, `ReleaseError`,
  `ContainerError`, `ImageDescriptorError`, `BuildArtifactReport`,
  `VersionDescriptor`, all release / container / image descriptor types, and the
  `logger` module.
- Phase 2 — `utils::subprocess::async_run_cmd` for all subprocess drives;
  `utils::podman` + `utils::buildah` for container assembly; `utils::git` for
  source fetches; `images::skopeo` for image copy.
- Phase 3 — `utils::s3` for RPM + release-descriptor uploads; `utils::vault` for
  transit signing; `config::Config` for `paths.scratch` / `signing.gpg` /
  `signing.transit` / `storage.s3.bucket` settings; `secrets::SecretsMgr` for
  resolved git credentials (consumed by `prepare::run` for private-repo source
  fetches), GPG passphrases (signing), and registry creds (upload).
- Phase 4 — Phase 5 does **not** call into the runner. The runner (host-side)
  spawns the container that invokes the in-container `cbsbuild` (Phase 6 CLI)
  which then calls `builder::run_build` (this phase). Phase 4 is a peer, not a
  dependency.
- Design 002 — §Build Pipeline (lines 866–938), §Image Sign & Sync (lines
  1073–1104), §Releases & S3 (lines 1106–1170).
- Design 001 — §Workspace Layout lines 133–166 (the `containers/`, `builder/`,
  `images/`, `releases/` subtrees of `cbscore/src/`).

## Out of scope

- The `cbsbuild` CLI wiring (`cbsbuild build`, `cbsbuild runner build`,
  `cbsbuild advanced …`) — Phase 6 owns the clap tree that invokes
  `builder::run_build`.
- The host-side runner — Phase 4 owns the podman lifecycle that spawns the
  container Phase 5's pipeline executes inside.
- Release-listing CLI (`cbsbuild versions list` reads releases from S3) — Phase
  6 owns the consumer **and the underlying S3 read operations**. Phase 5's
  `releases::s3` lands only the write path (`upload_release`); the read
  operations (`check_release_exists`, `check_released_components` per design 002
  §S3 operations lines 1158–1160) are added to `releases::s3` by Phase 6
  alongside the CLI surface that consumes them, since no Phase 5 caller needs
  them.
- Lift-out invariants — none of Phase 5's modules are lift-out candidates
  (design 001 §Lift-out invariants names only `utils::subprocess` and
  `utils::git`).
- `cbscommon-rs` extraction — out of scope across all M1 phases.

## Commit 1 — `builder::prepare` stage (sources + repo resolution)

Port `cbscore/builder/prepare.py` to Rust. First stage of the four- stage
pipeline: validate the descriptor, fetch component sources, resolve build repos,
write per-component `BuildComponentInfo` records.

**Files:**

- `cbsd-rs/cbscore/src/builder/mod.rs` — module entry. Declares
  `pub mod prepare; pub mod utils;` (later commits in this phase add
  `pub mod rpmbuild; pub mod signing; pub mod upload; pub mod report;`). Also
  hosts the public `BuildOptions` struct:
  ```rust
  pub struct BuildOptions {
      pub skip_build: bool,  // per design 002 line 930
      pub force:      bool,  // per design 002 line 931
  }
  ```
  Placed in `cbscore::builder` (the library crate), not `cbscore-types`:
  `BuildOptions` is consumed only inside the builder pipeline and by Phase 6's
  CLI; no external `cbscore-types` consumer needs it.
- `cbsd-rs/cbscore/src/builder/prepare.rs` — port of
  `cbscore/builder/prepare.py`. Public surface:
  - `pub async fn run(desc: &VersionDescriptor, config: &Config, secrets: &SecretsMgr, opts: &BuildOptions) -> Result<PrepareReport, BuilderError>`
    — the stage entry point. Returns a `PrepareReport` carrying the
    per-component `BuildComponentInfo` records that downstream stages consume.
    `PrepareReport` is declared inline in this file. The `secrets: &SecretsMgr`
    arg threads through to the underlying `utils::git` calls so that source
    fetches against private SSH/HTTPS repos can resolve the matching `GitCreds`
    entry by host (matches design 002's §Build Pipeline orchestrator sketch,
    which threads `secrets` into every stage uniformly). Even on M1 deployments
    with public-only repos, the param is present so the orchestrator's
    `prepare::run(desc, config, secrets, opts).await?` line matches the other
    stages 1:1.
  - Private helpers for source fetch (via `utils::git::git_clone` +
    `git_fetch`), patch-walker (per design 002 §Effects of UUIDv7 VERSIONs
    §Patches per design 005 — the patch walker that selects
    `components/<comp>/patches/<major>/`, `<major-minor>/`, `<full-version>/`
    subdirectories based on the descriptor's VERSION), repo URI resolution.
- `cbsd-rs/cbscore/src/builder/utils.rs` — shared builder helpers (scratch-dir
  setup, per-component path derivation, common error- wrapping shims).
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod builder;`.

**Design constraints:**

- Stage signature is a free `async fn` per design 002 line 911–916 ("each stage
  is a plain async function… `Result<StageReport, BuilderError>`"). No struct
  state; cancellation = future drop.
- Sources are fetched into `config.paths.scratch/<component>/` per design 002
  §Build Pipeline diagram. Existing scratch contents are reused when
  `opts.force` is false; cleared when true. **Clear-then-fetch ordering**
  (pinned): when `opts.force = true`, `prepare::run` calls
  `tokio::fs::remove_dir_all(scratch/<component>)` first, then
  `tokio::fs::create_dir_all(scratch/<component>)`, then proceeds with source
  fetch. If SIGTERM lands between `remove_dir_all` and `create_dir_all` (or
  between `create_dir_all` and the first git fetch), the scratch dir is left
  absent — the next build with the same VERSION rebuilds from scratch. This is
  the accepted recovery semantic for `force = true`: the operator explicitly
  asked for a fresh build, so a partial-clear that leaves nothing behind is
  correct behaviour.
- **Concurrent invocation: invoker responsibility.** Each `cbsbuild build`
  invocation derives `config.paths.scratch` from its `--config` (or
  default-resolved config). cbsbuild does **not** acquire a lock on the scratch
  path; the binary assumes the invoker (operator or cbsd-worker) ensures no two
  concurrent invocations share the same scratch path for the same component.
  cbsd-worker enforces this by handling one build at a time (Phase 7 spec).
  Operators running `cbsbuild build` directly are responsible for not
  parallelising two builds against the same `config.paths.scratch`. No internal
  mutual-exclusion mechanism ships with cbscore-rs.
- The patch walker handles both regex-parseable VERSIONs and malformed inputs
  per design 005 (the Rust port adds an explicit guard catching
  `Err(MalformedVersion)` from `get_major_version` / `get_minor_version`,
  returning "no major/minor/patch known" and skipping the subdirectory — Phase 2
  Commit 5 spelled this out). UUIDv7-style VERSIONs from design 005 (post-M1)
  will exercise this guard naturally; M1 exercises it via the existing
  regex-mismatch error path.

**Testable:**

- Unit tests on the patch walker against fixture directory trees built
  programmatically at test runtime via `tempfile::TempDir` (NOT stored on-disk
  under `tests/fixtures/`). The test constructs the `components/ceph/patches/`
  tree with `19/`, `19.2/`, `19.2.3/`, and top-level patch files via
  `std::fs::create_dir_all` + `std::fs::write` (each `.patch` is a one-line stub
  since the walker only inspects the tree shape, not patch content). The tempdir
  is cleaned up automatically when the test exits. Programmatic fixture
  authoring is preferred here because (a) the test's intent — "the walker
  selects the right subset for each VERSION shape" — is far clearer when the
  tree is built inline in the test body than when a reader has to
  cross-reference an out-of-band fixture directory; (b) the walker's behaviour
  depends on directory names exact-matching parsed VERSION components, and
  exercising this with hand-built trees keeps the test self-contained; (c)
  `tempfile::TempDir` is already a dev-dependency for the round-trip tests in
  Phase 1. Assert the right subset is selected for `ces-v19.2.3-dev.1`,
  `ces-v19.2`, and `0193e1a8-7c2e-7000-…` (UUIDv7 — only top-level applies, per
  design 005).
- Integration test (`#[ignore]`-able): full prepare run against a fixture `ceph`
  component with a real git clone, assert the resulting `BuildComponentInfo`
  carries the expected SHA + ref. Opted in via `CBSCORE_TEST_GIT_REMOTE=<url>`
  (defaults to skipping when unset); `#[ignore]`-skipped with a "set
  CBSCORE_TEST_GIT_REMOTE to enable" message.

## Commit 2 — `core::component` module (`load_components` IO)

Port `cbscore/core/component.py`'s on-disk component loader to Rust. The
function walks a directory tree of `cbs.component.yaml` files and returns the
typed component set that downstream consumers (`builder::prepare`'s caller,
`cbsbuild build` in Phase 6, the M2 worker in Phase 7) hand to
`builder::run_build`. Owned by this phase because the builder pipeline is the
primary consumer; Phase 6 imports it through the public surface.

**Files:**

- `cbsd-rs/cbscore/src/core/mod.rs` — new module file declaring
  `pub mod component;`. Phase 5 does not extend `core/` with other submodules;
  the file exists so the public path `cbscore::core::component` resolves.
- `cbsd-rs/cbscore/src/core/component.rs` — port of `cbscore/core/component.py`.
  Public surface:
  - `pub async fn load_components(root: &Utf8Path) -> Result<HashMap<String, CoreComponent>, ComponentError>`
    — walks `root` recursively, finds every `cbs.component.yaml` file, parses
    each via `serde_saphyr` + `VersionedCoreComponent::into_latest()` (Phase 1
    Commit 5), keys the result by `CoreComponent.name`. Async because file reads
    go via `tokio::fs`. The `CoreComponent` and `CoreComponentLoc` types come
    from `cbscore-types::core::component` (Phase 1 Commit 4); this file only
    adds IO around them.
  - `ComponentError` is **declared in Phase 1 Commit 2** at
    `cbsd-rs/cbscore-types/src/core/component/errors.rs` with variants
    `Walk { source: io::Error }`, `Yaml { path: Utf8PathBuf, message: String }`,
    `MissingSchemaVersion { path: Utf8PathBuf }`,
    `UnknownSchemaVersion { path: Utf8PathBuf, found: u64, max_supported: u64 }`,
    `DuplicateComponentName { name: String, first: Utf8PathBuf, second: Utf8PathBuf }`
    (mirroring `ConfigError`'s pattern). Phase 5 Commit 2 imports the type from
    `cbscore-types`; this commit does not redefine it. The loader captures
    `serde_saphyr::Error` from the YAML parse, converts to
    `ComponentError::Yaml { path, message: e.to_string() }` at the call site
    (the parser dep stays in `cbscore`, keeping `cbscore-types` free of
    format-crate `[dependencies]` per design 001).
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod core;`.

**Design constraints:**

- Walk uses `walkdir` (chosen for cycle protection out of the box). Configure
  `WalkDir::new(root).follow_links(true)` so symlinks resolve to their targets,
  matching Python `glob.glob` behaviour. walkdir detects symlink cycles
  internally and surfaces them as `walkdir::Error` with
  `loop_ancestor: Some(...)` populated; the loader **warns-and-continues** on
  cycle detection rather than aborting the walk:

  ```rust
  for entry in WalkDir::new(root).follow_links(true) {
      let entry = match entry {
          Ok(e) => e,
          Err(err) if err.loop_ancestor().is_some() => {
              tracing::warn!(
                  target: TARGET_CORE_COMPONENT,
                  path = %err.path().unwrap_or(root).display(),
                  loop_ancestor = %err.loop_ancestor().unwrap().display(),
                  "skipping symlink cycle during component walk",
              );
              continue;
          }
          Err(err) => return Err(ComponentError::Walk { source: err.into() }),
      };
      // … process entry …
  }
  ```

  This guarantees that an operator deployment with a stray symlink cycle in
  `components/` does not block component loading — the cycle is logged once with
  a structured `path` + `loop_ancestor` field so operators can find and fix it,
  and the rest of the tree loads normally. Other walk errors (permission denied,
  IO failure) still propagate via `ComponentError::Walk`.

- **Component-name comparison is case-sensitive.** Two component files declaring
  `name: ceph` and `name: Ceph` are **distinct** components, not duplicates —
  the `HashMap<String, CoreComponent>` keys them apart, and the
  `DuplicateComponentName` detection only triggers on exact string equality
  (Rust `String` equality is byte-equality, not Unicode-normalised). This
  matches Python's `dict` keying semantics. Operators with case-folding
  filesystems (HFS+ default, NTFS) who rely on case to disambiguate component
  names should be aware that the on-disk filename layout is independent of the
  in-descriptor `name:` field — two files at `Ceph/cbs.component.yaml` and
  `ceph/cbs.component.yaml` may collide at the filesystem layer on case-folding
  mounts, but cbscore-rs only sees the `name:` field inside each file's YAML, so
  the case-sensitivity guarantee is load-bearing.
- Parse failures on individual files do **not** cascade: log the per-file error
  at `tracing::warn!` with structured fields:
  ```rust
  tracing::warn!(
      target: TARGET_CORE_COMPONENT,
      path = %path,
      "component file parse failed: {}", err,
  );
  ```
  `path` is a structured field (not interpolated into the message string) so log
  parsers / filters can extract it. The function returns the **last** per-file
  error variant encountered (`ComponentError::Yaml`, `MissingSchemaVersion`, or
  `UnknownSchemaVersion`) only if **no** components were successfully loaded;
  mixed partial success is reported as `Ok(<the loaded subset>)` with the
  per-file errors as warnings on the log. This matches Python's
  `cbscore/core/component.py` which `try / except continue`s on per-file parse
  failure.
- Duplicate `name:` field across two component files **is** an error
  (`DuplicateComponentName`) — the in-memory map can only carry one value per
  key. Python raises at the same point.
- `load_components` is an IO function, lives in `cbscore` (not `cbscore-types`)
  per design 001 §Lift-out invariants. The types it returns are pure (Phase 1
  Commit 4); only the walker is here.
- Phase 5 Commit 1 (`builder::prepare`) and Phase 6 (`cbsbuild build`) consume
  this function. Phase 4 (runner) does **not** call it directly — the runner
  takes the loaded `HashMap<String, CoreComponent>` as an input parameter,
  marshalled in by whichever CLI handler invokes the runner.

**Testable:**

- Unit test on duplicate-name detection: two `cbs.component.yaml` fixtures with
  the same `name:` field in different subdirs →
  `Err(ComponentError::DuplicateComponentName)`.
- Unit test on per-file parse failure: one bad fixture + two good →
  `Ok(map_with_two_entries)` and the bad file's error is in the captured tracing
  log at WARN level.
- Unit test on the empty-tree case: a tree containing no `cbs.component.yaml`
  files → `Ok(HashMap::new())`.
- Integration test (`#[ignore]`-able): point at the real `components/` directory
  in the cbs.git checkout, assert the expected component names appear in the
  result and each carries the expected `loc.url` and `loc.ref`. Opted in via
  `CBSCORE_TEST_COMPONENTS_DIR=<path>` (typically the operator's local
  `components/` tree); `#[ignore]`-skipped with a "set
  CBSCORE_TEST_COMPONENTS_DIR to enable" message when unset.

**Commit-size rationale:** ~200 LOC sits at the lower end of the 400-line floor
named in `cbsd-rs/CLAUDE.md` §Commit Granularity, but justified because the
loader is independently testable, has a clean single-file scope
(`cbscore/src/core/component.rs`), and is consumed by both Phase 5 (the builder
pipeline) and Phase 6 (the CLI). Folding it into Commit 1 (prepare) would bundle
two semantically distinct concerns; folding into Commit 3 (rpmbuild) would have
it land after its first consumer.

## Commit 3 — `builder::rpmbuild` stage (per-component RPM builds)

Port `cbscore/builder/rpmbuild.py`. Second stage of the pipeline: spawns
`rpmbuild` per component, collects RPMs into the artifact dir, writes
`ComponentBuild` reports.

**Files:**

- `cbsd-rs/cbscore/src/builder/rpmbuild.rs` — port of
  `cbscore/builder/rpmbuild.py`. Public surface:
  - `pub async fn run(desc: &VersionDescriptor, config: &Config, prep: &PrepareReport, opts: &BuildOptions) -> Result<RpmbuildReport, BuilderError>`
  - `pub struct RpmArtifact { pub path: Utf8PathBuf, pub component: String, pub arch: ArchType, pub is_srpm: bool }`
    — declared here alongside its producer. `ArchType` comes from
    `cbscore-types::releases::desc` (Phase 1 Commit 4). Per V11-S1 from
    plan-review v11, `RpmbuildReport` carries `Vec<RpmArtifact>` so downstream
    stages (signing in Commit 5, upload in Commit 6) consume a self-describing
    artifact list rather than bare paths plus post-hoc metadata reconstruction.
  - `pub struct RpmbuildReport { pub rpms: Vec<RpmArtifact>, pub component_builds: Vec<ComponentBuild> }`
    — the stage's output. `ComponentBuild` is a per-component build summary
    (start/end time, exit code, log path) for the eventual `BuildArtifactReport`
    assembly in Commit 7.
- `cbsd-rs/cbscore/src/builder/mod.rs` — add `pub mod rpmbuild;`.

**Design constraints:**

- One `rpmbuild` invocation per component, in dependency order from the
  descriptor.
- `rpmbuild -bs` produces SRPMs; subsequent `rpmbuild --rebuild` (or the
  equivalent invocation per the Python source) produces binary RPMs.
- Per-component stdout / stderr streamed via the `async_run_cmd::out_cb`
  mechanism — the runner (Phase 4) is reading these on the host side via
  podman's stdout pipe.
- `opts.skip_build` short-circuits this stage with a no-op `RpmbuildReport` that
  lists no produced RPMs but signals success for downstream stages.
- Cancellation: dropping the future during a component's `rpmbuild` invocation
  kills the in-progress build via Phase 2 Commit 1's RAII drop guard on
  `async_run_cmd`.

**Testable:**

- Command construction tests: `rpmbuild` invocation per component produces the
  right `-bs` / `--rebuild` arg sequence with the right `--define _topdir`
  pointing at the scratch path.
- Integration test (`#[ignore]`-able): run rpmbuild on a tiny test SPEC file
  (e.g., a hello-world RPM), assert the resulting `.rpm` artifact path is in the
  report. Opted in via `CBSCORE_TEST_RPMBUILD=1` (the host must have a working
  `rpmbuild` binary on PATH); `#[ignore]`-skipped with a "set
  CBSCORE_TEST_RPMBUILD=1 to enable" message when unset.

## Commit 5 — `builder::signing` + `images::signing` (GPG + transit)

Two related but distinct signing operations: per-RPM GPG signing
(builder::signing) and per-image manifest signing (images::signing). Both share
GPG + Vault transit primitives.

**Files:**

- `cbsd-rs/cbscore/src/builder/signing.rs` — port of
  `cbscore/builder/signing.py`. RPM signing via `rpm --addsign`. Public surface:
  - `pub async fn run(desc: &VersionDescriptor, config: &Config, secrets: &SecretsMgr, rpms: &RpmbuildReport) -> Result<SigningReport, BuilderError>`
- `cbsd-rs/cbscore/src/builder/mod.rs` — add `pub mod signing;`.
- `cbsd-rs/cbscore/src/images/signing.rs` — port of `cbscore/images/signing.py`.
  Image manifest signing. Public surface:
  - `pub async fn sign_manifest(digest: &str, config: &SigningConfig, secrets: &SecretsMgr) -> Result<Vec<u8>, ImageDescriptorError>`
- `cbsd-rs/cbscore/src/images/mod.rs` — add `pub mod signing;` alongside the
  existing `pub mod skopeo;` (Phase 2 Commit 3).
- `cbsd-rs/cbscore/src/builder/signing/gpg.rs` — `gpg2` subprocess invocation,
  GPG home dir setup, `--pinentry-mode loopback` for passphrase passing. Pinned
  under `builder/signing/` (not the shared `utils/gpg.rs` location) because GPG
  is a builder-pipeline concern; `images::signing` (this same commit) re-imports
  the helpers from `cbscore::builder::signing::gpg`. This keeps the design 001
  §Lift-out invariants safe — `utils/` stays clean of cbscore-internal
  dependencies (GPG handling pulls in `SecretsMgr` and the resolved-keys store),
  so the future `cbscommon-rs` lift-out path for `utils/` is unaffected.

**Design constraints:**

- Two signing backends per design 002 §Image Sign & Sync lines 1086–1096:
  - **GPG detached signatures** via `gpg2 --detach-sign`. The runner (Phase 4)
    mounts a tempdir at GPG_HOME with the imported key set from the resolved
    secrets; `gpg2` is invoked with `GNUPGHOME=<that path>` and
    `--pinentry-mode loopback`.
  - **Vault transit signing** via `utils::vault::transit_sign` — declared in
    Phase 3 Commit 2 (parallel HTTP API to `kv_read`, same per-call auth
    security posture). Phase 5 Commit 5 consumes it; no Phase 5 changes to
    `utils/vault.rs`.
- Signing is **optional**: when `config.signing` is `None`, both
  `builder::signing::run` and `images::signing::sign_manifest` become no-ops.
  Per design 002 line 1094–1096 ("recent Python commit d2e8a91 cbscore: make
  signing optional"). Concretely:
  - `builder::signing::run` returns `Ok(SigningReport::empty())` without
    invoking `rpm --addsign` or the GPG subprocess.
  - `images::signing::sign_manifest` is simply not called by `images::sync` when
    `config.signing` is `None` (the sync orchestrator skips the sign step).
- `signing::run`'s signature carries `secrets: &SecretsMgr` per design 002
  §Build Pipeline (the orchestrator sketch shows
  `signing::run(desc, config, secrets, &rpms).await?`). The GPG passphrase and
  Vault transit key name come from `SecretsMgr`; without it in scope, the stage
  cannot drive its subprocess. Aligned with design 002 as of the audit-pass-3
  closure (signature pinned in design 002 §Build Pipeline orchestrator sketch).
- Per-RPM signing in `builder::signing` invokes `rpm --addsign` which itself
  shells out to `gpg2` — the cbscore wrapper supplies the passphrase via Phase 2
  Commit 1's `SecureArg::PasswordArg` (per CLAUDE.md §Correctness Invariants
  item 5).

**Testable:**

- Command construction: `rpm --addsign` per-RPM with the right passphrase-arg
  redaction (assert traced lines emit `****`, not the raw passphrase).
- Integration test (`#[ignore]`-able): GPG-sign a tiny fixture RPM against a
  test keyring, verify the signature via `rpm --checksig`. Opted in via
  `CBSCORE_TEST_GPG_KEYRING=<path>` (path to a test GPG keyring with
  `--pinentry-mode loopback` compatible setup); `#[ignore]`-skipped with a "set
  CBSCORE_TEST_GPG_KEYRING to enable" message when unset.
- Vault transit signing: round-trip a known manifest digest against a
  `vault server -dev` instance with a transit key configured. Reuses the
  `CBSCORE_TEST_VAULT_ADDR` / `CBSCORE_TEST_VAULT_TOKEN` env-var contract from
  Phase 3 Commit 2; `#[ignore]`-skipped with the same message pattern when
  unset.
- `images::sync` sign-before-push order test: with `config.signing.is_some()`,
  stub `sign_manifest` and `skopeo_copy`, drive `sync_image`, capture the call
  order, assert `sign_manifest` fires before `skopeo_copy`. This is the test
  that was deferred from Commit 4 §Testable per the sign-before-push invariant
  taking effect when this commit lands.

## Commit 6 — `builder::upload` + `releases` module (S3 publish)

Final builder stage + the supporting releases module. Uploads signed RPMs to S3,
pushes the built container image to the registry, writes the release descriptor.

**Files:**

- `cbsd-rs/cbscore/src/builder/upload.rs` — port of `cbscore/builder/upload.py`.
  Fourth stage of the pipeline. Public surface:
  - `pub async fn run(desc: &VersionDescriptor, config: &Config, secrets: &SecretsMgr, signed: &SigningReport, image: &ContainerImageReport) -> Result<UploadReport, BuilderError>`
    — takes `&SecretsMgr` for the registry creds consumed by image push (closes
    V11-B3 from plan-review v11). Takes `&ContainerImageReport` (produced by
    `containers::build_image` in Commit 3) so the locally-built image's tag /
    digest is known to the push step without forward-dependency on a later
    commit (closes V11-M1).
- `cbsd-rs/cbscore/src/builder/mod.rs` — add `pub mod upload;`.
- `cbsd-rs/cbscore/src/releases/mod.rs` — new module. Declares
  `pub mod s3; pub mod utils;`.
- `cbsd-rs/cbscore/src/releases/s3.rs` — orchestrator that calls Phase 3's
  `utils::s3` primitives to upload RPMs and the release descriptor. Public
  surface:
  - `pub async fn upload_release(desc: &ReleaseDesc, rpms: &[RpmArtifact], config: &Config) -> Result<(), ReleaseError>`
- `cbsd-rs/cbscore/src/releases/utils.rs` — small helpers (S3 key layout,
  descriptor → manifest projection).
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod releases;`.

**Design constraints:**

- Upload is **gated on `config.storage`** per design 002 §Build Pipeline
  diagram. When unset, upload is a no-op returning an empty `UploadReport`.
- S3 key layout matches Python: `s3://<bucket>/<loc>/<version>/<rpm-basename>`
  for RPMs, `s3://<bucket>/<loc>/<version>/release.json` for the release
  descriptor (per design 002 §S3 operations).
- Image push goes through `utils::buildah::buildah_push` (or the equivalent
  skopeo-copy from Phase 2 Commit 3) per design 002 §Image Sign & Sync; the
  in-container image push uses the registry creds resolved from `SecretsMgr`.
- The release descriptor is constructed from the `RpmbuildReport`
  - `SigningReport` + the version descriptor and written to S3 via
    `release_desc_upload` (Phase 3 Commit 1). The descriptor type comes from
    `cbscore-types::releases::desc::ReleaseDesc` (Phase 1 Commit 4).

**Testable:**

- Command construction tests: S3 key layout for a sample release matches the
  expected `<bucket>/<loc>/<version>/…` form.
- Integration test (`#[ignore]`-able) against local MinIO: upload a release with
  two RPMs + a manifest, verify all three objects exist with the right keys.
  Reuses the `AWS_ENDPOINT_URL` / `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`
  env-var contract from Phase 3 Commit 1; additionally requires
  `CBSCORE_TEST_S3_BUCKET=<bucket>` to name the test bucket; `#[ignore]`-skipped
  with a "set AWS_ENDPOINT_URL + CBSCORE_TEST_S3_BUCKET to enable" message when
  either is unset.
- Negative test: upload with `config.storage = None` returns an empty
  `UploadReport` and makes zero S3 calls.

## Commit 7 — `builder::run_build` orchestrator + report assembly

The orchestrator that chains the four stages, plus the `BuildArtifactReport`
assembly that ties their outputs together.

**Files:**

- `cbsd-rs/cbscore/src/builder/mod.rs` — extend with the public `run_build`
  async function:
  - `pub async fn run_build(desc: &VersionDescriptor, config: &Config, secrets: &SecretsMgr, opts: &BuildOptions) -> Result<BuildArtifactReport, BuilderError>`
- `cbsd-rs/cbscore/src/builder/report.rs` — `BuildArtifactReport` assembly. The
  Phase 1 Commit 4 type lives in `cbscore-types::builder::report` (carries
  `schema_version: 1` per Phase 1 Commit 5); this file holds the constructor
  that gathers per-stage reports into a single artifact summary.
- `cbsd-rs/cbscore/src/builder/mod.rs` — `pub mod report;`.

**Design constraints:**

- Stages chain in strict order: prepare → rpmbuild → containers::build → signing
  → upload (per design 002 §Build Pipeline diagram line 873–908, augmented with
  the container-build step that produces the image push upload consumes). A
  failure in any stage short-circuits the chain and returns `Err(BuilderError)`
  immediately. The `ContainerImageReport` from `containers::build::build_image`
  threads forward into `upload::run`'s `image: &ContainerImageReport` argument
  so the push step has the image tag / digest without a forward-dependency on
  later commits.
- **Scratch dir left in place on stage failure.** When any stage returns
  `Err(BuilderError)`, `run_build` short-circuits and returns the error without
  clearing `config.paths.scratch/<component>/`. The scratch contents (downloaded
  sources, build artefacts, partial RPMs, stage logs) remain on disk for the
  operator to inspect and debug. To re-run with a clean slate, the operator
  passes `opts.force=true`, which Phase 5 Commit 1 (`prepare`) clears via
  `remove_dir_all` + `create_dir_all`. This matches Python `cbsbuild`'s
  behaviour: build failures leave the scratch state for inspection rather than
  auto-cleaning. Each stage's RAII guards (Phase 2 Commit 1 subprocess kill;
  Phase 5 Commit 4 `BuildahWorkingContainer`) handle their own child-process /
  container cleanup; the scratch directory itself is operator-owned state.
- `opts.skip_build` (per design 002 line 930) propagates from `BuildOptions`
  through each stage's `run` call; stages decide how to interpret it (rpmbuild
  becomes a no-op; signing + upload see an empty RpmbuildReport and become
  no-ops too).
- `opts.force` (per design 002 line 931) tells `prepare` to clear the scratch
  dir before fetching sources.
- The orchestrator is cancellable via future drop. Each stage's RAII guards
  (Phase 2 Commit 1) handle their own child-process cleanup; the orchestrator
  does no additional cleanup beyond what the stages own.

**Commit-size rationale:** ~400 LOC sits at the lower end of the 400–800 sweet
spot. Kept as a standalone commit because the orchestrator is the integration
boundary between the four stage modules (prepare in Commit 1, rpmbuild in Commit
3, signing in Commit 5, upload in Commit 6) plus the supporting containers
module in Commit 4 — landing it here gives a clean
"now-the-pipeline-runs-end-to-end" review boundary. Bundling with Commit 6
(upload) would mix the final-stage implementation with the
chain-everything-together orchestration, two different review concerns.

**Testable:**

- Stub-stage integration test: record the actual call order through a shared
  side-channel rather than trying to substitute the stage free-fn (free async
  fns are not trait-object-replaceable cleanly). Pass an
  `Arc<Mutex<Vec<String>>>` test hook to `run_build` (gated on a
  `#[cfg(test)]`-only parameter or a `RunContext` extension); each stage's `run`
  pushes a marker (`"prepare"`, `"rpmbuild"`, `"containers"`, `"signing"`,
  `"upload"`) on entry. Drive `run_build` end-to-end with a minimal fixture
  descriptor; assert the recorded order matches the design 002 §Build Pipeline
  diagram.
- `opts.skip_build` test: when set, every stage receives the skip signal and
  returns an empty report.
- `opts.force` test: scratch dir is cleared at the start of prepare.
- Cancellation test: drop the `run_build` future mid-rpmbuild, assert the
  rpmbuild child is killed (relies on Phase 2 Commit 1's drop guard).

## Commit 4 — `containers` module + `images::sync` (container production)

Container build orchestration + image sync. The containers module takes a
VersionDescriptor and produces a container image; images:: sync orchestrates the
copy-and-sign-along-the-way flow per design 002 §Image Sign & Sync §Image sync
lines 1098–1104.

**Files:**

- `cbsd-rs/cbscore/src/containers/mod.rs` — new module. Declares
  `pub mod build; pub mod component; pub mod repos;`.
- `cbsd-rs/cbscore/src/containers/build.rs` — port of
  `cbscore/containers/build.py`. Container build driver: assembles the build
  context, calls `utils::buildah` to produce the image, tags it for upload.
  Public surface:
  - `pub async fn build_image(desc: &VersionDescriptor, config: &Config, rpms: &RpmbuildReport) -> Result<ContainerImageReport, ContainerError>`
  - `pub struct ContainerImageReport { pub local_tag: String, pub image_id: String, pub digest: Option<String> }`
    — declared here alongside `build_image`, its producer. The `local_tag` is
    the buildah-side tag used to push to the registry (Commit 6's `upload::run`
    consumes it); `image_id` and `digest` populate the eventual
    `BuildArtifactReport` assembly in Commit 7.
- `cbsd-rs/cbscore/src/containers/component.rs` — port of
  `cbscore/containers/component.py`. Per-container-component build driver (when
  a descriptor references multiple containers).
- `cbsd-rs/cbscore/src/containers/repos.rs` — port of
  `cbscore/containers/repos.py`. Repo handling for copr / file / url variants
  (which the Containerfile's `dnf install` step consumes). Unrecognised variants
  surface as `ContainerError::UnsupportedRepoType { value }` rather than an
  `unreachable!()` macro — preserves operator-actionable error messages when the
  descriptor uses a future repo variant the binary doesn't know about.
- `cbsd-rs/cbscore/src/images/sync.rs` — port of `cbscore/images/sync.py`.
  Orchestrator: `skopeo_copy` from source registry to destination registry,
  optionally signing along the way. Public surface:
  - `pub async fn sync_image(src: &ImageRef, dst: &ImageRef, config: &Config, secrets: &SecretsMgr) -> Result<(), ImageDescriptorError>`
- `cbsd-rs/cbscore/src/images/mod.rs` — add `pub mod sync;` alongside the
  existing `pub mod skopeo;` (Phase 2). `pub mod signing;` is added later in
  this phase by Commit 5; `images::sync` (this commit) does not call into
  `images::signing` until Commit 5 lands, so the optional-signing skip path is
  the live behaviour between Commits 4 and 5.
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod containers;`.

**Design constraints:**

- Containers are built via `utils::buildah::buildah_from` + `buildah_commit`
  (Phase 2 Commit 2). The build context is a scratch tempdir populated with the
  Containerfile + the RPMs from `RpmbuildReport`.
- **`BuildahWorkingContainer` RAII guard for cleanup on failure.**
  `containers::build::build_image` wraps the working container in a
  `BuildahWorkingContainer` struct whose `Drop` impl calls
  `buildah unmount <container-id>` + `buildah rm <container-id>` synchronously
  (fire-and-forget, errors swallowed — mirrors the Phase 2 Commit 1 RAII
  drop-guard pattern). On `buildah_commit` failure (or any `?`-early-return in
  `build_image`), the guard's `Drop` ensures the live buildah working container
  is unmounted and removed; without it, a failed build leaves an orphan
  container that future `buildah` invocations may collide with. The success path
  consumes the guard explicitly via a `commit` method that destructures it
  before tagging the committed image — same destructure-on- consume pattern as
  the Phase 4 runner cleanup guard.
- `containers::repos` resolves the descriptor's repo refs to:
  - **copr**: `dnf copr enable <user>/<project>`
  - **file**: a local `.repo` file mounted into the build context
  - **url**: a URL that the Containerfile's `dnf config-manager --add-repo` line
    consumes
- `images::sync` orchestrates per design 002 line 1098–1104. The
  **sign-before-push invariant** ("sign before push, not after" — matches the
  Python implementation and is a tested precondition of the downstream registry
  tooling) takes effect when Commit 5 lands `images::signing::sign_manifest`. In
  Commit 4, `sync_image` uses the optional-signing skip path (no `sign_manifest`
  call at all, whether `config.signing` is set or not — the function doesn't
  exist yet). Once Commit 5 lands, `sync_image` chains `sign_manifest` before
  `skopeo_copy` when `config.signing.is_some()` and continues to skip the sign
  step when it is `None`. The order test that asserts `sign_manifest` fires
  before `skopeo_copy` belongs in Commit 5's §Testable (after `sign_manifest`
  exists), not here.

**Commit-size rationale:** ~550 LOC sits in the sweet spot. Bundles containers +
images::sync because both produce container images: `containers::build` builds
the image locally, `images::sync` copies the built image to the destination
registry. Splitting would create intermediate states where one half exists
without the other. Positioning this commit at #4 (immediately after the two
RPM-side stages — Commit 3 rpmbuild — plus the load_components dependency in
Commit 2) makes `containers::build_image` and `images::sync` available to Commit
6's `builder::upload`, which pushes the locally-built image to the destination
registry — closes the V11-M1 forward-dependency concern from plan-review v11 by
ensuring the containers module exists before any commit that calls into it.
Note: `images::signing` lands in Commit 5 (after this commit), and
`images::sync` in this commit consumes it via the `pub mod` already declared —
so the `pub mod signing` lands in Commit 5 alongside the implementation, with
this commit's `pub mod sync` declared independently in `images/mod.rs`.
`images::sync` itself does not invoke `images::signing` directly at
function-call sites until Commit 5 lands — until then, `sync_image` skips the
sign step (as it does in the optional-signing path). Plus, keeping the images/
module assembled in one logical bundle (skopeo from Phase 2, signing in Commit
5, sync in Commit 4) reads cleanly in `git log`.

**Testable:**

- Command construction tests for `buildah_from` / `buildah_commit` with the
  right base image + Containerfile.
- `containers::repos` resolution tests for each repo variant.
- Integration test (`#[ignore]`-able) against a real podman daemon: build a tiny
  test image, verify the image exists locally via `podman image inspect`. Opted
  in via `CBSCORE_TEST_PODMAN=1` (host must have a working `podman` binary on
  PATH); `#[ignore]`-skipped with a "set CBSCORE_TEST_PODMAN=1 to enable"
  message when unset.
- `images::sync` no-sign path test: with `config.signing` either set or unset,
  `sync_image` does not attempt to call any signing function (no symbol exists
  yet in this commit) and the call is observably a plain `skopeo_copy`. The
  actual sign-before-push order assertion lives in Commit 5 §Testable after
  `sign_manifest` lands.

## End-of-phase acceptance

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace`, `cargo fmt --all --check` all pass.
- `cbscore` library exposes
  `builder::{run_build, prepare, rpmbuild, signing, upload, report, utils}`,
  `releases::{s3, utils}`, `images::{signing, sync}`,
  `containers::{build, component, repos}`.
- Integration tests against real `rpmbuild` + `gpg` + S3 (MinIO) + podman pass
  when reachable (otherwise `#[ignore]`). Un-ignore via
  `cargo test -- --include-ignored` when all four sidecars are available.
- `builder::run_build` is the in-container entry point that Phase 6's
  `cbsbuild runner build` CLI will invoke.
- M1 milestone gate per design 002 line 1269–1281 ("end state: `cargo run` the
  `cbsbuild` CLI, execute a build of the real `ceph` component from
  `components/ceph`, and compare the produced RPM set to the Python output") is
  technically reachable after Phase 5 + Phase 6 land. Phase 5 alone exposes the
  API; Phase 6 wires the CLI.
