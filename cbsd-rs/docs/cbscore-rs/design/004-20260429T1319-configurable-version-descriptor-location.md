# Configurable VersionDescriptor Location

## Status

**Draft — discussion phase.** This design refines the storage location of
version descriptors written by `cbsbuild versions create` and read by the rest
of cbscore-rs. It exists because the Python implementation hardcodes the path
`<git-repo-root>/_versions/<type>/<VERSION>.json` and carries a
`# FIXME: make this configurable` comment
(`cbscore/src/cbscore/cmds/versions.py:88`). The Rust port is a natural moment
to fix this; design 002 § Version Descriptors & Creation references this
follow-up.

Sections below are populated as the design discussion progresses with the user.
Open Questions block enumerates the decision points.

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
  `cbscore::versions::desc::descriptor_path(root, type, version) -> Utf8PathBuf`
  lives in one place and is shared between `versions create` (write) and every
  reader (cbsd, cbsd-rs, future tooling). The layout convention has exactly one
  place in the codebase that encodes it.

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

**Resolved: no bump.** `Config.schema_version` stays at 1 when this design
lands. Design 004 is a pre-M1 change; cbscore-rs is in its 0.x development
phase, where the schema is still being defined and accumulates additions into
`schema_version: 1` until M1 1.0.0 ships. Design 002 § Wire-Format Versioning
has been updated to make this qualifier explicit ("every change bumps" applies
from M1 onward; pre-M1 the schema is mutable).

Concrete consequences:

- The `Config` struct grows a `paths.versions: Option<Utf8PathBuf>` field in the
  M1 1.0.0 release. Files written by cbscore-rs M1 carry `schema_version: 1` as
  today; the field is just there in the schema.
- No transform code, no deprecation warning, no operator manual edit.
- The first post-1.0 change to `Config` (whatever it is) bumps to
  `schema_version: 2` per the standing rule.

This decision applies the same way to any other wire-format file extended during
M0–M1 development: extensions accumulate into v1 and the first bump comes after
the M1 cut.

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

(filled in after the Open Questions are resolved)

## Migration

(filled in after the Open Questions are resolved)
