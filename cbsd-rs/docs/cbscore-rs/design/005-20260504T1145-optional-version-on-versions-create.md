# Optional VERSION argument on `cbsbuild versions create`

## Status

**Complete — ready for review.** This design refines the CLI surface of
`cbsbuild versions create`: making the VERSION positional argument optional and
generating a UUIDv7 string when the operator omits it. It exists as a follow-up
to design 002 (which preserves Python parity — VERSION required) and design 004
(which made the descriptor-store location configurable).

This design is intentionally **out of M1 scope**. M1 ships with VERSION
required, matching Python parity. Once M1 is stable, 005's implementation lands
as a 1.x.0 minor add. All eight Open Questions are resolved (see § Resolved
Decisions); the per-affected-area analysis lives under § Effects of UUIDv7
VERSIONs; the §Design Sketch and §Migration sections are populated.

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

- Operators can run `cbsbuild versions create` without supplying a VERSION on
  every invocation. When the positional is omitted, the command generates a
  UUIDv7 string and uses it as the descriptor identifier — the file lands at
  `<root>/<type>/<UUIDv7>.json` (with `<root>` resolved per design 004) and
  `desc.version` carries the UUIDv7 string verbatim.
- Operators who continue passing an explicit VERSION see no behaviour change.
  The existing regex validation (`[prefix-]vM.m.p[-suffix]`),
  `parse_version()`-driven title format, and patch-walker matching all run
  unchanged on the supplied-VERSION path. Per CLAUDE.md correctness invariant 2
  (CLI UX parity), existing operator scripts and CI invocations keep working.
- The auto-derived path produces (a) chronologically-sortable filenames in
  `<root>/<type>/`, since UUIDv7 strings sort lexicographically by creation time
  per RFC 9562, and (b) a self-explanatory created-at title so operators reading
  `versions list` output see when each descriptor was minted instead of an
  opaque ID.

## Non-Goals

- Auto-derivation from a store, config template, env var, or git-describe.
  Rejected in OQ1; UUIDv7 is the only auto-derivation source.
- Reader-side discovery. `cbsbuild build` still takes an explicit descriptor
  path; design 004 OQ4 locked that in. A "build latest in `<type>`" shortcut is
  also out of scope (item 6).
- Type-specific behaviour (`release` vs `dev`/`test`/`ci`). OQ2 resolved that
  all four types behave uniformly. No `--type`-based refusal of the no-VERSION
  shortcut.
- Schema / wire-format changes. No `schema_version` bump on `VersionDescriptor`,
  `Config`, or any other wire format (item 7 / OQ8).
- Cross-language interchange. Per design 002 §Python Coexistence, mixing Python
  and Rust against the same on-disk files is not supported; operators run one
  implementation per deployment. UUIDv7 descriptors are not portable to Python
  `cbc`/`crt`, regardless of how those tools handle the `desc.version` field
  internally.
- Python-side changes. `cbscore/` (the Python package) keeps the
  VERSION-required CLI. This design lands only in the Rust `cbsbuild` /
  `cbscore` crates.
- M1 scope. The design ships post-M1 as a 1.x.0 minor add. M1 itself ships with
  VERSION required, matching Python parity.

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

The Python walker does **not** currently degrade gracefully on a malformed
VERSION. `_get_patches_by_prio` calls `get_major_version(filter_version)` and
`get_minor_version(filter_version)` (`cbscore/versions/utils.py:73–104`) without
a `try/except`; both functions delegate to `parse_version` and re-raise
`MalformedVersionError` on a non-matching string. A UUIDv7 fed to the existing
Python walker would propagate that exception through `_apply_patches` and fail
the build, not at `versions create` time but at build time.

The Rust port therefore **adds a guard**: it treats `Err(MalformedVersion)` from
the major/minor extractors as "no major/minor/patch known" and skips the
subdirectory rather than propagating. The result is the desired graceful
degradation — only top-level `patches/*.patch` apply when VERSION is a UUIDv7,
per-major and per-minor-patch subdirectories are unreachable for UUIDv7 builds.
See §Design Sketch › §Patch walker for the precise change. Operators who need
version-specific patches for a UUIDv7 build place them at the top level.

### Consumer parsing: covered by items 1 + 2

`parse_version()` is the function that fails on a UUIDv7. Inside cbscore, it is
called by exactly two sites: the patch walker (item 1) and the title generator
(item 2). Both are handled. A third reference in
`cbscore/containers/component.py:49` is a comment about a copied regex, not a
call. The internal `_validate_version()` check (`cbscore/versions/create.py:39`)
runs only on the **supplied-VERSION** path; the UUIDv7 path skips it.

External Python consumers (`cbc`, `crt`) are not part of cbscore-rs's
compatibility surface. Per design 002 §Python Coexistence — "no cross-language
file interchange" — operators run one implementation per deployment; mixing
UUIDv7 descriptors with Python `cbc`/`crt` is not supported, regardless of
whether those tools call `parse_version()` directly or pass `desc.version`
through to other layers. They will be addressed when / if they are rewritten to
Rust.

