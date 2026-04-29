# Interactive `config init` for `cbsbuild`

## Status

**Deferred from M1.** This design specifies the interactive
`cbsbuild config init` UX that is intentionally out of scope for the M1
cbscore-rs port (see Open Question 8 in
[design 002](002-20260418T2120-cbscore-rust-port-design.md)). It is intended for
implementation post-M1, once the non-interactive flag modes are shipped and
stable.

## Overview

`cbsbuild config init` produces a `cbs-build.config.yaml` file (and optionally a
`cbs-build.vault.yaml`) by walking the user through a series of prompts. The
Python implementation lives in `cbscore/src/cbscore/cmds/config.py` and uses
`click.prompt`, `click.confirm`, and `click.prompt(hide_input=True)` for
password fields.

This design preserves the Python flow's UX while porting the implementation to
`dialoguer`.

## Goals

1. **UX parity with Python.** The same prompts in the same order, with the same
   defaults, the same yes/no confirmations, and the same password-hiding
   behaviour for vault credentials.
2. **Bypass parity.** The flag-based bypasses (`--for-systemd-install`,
   `--for-containerized-run`, and the per-field flags) skip prompting for any
   field they pre-fill. Running `cbsbuild config init` with all required values
   supplied via flags produces a config file with zero prompts (the same
   behaviour M1 ships).
3. **Testability.** Interactive code is notoriously hard to test; the design
   isolates the pure-data computation from the prompt IO so the data layer can
   be unit-tested without spawning a TTY.

## Non-Goals

- A full TUI (terminal user interface) with multi-line forms, cursor navigation,
  or live validation. `dialoguer` is a prompt-at-a-time library and that matches
  Python's flow.
- Replacing the flag-based bypass modes — they remain the canonical entry point
  for automation. Interactive mode is for workstation onboarding only.
- Reading values from environment variables. Anything not supplied via flags is
  prompted (or, post-M1, asked interactively).

## Library Choice: `dialoguer`

`dialoguer` is chosen over `inquire` because:

- It is sync-only, which fits `config init`'s sequential flow. Async via
  `inquire` adds complexity for no benefit — there is no IO concurrency to
  exploit when prompting one question at a time.
- It is widely used (millions of downloads), well-maintained, and stable.
- Its primitives map one-to-one to `click`'s primitives we depend on:
  - `click.prompt(...)` → `dialoguer::Input`
  - `click.prompt(..., hide_input=True)` → `dialoguer::Password`
  - `click.confirm(...)` → `dialoguer::Confirm`
- The crate is small enough that the binary cost is negligible for a one-shot
  setup command.

`inquire` was considered. It is async-aware and has a richer prompt set
(`Select`, `MultiSelect`, autocompletion). The existing flow does not use any of
those features, so `inquire`'s extra surface area is dead weight here.

## Prompt-by-Prompt Mapping

The Python implementation is the spec; this section maps each Python prompt to
its Rust equivalent. Defaults, fall-backs, and control flow remain identical.

### `cmd_config_init` entry point

| Step                                                                 | Python (`config.py`)                               | Rust (`cbscore::cmds::config`)                                              |
| -------------------------------------------------------------------- | -------------------------------------------------- | --------------------------------------------------------------------------- |
| 1. Pre-fill from `--for-systemd-install` / `--for-containerized-run` | Lines 435-441 (overwrite all path / secret fields) | Same: pre-fill the `Init` struct before any prompting; matches M1 behaviour |
| 2. Pre-fill from per-field flags                                     | `cmd_config_init` parameters                       | Same: clap `Args` struct populates the same fields                          |
| 3. Call `config_init(...)`                                           | Lines 251-309                                      | `config_init(...)` Rust function with the same shape                        |

### `config_init_paths`

Each entry below is a single user-visible prompt; bypass means the prompt is
suppressed when the corresponding CLI flag was supplied.

1. **Components path (default)** — if `${cwd}/components` exists, ask `Confirm`:
   "Use '${cwd}/components' as components path?"
   (`dialoguer::Confirm::default(true)`).
2. **Components paths (additional)** — `Confirm`: "Specify additional paths?"
   then loop on `Input::<String>` for each path; validate that the path exists
   and is a directory; loop until `Confirm`: "Add another?" returns `false`.
