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

The per-type subdirectory layout (`<type>/<VERSION>.json`) under the resolved
root is OQ3's call.

## Open Questions

The discussion progresses one item at a time; each entry below moves to Resolved
Decisions once landed.

3. **Per-type subdirectory layout under the configured root.** Today the path is
   `<root>/<type>/<VERSION>.json`. Keep that layout under any configured root,
   flatten to `<root>/<VERSION>.json` (no per-type directory), or make the
   layout itself a separate config field?
4. **Read vs write paths.** Does `versions create` write to one location and
   consumers (cbsd, cbsd-rs, future tooling) read from a different one — or even
   a search path of multiple locations? Or is there a single "the descriptor
   store" location used by all paths?
5. **Backwards compatibility for existing `_versions/` trees.** What happens to
   the descriptor files already populated in operator repos? Migration step,
   automatic detection, or manual operator action?
6. **Schema-version implications.** Adding `Config.paths.versions` is a schema
   change to `Config`. Does this bump `Config.schema_version` to 2, or stay at 1
   because the field is `Option` with a default? See design 002 § Wire-Format
   Versioning for the dispatch policy.
7. **CLI-flag bypass interactions.** Does `--for-systemd-install` /
   `--for-containerized-run` pre-fill `--versions-dir` like the other paths? If
   yes, what value? If no, why is `versions` the exception?

## Design Sketch

(filled in after the Open Questions are resolved)

## Migration

(filled in after the Open Questions are resolved)
