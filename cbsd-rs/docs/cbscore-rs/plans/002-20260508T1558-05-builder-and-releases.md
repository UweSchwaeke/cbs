# Phase 5 — M1.4: Builder pipeline + releases + image sign/sync

## Progress

| #   | Commit                                                                 | ~LOC | Status  |
| --- | ---------------------------------------------------------------------- | ---- | ------- |
| 1   | `cbscore: add builder::prepare stage (sources + repo resolution)`      | ~550 | Pending |
| 2   | `cbscore: add builder::rpmbuild stage (per-component RPM builds)`      | ~500 | Pending |
| 3   | `cbscore: add builder::signing + images::signing (GPG + transit)`      | ~500 | Pending |
| 4   | `cbscore: add builder::upload + releases module (S3 publish)`          | ~600 | Pending |
| 5   | `cbscore: add builder::run_build orchestrator + report assembly`       | ~400 | Pending |
| 6   | `cbscore: add containers module + images::sync (container production)` | ~550 | Pending |

**Estimate:** ~3100 LOC, 6 commits.

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
`Result<BuildArtifactReport, BuilderError>` after executing the four-stage
pipeline (prepare → rpmbuild → signing → upload); integration tests gated on
real `rpmbuild` / `gpg` / S3 / podman endpoints pass when reachable (otherwise
`#[ignore]`); the `cbsbuild` binary still prints its placeholder string (CLI
tree lands in Phase 6).

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
  resolved GPG passphrases and registry creds.
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
  6 owns the consumer; Phase 5 lands the underlying S3 read operations in
  `releases::s3`.
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
  `pub mod rpmbuild; pub mod signing; pub mod upload; pub mod report;`).
- `cbsd-rs/cbscore/src/builder/prepare.rs` — port of
  `cbscore/builder/prepare.py`. Public surface:
  - `pub async fn run(desc: &VersionDescriptor, config: &Config, opts: &BuildOptions) -> Result<PrepareReport, BuilderError>`
    — the stage entry point. Returns a `PrepareReport` carrying the
    per-component `BuildComponentInfo` records that downstream stages consume.
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
  `opts.force` is false; cleared when true.
- The patch walker handles both regex-parseable VERSIONs and malformed inputs
  per design 005 (the Rust port adds an explicit guard catching
  `Err(MalformedVersion)` from `get_major_version` / `get_minor_version`,
  returning "no major/minor/patch known" and skipping the subdirectory — Phase 2
  Commit 5 spelled this out). UUIDv7-style VERSIONs from design 005 (post-M1)
  will exercise this guard naturally; M1 exercises it via the existing
  regex-mismatch error path.

**Testable:**

- Unit tests on the patch walker against fixture directory trees: given a
  `components/ceph/patches/` with `19/`, `19.2/`, `19.2.3/`, and top-level
  patches, assert the right subset is selected for `ces-v19.2.3-dev.1`,
  `ces-v19.2`, and `0193e1a8-7c2e-7000-…` (UUIDv7 — only top-level applies, per
  design 005).
- Integration test (`#[ignore]`-able): full prepare run against a fixture `ceph`
  component with a real git clone, assert the resulting `BuildComponentInfo`
  carries the expected SHA + ref.

## Commit 2 — `builder::rpmbuild` stage (per-component RPM builds)

Port `cbscore/builder/rpmbuild.py`. Second stage of the pipeline: spawns
`rpmbuild` per component, collects RPMs into the artifact dir, writes
`ComponentBuild` reports.

**Files:**

- `cbsd-rs/cbscore/src/builder/rpmbuild.rs` — port of
  `cbscore/builder/rpmbuild.py`. Public surface:
  - `pub async fn run(desc: &VersionDescriptor, config: &Config, prep: &PrepareReport, opts: &BuildOptions) -> Result<RpmbuildReport, BuilderError>`
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
  report.

## Commit 3 — `builder::signing` + `images::signing` (GPG + transit)

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
- `cbsd-rs/cbscore/src/builder/signing/gpg.rs` (or a shared
  `cbscore/src/utils/gpg.rs` if the GPG primitives are shared with
  `images::signing` — decide at implementation time, the plan does not
  pre-commit) — `gpg2` subprocess invocation, GPG home dir setup,
  `--pinentry-mode loopback` for passphrase passing.

**Design constraints:**

- Two signing backends per design 002 §Image Sign & Sync lines 1086–1096:
  - **GPG detached signatures** via `gpg2 --detach-sign`. The runner (Phase 4)
    mounts a tempdir at GPG_HOME with the imported key set from the resolved
    secrets; `gpg2` is invoked with `GNUPGHOME=<that path>` and
    `--pinentry-mode loopback`.
  - **Vault transit signing** via `utils::vault::transit_sign` (Phase 3 Commit 2
    ships KV read; transit signing is the parallel HTTP API — implementation
    lands here in Phase 5 if not previously).
- Signing is **optional**: when `config.signing` is `None`, both
  builder::signing and images::signing become no-ops returning empty reports.
  Per design 002 line 1094–1096 ("recent Python commit d2e8a91 cbscore: make
  signing optional").
- Per-RPM signing in `builder::signing` invokes `rpm --addsign` which itself
  shells out to `gpg2` — the cbscore wrapper supplies the passphrase via Phase 2
  Commit 1's `SecureArg::PasswordArg` (per CLAUDE.md §Correctness Invariants
  item 5).