3. **Scratch path** — `Input::<String>`: "Scratch path".
4. **Scratch containers path** — `Input::<String>`: "Scratch containers path".
5. **ccache path (optional)** — `Confirm`: "Specify ccache path?" then
   `Input::<String>`: "ccache path".
6. **Versions path (optional)** — `Confirm`: "Specify versions path?" then
   `Input::<String>`: "Versions path". The field is `Config.paths.versions`
   (added by design 004); when unset, cbscore-rs falls back at runtime to
   `<git-root>/_versions` (per design 004 OQ2). Setting this decouples
   `cbsbuild versions create` from being inside a git checkout.

### `config_init_storage`

1. **Configure storage?** — `Confirm`: "Configure storage?". If no, return
   `None` and skip the rest.
2. **S3 storage** — `Confirm`: "Configure S3 storage for artifact upload?". If
   yes, prompt for: S3 URL (URL-validated), artifacts bucket, artifacts
   location, releases bucket, releases location.
3. **Registry storage** — `Confirm`: "Configure registry storage for container
   image upload?". If yes, prompt for registry URL (URL-validated).

URL-validated prompts wrap the `Input::<String>` in `dialoguer`'s
`validate_with` hook, calling `url::Url::parse` and re-asking the user on parse
failure (see § URL validation).

### `config_init_signing`

1. **Configure signing?** — `Confirm`. If no, return `None`.
2. **GPG signing** — `Confirm`: "Specify package GPG signing secret name?". If
   yes, `Input::<String>`: "GPG signing secret name".
3. **Transit signing** — `Confirm`: "Specify container image transit signing
   secret name?". If yes, `Input::<String>`: "Transit signing secret name".
4. **Skip if neither set** — if both `gpg_secret` and `transit_secret` are
   `None`, log "no signing methods specified, skipping" and return `None`.

### `config_init_secrets_paths`

1. **Specify secrets files?** — `Confirm`. If no, return empty list.
2. **First path** — `Input::<String>`: "Path to secrets file".
3. **Loop** — `Confirm`: "Specify additional secrets files?"; if yes,
   `Input::<String>` again; loop until `Confirm` returns `false`.

### `config_init_vault` (separate `init-vault` subcommand)

The prompts in this subsection fire from `cbsbuild config init-vault` — a
separate subcommand — **not** from `cbsbuild config init`. The primary
`config init` flow only records the vault config file _path_ (it stores
`Config.vault = <path>` and writes the main config); the vault address, auth
method, and credentials are gathered when the operator runs `init-vault` to
populate the vault file. The prompts here mirror Python `config_init_vault` in
`cbscore/cmds/config.py`.

