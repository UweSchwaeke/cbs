# 018 — Replace `serde_yml` with `serde-saphyr` for YAML parsing

- **Component:** `cbsd-rs` (`cbsd-server` + `cbsd-worker`)
- **Status:** Proposed
- **Author:** Joao Eduardo Luis
- **Date:** 2026-05-06

## Motivation

`cbsd-server` and `cbsd-worker` currently deserialize YAML configuration through
`serde_yml = "0.0.12"`. That crate is a single-maintainer republish of the
abandoned `serde-yaml`, built on `libyml` — a fork of `unsafe-libyaml`. It
carries three concerns:

1. **Maintenance posture.** Single maintainer, version stuck at `0.0.12`, with
   ongoing reputational concerns in the Rust ecosystem.
2. **C-derived parser via `unsafe` FFI.** `libyml` brings a forked YAML 1.1-era
   C parser into the dependency tree. We do not need YAML 1.1 quirks.
3. **No active YAML 1.2 conformance work.** Stricter conformance would protect
   us from operator-authored config foot-guns (Norway problem, sexagesimals,
   octal/leading-zero ints).

`serde-saphyr` is a pure-Rust deserializer (and now a serializer too) on top of
the `saphyr` YAML 1.2 parser. It is actively developed, has no `unsafe-libyaml`
fork in its tree, and emphasizes "panic-free parsing and good error reporting".

## Goals

- Drop the `serde_yml` (→ `libyml`) dependency from the cbsd-rs workspace.
- Use `serde-saphyr` (`serde_saphyr::from_str`) for all YAML config loading in
  `cbsd-server` and `cbsd-worker`.
- Keep behaviour identical for the existing config templates and on-disk
  component manifests — no schema changes, no field renames, no new typing.
- Surface YAML parse errors with the same `Display` quality (or better) as
  today's `ConfigError::Parse(...)` flow.

## Non-goals

- No introduction of YAML serialization. Nothing in `cbsd-rs` writes YAML.
- No schema changes to `ServerConfig`, `WorkerConfig`, or `ComponentYaml`.
- No new YAML files added. No migration of `cbs.component.yaml` consumers.
- No re-architecture of how config files are discovered / loaded / reloaded.
- No backwards-compat shim: the swap is a single mechanical replacement.

## Current state

YAML deserialization is performed at exactly three call sites, all loading
fully-typed structs with `#[derive(Deserialize)]` and
`#[serde(rename_all = "kebab-case")]` (where applicable). No use of
`serde_yml::Value`, no `serde(flatten)`, no untagged enums, no custom
`deserialize_with`, no anchors/aliases.

| Site                                   | Purpose                                 | Struct                      |
| -------------------------------------- | --------------------------------------- | --------------------------- |
| `cbsd-server/src/config.rs:342`        | Server config bootstrap                 | `ServerConfig` (and nested) |
| `cbsd-server/src/components/mod.rs:54` | Component discovery (reads `name` only) | `ComponentYaml`             |
| `cbsd-worker/src/config.rs:136`        | Worker config bootstrap                 | `WorkerConfig` (and nested) |

Plus one type reference:

| Site                            | Purpose                                                |
| ------------------------------- | ------------------------------------------------------ |
| `cbsd-worker/src/config.rs:277` | `ConfigError::Parse(serde_yml::Error)` variant payload |

Authoritative YAML files consumed by these binaries:

- `cbsd-rs/systemd/templates/config/server.yaml.in`
- `cbsd-rs/systemd/templates/config/worker.yaml.in`
- `components/<name>/cbs.component.yaml` (e.g.
  `components/ceph/cbs.component.yaml`)

Other `.yaml` files in the repo (e.g. `podman-compose.cbsd-rs.yaml`) are not
consumed by `cbsd-rs` and are out of scope.

## Library facts (verified at `serde-saphyr 0.0.26`)

- Crate name in `Cargo.toml`: `serde-saphyr`. Module path: `serde_saphyr`.
- Entry point:
  `pub fn from_str<'de, T: Deserialize<'de>>(input: &'de str) -> Result<T, Error>`.
- Single-document (returns an error if multiple documents are present) — matches
  our usage.
- Public error type: `serde_saphyr::Error` — `#[non_exhaustive]` enum, derives
  `Debug` and `Display`, implements `std::error::Error`, `Send + Sync`.
- License: MIT OR Apache-2.0 (compatible with `cbsd-rs` AGPL-3.0-or-later).
- Routes through the standard `serde::de::Deserialize` machinery (the error enum
  has explicit
  `Serde{InvalidType,InvalidValue,UnknownVariant,UnknownField, MissingField,VariantId}`
  variants), so `#[serde(rename_all = "kebab-case")]`, `#[serde(default)]`,
  `#[serde(default = "...")]`, `Option<T>`, and nested structs all work as with
  any other serde data format.

