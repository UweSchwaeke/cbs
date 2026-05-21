# Configurable VersionDescriptor Location

## Status

**Approved, lands post-M2 as a 1.x.0 backwards-compatible minor add.** All seven
Open Questions (OQ1–OQ7) are resolved (see § Resolved Decisions). The § Design
Sketch and § Migration sections describe the concrete shape of the change. This
design refines the storage location of version descriptors written by
`cbsbuild versions create` and read by the rest of cbscore-rs. It exists because
the Python implementation hardcodes the path
`<git-repo-root>/_versions/<type>/<VERSION>.json` and carries a
`# FIXME: make this configurable` comment
(`cbscore/src/cbscore/cmds/versions.py:88`). The Rust port is a natural moment
to fix it; design 002 § Version Descriptors & Creation references this
follow-up.

**Sequencing note (2026-05-21).** This design was originally drafted as a pre-M1
change interleaved into seq-002 Phase 6. That interleave slipped: Phase 6 landed
end-to-end (M1 cut at 1.0.0) and Phase 7 followed (M2 cut), both without
seq-004. The design now lands post-M2 as a backwards-compatible additive minor
add — operator configs that omit `paths.versions` keep working unchanged via the
`<git-root>/_versions` fallback, so no migration step is required for the
existing operator population. The OQ6 schema-version rationale is updated below
to reflect this.

## Context

The current Python `cbsbuild versions create VERSION` command:

1. Resolves the current git repository root via `git rev-parse --show-toplevel`.
2. Constructs the output path as `<git-root>/_versions/<type>/<VERSION>.json`,
   where `<type>` is the `--type` flag (`release`, `dev`, `test`, `ci`).
3. Refuses to overwrite an existing file (exits `EEXIST`).
4. `mkdir -p`'s the parent and writes the descriptor as pretty-printed JSON.

This couples descriptor storage to the git repository layout in two ways that
are not necessarily desirable:

- `cbsbuild versions create` requires the cwd to be inside a git checkout.
  Operators who want to manage descriptors outside a git repository (e.g., on a
  worker host that does not clone cbs.git) cannot run the command.
- The location is fixed across deployments. A multi-deployment site cannot point
  different deployments at different descriptor stores.

The Rust port is a clean place to fix this. The shape of the fix (config field
vs CLI flag vs both, default behaviour, layout) is the subject of this design.

## Goals

- Operators can store the descriptor file outside the git repository if they
  choose, by setting a path in `cbs-build.config.yaml` or via a CLI flag.
- Operators who do nothing keep the current Python behaviour bit-for-bit:
  descriptors land at `<git-root>/_versions/<type>/<VERSION>.json`, and
  `cbsbuild versions create` requires being inside a git checkout only when no
  override is set.
- Precedence and override semantics match every other path-typed cbscore config
  field (CLI > config > default).

## Non-Goals

- Adding a third surface (env var) for path overrides. cbscore today uses env
  vars only for `CBS_DEBUG`; paths come from config or CLI. No reason to break
  that pattern.
- Multi-deployment indirection (a search path of multiple version stores). Out
  of scope for v1; revisit if a concrete need shows up.
- Changing the wire format of `VersionDescriptor` itself. Only its on-disk
  location changes.

## Resolved Decisions

### OQ1 — Configuration surface

**Resolved: config field plus CLI flag.** The descriptor-store root is
configurable via:

- `Config.paths.versions: Option<Utf8PathBuf>` in `cbs-build.config.yaml`.
- `--versions-dir <path>` on `cbsbuild versions create`.

Precedence: CLI flag > config field > default (see OQ2). This mirrors the
existing pattern for every other path-typed config field (`components`,
`scratch`, `scratch_containers`, `ccache`) and the per-field override flags they
each have on the CLI side. No env var; consistent with the rest of cbscore.

### OQ2 — Default behaviour when nothing is set

**Resolved: preserve Python behaviour.** When neither `--versions-dir` nor
`Config.paths.versions` is set, the descriptor-store root resolves at runtime to
`<git-rev-parse --show-toplevel>/_versions`. The command thus requires the
caller to be inside a git checkout only in this fallback case; supplying either
of the explicit overrides removes that constraint.

This is byte-identical to current Python behaviour for operators who neither
edit their config nor pass the flag, so the change is fully backwards-compatible
by default.

### OQ3 — Per-type subdirectory layout under the configured root

