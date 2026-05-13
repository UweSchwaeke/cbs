# Phase 3 — M1.2: S3, Vault, secrets manager, config IO

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

| #   | Commit                                                  | ~LOC | Status  |
| --- | ------------------------------------------------------- | ---- | ------- |
| 1   | `cbscore: add utils::s3 wrapper (aws-sdk-s3)`           | ~500 | Pending |
| 2   | `cbscore: add utils::vault wrapper (vaultrs)`           | ~300 | Pending |
| 3   | `cbscore: add secrets module (SecretsMgr + merge/dump)` | ~500 | Pending |
| 4   | `cbscore: add config IO (Config::load + Config::store)` | ~250 | Pending |

**Estimate:** ~1550 LOC, 4 commits.

## Goal

Land the storage- and secrets-bearing subsystems on top of Phase 2's subprocess
foundation. After Phase 3, `cbscore` can read and write the config / secrets /
vault files, resolve vault-ref credentials to plain form, and talk to S3 + Vault
from the library APIs that Phase 4 (runner) and Phase 5 (builder + releases +
images sign / sync) consume.

End state: `cargo build --workspace` and `cargo test --workspace` pass;
`cbscore` exposes `utils::s3`, `utils::vault`, `secrets`, and `config` modules;
integration tests against a local Vault dev server and a local MinIO (or
LocalStack) instance pass when those endpoints are reachable (marked `#[ignore]`
otherwise); the `cbsbuild` binary still prints its placeholder string (CLI tree
lands in Phase 6).

## Depends on

- Phase 1 — `cbscore-types` provides all wire-format types (`Config`, `Secrets`,
  `VaultConfig`, the four credential families — `GitCreds`, `StorageCreds`,
  `SigningCreds`, `RegistryCreds`), the `VersionedX` wrappers, the matching
  `ConfigError`, `SecretsError`, `MissingSchemaVersion`, `UnknownSchemaVersion`
  variants, and the `logger` module.
- Phase 2 — Phase 3 does **not** strictly require any Phase 2 module (S3 uses
  aws-sdk-s3 directly; Vault uses vaultrs over HTTP; secrets manager dumps via
  `tokio::fs`; config IO uses `serde_saphyr` / `serde_json`). The linear
  ordering in the README reflects design 002 §Migration Strategy
  (`subprocess → … → s3 → vault → secrets → config IO → …`), not a hard
  dependency.
- Design 002 — §Capability Mapping (lines 197–203, 199), §Configuration &
  Secrets Subsystem (lines 419–637), §Releases & S3 §S3 operations (lines
  1156–1165).

## Out of scope

- Higher-level callers — runner (Phase 4) reads the dumped secrets file to mount
  into podman; builder upload (Phase 5) calls `s3_upload_rpms`; releases
  (Phase 5) writes the release descriptor to S3. The wrappers / manager / config
  IO land here; the orchestrators land in their respective later phases.
- `cbsbuild config init` — the interactive `--for-*` flag-driven config
  generator. Bypass-mode flags are Phase 6 (per design 002 §Open Questions
  resolution line 1424–1432); the interactive prompt-based UX is seq-003
  (post-M1).
- Image sign / sync — `cbscore::images::sign` (which uses `utils::vault` for
  transit signing) lands in Phase 5 alongside the builder pipeline.
- Lift-out invariants — `utils::s3` and `utils::vault` are **not** lift-out
  candidates (design 001 §Lift-out invariants names only `utils::subprocess` and
  `utils::git`). Phase 3 modules can freely depend on cbscore-internal types
  without breaking any lift-out contract.
- **Runner-side mount of the dumped secrets file** is a Phase 4 responsibility.
  Phase 3's `dump_to_runner(path: &Utf8Path)` takes the host-side tempfile path
  as an argument and writes the merged-and- resolved Secrets YAML to it. The
  Phase 4 runner is responsible for (a) creating the host tempfile via
  `camino-tempfile` with mode 0600, (b) calling `SecretsMgr::dump_to_runner`
  with the resulting path, and (c) passing the path to
  `podman run --volume <path>:/runner/cbs-build.secrets.yaml`. Phase 3 does not
  enforce this contract — flagging it here so the Phase 4 plan author wires the
  steps together explicitly.