0. **Skip if pre-configured.** If `vault_config_path` was supplied (via the
   `--vault` flag or by a `--for-systemd-install` / `--for-containerized-run`
   bypass mode) AND the file already exists on disk, return that path unchanged
   with no prompts. Mirrors Python `config_init_vault` lines 42-44. This is the
   path operators take on every re-run of `cbsbuild config init` against an
   already-configured deployment, and the bypass must be silent (no "Configure
   vault?" prompt) to preserve the muscle-memory of operators who currently see
   zero prompts in that scenario.
1. **Configure vault?** — `Confirm`. If no, return `None`.
2. **Vault config path** — `Input::<String>` with default
   `${cwd}/cbs-build.vault.yaml`.
3. **Overwrite existing?** — if the path exists, `Confirm`: "Vault config path
   '${path}' already exists. Overwrite?". If no, return the existing path
   unchanged.
4. **Vault address** — `Input::<String>`: "Vault address" (URL-validated; see §
   URL validation).
5. **Auth method** — `Confirm`: "Specify user/pass auth for vault?". If yes,
   prompt for username (`Input`) and password (`Password`).
6. **AppRole fallback** — if user/pass declined, `Confirm`: "Specify AppRole
   auth for vault?". If yes, prompt for role ID and secret ID.
7. **Token fallback** — if both declined, `Input::<String>`: "Vault token". If
   empty, exit with `EINVAL` matching Python.

### Final confirmation

1. **Suffix normalisation.** If the target `config_path` does not end in
   `.yaml`, rename it to use the `.yaml` extension
   (`Path::with_extension("yaml")` / `Utf8PathBuf::with_extension`) and echo a
   warning: `"config at '${old}' not YAML, use '${new}' instead."`. Mirrors
   Python `config_init` lines 283-288. This is load-bearing: `Config::load`
   picks the deserialiser by extension (YAML vs JSON), so writing YAML to a
   `.json`-named file would cause a parse failure on the next invocation.
   Non-interactive — a log line, not a prompt.
2. **Overwrite confirm.** If `config_path` already exists on disk, prompt
   `Confirm`: `"Config file exists, overwrite?"`. If no, echo
   `"do not write config file to '${path}'"` to stderr and exit with
   `ENOTRECOVERABLE`. Mirrors Python lines 290-292. This is separate from Step 4
   below; Python emits both prompts when the file exists.
3. **Print rendered config** — serialise the assembled `Config` to YAML and echo
   to stdout.
4. **Confirm write** — `Confirm`: `"Write config to '${path}'?"`. If no, exit
   with `ENOTRECOVERABLE` matching Python.
5. **Write file** — call `Config::store(path)`. `Config::store` creates the
   parent directory if needed (mirrors Python `config_path.parent.mkdir(...)` on
   line 302 — see design 002 § Configuration & Secrets / IO for the contract).
   On error, exit with `ENOTRECOVERABLE` and print the error to stderr.

### URL validation

Three prompts accept URLs: the S3 storage URL, the registry storage URL, and the
Vault address. Python cbscore accepts any string and surfaces malformed URLs
only when the SDK first tries to connect — typically during the next build, far
from the input that caused the problem. The Rust port validates URL shape at
prompt time using `url::Url::parse` via dialoguer's `validate_with` hook:

```rust
let url = Input::<String>::new()
    .with_prompt("Vault address (incl. scheme, e.g. https://...)")
    .validate_with(|s: &String| -> Result<(), String> {
        url::Url::parse(s)
            .map(|_| ())
            .map_err(|e| format!("invalid URL: {e}"))
    })
    .interact()?;
```

The validator catches missing-scheme / typo cases (`htps://`, `vault.local`
without a scheme, etc.) and re-prompts in-place. Semantic validity (host
reachable, port correct, TLS cert valid) is **not** checked here; that remains
the SDK's job at first connect. The point is to fail fast on syntactic typos,
not to verify deployment.

The validation logic lives in a free function:

```rust
pub fn validate_url(s: &str) -> Result<(), String> {
    url::Url::parse(s)
        .map(|_| ())
        .map_err(|e| format!("invalid URL: {e}"))
}
```

Each URL prompt's `validate_with` closure simply delegates:
`|s: &String| validate_url(s)`. The function is unit-tested directly,
parametrised over a list of well-formed and malformed URLs — that is the
load-bearing test for URL validation. The `Prompter` trait does **not** thread
the validator through `input()`; widening the trait to carry a validator at
every call site adds surface area for the three URL prompts and zero benefit for
the ~20 other text prompts. The trade-off: `ScriptedPrompter` (used in
prompt-flow unit tests) does not invoke `validate_url`, so scripts must supply
pre-validated answers — invalid URLs in test scripts won't trigger a re-prompt
loop. That is acceptable because the URL validator is exercised directly by its
own tests; the prompt-flow tests cover ordering / defaults / skip logic, not URL
parsing.

## Module Layout

```
cbsbuild/src/cmds/config/
  mod.rs              # clap subcommand definitions
  init.rs             # `config init` implementation
  init_vault.rs       # `config init-vault` implementation
  prompts.rs          # thin dialoguer wrappers + a Prompter trait
                      # for testability + the free validate_url fn
```

The `Prompter` trait abstracts every interactive call so unit tests can swap in
a scripted prompter:

```rust
pub trait Prompter {
    fn input(&mut self, label: &str, default: Option<&str>) -> Result<String, PromptError>;
    fn password(&mut self, label: &str) -> Result<String, PromptError>;
    fn confirm(&mut self, label: &str, default: bool) -> Result<bool, PromptError>;
}

pub struct DialoguerPrompter;
impl Prompter for DialoguerPrompter { /* delegates to dialoguer */ }

#[cfg(test)]
pub struct ScriptedPrompter {
    answers: VecDeque<PromptAnswer>,
}
```

The `config_init` function takes `&mut dyn Prompter`, so the same function is
exercised in unit tests with a `ScriptedPrompter` and in production with
`DialoguerPrompter`. All sub-functions in the call chain (`config_init_paths`,
`config_init_storage`, `config_init_signing`, `config_init_secrets_paths`,
`config_init_vault`) take the same `&mut dyn Prompter` so the seam is uniform.

## Testing

Three layers:

1. **Unit tests for the data assembly** (no prompting): build a `Config` from a
   fully-pre-filled `Init` struct; assert the resulting YAML matches an expected
   snapshot. This covers the "all flags supplied" path that M1 already
   implements.
2. **Unit tests for the prompt flow** (scripted prompter): script answers in a
   `VecDeque`, run `config_init` with a `ScriptedPrompter`, assert the resulting
   `Config` and that the prompter was called in the expected order with the
   expected labels. This catches regressions in prompt order, defaults, and skip
   logic. Scripts supply pre-validated answers; URL validation is exercised
   separately (layer 3) — see § URL validation.
3. **Unit tests for `validate_url`** (pure function): parametrised over a list
   of well-formed URLs (`https://...`, `http://localhost:9000`, `s3://bucket`,
   etc.) that must accept and a list of malformed inputs (`htps://`,
   `vault.local`, empty string, etc.) that must reject with a usable error
   message. Independent of the prompt flow.

No TTY-driving integration test is part of this design — the `Prompter` seam
plus the scripted prompter cover the prompt flow without the flakiness that
real-pty tests would introduce on CI.

## Bypass Behaviour (consistent with M1)

The flag-based bypass modes ship in M1; this design preserves them unchanged
when interactive mode lands:

- `--for-systemd-install`: pre-fill paths for the systemd worker layout
  (`/cbs/components`, `/cbs/scratch`, `/cbs/_versions` for
  `Config.paths.versions` per design 004 OQ7, etc.) and write to
  `~/.config/cbsd/${deployment}/worker/cbscore.config.yaml`.
- `--for-containerized-run`: pre-fill the same paths as systemd-install but
  write to the user-supplied `--config` path.
- Per-field flags: `--components`, `--scratch`, `--containers-scratch`,
  `--ccache`, `--versions-dir` (design 004), `--vault`, `--secrets`. Any field
  supplied via a flag skips its corresponding prompt.
- `--vault <path>` is special: when the supplied path already exists on disk,
  the entire vault flow (the `config_init_vault` Step 0 short-circuit) is
  skipped — no prompts, no overwrite confirmation. Operators re-running
  `cbsbuild config init` against an already-configured deployment see zero
  prompts on the vault side, matching Python.

After this design ships, running `cbsbuild config init` with no flags activates
the interactive flow. Until then, M1 errors out with a usage hint pointing the
user at the flag modes.

## Migration from M1 to This Design

When this design is implemented:

1. Add `dialoguer` and `url` to the `cbsbuild` crate dependencies (`url` is
   already a `cbscore` dep but is needed at the binary level for prompt-time
   validation; depending on the cbscore re-export is also acceptable).
2. Add the `prompts.rs` module with the `Prompter` trait and `DialoguerPrompter`
   impl.
3. Replace the M1 "no flags → error" branch in `cmd_config_init` with a call
   into the new interactive flow.
4. Add unit and scripted-prompter tests.
5. Update `cbsbuild config init --help` text to describe the interactive mode.

No on-disk format change. No CLI flag deprecations. No breaking change for
automation that uses the flag-based modes.

## Resolved Decisions

- **ccache prompt default: no.** The Python flow asks "Specify ccache path?"
  with default **no**, and the Rust port preserves that behaviour. Operators who
  want ccache opt in either via the prompt or by passing `--ccache <path>` on
  the command line; the `--for-systemd-install` and `--for-containerized-run`
  bypass flags continue to pre-fill `/cbs/ccache`. Default-yes was considered
  and rejected: muscle-memory parity with Python cbscore wins over the
  build-speedup convenience for the (small) interactive workstation cohort.
- **URL validation: yes, via `url::Url::parse`.** The S3 storage URL, registry
  storage URL, and Vault address prompts all use dialoguer's `validate_with`
  hook to call `url::Url::parse` and re-prompt on syntactic failure. See § URL
  validation above for the rationale and the contract (catch typos at prompt
  time, not semantic deployment errors).