`parse_version()`'s contract does not change. Each Rust call site that needs
major / minor / patch must handle the `MalformedVersion` error case. The
supplied-VERSION path validates upfront via `validate_version`, so the error
never reaches downstream sites in practice today; the UUIDv7 path skips that
upfront validation, so each downstream site (currently just the patch walker,
item 1) gains a guard that treats the malformed case as "skip" rather than
propagating.

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

The displayed timestamp is rendered at seconds precision (`%H:%M:%SZ`), even
though UUIDv7 stores millisecond precision. This is a readability choice for the
title — the full millisecond timestamp remains in the UUID itself for any
consumer that needs it, and chronological ordering is unaffected (the tie-break
for two UUIDv7s minted in the same second lives in the random bits, not in the
displayed seconds).

When VERSION is a UUIDv7, no major/minor/patch can be extracted. All
subdirectory matches fail by definition, so **only top-level patches apply**.
Per-major and per-minor-patch subdirectories are unreachable for UUIDv7 builds.
This requires a small change in the Rust port relative to the Python source —
see §Design Sketch › §Patch walker. Operators who need version-specific patches
for a UUIDv7 build place them at the top level.

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

A practical side benefit: `ls -1 <root>/<type>/` returns descriptors in
chronological creation order without needing `-t` (mtime) or any custom sort,
because UUIDv7 strings sort lexicographically by their leading 48-bit timestamp
(per §Sortability). Operators accumulating multiple auto-derived descriptors can
pick the most recent one with a plain `ls | tail -1`.

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

The change consists of one CLI-shape edit, one resolver helper, one branch in
the title generator, and one Cargo-feature add. No new config field, no new
flag, no schema change, no patch-walker code (item 1 is graceful degradation of
the existing walker).

### CLI shape

`cbsbuild versions create` (in `cbsbuild/src/cmds/versions.rs`) gains an
optional positional, replacing the required `String` form:

```rust
#[arg(value_name = "VERSION")]
version: Option<String>,
```

clap parses an absent positional as `None`. The handler keeps every other flag —
`--type`, `--image-tag`, `--versions-dir` (design 004) — unchanged.

### Resolver

`cbscore::versions::resolve_version` (alongside `resolve_root` from design 004,
in `cbscore/src/versions/mod.rs`):

```rust
pub fn resolve_version(cli: Option<&str>) -> String {
    match cli {
        Some(v) => v.to_owned(),
        None => uuid::Uuid::now_v7().to_string(),
    }
}
```

The function is sync and infallible. `Uuid::now_v7()` reads the system clock
internally; no IO, no errors to propagate. Returns the canonical hyphenated
36-char form.

The supplied-VERSION path keeps the existing regex validation by calling the
equivalent of `_validate_version()` after resolution but only when the caller
passed an explicit value:

```rust
let version = resolve_version(args.version.as_deref());
if args.version.is_some() {
    validate_version(&version)?;   // existing [prefix-]vM.m.p[-suffix] check
}
```

A UUIDv7 deliberately bypasses validation — it does not match the regex by
construction.

### Title generator

`cbscore::versions::create::do_version_title` (port of
`cbscore/versions/create.py:_do_version_title`) branches on whether the version
is a UUIDv7:

```rust
pub fn do_version_title(
    version: &str,
    version_type: VersionType,
) -> Result<String, VersionError> {
    let type_desc = version_type.get_desc();   // "Development" / "General Availability" / ...
    if let Ok(uuid) = uuid::Uuid::parse_str(version) {
        if uuid.get_version_num() == 7 {
            let ts = uuid_v7_timestamp(&uuid);     // chrono::DateTime<Utc>
            return Ok(format!(
                "Release {type_desc} version created at {}",
                ts.format("%Y-%m-%dT%H:%M:%SZ"),
            ));
        }
    }
    // Supplied-VERSION path: existing parse_version + format.
    let parsed = parse_version(version)?;        // -> MalformedVersion on regex miss
    Ok(format!("Release {type_desc} {}", parsed.title()))
}
```

The return type is `Result<String, VersionError>` because the supplied- VERSION
branch's `parse_version()` is fallible. In practice the error is unreachable:
`cbsbuild versions create` calls `validate_version` (the same regex) earlier in
the handler, so a malformed VERSION fails before `do_version_title` runs. The
`?` therefore never fires on the live path — but the type signature stays honest
about the failure mode rather than relying on a structural "this can't happen"
assumption. Callsite is
`let title = do_version_title(&version, version_type)?;`.

`uuid_v7_timestamp()` extracts the leading 48 bits as milliseconds since the
Unix epoch (per RFC 9562 §5.7) and converts to `chrono::DateTime<Utc>`. The
`uuid` crate exposes the timestamp via `Uuid::get_timestamp()` returning a
`uuid::Timestamp` for v6/v7/v1 inputs; convert with
`Timestamp::to_unix_millis()` (returns `u64`) and feed the result to
`chrono::DateTime::<Utc>::from_timestamp_millis()`. The alternative
`Timestamp::to_unix()` returns `(seconds, nanoseconds)` and is also valid but
requires more arithmetic.

