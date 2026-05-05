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

All open questions have been resolved. See § Resolved Decisions below.

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

### OQ2 — Behaviour for `release` type

**Resolved: allow uniformly across all types.** No `--type`-based special-
casing. The CLI behaviour is one rule everywhere: positional VERSION supplied →
use it; omitted → generate UUIDv7. `cbsbuild versions create -t release` (no
VERSION) generates a UUIDv7 just like `-t dev`/`-t test`/`-t ci`.

The earlier framing of this OQ assumed `release` was semantically distinct.
Inspecting the existing Python code shows the four types differ in only two
places: the `<type>/` subdirectory of the descriptor store, and the human-
readable phrase that goes into the auto-generated title
(`"General Availability"` vs `"Development"` vs `"Testing"` vs `"CI/CD"`). There
is no type-specific validation, no different build/sign/upload path, no special
handling. `release` and `dev` are functionally identical labels. Refusing the
no-VERSION shortcut for `-t release` would lean on cultural intuition ("releases
are real, dev is throwaway") rather than any code-level distinction, and would
add an asymmetric error path for no payoff.

Operators who want a human-readable release name pass it explicitly, exactly
like for any other type.

### OQ3 — CLI shape

**Resolved: optional positional argument.** VERSION stays a positional argument;
clap declares it as `Option<String>`. Absent positional → resolver generates a
UUIDv7. The CLI signature becomes:

```
cbsbuild versions create [OPTIONS] [VERSION]
```

Migrating to a named `--version <V>` flag was rejected. The current command has
been positional since day one (`cbscore/cmds/versions.py:180`); changing the
shape would break every existing operator script and CI invocation. CLAUDE.md
correctness invariant 2 (CLI UX parity) says subcommand names, flags, and
stdout/stderr contracts remain the same unless a design document says otherwise
— making the positional optional satisfies that constraint without breaking
anything, while migrating to a named flag would not.

The optional positional is unambiguous: there is only one positional slot on
this subcommand, so an absent argument means absent. clap's
`Option<String>`-on-positional handles the parsing; the resolver picks UUIDv7
when the parsed value is `None`.

### OQ4 — Echo / output of the derived VERSION

**Resolved: no change to existing output.** `cbsbuild versions create` already
prints `version: <desc.version>`, `version title: <title>`, the rendered JSON,
and `-> written to <path>`. When VERSION is auto-derived as a UUIDv7, those same
lines are emitted with the UUIDv7 string in the `version:` slot and the
created-at title (per item 2) in the `version title:` slot.

No new `derived-version=…` line, no `--print-path-only` flag. Two reasons:

- **Human readers** get the path from the existing `-> written to <path>` echo,
  which is load-bearing per item 6 (operator UX).
- **Script consumers** can grep `^version: ` or `^-> written to ` from the
  existing output. Adding a parallel machine-readable form duplicates
  information that is already there.

The output shape is uniform — supplied-VERSION and auto-derived-VERSION paths
emit the same line structure. No CLI-side branching on whether VERSION came from
the operator or the resolver.

### OQ5–OQ8 — Dissolved or subsumed

The remaining open questions presupposed the rejected "derive-from-something"
model (store-bump / config template / env var / git describe) or were answered
by the per-item analysis above. They are recorded here as
resolved-by-elimination rather than as separate decisions:

- **OQ5 — Determinism / racing.** Dissolved by OQ1's UUIDv7 resolution. Each
  invocation calls `Uuid::now_v7()` and gets a fresh value. The 74 random bits
  remaining after the 48-bit timestamp and 6 version/variant bits make collision
  astronomically unlikely at any plausible CLI invocation rate. Two concurrent
  operators produce two distinct UUIDv7s and two distinct descriptor files. No
  file locking, no race window, no retry logic.
- **OQ6 — Interaction with design 004.** Dissolved by OQ1's UUIDv7 resolution.
  The resolver does not read the descriptor store: it calls `Uuid::now_v7()` and
  returns the string. The descriptor write still uses design 004's resolved root
  as the destination for `<root>/<type>/<UUIDv7>.json`, but that's the standard
  write path (already exercised by the supplied-VERSION case) — no
  auto-discovery, no walk.
- **OQ7 — Image tag when VERSION is auto-derived.** Subsumed by item 5 (image
  tag) under § Effects of UUIDv7 VERSIONs. The OCI tag fallback works as-is for
  UUIDv7 strings; operators wanting a stable tag across a sequence pass
  `--image-tag` explicitly.
- **OQ8 — Schema / wire-format implications.** Subsumed by item 7 (schema) under
  § Effects of UUIDv7 VERSIONs. No `schema_version` bump on any wire format —
  `desc.version` stays a string field, `Config` gains no new field.

## Effects of UUIDv7 VERSIONs

Each subsection records one affected area: what breaks when the VERSION is a
UUIDv7 instead of a parseable version string, and how the design handles it.

### Patches: only top-level apply

`cbscore/builder/prepare.py:_get_patches_by_prio` walks
`components/<comp>/patches/` and matches subdirectory names against parsed major
/ major-minor / full-version components of VERSION. Only the top-level
`patches/*.patch` files apply unconditionally; deeper subdirectories apply only
when their name matches a parsed VERSION component.

### Consumer parsing: covered by items 1 + 2

`parse_version()` is the function that fails on a UUIDv7. Inside cbscore, it is
called by exactly two sites: the patch walker (item 1) and the title generator
(item 2). Both are handled. A third reference in
`cbscore/containers/component.py:49` is a comment about a copied regex, not a
call. The internal `_validate_version()` check (`cbscore/versions/create.py:39`)
runs only on the **supplied-VERSION** path; the UUIDv7 path skips it.

External Python consumers (`cbc`, `crt`) call `parse_version()` against
descriptor values. Per design 002 §Python Coexistence — "no cross-language file
interchange" — these are not part of cbscore-rs's compatibility surface. They
will be addressed when/if they are rewritten to Rust; until then, mixing UUIDv7
descriptors with Python `cbc`/`crt` is not supported (operators run one
implementation per deployment, per that policy).

`parse_version()`'s contract does not change. Each Rust call site that needs
major / minor / patch must already handle the `MalformedVersionError` case
(because the regex can fail on malformed operator input today); no new error
path is added by UUIDv7.

### Sortability: no change needed

UUIDv7 strings sort lexicographically by creation time (the timestamp lives in
the high-order bits per RFC 9562). No in-tree call site compares two VERSIONs:
`versions list` lists releases from S3 in dict-iteration order, and the existing
parseable-VERSION sort behaviour (filesystem listing of `<root>/<type>/`) lines
up naturally with UUIDv7's chronological ordering. No design action required.

### Title: emit a created-at form

`cbscore/versions/create.py:_do_version_title()` parses VERSION via
`parse_version()` to produce a title like
`Release Development version 19.2.3 (DEV 1)`. With a UUIDv7, the parse raises
`MalformedVersionError` and the whole command fails.

**When VERSION is a UUIDv7**, the title generator skips the parser and emits:

```
Release <type-desc> version created at <timestamp>
```

where `<type-desc>` is the existing `get_version_type_desc(type)` value
(`Development`, `General Availability`, `Testing`, `CI/CD`), and `<timestamp>`
is the creation time extracted from the UUIDv7's first 48 bits (milliseconds
since the Unix epoch per RFC 9562), formatted as ISO 8601 in UTC. Example for a
`dev`-type UUIDv7 created on 2026-05-04 at 11:45 UTC:

```
Release Development version created at 2026-05-04T11:45:00Z
```

ISO 8601 in UTC is chosen because it is unambiguous, sortable, and locale-free —
operators reading titles in `versions list` output get a self-explanatory
creation time rather than a placeholder. The `<type-desc>` prefix preserves the
existing title structure for the parseable-VERSION path; only the body shape
changes.

When VERSION is a UUIDv7, no major/minor/patch can be extracted. All
subdirectory matches fail by definition, so **only top-level patches apply**.
Per-major and per-minor-patch subdirectories are unreachable for UUIDv7 builds.
This is a natural extension of the existing walker behaviour ("subdirectory
whose name doesn't match is skipped"), not a new code path. Operators who need
version-specific patches for a UUIDv7 build place them at the top level.

### Image tag: no change needed

`cbscore/versions/create.py:133` falls back to VERSION when `--image-tag` is
unsupplied:

```python
image_tag_str = image_tag if image_tag else version
```

When VERSION is a UUIDv7, the fallback yields the UUIDv7 string as the image
tag. OCI image-tag rules (alphanumerics plus `_`, `.`, `-`; ≤128 chars) admit
the 36-char hyphenated UUIDv7 form. The fallback works as-is. Operators who want
a stable image tag across a sequence of UUIDv7 builds pass `--image-tag`
explicitly — same escape hatch as today.

### Operator UX: rely on the printed path

A UUIDv7 string (`0193e1a8-7c2e-7000-89ab-1234567890ab`) is unambiguous but
unfriendly to type. With the no-VERSION flow, the descriptor lands at e.g.
`<root>/dev/0193e1a8-7c2e-7000-89ab-1234567890ab.json` and the operator's next
command is `cbsbuild build <that-path>`.

`cbsbuild versions create` already prints `-> written to <path>` on success.
That printed path is the operator's handle: they copy it into the subsequent
`cbsbuild build` invocation. No new flag, no "build latest" shortcut, no
discovery affordance — the path echo is load-bearing for this UX, but it already
exists. Per design 004 OQ4, readers do not auto-discover against the configured
root; an explicit-path-only `cbsbuild build` is the established convention and
is preserved here.

Operators who prefer human-readable VERSIONs continue to pass an explicit
positional VERSION as today.

### Schema / wire format: no bump

Design 002 §Wire-Format Versioning's post-M1 rule is that the first schema
change to any wire format bumps that format's `schema_version` to 2. Design 005
touches no schema:

- **`VersionDescriptor` JSON** — the descriptor's contents are unchanged.
  `desc.version` is a string field today and stays a string field; UUIDv7 just
  produces different _values_. No `VersionDescriptor.schema_version` bump.
- **`Config` YAML** — the resolved shape (UUIDv7 when no positional VERSION) is
  pure CLI-side behaviour. No new `Config` field, no `Config` schema change. No
  `Config.schema_version` bump.

Other wire formats (`ReleaseDescriptor`, `ContainerDescriptor`,
`cbs.component.yaml`, `secrets.yaml`) are not touched by this design.

## Design Sketch

(filled in after the Open Questions are resolved)

## Migration

(filled in after the Open Questions are resolved)