**Resolved: keep `<root>/<type>/<VERSION>.json`.** The configured root acts
exactly like the current `<git-root>/_versions` directory in Python: per-type
subdirectories (`release/`, `dev/`, `test/`, `ci/`) under it, with each
descriptor named `<VERSION>.json`. Whatever value the operator sets for
`Config.paths.versions` (or passes via `--versions-dir`), the layout under that
path is unchanged from Python.

Three reasons:

- **Preserves Python parity by default.** Combined with OQ2, an operator who
  does nothing sees the same on-disk layout under the same path. An operator who
  relocates the root sees the same in-directory structure under the new
  location. Zero filesystem-layout drift.
- **Avoids a `VersionDescriptor` wire-format change.** Flattening the layout
  (Option B in the discussion) would have required moving the build-type from
  filesystem-encoded to a new field in `VersionDescriptor`, contradicting this
  design's Non-Goal "Changing the wire format of `VersionDescriptor` itself".
- **Single read/write path-resolution function.** A helper
  `cbscore_types::versions::desc::descriptor_path(root, type, version) -> Utf8PathBuf`
  lives in one place and is shared between `versions create` (write) and every
  reader (cbsd, cbsd-rs, future tooling). The layout convention has exactly one
  place in the codebase that encodes it. The helper is pure (a chain of
  `Utf8Path::join` calls), has no IO or async, and lives in `cbscore-types` per
  the cbscore-types-vs-cbscore split in design 001 (types in `cbscore-types`, IO
  in `cbscore`).

The "type encoded in directory layout, not in descriptor" property is an
existing Python-side invariant; this design preserves it.

### OQ4 — Read vs write paths

**Resolved: single configured store; read sites stay explicit-path.**
`versions create` is the only **write** site and writes to the resolved root
(per OQ1 / OQ2 / OQ3). The **read** sites — `cbsbuild build`,
`cbsbuild advanced` builds, `cbscore::runner::run`, and any future tooling —
continue to take the descriptor path as an explicit CLI argument or function
parameter from the caller. None of them resolves descriptors automatically
against the configured root.

This matches actual current Python behaviour: every read site already takes a
`desc_path` argument supplied by the caller (verified across
`cbscore/cmds/builds.py:135`, `cbscore/cmds/builds.py:263`,
`cbscore/runner.py:202`, and `cbsd-rs/scripts/cbscore-wrapper.py` — the wrapper
builds the `VersionDescriptor` in-memory rather than reading from disk). The
only place a known-layout path is computed today is the write site in
`cbscore/cmds/versions.py:90`. Making that write location configurable does not
require changing any reader.

A multi-root **search path** (`Config.paths.versions: Vec<Utf8PathBuf>`) and a
**descriptor auto-discovery** UX (e.g. `cbsbuild build VERSION --type dev`
resolving `<root>/dev/<VERSION>.json` for the operator) are separate features.
Both are out of scope here — the multi-root option is listed under Non-Goals;
auto-discovery would expand `cbsbuild build`'s CLI surface and warrants its own
design pass if ever pursued.

### OQ5 — Backwards compatibility for existing `_versions/` trees

**Resolved: no migration tooling and no auto-detection.** Operators who do
nothing keep working — the OQ2 default fallback resolves to
`<git-root>/_versions`, which is exactly where their existing descriptor files
already live. Operators who choose to relocate the root by setting
`Config.paths.versions` or passing `--versions-dir` are making an informed
choice; moving existing descriptor files into the new root (typically a one-shot
`cp -r <git-root>/_versions/* <new-root>/`) is left to the operator. cbsbuild
does not detect, warn about, or migrate descriptor files between roots.

If a concrete operator ask for a migration helper or auto-detection warning
shows up later, it can be added as a small follow-up. Designing it now means
committing to a UX shape with no concrete requirements.

One implementation note for the OQ2 fallback's error path: when no
`--versions-dir` is supplied, no `Config.paths.versions` is set, and the caller
is **not** inside a git checkout, the error message must name both overrides
explicitly so the operator knows the two ways to fix it (rather than the bare
`git rev-parse` error). For example:

```
error: cannot resolve descriptor store location.
  no --versions-dir flag was supplied,
  no `paths.versions` field is set in cbs-build.config.yaml,
  and the current directory is not inside a git checkout.
  set one of the above to choose where descriptors live.
```

### OQ6 — Schema-version implications

**Resolved: no bump.** The `Config`'s `schema-version` marker stays at 1 when
this design lands. The rationale is operational, not derived from design 002's
versioning rule:

