# Optional VERSION argument on `cbsbuild versions create`

## Status

**Draft — discussion phase.** This design refines the CLI surface of
`cbsbuild versions create`: making the VERSION positional argument optional and
providing some way to derive it automatically. It exists as a follow-up to
design 002 (which preserves Python parity — VERSION required) and design 004
(which made the descriptor-store location configurable and is the right
neighbour for any auto-derivation logic that has to read the store).

This design is intentionally **out of M1 scope**. M1 ships with VERSION
required, matching Python parity. Once M1 is stable, 005's implementation lands
as a 1.x.0 minor add. Sections below are populated as the design discussion
progresses with the user. The Open Questions block enumerates the decision
points.

## Context

`cbsbuild versions create VERSION` requires a positional VERSION string today
(`cbscore/cmds/versions.py:180`). The string is used in four distinct places
after creation:

1. **Filename** — the descriptor lands at `<root>/<type>/<VERSION>.json`, where
   `<root>` resolves per design 004 OQ1+OQ2 and `<type>` is the `--type` flag
   value.
2. **`desc.version` field** inside the JSON — downstream consumers read it.
3. **Auto-generated title** — `_do_version_title()` parses VERSION to produce a
   human-readable title (e.g. "Release Development version 19.2.3 (DEV 1)").
4. **Default `--image-tag` value** — when the operator does not supply
   `--image-tag`, the image tag in `VersionImage` falls back to VERSION.

VERSION must match `[prefix-]vM.m.p[-suffix]` with major+minor+patch all present
(`cbscore/versions/utils.py:parse_version` plus `create.py:_validate_version`).
A bare `99` or `99.99` is rejected.