The crate is pre-1.0 (`0.0.26`); `serde_yml` is also pre-1.0 (`0.0.12`). Trading
equivalent versioning posture; net direction is healthier.

## Proposed change

A single, mechanical replacement of the YAML parser. Aside from the dependency
declarations, the change is four edits.

### 1. Dependency declarations

**Approach (recommended):** hoist the new dep to `workspace.dependencies` since
two crates use it (matches the rust-2024 skill's guidance for shared deps; both
consumers want the same version of the YAML parser at all times).

```diff
 [workspace.dependencies]
 serde = { version = "1", features = ["derive"] }
 serde_json = "1"
+serde-saphyr = "0.0.26"
 chrono = { version = "0.4", default-features = false, features = ["serde", "clock"] }
```

```diff
 # cbsd-server/Cargo.toml
-serde_yml = "0.0.12"
+serde-saphyr.workspace = true
```

```diff
 # cbsd-worker/Cargo.toml
-serde_yml = "0.0.12"
+serde-saphyr.workspace = true
```

**Alternative (simpler, smaller diff):** keep the dep declared per-crate, no
workspace hoist:

```diff
-serde_yml = "0.0.12"
+serde-saphyr = "0.0.26"
```

I lean toward the workspace-hoisted form. It is the idiomatic placement, and
we're already touching both `Cargo.toml` files in this commit. The cost (three
lines added at the workspace level) is trivial; the benefit (single source of
truth for the YAML parser version) is real.

### 2. Call site swap (3 sites)

```diff
-        let parsed: ComponentYaml = serde_yml::from_str(&yaml_contents).map_err(|e| {
+        let parsed: ComponentYaml = serde_saphyr::from_str(&yaml_contents).map_err(|e| {
             std::io::Error::new(
                 std::io::ErrorKind::InvalidData,
                 format!("failed to parse {}: {e}", yaml_path.display()),
             )
         })?;
```

```diff
-    let config: ServerConfig = serde_yml::from_str(&contents)
+    let config: ServerConfig = serde_saphyr::from_str(&contents)
         .unwrap_or_else(|e| panic!("failed to parse config file {}: {e}", path.display()));
```

```diff
-        let config: WorkerConfig = serde_yml::from_str(&contents).map_err(ConfigError::Parse)?;
+        let config: WorkerConfig = serde_saphyr::from_str(&contents).map_err(ConfigError::Parse)?;
```

### 3. Error variant payload (1 site)

```diff
 #[derive(Debug)]
 pub enum ConfigError {
     Read(PathBuf, std::io::Error),
-    Parse(serde_yml::Error),
+    Parse(serde_saphyr::Error),
     Validation(String),
 }
```