## Commit 1 — `utils::s3` wrapper

Port `cbscore/utils/s3.py` (~376 LoC) to Rust on top of the `aws-sdk-s3` crate,
replacing the Python `aioboto3` driver.

**Files:**

- `cbsd-rs/cbscore/src/utils/s3.rs` — module entry. Free async functions per
  design 002 §Releases & S3 §S3 operations (lines 1156–1165):
  - `check_release_exists(bucket, loc, version) -> Result<bool, S3Error>` (HEAD
    object; map 404 → `Ok(false)`).
  - `check_released_components(bucket, prefix) -> Result<Vec<String>, S3Error>`
    (list-objects-v2 with prefix; paginate).
  - `release_desc_upload(bucket, key, body) -> Result<(), S3Error>` (PUT
    object).
  - `release_upload_components(bucket, key_prefix, files) -> Result<(), S3Error>`
    (bulk PUT; detect content-type per extension, e.g., RPM →
    `application/x-rpm`, JSON → `application/json`).
  - `s3_upload_rpms(bucket, key_prefix, rpm_paths) -> Result<(), S3Error>` (used
    by `builder::upload` in Phase 5).
- `cbsd-rs/cbscore/src/utils/s3/errors.rs` — `S3Error` enum wrapping
  `aws_sdk_s3::Error` and `aws_sdk_s3::operation::*::*Error` per operation via
  `#[from]`. Design 002 §Error Taxonomy line 239–240 explicitly allows boxing
  framework errors (`aws_sdk_s3`, `reqwest`, …) that cannot be exhaustively
  matched. `S3Error` lives in `cbscore`, not `cbscore-types`, because the
  cbscore-types error taxonomy doesn't include S3 — callers in `releases::s3`
  (Phase 5) wrap via `#[from] S3Error` into their domain `ReleaseError`.
- `cbsd-rs/cbscore/Cargo.toml` — add `aws-config = "1"` and `aws-sdk-s3 = "1"`
  per design 001 §Cargo Sketch (already listed in the cbscore Cargo sketch as
  IO-side deps that fill in over Phases 2–5).

**Design constraints:**