- The schema-version bump policy in design 002 §Wire-Format Versioning ("every
  change bumps; additive changes are not exempt") has been deferred across every
  design currently in flight (seq-002 through seq-005). No design in the current
  corpus bumps `schema-version`, and no plan currently mandates a bump-policy
  enforcement pass. seq-004 follows the same posture: leave `schema-version` at
  the value on HEAD (`1`, confirmed at
  `cbscore-types/src/config/versioned.rs:65`) and let the bump policy be
  revisited under its own future design.
- Operationally, the new field is additive and optional
  (`paths.versions: Option<Utf8PathBuf>` with `#[serde(default)]` +
  `#[serde(skip_serializing_if = "Option::is_none")]`), so operator YAML files
  that omit it deserialise unchanged on a new binary, and files written by a new
  binary with the field unset serialise as absent — round-trip stable in both
  directions against existing binaries. No operator action is required at the
  seq-004 cutover.

Concrete consequences:

- The `Config` struct grows a `paths.versions: Option<Utf8PathBuf>` field at the
  seq-004 cutover. Files written by either side carry `schema-version: 1`
  (kebab) unchanged.
- No transform code, no deprecation warning, no operator manual edit.
- When the schema-version bump policy is brought back into scope (in some future
  design that is out of scope here), seq-004's no-bump landing will be reviewed
  alongside every other deferred-bump change. The expected outcome is either a
  one-shot retroactive bump of all deferred additive changes together, or a
  carve-out in the bump rule for `Option<T>` with `#[serde(default)]` +
  `#[serde(skip_serializing_if)]`; either resolution is compatible with
  seq-004's current shape.

### OQ7 — CLI-flag bypass interactions

**Resolved: `--for-systemd-install` / `--for-containerized-run` pre-fill
`Config.paths.versions = "/cbs/_versions"`** alongside the other path fields.
This makes `versions` symmetric with `components`, `scratch`,
`scratch_containers`, `ccache`, `secrets`, and `vault` — every path field has
both a config-init interactive prompt and a slot in the bypass-mode pre-fill
set, and `versions` joins them.

The systemd / containerized deployments do not necessarily _use_
`cbsbuild versions create` today (workers typically read descriptors authored
elsewhere via an explicit `desc_path` per OQ4), but pre- filling the path keeps
the layout uniform and lets future workflows (e.g., a CI worker that authors
descriptors locally) work without a config edit.

The chosen value `/cbs/_versions` keeps the leading-underscore convention from
the Python `<git-root>/_versions` directory, so operators familiar with the
existing layout see a recognisable name under `/cbs/`.

Concrete consequences for design 003 (interactive `config init`):

- §`config_init_paths` gains a new prompt — "Specify versions path?" `Confirm`
  (default no), then `Input::<String>` for the path. Mirrors the existing
  optional `ccache path` prompt.
- §Bypass Behaviour: `--versions-dir` is added to the per-field flags list (it
  skips the prompt above when supplied). The systemd / containerized bypass-mode
  pre-fill set gains `versions = /cbs/_versions`, listed alongside the other
  paths.

Both edits land in design 003 in the same commit as this resolution.

## Design Sketch

The change consists of one new config field, one new CLI flag, one resolver, one
path-builder, and a small set of edits at the existing write site. All pieces
land in the same post-M2 1.x.0 minor release (seq-004); nothing is staged
separately.

### Config schema

`cbscore-types/src/config/paths.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PathsConfig {
    pub components:         Vec<Utf8PathBuf>,
    pub scratch:            Utf8PathBuf,
    pub scratch_containers: Utf8PathBuf,
    #[serde(default)]
    pub ccache:             Option<Utf8PathBuf>,
    #[serde(default)]
    pub versions:           Option<Utf8PathBuf>,   // NEW (design 004)
}
```

YAML key is `versions` (kebab-case is a no-op for a single word). The field is
`Option`; absent means "fall back to the OQ2 default at runtime".

### CLI flag

`cbsbuild versions create` gains:

```rust
#[arg(long, value_name = "PATH")]
versions_dir: Option<Utf8PathBuf>,
```

Per design 002 §Capability Mapping the type is `Utf8PathBuf` (camino). clap
parses it via the `Utf8PathBuf: FromStr` impl.

### Resolver

`cbscore::versions::resolve_root` in `cbscore/src/versions/mod.rs`:

```rust
pub async fn resolve_root(
    cli: Option<&Utf8Path>,
    config: &Config,
) -> Result<Utf8PathBuf, VersionError> {
    if let Some(p) = cli {
        return canonicalize_root(p).await;
    }
    if let Some(p) = config.paths.versions.as_deref() {
        return canonicalize_root(p).await;
    }
    // Fallback: <git-rev-parse --show-toplevel>/_versions. The git root
    // is already absolute, so no further canonicalization is needed.
    match cbscore::utils::git::repo_root().await {
        Ok(root) => Ok(root.join("_versions")),
        Err(_) => {
            // Best-effort cwd capture for the error context; never propagate
            // a std::io::Error here, that would bypass the OQ5 friendly text.
            let cwd = std::env::current_dir()
                .ok()
                .and_then(|p| Utf8PathBuf::try_from(p).ok())
                .unwrap_or_else(|| Utf8PathBuf::from("<unknown>"));
            Err(VersionError::NoDescriptorRoot { cwd })
        }
    }
}

/// Resolve an operator-supplied root path to an absolute, symlink-
/// resolved `Utf8PathBuf`. Requires the directory to exist on disk —
/// operators must create it (`mkdir -p`) before passing `--versions-dir`
/// or setting `paths.versions` in config. Without canonicalization,
/// downstream consumers (`descriptor_path`, the patch walker, the runner
/// mount line) would each have to defensively re-resolve a possibly-
/// relative path against an unknown cwd.
async fn canonicalize_root(
    p: &Utf8Path,
) -> Result<Utf8PathBuf, VersionError> {
    let abs = tokio::fs::canonicalize(p.as_std_path())
        .await
        .map_err(|source| VersionError::DescriptorRootResolve {
            path: p.to_owned(),
            source,
        })?;
    Utf8PathBuf::try_from(abs).map_err(|err| {
        VersionError::DescriptorRootNotUtf8 {
            path: err.into_path_buf().to_string_lossy().into_owned(),
        }
    })
}
```

`VersionError::NoDescriptorRoot` carries enough context that its `Display` impl
produces the OQ5 error message (no git, no override, mention both
`--versions-dir` and `Config.paths.versions`). The `cwd` is captured
best-effort: if `std::env::current_dir()` itself fails (working directory
deleted under the process) or the path is not UTF-8, the error renders with
`<unknown>` rather than propagating a `std::io::Error` that would mask the
intended message.

Two additional error variants surface from the canonicalize step:

- `VersionError::DescriptorRootResolve { path, source: std::io::Error }` —
  `tokio::fs::canonicalize` failed (most commonly `ENOENT` because the
  operator-supplied directory does not exist yet). The `Display` impl names the
  path and includes the underlying error, with hint text pointing the operator
  at `mkdir -p`.
- `VersionError::DescriptorRootNotUtf8 { path: String }` — the resolved absolute
  path contains non-UTF-8 bytes (a symlink resolved into a filesystem location
  with exotic encoding). The `String` form is `path.to_string_lossy()`, lossy by
  design — the error path's only job here is to tell the operator which path was
  rejected.

### Path builder

`cbscore_types::versions::desc::descriptor_path` in
`cbscore-types/src/versions/desc.rs`:

````rust
/// Build the on-disk path for a version descriptor.
///
/// `root` MUST be absolute. `resolve_root` canonicalizes operator
/// input before returning, so any caller routing through the standard
/// resolver satisfies this contract. The `debug_assert!` flags
/// violations in tests; release builds still return a path, but
/// downstream code that depends on absolute paths (the runner mount
/// line, the descriptor-write site) may misbehave.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore_types::versions::{VersionType, desc::descriptor_path};
///
/// let p = descriptor_path(
///     Utf8Path::new("/var/cbs/_versions"),
///     VersionType::Dev,
///     "19.2.3",
/// );
/// assert_eq!(
///     p.as_str(),
///     "/var/cbs/_versions/dev/19.2.3.json",
/// );
/// ```
pub fn descriptor_path(
    root: &Utf8Path,
    ty: VersionType,
    version: &str,
) -> Utf8PathBuf {
    debug_assert!(
        root.is_absolute(),
        "descriptor_path: root must be absolute (got {root}); \
         resolve_root canonicalizes operator input — bypass that path \
         only with great care",
    );
    root.join(ty.as_dir_name()).join(format!("{version}.json"))
}
````

`VersionType::as_dir_name(&self) -> &str` returns `"release"`, `"dev"`,
`"test"`, `"ci"` — the existing pydantic enum value strings, locked in by
Correctness Invariant 4 (snake_case wire keys).

This helper is the single source of truth for the `<root>/<type>/<VERSION>.json`
layout. Both write (`versions create`) and any future read-side consumer call
it.

### Write site

`cbsbuild versions create` (in `cbsbuild/src/cmds/versions.rs`):

```rust
let root = cbscore::versions::resolve_root(
    args.versions_dir.as_deref(),
    &config,
).await?;
let path = cbscore_types::versions::desc::descriptor_path(
    &root, version_type, &desc.version,
);

if path.exists() {
    return Err(VersionError::AlreadyExists { path });
}

cbscore::versions::desc::write_descriptor(&desc, &path).await?;
```

The `create_dir_all` lives **inside `write_descriptor`** — the helper creates
the parent directory if missing (via `tokio::fs::create_dir_all`) before writing
the JSON, matching the same `mkdir -p` semantic that `Config::store` already
carries (design 002 F3). The call site does **not** repeat the `mkdir -p`;
future callers (e.g., `cbsd-rs` after M2) inherit the same
parent-create-on-write contract without having to know it. This is the pinned
public API contract for `write_descriptor`.

### Bypass-mode pre-fill

In `cbsbuild config init`'s `--for-systemd-install` / `--for-containerized-run`
handler, alongside the other path pre-fills:

```rust
init.paths.versions = Some(Utf8PathBuf::from("/cbs/_versions"));
```

Listed in design 003 §Bypass Behaviour for completeness.

## Migration

### Code

| Step | Where                                              | What                                                                                                                                                                                                                                 |
| ---- | -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1    | `cbscore-types/src/config/paths.rs`                | Add `versions: Option<Utf8PathBuf>` field with `#[serde(default)]`.                                                                                                                                                                  |
| 2    | `cbscore-types/src/versions/desc.rs`               | Add `descriptor_path()` helper with absolute-root doc contract and a `debug_assert!(root.is_absolute())` guard. Add `VersionType::as_dir_name()` if not already present.                                                             |
| 3    | `cbscore/src/versions/mod.rs`                      | Add `resolve_root()` and its `canonicalize_root()` helper. Add three `VersionError` variants: `NoDescriptorRoot` (OQ5 text), `DescriptorRootResolve { path, source: std::io::Error }`, and `DescriptorRootNotUtf8 { path: String }`. |
| 4    | `cbsbuild/src/cmds/versions.rs`                    | Add `--versions-dir` flag. Call `resolve_root()` then `descriptor_path()`. Drop the old hardcoded `repo_root.join("_versions").join(type).join(...)` chain.                                                                          |
| 5    | `cbsbuild/src/cmds/config/init.rs` (later seq-003) | Add the optional "Versions path" prompt. Add `versions = "/cbs/_versions"` to the bypass-mode pre-fill set.                                                                                                                          |

Steps 1–4 land in the seq-004 post-M2 minor release. Step 5 lands when design
003 (interactive `config init`) is implemented under seq-003.

### Operator-side

| Operator scenario                                                       | Required action at upgrade                                                                                                                                                                                                                                                                                                  |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Existing operator with `<git-root>/_versions/` populated, doing nothing | None. Default fallback resolves to `<git-root>/_versions`. Bit-identical behaviour.                                                                                                                                                                                                                                         |
| Operator who wants to relocate the descriptor store                     | Set `paths.versions: /new/path` in `cbs-build.config.yaml`, or pass `--versions-dir /new/path` per invocation. The directory must exist (`mkdir -p /new/path` first) — `resolve_root` canonicalizes the path and errors out if it does not exist. Move existing files via shell: `cp -r <git-root>/_versions/* /new/path/`. |
| Operator using `--for-systemd-install` / `--for-containerized-run`      | Re-run `cbsbuild config init --for-systemd-install` (or equivalent) on the bypass-pre-fill side; the regenerated `cbscore.config.yaml` will include `paths.versions: /cbs/_versions`. Alternatively, manually add the field to the existing config.                                                                         |
| Operator on a worker host without a git checkout (today: blocked)       | Set `paths.versions` in config or pass `--versions-dir`. The blocking constraint is removed.                                                                                                                                                                                                                                |

No Python-side patches; no schema-version bump (per OQ6, the new field is an
optional additive extension that round-trips through both old and new binaries —
the "every change bumps" rule applies only to changes that alter the
interpretation of existing fields).