**Testable:**

- Command construction: `rpm --addsign` per-RPM with the right passphrase-arg
  redaction (assert traced lines emit `****`, not the raw passphrase).
- Integration test (`#[ignore]`-able): GPG-sign a tiny fixture RPM against a
  test keyring, verify the signature via `rpm --checksig`.
- Vault transit signing: round-trip a known manifest digest against a
  `vault server -dev` instance with a transit key configured.

## Commit 4 — `builder::upload` + `releases` module (S3 publish)

Final builder stage + the supporting releases module. Uploads signed RPMs to S3,
pushes the built container image to the registry, writes the release descriptor.

**Files:**

- `cbsd-rs/cbscore/src/builder/upload.rs` — port of `cbscore/builder/upload.py`.
  Fourth stage of the pipeline. Public surface:
  - `pub async fn run(desc: &VersionDescriptor, config: &Config, signed: &SigningReport) -> Result<UploadReport, BuilderError>`
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
- Negative test: upload with `config.storage = None` returns an empty
  `UploadReport` and makes zero S3 calls.

## Commit 5 — `builder::run_build` orchestrator + report assembly

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

- Stages chain in strict order: prepare → rpmbuild → signing → upload (per
  design 002 §Build Pipeline diagram line 873–908). A failure in any stage
  short-circuits the chain and returns `Err(BuilderError)` immediately.
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
boundary between the four stage modules (Commits 1–4) — landing it here gives a
clean "now-the-pipeline-runs-end-to-end" review boundary. Bundling with Commit 4
(upload) would mix the final-stage implementation with the
chain-everything-together orchestration, two different review concerns.

**Testable:**

- Stub-stage integration test: replace each stage's `run` with a test double
  that records its arguments + returns a fixed report; drive `run_build`
  through; assert the chain order and the input/ output threading match design
  002 §Build Pipeline.
- `opts.skip_build` test: when set, every stage receives the skip signal and
  returns an empty report.
- `opts.force` test: scratch dir is cleared at the start of prepare.
- Cancellation test: drop the `run_build` future mid-rpmbuild, assert the
  rpmbuild child is killed (relies on Phase 2 Commit 1's drop guard).

## Commit 6 — `containers` module + `images::sync` (container production)

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
- `cbsd-rs/cbscore/src/containers/component.rs` — port of
  `cbscore/containers/component.py`. Per-container-component build driver (when
  a descriptor references multiple containers).
- `cbsd-rs/cbscore/src/containers/repos.rs` — port of
  `cbscore/containers/repos.py`. Repo handling for copr / file / url variants
  (which the Containerfile's `dnf install` step consumes).
- `cbsd-rs/cbscore/src/images/sync.rs` — port of `cbscore/images/sync.py`.
  Orchestrator: `skopeo_copy` from source registry to destination registry,
  optionally signing along the way. Public surface:
  - `pub async fn sync_image(src: &ImageRef, dst: &ImageRef, config: &Config, secrets: &SecretsMgr) -> Result<(), ImageDescriptorError>`
- `cbsd-rs/cbscore/src/images/mod.rs` — add `pub mod sync;` alongside the
  existing `pub mod skopeo;` (Phase 2) and `pub mod signing;` (Phase 5 Commit
  3).
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod containers;`.

**Design constraints:**

- Containers are built via `utils::buildah::buildah_from` + `buildah_commit`
  (Phase 2 Commit 2). The build context is a scratch tempdir populated with the
  Containerfile + the RPMs from `RpmbuildReport`.
- `containers::repos` resolves the descriptor's repo refs to:
  - **copr**: `dnf copr enable <user>/<project>`
  - **file**: a local `.repo` file mounted into the build context
  - **url**: a URL that the Containerfile's `dnf config-manager --add-repo` line
    consumes
- `images::sync` orchestrates per design 002 line 1098–1104: **sign before push,
  not after** — the order is enforced by chaining
  `images::signing::sign_manifest` before `skopeo_copy` rather than after. This
  matches the Python implementation and is a tested precondition of the
  downstream registry tooling.

**Commit-size rationale:** ~550 LOC sits in the sweet spot. Bundles containers +
images::sync because both produce container images: `containers::build` builds
the image locally, `images::sync` copies the built image to the destination
registry. Splitting would create intermediate states where one half exists
without the other (and the builder::upload stage in Commit 4 calls both via the
same `UploadReport` flow). Plus, images::sync depends on images::signing
(Commit 3) and images::skopeo (Phase 2), keeping the images/ module assembled in
one bundle.

**Testable:**

- Command construction tests for `buildah_from` / `buildah_commit` with the
  right base image + Containerfile.
- `containers::repos` resolution tests for each repo variant.
- Integration test (`#[ignore]`-able) against a real podman daemon: build a tiny
  test image, verify the image exists locally via `podman image inspect`.
- `images::sync` order test: assert `sign_manifest` is called before
  `skopeo_copy` (stub both, capture call order).

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