The downstream `cbsbuild build <desc_path>` consumes the descriptor file by
explicit path (per design 004 OQ4 — readers do not auto-discover against the
configured root). So the VERSION the create command picks has to be findable
afterwards either via the echoed success message ("wrote config file to
`<path>`") or via a deterministic naming scheme that scripts can predict.

The current CLI requires the operator to type the full VERSION string each time.
For workflows that iterate on dev versions (`ces-v19.2.3-dev.1`, `dev.2`,
`dev.3`, …) this is tedious; for CI pipelines that derive a VERSION from a
build-id env var, an explicit positional means the runner has to inject the
value into the command line. A way to derive VERSION without the operator typing
it on every invocation is the design goal.

## Goals

(filled in as decisions land)

## Non-Goals

(filled in as decisions land)

## Open Questions

The discussion progresses one item at a time; each entry below moves to Resolved
Decisions once landed. Same convention as design 004.

- **OQ1 — Source of the auto-derived VERSION.** Five plausible sources, each
  suiting a different workflow:
  - **A. Latest-in-store + bump.** Read `<root>/<type>/`, parse filenames, find
    the highest, increment the suffix counter (`dev.42` → `dev.43`).
  - **B. Timestamp suffix.** `<base>-<type>.YYYYMMDDTHHmm`. The `<base>` still
    has to come from somewhere.
  - **C. Config template.** A new `Config.versions.default_template` plus a
    counter.
  - **D. Env var.** `CBS_VERSION="ces-v19.2.3-dev.42"` exported by the operator
    or CI.
  - **E. Git describe.** `git describe --tags` against a configured repository.

  Multiple sources may coexist with a precedence rule. The decision is which to
  support and in what order.

- **OQ2 — Behaviour for `release` type.** The auto-derivation workflows are most
  useful for transient `dev`/`test`/`ci` types. For `release`, operators usually
  want to be explicit. Does `cbsbuild versions create -t release` (no VERSION)
  refuse, or derive like the others?

- **OQ3 — CLI shape.** Is VERSION an optional positional argument
  (`cbsbuild versions create [VERSION]`), or does it migrate to a named flag
  (`--version <V>` or similar) so the positional becomes unambiguously absent?
  clap supports both shapes; the positional form is more compact, the named-flag
  form is more explicit.

- **OQ4 — Echo / output of the derived VERSION.** When VERSION is auto-derived,
  what does `versions create` print? The current command prints
  `version: {desc.version}`, `version title: …`, the rendered JSON, and
  `-> written to {path}`. With auto-derivation, scripts may want a
  machine-readable form — e.g., a final line
  `derived-version=ces-v19.2.3-dev.43` for shell `eval`.

- **OQ5 — Determinism / racing.** If two operators run
  `cbsbuild versions create -t dev` concurrently against the same store, both
  might derive the same next VERSION (Option A) and one hit the existing-file
  `EEXIST` check. Is that an acceptable failure mode (operator retries) or do we
  add file locking? Option D (env var) sidesteps this by making the source
  explicit.

- **OQ6 — Interaction with design 004.** If Option A or C is picked, the
  auto-derivation reads the configured descriptor store (which moved to
  `Config.paths.versions` per design 004). Walking that directory must respect
  the same `<type>/` layout (design 004 OQ3) and the same fallback when the
  directory does not exist.

- **OQ7 — Image tag when VERSION is auto-derived.** Today the image tag defaults
  to VERSION when `--image-tag` is unsupplied. With an auto-derived VERSION, the
  image tag follows the derived value — but operators who want a stable image
  tag across a sequence of dev versions (`dev.42`, `dev.43`, … all pushed to the
  same image tag) will not be served by the default. Document or change?

- **OQ8 — Schema / wire-format implications.** This design is also a post-M1
  change (per the framing in §Status), so by the design 002 rule the answer is
  "the first post-1.0 schema change to any format bumps that format's
  `schema_version` to 2". Does this design force such a bump? Likely no for
  `VersionDescriptor` (the descriptor's contents are unchanged); possibly yes
  for `Config` if Option C lands (new field).

## Resolved Decisions

### OQ1 — Source of the auto-derived VERSION

**Resolved: generate a UUIDv7 when no positional VERSION is supplied.** The
shape is binary: the operator either passes a positional VERSION (as today,
regex-validated against `[prefix-]vM.m.p[-suffix]`) or omits it, in which case
`cbsbuild versions create` generates a UUIDv7 and uses that string as the
VERSION. No env var, no store-bump, no config template, no git-describe — those
alternatives were rejected (see discussion notes).

UUIDv7 is preferred over UUIDv4 because v7 is timestamp-prefixed: strings sort
approximately by creation time, which gives operators a useful ordering when
listing the descriptor store.

A UUIDv7 does not match the existing `[prefix-]vM.m.p[-suffix]` regex. The
downstream code paths and conventions that depend on parsing VERSION need
adjustment to handle the no-positional case. Each affected area is enumerated
under § Effects of UUIDv7 VERSIONs below; the discussion progresses one item at
a time.

## Effects of UUIDv7 VERSIONs

Each subsection records one affected area: what breaks when the VERSION is a
UUIDv7 instead of a parseable version string, and how the design handles it.

### Patches: only top-level apply

`cbscore/builder/prepare.py:_get_patches_by_prio` walks
`components/<comp>/patches/` and matches subdirectory names against parsed major
/ major-minor / full-version components of VERSION. Only the top-level
`patches/*.patch` files apply unconditionally; deeper subdirectories apply only
when their name matches a parsed VERSION component.

When VERSION is a UUIDv7, no major/minor/patch can be extracted. All
subdirectory matches fail by definition, so **only top-level patches apply**.
Per-major and per-minor-patch subdirectories are unreachable for UUIDv7 builds.
This is a natural extension of the existing walker behaviour ("subdirectory
whose name doesn't match is skipped"), not a new code path. Operators who need
version-specific patches for a UUIDv7 build place them at the top level.

## Design Sketch

(filled in after the Open Questions are resolved)

## Migration

(filled in after the Open Questions are resolved)