A unit test for `uuid_v7_timestamp` constructs a UUIDv7 from a fixed
`uuid::Timestamp` via `Uuid::new_v7(...)` and asserts the round-tripped
`chrono::DateTime<Utc>` matches — cheap to write and pins the title format
against future regressions.

The `parse_str` + `get_version_num` check is robust: a UUIDv4 (which today's
`gen_run_name` uses) would not match the v7 branch; an arbitrary non-UUID string
(any operator-supplied VERSION) fails `parse_str` and falls through. No false
positives.

### Patch walker

The Rust port of `cbscore/builder/prepare.py:_get_patches_by_prio` adds a guard
around the major/minor extractors so that a UUIDv7 input does not propagate a
`MalformedVersion` error. Schematically:

```rust
match get_minor_version(filter_version) {
    Ok(mv) if path.file_name() == Some(mv.as_str()) => { /* match: descend */ }
    Ok(_) | Err(MalformedVersion) => { /* skip this subdirectory */ }
}
// same shape for get_major_version
```

For a UUIDv7, both `get_minor_version` and `get_major_version` return
`Err(MalformedVersion)`; the walker skips the subdirectory and falls through to
the top-level `patches/*.patch` apply path, producing the desired graceful
degradation (item 1 under §Effects of UUIDv7 VERSIONs).

This is a behavioural divergence from the Python source, which propagates
`MalformedVersionError` uncaught through `_apply_patches`. The Rust port treats
the malformed-version case as "skip" rather than "fail" so that the new UUIDv7
path does not reach `_apply_patches` with an exception in flight. For a supplied
VERSION that fails the regex, the same guard applies — but
`cbsbuild versions create` already validates supplied VERSION upfront via
`validate_version`, so a malformed-supplied-VERSION descriptor is not written in
the first place; the guard's only live trigger is UUIDv7 builds.

### Cargo dep delta

`cbscore/Cargo.toml`:

```toml
uuid = { version = "1", features = ["v4", "v7"] }
```

The `v4` feature is already listed (design 001 §Cargo Sketch); design 005 adds
`v7`. No new crate is added; no other dependency changes.

## Migration

### Code

| Step | Where                            | What                                                                                                                                                                                                                                                                                                                                                     |
| ---- | -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | `cbscore/Cargo.toml`             | Add `"v7"` to the `uuid` features list (existing `["v4"]` becomes `["v4", "v7"]`).                                                                                                                                                                                                                                                                       |
| 2    | `cbscore/src/versions/mod.rs`    | Add `resolve_version(cli: Option<&str>) -> String`. Add `uuid_v7_timestamp` helper (or inline at the single call site in the title generator).                                                                                                                                                                                                           |
| 3    | `cbscore/src/versions/create.rs` | Branch `do_version_title` on UUIDv7 (parse + version-num check) and emit the created-at form. Keep the supplied-VERSION path unchanged.                                                                                                                                                                                                                  |
| 4    | `cbscore/src/builder/prepare.rs` | In the Rust port of `_get_patches_by_prio`, treat `Err(MalformedVersion)` from `get_minor_version` / `get_major_version` as "skip this subdirectory" rather than propagating. **New behaviour relative to the Python source**, which propagates the error through `_apply_patches`. Required for UUIDv7 builds to terminate in `_apply_patches` cleanly. |
| 5    | `cbsbuild/src/cmds/versions.rs`  | Change the positional `version: String` to `version: Option<String>`. Call `resolve_version` and gate the regex `validate_version` call on `args.version.is_some()`. No flag, env-var, or output-line changes.                                                                                                                                           |

All five steps land in a single 1.x.0 release post-M1. They are tightly coupled
(clap shape + resolver + title generator + patch-walker guard must change
together) so splitting would create broken intermediate commits.

### Operator-side

| Operator scenario                                                            | Required action at upgrade                                                                                                                                                                       |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Operator who passes a VERSION on every `cbsbuild versions create` invocation | None. The supplied-VERSION path is unchanged: same regex, same title format, same descriptor contents.                                                                                           |
| Operator who wants the new no-VERSION shortcut                               | Drop the positional from the command. The descriptor lands at `<root>/<type>/<UUIDv7>.json`; the printed `-> written to <path>` echo is the handle to feed to `cbsbuild build`.                  |
| Operator who runs Python `cbc` / `crt` against descriptors created by Rust   | Do not mix UUIDv7 descriptors with Python tooling. Per design 002 §Python Coexistence, operators run one implementation per deployment; UUIDv7 descriptors are not portable to Python consumers. |
| Operator with a CI pipeline that wraps `cbsbuild versions create`            | None unless the pipeline wants the new shortcut. Existing wrappers continue to pass an explicit VERSION as today.                                                                                |

No file-format migration is needed (no schema change). No deployment migration
is needed (cbsbuild is a CLI tool — no daemon, no API surface). The change is
opt-in: operators who keep typing the VERSION see no difference at all.
