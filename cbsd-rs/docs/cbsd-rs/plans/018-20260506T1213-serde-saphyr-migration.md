# Plan 018 — `serde_yml` → `serde-saphyr` migration

- **Component:** `cbsd-rs` (`cbsd-server` + `cbsd-worker`)
- **Design:**
  [018 — serde-saphyr migration](../design/018-20260506T1015-serde-saphyr-migration.md)
- **Status:** Pending
- **Author:** Joao Eduardo Luis
- **Date:** 2026-05-06

## Decisions resolved from design open questions

The design proposed three open questions for review. Resolutions:

1. **Workspace-hoist vs. per-crate dep:** **Per-crate.** The workspace
   dependencies set is reserved for genuinely workspace-spanning deps; YAML
   parsing is consumer-specific. Each of `cbsd-server` and `cbsd-worker`
   declares `serde-saphyr = "0.0.26"` directly.
2. **Pin form:** **`"0.0.26"`** (caret form, not `=0.0.26`). For Cargo `0.0.x`
   deps this resolves to a single version anyway, but the form is friendlier to
   future bumps (one digit changes).
3. **Rich snippet errors:** **In scope for this change.** `from_str` already
   wraps errors with snippets via `Options::default()` (`with_snippet: true`),
   so the work is to make sure our own formatting renders the snippet legibly
   and that operators see a useful path-qualified context line.

## Single-phase plan

This is a one-commit change. The dependency swap, the call-site swap, the
error-variant payload swap, and the rich-error formatting tweaks form one
atomic, irreducible unit. Splitting them would create broken intermediate
commits.

| Phase   | Description                                                  | Commits | Status  |
| ------- | ------------------------------------------------------------ | ------- | ------- |
| Phase 1 | Replace `serde_yml` with `serde-saphyr`; surface rich errors | 1       | Pending |

### Phase 1 — replace `serde_yml` with `serde-saphyr`

#### 1.1 — Cargo.toml edits (per-crate)

`cbsd-rs/cbsd-server/Cargo.toml`:

```diff
-serde_yml = "0.0.12"
+serde-saphyr = "0.0.26"
```

`cbsd-rs/cbsd-worker/Cargo.toml`:

```diff
-serde_yml = "0.0.12"
+serde-saphyr = "0.0.26"
```

No changes to `cbsd-rs/Cargo.toml` (workspace root); `serde-saphyr` is not added
to `[workspace.dependencies]`.

#### 1.2 — `cbsd-server/src/components/mod.rs` (single call site)

Swap parser and reformat error so the rustc-like snippet renders on its own
lines.

```diff
-        let parsed: ComponentYaml = serde_yml::from_str(&yaml_contents).map_err(|e| {
+        let parsed: ComponentYaml = serde_saphyr::from_str(&yaml_contents).map_err(|e| {
             std::io::Error::new(
                 std::io::ErrorKind::InvalidData,
-                format!("failed to parse {}: {e}", yaml_path.display()),
+                format!("failed to parse '{}':\n{e}", yaml_path.display()),
             )
         })?;
```

#### 1.3 — `cbsd-server/src/config.rs` (single call site)

Swap parser and reformat the panic message similarly.

```diff
-    let config: ServerConfig = serde_yml::from_str(&contents)
-        .unwrap_or_else(|e| panic!("failed to parse config file {}: {e}", path.display()));
+    let config: ServerConfig = serde_saphyr::from_str(&contents)
+        .unwrap_or_else(|e| panic!("failed to parse config file '{}':\n{e}", path.display()));
```

#### 1.4 — `cbsd-worker/src/config.rs` (call site, error type, Display)

Three coupled edits:

1. Extend `ConfigError::Parse` to carry the file path so the worker's
   parse-error message gets the same path-qualified treatment as the server's.
   This is the only structural change and is required to render rich errors
   usefully on the worker side.

```diff
 #[derive(Debug)]
 pub enum ConfigError {
     Read(PathBuf, std::io::Error),
-    Parse(serde_yml::Error),
+    Parse(PathBuf, serde_saphyr::Error),
     Validation(String),
 }
```

2. Update the call site at `cbsd-worker/src/config.rs:136` to attach the path:

```diff
-        let config: WorkerConfig = serde_yml::from_str(&contents).map_err(ConfigError::Parse)?;
+        let config: WorkerConfig = serde_saphyr::from_str(&contents)
+            .map_err(|e| ConfigError::Parse(path.to_path_buf(), e))?;
```

3. Update the `Display` and `source` impls so the new payload is formatted
   (snippet on its own lines) and still chainable:

```diff
 impl std::fmt::Display for ConfigError {
     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
         match self {
             Self::Read(path, err) => {
                 write!(f, "failed to read config file '{}': {err}", path.display())
             }
-            Self::Parse(err) => write!(f, "failed to parse config YAML: {err}"),
+            Self::Parse(path, err) => {
+                write!(f, "failed to parse config file '{}':\n{err}", path.display())
+            }
             Self::Validation(msg) => write!(f, "config validation error: {msg}"),
         }
     }
 }

 impl std::error::Error for ConfigError {
     fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
         match self {
             Self::Read(_, err) => Some(err),
-            Self::Parse(err) => Some(err),
+            Self::Parse(_, err) => Some(err),
             Self::Validation(_) => None,
         }
     }
 }
```

Verify no other code constructs `ConfigError::Parse(...)` with the old
single-argument signature; if any do, update them.

#### 1.5 — Note: read vs parse formatting asymmetry is intentional

