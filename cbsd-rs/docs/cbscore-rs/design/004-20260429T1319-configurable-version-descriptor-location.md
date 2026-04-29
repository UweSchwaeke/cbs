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

(filled in as decisions land)

## Non-Goals

(filled in as decisions land)

## Open Questions

The discussion progresses one item at a time; each entry below moves from this
section to a Resolved Decisions section once landed.

1. **Configuration surface.** Where does the descriptor location come from at
   runtime? Options: a `Config` field, a CLI flag, an env var, or some
   combination. Precedence rules if multiple are set.
2. **Default behaviour.** What happens if no flag, config, or env is set?
   Options: fall back to the current `<git-root>/_versions/<type>` (preserving
   Python parity), use a sensible default like `${cwd}/versions`, or require the
   operator to set it explicitly (fail-fast).
3. **Per-type subdirectory layout.** Today: `_versions/<type>/<VERSION>.json`.
   Should the configurable root keep the per-type subdirectory layout, flatten
   it, or make the layout itself configurable?
4. **Read vs write paths.** Does `versions create` write to one place and
   consumers (cbsd, cbsd-rs, future tooling) read from a different place — or
   even a search path of multiple locations? Or is there a single "the
   descriptor store" location used by all paths?
5. **Backwards compatibility.** What happens to existing `_versions/`
   directories already populated in operator repos? Migration step, automatic
   detection, or manual operator action?
6. **Schema-version implications.** The `Config.paths` struct grows a field (or
   several). Does this constitute a wire-format break for `Config` and bump its
   `schema_version`? See design 002 § Wire-Format Versioning for the dispatch
   policy.
7. **CLI-flag bypass interactions.** If `versions create` grows a
   `--versions-dir` flag, how does it interact with the `cbsbuild config init`
   flow (design 003)? Does `--for-systemd-install` pre-fill it like the other
   paths?

## Resolved Decisions

(filled in as Open Questions are settled)

## Design Sketch

(filled in after the Open Questions are resolved)

## Migration

(filled in after the Open Questions are resolved)
