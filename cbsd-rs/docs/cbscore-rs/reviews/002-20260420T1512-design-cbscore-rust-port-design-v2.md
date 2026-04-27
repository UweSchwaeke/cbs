# Design Review v2: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Prior review:** `002-20260420T1132-design-cbscore-rust-port-design-v1.md`

---

## Summary

**Verdict: approve-with-changes.**

All seven remaining v1 findings are fully closed. F6 (secrets
discriminator) is now handled per-family: git takes a deliberately
scoped wire-format break to `#[serde(tag = "type")]` — Rust
requires the new tagged shape with no compat path, and Python
keeps reading Rust-written files because pydantic ignores unknown
fields; signing uses `#[serde(tag = "type")]` directly; registry
uses a single-level `#[serde(tag = "creds")]`. No migration
subcommand ships. Two new issues need attention before
implementation, both in the bundled entrypoint script: it uses
`$*` where `"$@"` is correct, and `set -e` is present but not
load-bearing. Neither blocks M0 or M1 design approval.

---

## v1 Finding Verification

**F1 — Runner mount path.** CLOSED. Design lines 563-566 show
`exec "${RUNNER_PATH}/cbsbuild" ...` invoked by absolute path. The
mount table (line 527) correctly records `/runner/cbsbuild`. Both
design 001 (Runner Container section, line 528) and design 002 now
agree on the path and the new entrypoint script content is shown
verbatim. No residual gap.

**F2 — Timeout/cancellation contract.** CLOSED. Lines 795-833
specify the full two-case contract: (1) internal timeout fires →
`Child::start_kill()`, reap, return `Err(CommandError::Timeout)`;
(2) future dropped by outer cancellation → RAII guard kills child
in `Drop`. The runner cleanup path (lines 812-827) shows the exact
`match` on `CommandError::Timeout` that triggers `podman_stop`.
The prose is explicit that `async_run_cmd` owns its timeout entirely
and callers see a domain error, not a `Future::drop`. Fully
specified.

**F4 — `reset_python_env` PATH rewrite rule.** CLOSED. Lines
846-858 state the four-step exact-match rule derived from the
Python source: resolve `python3` via `which`, compute its parent,
bail early if parent is `/usr/bin`, otherwise drop PATH entries
that equal (not prefix-match) the parent dir. "Exact-match
pruning, not prefix-match" is stated explicitly.

**F5 — `--cbscore-path` CLI break.** CLOSED. Lines 1076-1084
record the flag drop as a deliberate, design-approved UX parity
break with explicit callout that operators must be notified and
that clap will emit "unexpected argument" on M1. Cross-reference
to 001 is present.

**F6 — Secrets model discriminator.** CLOSED. Option A
(`#[serde(tag = "type")]`) chosen for git — a deliberately
scoped wire-format break on git entries only. Rust requires the
new tagged shape on read; no compat path exists for shape-based
legacy YAML. Compatibility is one-way: Python can still read
Rust-written files because pydantic ignores unknown fields, but
Rust will not read files that lack the `type:` tag. Operators
moving a deployment from Python to Rust re-tag `secrets.yaml`
once by hand (M1 release notes include worked examples). Signing
uses `#[serde(tag = "type")]` directly (field already present in
deployed YAML); registry uses single-level `#[serde(tag =
"creds")]`. No migration subcommand ships.

**F7 — `VersionDescriptor` `rename_all`.** CLOSED. Lines 408-411
carry an explicit `// NO rename_all attribute here —` comment
explaining why snake_case is correct. Lines 429-435 add the
"Wire-format distinction across the codebase" block as a hard
invariant. Release-descriptor structs (lines 944-964) carry the
same `// NO rename_all` comments.

**F9 — Missing version helpers.** CLOSED. Lines 451-461 add
`get_major_version`, `get_minor_version`, and `normalize_version`
to `cbscore-types::versions::utils` with docstrings and Python
cross-references.

---

## New Findings

### N1 — Entrypoint script uses `$*` instead of `"$@"`

**Section:** "Runner Subsystem — Entrypoint script" (line 566).

The bundled script ends with:

```bash
exec "${RUNNER_PATH}/cbsbuild" \
  --config "${RUNNER_PATH}/cbs-build.config.yaml" ${dbg} \
  runner build $*
```

`$*` (unquoted) word-splits arguments containing spaces and joins
all positional args into a single IFS-delimited string. If
`cbsbuild runner build` is ever invoked with an argument that
contains a space (e.g. a descriptor path with a space, or a future
flag value), the argument will be silently split. The original
Python entrypoint also uses `$*` (line 58 of
`cbscore-entrypoint.sh`), so this is not a regression — but the
Rust design copies the wart into the new bundled script, where it
will persist indefinitely.

`"$@"` preserves argument boundaries exactly; there is no reason
to use `$*` here.

**Action:** Replace `$*` with `"$@"` in the bundled entrypoint
script. Low-risk change with correct semantics.

---

### N2 — `set -e` in the bundled entrypoint changes failure
         semantics vs. the Python original

**Section:** "Runner Subsystem — Entrypoint script" (line 551).

The Python original does **not** use `set -e`. It uses explicit
`|| exit 1` on the two commands that can fail (`uv tool install`
and the final `cbsbuild` invocation). The Rust entrypoint uses
`set -e` at the top instead.

In the new script there are only three statements after the
preamble: the `HOME` guard (always succeeds), the `dbg` assignment
(always succeeds), and the `exec` call. `exec` replaces the shell
process — if it fails (binary not found, permission denied), bash
exits with a non-zero status regardless of `set -e`. So in
practice `set -e` has no effect in the new script because every
statement either cannot fail or uses `exec`.

This means the `set -e` is not harmful, but it is also not load-
bearing, and it documents a behaviour (fail-fast) that is achieved
differently in the original. A future edit that adds a pre-exec
command (e.g. a `chmod`, a `mkdir`, a `test -x`) will have its
failure silently converted into a container exit with no error
message, which can be confusing during debugging.

**Action:** Either keep `set -e` and add a brief comment
explaining why it is safe here (all post-preamble commands either
cannot fail or use `exec`), or drop it to avoid setting an
expectation that any shell error will surface an error message.
This is a minor style point; either choice is defensible.

---

## Cross-Document Notes

**001 / 002 consistency.** Both documents were updated together and
are now consistent on the runner mount path (F1), the `--cbscore-
path` drop (F5), and the `cbscore-types` dependency graph (001's
F4: serde format crates removed from `cbscore-types`). No
cross-doc drift found on those axes.

**`cbscore-types` dependency list in 001.** After the 001 v1
review, `serde_yaml_ng` and `serde_json` were removed from the
`cbscore-types` Cargo sketch. Design 002 is consistent with this:
it places `Config::load`/`store` IO in `cbscore`, not in
`cbscore-types`. No drift.

**`anyhow` in `cbscore` library (001 F5).** Design 001 removed
`anyhow` from the `cbscore` library Cargo sketch. Design 002 is
silent on this (it was never in 002's scope) but its § "Error
Taxonomy" (lines 243-248) still says "Library code never uses
`anyhow`", consistent with 001's correction.

**`cbsd` consumer import table (001 F1).** Design 001 corrected
the import table to include `VersionDescriptor`, `ConfigError`, and
`logger.logger`. No drift between 001 and 002.

---

## Suggested Follow-ups

- Replace `$*` with `"$@"` in the bundled entrypoint (N1 —
  trivial fix, copy-paste the new script into the design).
- Add a one-line comment to `set -e` in the entrypoint explaining
  why it is safe, or drop it (N2 — minor, no functional impact
  in the current script).