The `read` paths in §1.3 (`load_config`) and §1.4 (the `Read` arm of
`ConfigError`'s `Display` impl) keep their existing `{path}: {err}` form —
unquoted path, single-line, inline. Only the `parse` paths are reformatted to
`'{path}':\n{err}`. This divergence is intentional because the two error kinds
render different payload shapes:

- `std::io::Error` from `read_to_string` is a single-line message
  (`No such file or directory (os error 2)`) and reads naturally inline after
  the colon. Adding a leading newline would just push a one-liner onto its own
  line for no benefit.
- `serde_saphyr::Error` is a multi-line, rustc-style snippet whose
  `--> file:line:col` header and caret-aligned gutter rely on starting at
  column 1. Inlining the snippet after `: ` mangles that alignment; a leading
  newline preserves it. Path-quoting on the parse side helps disambiguate the
  path next to a multi-line body that no longer ends visually adjacent to it.

Standardising read/parse formatting is **not** in scope for this commit. We are
rendering different things; the formatting tracks the difference.

### Verification

Run from the `cbsd-rs/` directory in the order prescribed by the project's
pre-commit checks:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
SQLX_OFFLINE=true cargo check --workspace
cargo test --workspace
```

In addition, exercise the three real consumers (no `.sqlx/` regen needed — this
change does not touch SQL):

1. **Worker config load.** Run any existing `cbsd-worker` tests that exercise
   `WorkerConfig::load`. If none cover real YAML loading, add a minimal
   positive-path unit test in `cbsd-worker/src/config.rs`'s
   `#[cfg(test)] mod tests` that loads
   `cbsd-rs/systemd/templates/config/worker.yaml.in` (with placeholder values
   filled in) and asserts it deserializes.
2. **Server config load.** Smoke-load
   `cbsd-rs/systemd/templates/config/server.yaml.in` (placeholders filled in)
   through `cbsd-server::config::load_config`. If a test harness exists,
   leverage it; otherwise a one-off
   `cargo run --bin cbsd-server -- --config <path>` until the next step (e.g.
   failing OAuth) is acceptable evidence the parser succeeded.
3. **Component discovery.** Run `cbsd-server::components::load_components`
   against the on-disk `components/` directory and confirm
   `components/ceph/cbs.component.yaml` parses (only `name` is read, so no
   schema drift expected).
4. **Rich-error sanity check.** Manually corrupt one config file (for example,
   replace `listen-addr: "0.0.0.0:8080"` with `listen-addr: [bad]`) and confirm
   the produced error message includes a rustc-like snippet pointing at the
   offending line, with the file path on its own line. Revert the corruption.

### Pre-commit checklist

- [ ] `cargo fmt --all` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `SQLX_OFFLINE=true cargo check --workspace` clean
- [ ] `cargo test --workspace` passes
- [ ] All three consumer paths verified (worker config, server config, component
      discovery)
- [ ] Rich-error sanity check performed on a deliberately broken config
- [ ] `Cargo.lock` updated (auto-regenerated by `cargo build`)

### Commit message

Subject (≤72 chars):

```
cbsd-rs: replace serde_yml with serde-saphyr for YAML parsing
```

Body (≤80 cols):

```
Switch the YAML deserializer used by cbsd-server and cbsd-worker from
serde_yml — a single-maintainer fork of serde-yaml on top of libyml,
itself a fork of unsafe-libyaml — to serde-saphyr, a pure-Rust YAML
1.2 deserializer built on saphyr. Drops the libyml C-derived parser
from the tree and replaces it with an actively maintained library.

The dependency is declared per-crate in cbsd-server and cbsd-worker
(no workspace hoist). Three call sites swap serde_yml::from_str for
serde_saphyr::from_str, and cbsd-worker's ConfigError::Parse variant
is extended to carry the source file path alongside the underlying
serde_saphyr::Error so that operator-facing messages are
path-qualified.

serde-saphyr's from_str wraps errors with rustc-like snippets via
Options::default(). The Display formatting at the three error sites
puts the snippet on its own line so it renders legibly. No schema,
field, or behaviour changes — verified against the authoritative
config templates under cbsd-rs/systemd/templates/config/ and the
on-disk component manifest at components/ceph/cbs.component.yaml.
```

(Trailers — `Signed-off-by`, `Co-authored-by` — are added by `git commit -s`
flags / hooks per the project's autonomous-commit conventions, not by the
message text.)

## Rollback strategy

Mechanical revert: `git revert <commit>` restores `serde_yml`, the prior call
sites, and the prior `ConfigError::Parse` shape. There is no schema or
persisted-state migration to undo, no config-file format change, and no API
surface change visible to external consumers. Operators experience no observable
difference if a rollback occurs mid-deploy: configs written for `serde_yml`
continue to parse under `serde-saphyr` (per the audit) and vice versa.

## Out of scope (explicitly deferred — not in this commit)

- Customising `MessageFormatter` to use `UserMessageFormatter` for
  end-user-friendly wording. Default formatter is fine.
- Customising `Localizer`. English default is fine.
- Tightening to strict YAML 1.2 (disabling YAML 1.1 boolean forms in `Options`).
  Default leniency matches today's `serde_yml` behaviour.
- Adding YAML serialization. Nothing in `cbsd-rs` writes YAML.
- Hoisting `serde-saphyr` to `[workspace.dependencies]`.

## Progress

| Step                                            | Status  |
| ----------------------------------------------- | ------- |
| 1.1 Cargo.toml swap (server)                    | Pending |
| 1.1 Cargo.toml swap (worker)                    | Pending |
| 1.2 components/mod.rs swap + reformat           | Pending |
| 1.3 server/config.rs swap + reformat            | Pending |
| 1.4 worker/config.rs swap + Parse(PathBuf, ...) | Pending |
| Verification (cargo + manual smoke)             | Pending |
| Rich-error sanity check                         | Pending |
| Commit                                          | Pending |