- Auth is AWS-SDK-native (env vars, shared credential file, IAM role). Same env
  vars `aioboto3` reads today (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`,
  `AWS_REGION`, `AWS_PROFILE`) — no deployment-level behaviour change.
- `aws_config::load_defaults(BehaviorVersion::latest())` at module init, cached
  in a `OnceCell` per process.
- All operations are async free functions; no struct state.
- HTTP timeouts go via `aws_sdk_s3::Config::builder().timeout_config(…)`;
  default to 30s read / 30s connect.
- `check_release_exists` distinguishes 404 (returns `Ok(false)`) from permission
  / network errors (returns `Err`).
- Content-type detection in `release_upload_components` is a simple match on
  extension; RPMs → `application/x-rpm`, JSON → `application/json`, anything
  else → `application/octet-stream`.

**Testable:**

- Unit tests on content-type detection: assert each known extension maps to the
  right MIME string.
- Unit test on `check_release_exists` 404 handling: feed a fake `aws_sdk_s3`
  error with status 404 into the error decoder, assert `Ok(false)`.
- Integration tests (`#[ignore]`-able) against a local MinIO or LocalStack
  endpoint: round-trip a known body via `release_desc_upload` + GET, list with
  prefix and verify the count. Document the env vars (`AWS_ENDPOINT_URL`,
  `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`) that the test reads to find the
  local endpoint. Un-ignore in CI via `cargo test -- --include-ignored` once the
  MinIO / LocalStack sidecar is available.

## Commit 2 — `utils::vault` wrapper

Port `cbscore/utils/vault.py` (184 LoC) to Rust on top of `vaultrs`, replacing
the Python `hvac` driver.

**Files:**

- `cbsd-rs/cbscore/src/utils/vault.rs` — module entry. Free async functions for
  KV reads, AppRole login, userpass login, token renewal. Supports KV v1 and v2
  mounts (auto-detect via mount metadata, matching Python). Per design 002
  §Vault lines 632–636.
- `cbsd-rs/cbscore/src/utils/vault/errors.rs` — `VaultError` enum. Same
  cbscore-internal rationale as `S3Error`: callers in `secrets::mgr` (Commit 3)
  and `images::sign` (Phase 5) wrap into domain errors. Variants:
  - `PathNotFound { mount: String, path: String }` — `kv_read` got a 404 or
    `errors: [...]` response indicating the secret does not exist at the
    requested path. Operator-actionable; the test in §Testable asserts this
    variant specifically.
  - `AuthFailed { method: &'static str, source: vaultrs::error::ClientError }` —
    token / AppRole / userpass login failed. `method` is one of `"token"`,
    `"approle"`, `"userpass"` for the operator-visible message.
  - `RequestFailed { source: vaultrs::error::ClientError }` — generic transport
    / 5xx / unexpected-response error; `#[from]` for the boxed
    `vaultrs::error::ClientError`.
  - `BadResponse { message: String }` — Vault returned a 200 with a body shape
    the wrapper didn't expect (e.g., missing `data` key in a KV v2 response).
- `cbsd-rs/cbscore/Cargo.toml` — add `vaultrs = "0.8"` per design 002 Capability
  Mapping line 197.

**Design constraints:**

- **Auth order matches Python:** explicit token → AppRole → userpass. The
  wrapper takes a `VaultConfig` (from `cbscore-types::config`) and picks the
  first auth method whose fields are populated. Design 002 line 636 is the
  source.
- **No token caching across calls.** The wrapper re-authenticates per Vault
  call, matching the Python `utils/vault.py` behaviour. This keeps the security
  posture identical across the Python → Rust cutover: minimal token-in-memory
  window (one call duration), full Vault audit signal (every operation logged),
  and zero blast radius from a stolen-token attack on a long-lived `cbsd-worker`
  (Phase 7 context). The cost is extra Vault RTTs per secrets-resolution pass —
  negligible for a build tool. Caching can be revisited as a separate design if
  RTTs become observable in benchmarks; the addition would require introducing a
  struct shape (`VaultClient { … }`) and a cancellation/ownership story for any
  background renewal task, neither of which is in scope here.
- `kv_read(mount, path) -> Result<HashMap<String, String>, VaultError>` is the
  primary read operation. Returns a flat map for KV v1; for KV v2, reads the
  latest version's data sub-tree and returns the same shape.
- `transit_sign(config: &VaultConfig, key_name: &str, input: &str) -> Result<String, VaultError>`
  is the Vault Transit signing operation, parallel HTTP API to `kv_read`. Used
  by Phase 5 Commit 3 (`builder::signing` + `images::signing`) for the
  transit-backed signing path per design 002 §Image Sign & Sync lines 1085–1096.
  Returns the Vault-formatted signature (e.g., `vault:v1:<base64>`). Per-call
  auth applies (no token caching) — same security posture as `kv_read`.
- All vault traffic uses `rustls` (no native TLS). HTTP timeouts as for S3
  (30s).

**Commit-size rationale:** ~300 LOC sits below the 400-line sweet spot named in
`cbsd-rs/CLAUDE.md` §Commit Granularity. Kept as a standalone commit because
`utils::vault` is a self-contained SDK facade (KV reads, auth-method selection,
the per-call auth contract) that is independently testable against a local
`vault server -dev` instance. Bundling with Commit 3 (`secrets::mgr`, async
Vault calls, file IO) would tie the HTTP wrapper to the secrets-orchestration
layer in a single blast radius — two separable concerns that benefit from
independent review.

**Testable:**

- Unit tests on auth-method selection: construct `VaultConfig` with each subset
  of populated fields (`auth_token` only, `auth_approle` only, `auth_user` only,
  multiple — token wins) and assert the selected method.
- Integration tests (`#[ignore]`-able) against `vault server -dev` (a local
  dev-mode Vault binary): round-trip a KV write + read, verify AppRole login
  produces a usable token, verify per-call auth produces a fresh token on each
  `kv_read`. Tests read `CBSCORE_TEST_VAULT_ADDR` (defaults to
  `http://127.0.0.1:8200` when unset) and `CBSCORE_TEST_VAULT_TOKEN` (the root
  token printed by `vault server -dev` at startup) to find the local endpoint;
  tests are `#[ignore]`-skipped with a clear "set CBSCORE*TEST_VAULT_ADDR /
  CBSCORE_TEST_VAULT_TOKEN to enable" message when either is missing. Un-ignore
  in CI via `cargo test -- --include-ignored` once the dev-Vault sidecar is
  configured. Matches the env-var contract pattern from Phase 3 Commit 1
  (AWS*_), Phase 6 Commit 5 (CBSCORE*TEST*_), and Phase 7 Commit 3.
- Negative test: KV read on a missing path returns
  `Err(VaultError::PathNotFound)` (not a generic `RequestFailed`).
- Negative test: AppRole login with an invalid `role_id` returns
  `Err(VaultError::AuthFailed { method: "approle", .. })`.

## Commit 3 — `secrets` module (SecretsMgr + merge / dump)

Port `cbscore/utils/secrets/` (Python ~600 LoC across models + mgr + git +
registry + signing + storage) to Rust. The Python tree split mirrored in design
001 §Workspace Layout (lines 158–166) is preserved.

**Files:**

- `cbsd-rs/cbscore/src/secrets/mod.rs` — module entry; re-exports `SecretsMgr`
  and the leaf submodule functions.
- `cbsd-rs/cbscore/src/secrets/models.rs` — Rust-side wrapper struct
  `Secrets { git: HashMap<String, GitCreds>, storage: HashMap<String, StorageCreds>, signing: HashMap<String, SigningCreds>, registry: HashMap<String, RegistryCreds> }`
  that owns the typed HashMaps (four families keyed by operator-chosen name,
  mirroring the Python `Secrets` container — `dict[str, FamilySecret]` per
  family per design 002 §Secrets). The serde-derived leaf types (`GitCreds`,
  `StorageCreds`, `SigningCreds`, `RegistryCreds`) come from
  `cbscore-types::utils::secrets` (Phase 1 Commit 3); this file does NOT
  redefine them. Also hosts a private helper
  `fn Secrets::load(path: &Utf8Path) -> Result<Secrets, SecretsError>` that
  performs the single-file YAML parse via `serde_saphyr` +
  `VersionedSecrets::into_latest()` (Phase 1 Commit 5). Not a public parallel to
  `Config::load` — it's called only by `SecretsMgr::load_files` (below) and is
  scoped accordingly.
- `cbsd-rs/cbscore/src/secrets/mgr.rs` — `SecretsMgr` struct with:
  - `pub async fn load_files(paths: &[Utf8Path]) -> Result<SecretsMgr, SecretsError>`
    — load each file via `Secrets::load` (the private helper in `models.rs`),
    call `merge()` per design 002 line 628–629. Async because file reads go via
    `tokio::fs`.
  - `pub fn merge(&mut self, other: Secrets)` — append the per-family Vecs.
  - `pub async fn resolve_vault_refs(&mut self, config: &VaultConfig) -> Result<(), SecretsError>`
    — walks each Vault-side entry across all four families (`GitVaultCreds`,
    `StorageVaultCreds`, `SigningVaultCreds`, `RegistryCreds::Vault`), calls
    `utils::vault::kv_read(config, mount, path)` (per-call auth per Commit 2's
    design constraints) to fetch the secret, replaces the vault-ref variant with
    the plain variant in-place. Takes `&VaultConfig` (not a `&VaultClient`
    struct) because the Vault wrapper is free async functions per Commit 2.
  - `pub async fn dump_to_runner(&self, path: &Utf8Path) -> Result<(), SecretsError>`
    — serialise the merged + resolved set to YAML, write to a tempfile that the
    runner (Phase 4) mounts at `/runner/cbs-build.secrets.yaml` (design 002
    §Runner Subsystem mount table).
- `cbsd-rs/cbscore/src/secrets/git.rs` — git-secret-specific helpers (e.g.,
  extracting an SSH key from a `GitVaultCreds::Ssh` entry's vault payload into a
  temp `~/.ssh/key` file with mode 0600).
- `cbsd-rs/cbscore/src/secrets/registry.rs` — registry-secret-specific helpers
  (e.g., constructing a podman `--creds user:pass` flag from a
  `RegistryCreds::Plain` entry).
- `cbsd-rs/cbscore/src/secrets/signing.rs` — signing-secret-specific helpers
  (gpg keyring import, transit-key reference resolution).
- `cbsd-rs/cbscore/src/secrets/storage.rs` — storage-credential resolution (S3
  access-id / secret-id resolved from `StorageVaultCreds::S3` references at
  runtime). Mirrors the role of `git.rs` for the storage family.
- `cbsd-rs/cbscore/src/secrets/utils.rs` — small shared utilities
  (tempfile-with-permissions, vault-ref-to-plain transform).
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod secrets;`.

**Design constraints:**

- `SecretsError` is owned by `cbscore-types::utils::secrets::errors` (Phase 1
  Commit 2). Phase 3's `secrets` module imports it; the `Manager` variant covers
  wrap-and-pass for `VaultError` etc.
- The merged-and-resolved Secrets file written by `dump_to_runner` is the one
  the runner mounts into the builder container. The runner reads its own copy at
  `/runner/cbs-build.secrets.yaml` (design 002 §Runner Subsystem in-container
  mount layout line 784).
- Vault-ref resolution is async because each ref triggers a Vault HTTP read.
  Operations are sequential (no concurrent fan-out) to match Python's behaviour;
  can be revisited later if performance demands.
- `Secrets::load` (the private helper in `models.rs`) is YAML parsing through
  `serde_saphyr` + `VersionedSecrets::into_latest()` (Phase 1 Commit 5). Phase 3
  wires the file IO; Phase 1 owns the wire-format dispatch.

**Testable:**

- Unit test on `merge`: load two Secrets values with disjoint per-family keys,
  merge, assert each family's HashMap length is the sum of the inputs.
- Unit test on `merge` with overlapping keys: two `GitCreds` entries sharing the
  same operator-chosen name → the value from `other` overwrites the receiver's
  entry, matching Python's `dict.update()` semantics
  (`cbscore/utils/secrets/models.py:Secrets.merge`).
- Unit test on `resolve_vault_refs` with a stub `kv_read` (substituting the
  `utils::vault::kv_read` call via dependency injection or a feature-gated test
  double) that returns a fixed `HashMap`: assert each `*Vault*` variant is
  replaced with the corresponding `*Plain*` variant.
- Integration test (`#[ignore]`-able): write a real `secrets.yaml` to tempfile,
  load via `load_files`, dump via `dump_to_runner`, parse the dumped file and
  assert structural equality (round-trip on a realistic shape).
- Mode-0600 assertion on the dumped tempfile: the file is not world-readable.

## Commit 4 — `config` IO (`Config::load` + `Config::store`)

Land the file IO for the config types defined in Phase 1. Per design 002
§Configuration & Secrets Subsystem §IO lines 485–507.

**Files:**

- `cbsd-rs/cbscore/src/config.rs` — single-file module per design 001 §Workspace
  Layout line 127. Contains:
  - `pub async fn Config::load(path: &Utf8Path) -> Result<Config, ConfigError>`
    — read the file via `tokio::fs::read_to_string`, choose YAML vs JSON by
    extension (`.yaml` / `.yml` → YAML, anything else → JSON), parse via
    `VersionedConfig::into_latest()` (Phase 1 Commit 5).
  - `pub async fn Config::store(&self, path: &Utf8Path) -> Result<(), ConfigError>`
    — serialise via `serde_saphyr::to_string` (two-space indent, flow-style
    off), wrap as `VersionedConfig::V1`, write to disk via `tokio::fs::write`.
    Per design 002 line 498–507: creates the parent dir if it does not exist
    (`tokio::fs::create_dir_all`), mirroring Python's
    `mkdir(exist_ok=True, parents=True)` in `cmds/config.py:302`. **Both
    functions are `async fn`** because they do filesystem IO via `tokio::fs`.
    (Design 002's sketch lines 506–507 uses `pub fn` matching the Python
    signature; the Rust port is fully async, so the IO operations become
    `async fn` to avoid blocking the tokio runtime.)
- `cbsd-rs/cbscore/src/lib.rs` — `pub mod config;`.

**Design constraints:**

- File-format dispatch is by extension only (no content sniffing).
- `Config::store` writes YAML unconditionally (the design 002 line 498 reference
  notes the Python implementation also produces YAML).
- The parent-dir-create behaviour is load-bearing: `cbsbuild config init` writes
  to `~/.config/cbsd/${deployment}/worker/cbscore.config.yaml` on a fresh
  workstation (design 002 line 504–505), so the parent dir does not yet exist on
  first run.
- `schema-version: 1` is emitted as the first key on write (kebab per design 002
  §Wire-Format Versioning — `Config` is a kebab-case struct), per the
  `VersionedConfig::V1` wrapper from Phase 1. Reads without the kebab
  `schema-version` key produce `ConfigError::MissingSchemaVersion`; reads with a
  higher-than-supported value produce
  `ConfigError::UnknownSchemaVersion { found, max_supported }`.

**Commit-size rationale:** ~250 LOC sits at the lower end of the 400–800 sweet
spot. Kept as a standalone commit because it closes out Phase 3's M1.2 milestone
with a single self-contained file (`config.rs`) and a single semantic concept
(config-file IO). Bundling with Commit 3 (`secrets` module, ~500 LOC) would tie
two loosely-coupled namespaces together and complicate review — config IO is a
pure-types-plus-fs concern, secrets manager is an async-Vault-resolving concern.

**Testable:**

- Round-trip test: construct a `Config` Rust value, store it, load it back,
  assert equality (`create → store → load == create`, per CLAUDE.md §
  Correctness Invariants item 1).
- YAML / JSON dispatch: load the same `Config` from both `.yaml` and `.json`
  fixture files, assert equality.
- Parent-dir create: store to a path whose parent dir does not exist, assert the
  dir is created and the file lands.
- `schema-version: 1` is the first key in the YAML output (kebab, parse the raw
  bytes and assert position).
- Negative tests inherited from Phase 1 Commit 5: missing `schema-version`
  (kebab) → `MissingSchemaVersion`; future-version `schema-version: 99` →
  `UnknownSchemaVersion { found: 99, max_supported: 1 }`.

## End-of-phase acceptance

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace`, `cargo fmt --all --check` all pass.
- `cbscore` library exposes `utils::s3`, `utils::vault`, `secrets`, `config`.
- Integration tests against local MinIO + Vault dev server pass when reachable
  (otherwise `#[ignore]`).
- Phase 3 module dep graph: `utils::s3` and `utils::vault` are self-contained
  (depend only on `cbscore-types::errors` + their own framework crates +
  `cbscore::logger`); `secrets` depends on `utils::vault` (for ref resolution)
  and `cbscore-types::utils::secrets`; `config` depends only on
  `cbscore-types::config` + `tokio::fs` + `serde_*`. No cross-deps with later
  phases.