`serde_saphyr::Error` is `#[non_exhaustive]`, but `ConfigError` only uses it
through `Display` (formatted via the parent's `Display` impl) and through
`std::error::Error::source`. Both impls are present on `serde_saphyr::Error`,
and we never `match` on its variants. `#[non_exhaustive]` is therefore a
non-issue in our usage.

### Total footprint

- Cargo.toml: +3 / −2 lines (workspace) + 1 / −1 lines (each consumer) ≈ 5 net
- Rust source: 4 line edits (3 call sites, 1 type reference)
- Cargo.lock: regenerated by `cargo build` (auto-generated, doesn't count)

This is well under the 200-line floor in the `/git-commits` skill, but the
change is intrinsically atomic: it cannot be split (the deps and the call sites
must change together to keep the workspace compiling), and reverting "swap the
YAML parser" is a meaningful, named operation. **One commit.**

## Risk analysis

### Already audited (no regressions expected)

1. **YAML 1.2 strictness vs. authoritative templates** —
   `cbsd-rs/systemd/templates/config/{server,worker}.yaml.in` and
   `components/ceph/cbs.component.yaml` are clean of every divergence class
   (Norway problem, leading-zero ints, sexagesimals, unquoted scalars containing
   `@:#`, anchors, custom tags, multi-document streams, duplicate keys). All
   scalars that need quoting are already quoted.

2. **`serde(default)` and `Option<T>`** — Both configs lean heavily on
   `#[serde(default)]` for omitted optional sections. `serde-saphyr` routes
   through standard serde derive plumbing (evidenced by the explicit
   `SerdeMissingField` / `SerdeUnknownField` error variants); behavior is the
   same as any other serde data format.

3. **`#[serde(rename_all = "kebab-case")]`** — Same story; this is a serde
   feature, not a format-specific feature. No special handling needed.

4. **Unknown fields** — `ComponentYaml` deserializes only the `name` field from
   a file that also contains `repo`, `build`, `containers`. Neither `serde_yml`
   nor `serde-saphyr` reject unknown fields by default; the struct does not opt
   into `#[serde(deny_unknown_fields)]`. Behavior preserved.

### Residual / monitor

5. **Pre-1.0 dependency.** `serde-saphyr 0.0.26` is in zerover. We mitigate by
   pinning the minor version (`"0.0.26"`) and revisiting on each upgrade. This
   is a strict improvement over the status quo (`serde_yml 0.0.12`, pinned the
   same way).

6. **Error message text changes.** Any operator-facing error message embedded in
   `ConfigError::Parse(...)` or the panic in `load_config` will look different.
   We don't pattern-match on these strings anywhere; no docs or tests assert on
   the exact wording. Acceptable.

7. **`Error::WithSnippet` variant.** `serde-saphyr` can produce snippet-style
   errors with file location annotations. By default `from_str` is unaware of
   the source file path; the resulting `Display` output should be a
   human-readable parse-error message comparable to `serde_yml`'s. We are not
   wiring up snippet-rich errors as part of this change.

8. **Field-name suggestions / fuzzy matching.** Some of `serde-saphyr`'s
   variants (e.g. `SerdeUnknownField`) hint at richer error messages than
   `serde_yml`. This is upside, not risk.

## Verification plan

Run, in order, from `cbsd-rs/`:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
SQLX_OFFLINE=true cargo check --workspace
cargo test --workspace
```

In addition, exercise the three real consumers:

1. **Worker config load** — `cbsd-worker` already has unit tests that exercise
   `WorkerConfig::load`. Confirm they pass.
2. **Server config load** — manually point `cbsd-server` at a copy of
   `cbsd-rs/systemd/templates/config/server.yaml.in` (with placeholder secrets
   filled in) and confirm `load_config` parses without panic. If we have a test
   fixture that already does this, leverage it; if not, a one-shot
   `cargo run -- --config <path> --print-config-and-exit` smoke is acceptable.
3. **Component discovery** — confirm `load_components(./components)` succeeds
   against `components/ceph/cbs.component.yaml` (only `name` is read). This can
   be done as part of the existing `cbsd-server` tests if any cover it, or via a
   quick manual `cargo run -- list-components`-style invocation if such a path
   exists. Otherwise, a tiny ad-hoc test reading the on-disk file is sufficient.

No `.sqlx/` cache regen is required — this change does not touch SQL.

## Commit plan

**One commit.** The change is atomic and irreducible: deps and call sites must
move together for the workspace to compile.

**Subject (≤72 chars):**

```
cbsd-rs: replace serde_yml with serde-saphyr for YAML parsing
```

**Body:**

```
Switch the YAML deserializer used by cbsd-server and cbsd-worker from
serde_yml — a single-maintainer fork of serde-yaml on top of libyml,
itself a fork of unsafe-libyaml — to serde-saphyr, a pure-Rust YAML 1.2
deserializer built on saphyr. Drops the libyml C-derived parser from
the tree and replaces it with an actively maintained library.

The change is mechanical: three call sites swap serde_yml::from_str for
serde_saphyr::from_str, and the Parse variant of cbsd-worker's
ConfigError carries serde_saphyr::Error in place of serde_yml::Error.
The shared dependency is hoisted to workspace.dependencies. No schema,
field, or behavior changes — verified by audit of the authoritative
config templates under cbsd-rs/systemd/templates/config/ and the
on-disk component manifest at components/ceph/cbs.component.yaml.
```

(Trailers — `Signed-off-by`, `Co-authored-by` — added by `git commit -s` flags /
hooks per the project's autonomous-commit conventions, not by the message text.)

## Rollback strategy

Mechanical revert: `git revert <commit>` restores `serde_yml` and the prior call
sites. There is no schema or persisted-state migration to undo, no config-file
format change, and no API surface change. Operators do not see any difference if
a rollback occurs mid-deploy: the YAML files they wrote under `serde_yml`
continue to parse under `serde-saphyr` (per the audit) and vice versa.

## Open questions for review

1. **Workspace-hoist vs. per-crate dep.** Recommended: hoist. Either is
   defensible; reviewer choose.
2. **Pin form.** `"0.0.26"` (caret-equivalent for zerover, allows patch bumps
   inside the same `0.0.x` line — though by Cargo's zerover semantics this is
   actually equivalent to `=0.0.26`, since `0.0.x` versions are mutually
   incompatible). Alternative: `"=0.0.26"` to be unambiguous. Reviewer choose.
3. **Snippet-style errors.** `serde-saphyr` supports rich `WithSnippet` errors.
   We are deliberately not wiring those up here to keep the change strictly
   mechanical, but it is a follow-up worth considering if operator error UX
   matters.
