# seq-003 — Interactive `cbsbuild config init` + `init-vault`

## Status

**Drafted — under design-review.** Implements design 003
(`design/003-20260427T1255-interactive-config-init.md`). Lands post-M2 as a
backwards-compatible additive change against the M1-1.0.0 baseline — operators
using the existing `--for-systemd-install` / `--for-containerized-run` bypass
modes or the per-field override flags see the same flow as today; operators who
run `cbsbuild config init` with no flags previously got an error pointing at the
bypass modes and will now get an interactive prompt flow. cbscore-rs stays at
1.0.0 (no crate-version bump for additive features; same posture as seq-004 +
seq-005).

One design–code mismatch surfaced during drafting was recorded as an **Open
Question** below; it is now resolved (see OQ-A's body for the chosen path). The
plan reflects the decision; the remainder of the design maps cleanly to three
commits on the post-seq-005 main.

**Review trail:**

- Plan drafted 2026-05-22 against design 003 v1.
- OQ-A resolved 2026-05-22: per-template `versions` pre-fill matching the
  template's existing filesystem prefix (`systemd_install_template` →
  `/var/lib/cbsd/_versions`; `containerized_run_template` → `/cbs/_versions`).
  Amend design 004 §OQ7 to drop the literal `/cbs/_versions` claim for both
  templates and say "matches the template's existing prefix" instead.
- v2 design-review pass (2026-05-22) — verdict "needs another rework pass".
  Findings closed: MAJOR-1 (design 003 §Bypass Behaviour + §Resolved Decisions
  ccache note carried stale `/cbs/...` prefixes for the systemd template — never
  amended in the OQ-A sweep, which only touched design 004; both passages
  rewritten to per-template prefix-matching), MINOR-1 (`PromptError::Cancelled`
  is not producible from `dialoguer 0.11.0`'s error type — renamed to
  `EmptyScript`, documented as test-only, reframed the "Ctrl+C / EOF" claim),
  MINOR-2 (Commit 2 vault-file-write hedge resolved —
  `cbscore_types::config::vault::VaultConfig` + `VersionedVaultConfig` exist and
  are the right shape; Commit 2 now also adds a new
  `cbscore::config::store_vault` write function mirroring `store`), SUGGESTION-1
  (Commit 3 §Files gains `InitArgs` clap doc-comment update + CHANGELOG entry
  for the no-flags-path behaviour change), SUGGESTION-2 (§Status flipped to
  "Drafted — under design-review").
- v3 design-review pass (2026-05-22) — verdict "approve with MINOR cleanups".
  Findings closed: MINOR-1 (`EmptyScript` payload contradiction between §Files
  and §Testable — picked the unit-variant + scripted-prompter recorded-calls log
  shape; both sections now agree the variant carries no payload and the test
  inspects the log to identify which prompt was expected next), MINOR-2 (Commit
  3 step 9 called `serde_saphyr::to_string` but `cbsbuild` doesn't depend on
  `serde-saphyr` — added `serde-saphyr = "0.0.24"` to `cbsbuild/Cargo.toml` in
  §Commit 3 §Files; the YAML preview matches design 003 §Final confirmation step
  3 and Python parity). v3 also confirmed OQ-A cross-document consistency and
  reviewed the existing CHANGELOG hedge in Commit 3 §Files — no CHANGELOG.md
  exists in the project, so the implementer captures the behaviour change in the
  final commit message; the hedge text is correct as-is.
- Implementation-time finding (2026-05-22) — `url::Url::parse` accepts arbitrary
  scheme strings as syntactically valid, so the v1-design claim that
  `validate_url("htps://typo.example.com")` would be rejected was wrong. The
  validator catches missing-scheme / empty / garbled inputs but cannot
  distinguish scheme typos without a scheme allowlist. Fixed: design 003 §URL
  validation rewritten to drop the "typo cases" framing and add a "Documented
  limitation" block; plan §Commit 1 §Testable flipped the `htps://` case to
  `Ok(())`. Scope-control: deliberately did not introduce a scheme allowlist —
  that's a follow-up if scheme typos prove a recurring foot-gun.

## Progress

| #   | Commit                                                                            | ~LOC | Status  |
| --- | --------------------------------------------------------------------------------- | ---- | ------- |
| 1   | `cbsbuild config: add prompts module (Prompter trait + dialoguer + validate_url)` | ~250 | Pending |
| 2   | `cbsbuild config: add init-vault subcommand`                                      | ~400 | Pending |
| 3   | `cbsbuild config init: interactive flow + bypass versions pre-fill`               | ~500 | Pending |

**Estimate:** ~1150 LOC, 3 commits.

## Goal

Replace the M1 "no flags → error" branch in `cbsbuild config init` with the
interactive prompt flow design 003 specifies, and add the matching
`cbsbuild config init-vault` subcommand. After this seq:

- `cbsbuild config init` with no flags walks the operator through every field
  via `dialoguer` prompts (UX parity with Python `click.prompt` / `confirm` /
  `Password`).
- `cbsbuild config init --for-systemd-install` (or `--for-containerized-run`)
  with no per-field overrides skips every prompt and writes the bypass template
  directly, byte-identical to today's M1 behaviour.
- `cbsbuild config init --for-systemd-install --components /custom` (any mix of
  bypass + per-field) skips prompts for every pre-filled field but prompts for
  fields neither the bypass nor the per-field flags set.
- `cbsbuild config init-vault [--vault PATH]` writes a `cbs-build.vault.yaml`
  file via prompts for the vault address, auth method (user/pass / AppRole /
  token), and credentials.
- The bypass templates' `paths.versions` pre-fill (design 004 OQ7 step 5,
  deferred from seq-004) flips from `None` to `Some("/cbs/_versions")` for
  containerized and `Some("/var/lib/cbsd/_versions")` for systemd-install (see
  OQ-A below for the exact systemd-install value).

The data assembly is isolated from the prompt IO via a `Prompter` trait so the
flow can be unit-tested without spawning a TTY (design 003 §Testing).

## Depends on

- **seq-002 Phase 6 Commit 4** — `cbsbuild config init` exists with the
  `--for-*` bypass modes + per-field overrides + the "no flags → error" branch
  that Commit 3 of this plan replaces.
- **seq-002 Phase 3 Commit 4** — `cbscore::config::store(&Config, path)` exists
  and creates the parent directory if needed (`cbscore/src/config.rs:143–150`,
  verified). The Python flow's
  `config_path.parent.mkdir(exist_ok=True, parents=True)` step (line 302) is
  therefore a no-op in the Rust port — `Config::store` already does it.
- **seq-004 Commit 1** — `PathsConfig.versions: Option<Utf8PathBuf>` field
  exists. Commit 3 of this plan flips the bypass templates' `versions: None` to
  `Some(_)` per design 004 OQ7 step 5.
- **seq-005** — no direct dependency, but seq-005 introduced the
  `validate_version` pattern (validator as a free function + unit tests) that
  this plan's `validate_url` mirrors.
- **dialoguer crate** — new dependency for `cbsbuild`. The design picks
  dialoguer over inquire because the existing Python flow is sync-only
  prompt-at-a-time and dialoguer maps 1:1 to `click`'s primitives (`Input`,
  `Password`, `Confirm`).
- **url crate** — new dependency for `cbsbuild` (already a `cbscore` dep for
  git/s3 url parsing). Used by `validate_url` to catch syntactically-malformed
  URL inputs at prompt time.

Design references: design 003 (this plan implements its §Migration table) and
design 004 §OQ7 (bypass-mode `versions` pre-fill).

## Open Questions

### OQ-A — bypass-template `versions` pre-fill values — **RESOLVED**

**Resolution: per-template `versions` value matching the template's existing
filesystem prefix.**

- `systemd_install_template()` →
  `paths.versions: Some("/var/lib/cbsd/_versions".into())`. Matches the
  template's existing `/var/lib/cbsd/...` prefix on every other path field
  (`scratch`, `ccache`, `scratch_containers`, `secrets`, `vault`).
- `containerized_run_template()` →
  `paths.versions: Some("/cbs/_versions".into())`. Matches the template's
  existing `/cbs/...` prefix.

This deviates from design 004 OQ7's literal claim that both templates pre-fill
`/cbs/_versions`, but the deviation preserves the systemd template's
filesystem-prefix consistency — design 004 OQ7's spirit was "symmetric with the
other path fields", which here means per-template prefix-matching rather than a
single shared literal.

**Required design amendment (queued for the design-review sweep below).** Design
004 §OQ7 needs updating: drop the literal `/cbs/_versions` claim for both
templates and say "matches the template's existing prefix" instead. The seq-003
plan's §Commit 3 §Files entry already records the two per-template values;
design 004 just needs to retroactively name the rule those values follow.

**Background.** The plan-drafting sweep found that the two bypass templates
(`systemd_install_template`, `containerized_run_template`) use DIFFERENT
filesystem prefixes for every existing path field — `/var/lib/cbsd/` for systemd
and `/cbs/` for containerized. Pre-filling `versions` to the same literal
`/cbs/_versions` in both, as design 004 OQ7 reads literally, would mix prefixes
inside the systemd template (an operator running `--for-systemd-install` would
get `scratch: /var/lib/cbsd/scratch` and `versions: /cbs/_versions` in the same
file). Per-template prefix-matching avoids that surprise.

**Why not OQ-A.2 (literal `/cbs/_versions` in both).** Faithful to the design as
written, but creates the operational inconsistency described above. The design's
wording was set down before the two templates diverged on prefix; literal
application no longer reflects the design's intent.

**Why not OQ-A.3 (defer; always prompt).** Loses the "zero prompts in bypass
mode" property pinned by design 003 §Goals second bullet, which operators using
`--for-systemd-install` / `--for-containerized-run` on fully-automated
provisioning depend on.

## Sequencing

Three commits, ordered. Each is individually compilable + testable + does not
require subsequent commits to make sense:

1. **Commit 1** (prompts infrastructure) — adds the `prompts` module with
   `Prompter` trait, `DialoguerPrompter` impl, `ScriptedPrompter` (test-only)
   struct, and the free `validate_url` function. Adds `dialoguer` + `url` to
   `cbsbuild`'s `[dependencies]`. No callers yet; functions are unit-tested in
   isolation (~250 LOC).
2. **Commit 2** (init-vault subcommand) — adds `cbsbuild config init-vault` as a
   new `ConfigCommand::InitVault` variant. Implements
   `config_init_vault(&mut dyn Prompter, vault_config_path: Option<...>) -> ...`
   and the matching `cbscore::config::store_vault` write helper (mirrors
   `cbscore::config::store`). Vault file shape is the existing
   `cbscore_types::config::vault::VaultConfig` + `VersionedVaultConfig` pair.
   Scripted-prompter tests cover the prompt-flow / auth-method branches (~400
   LOC including the `store_vault` helper).
3. **Commit 3** (interactive config init + bypass pre-fill) — replaces the "no
   flags → error" branch in `handle_init` with the interactive flow. Adds the
   `config_init` + `config_init_paths` / `config_init_storage` /
   `config_init_signing` / `config_init_secrets_paths` sub-functions. Flips
   bypass templates' `paths.versions` from `None` to `Some(_)` per OQ-A.1. Adds
   `--versions-dir` per-field override (matches the seq-004 flag's shape).
   Scripted-prompter tests for the full flow (~500 LOC).

Splitting into three commits **does not** create broken intermediates, because:

- Commit 1's `Prompter` trait + `DialoguerPrompter` are dead code until Commit
  2 + 3 wire them up. Dead code with unit tests compiles fine; no
  `clippy::dead_code` triggers because the test module is a caller.
- Commit 2's `init-vault` subcommand is independent of the `init` subcommand.
  After Commit 2 lands, `init-vault` works; `init` is still the M1-shape "no
  flags → error" + bypass templates.
- Commit 3 turns the "no flags → error" branch into the interactive flow. After
  Commit 3, both `init` and `init-vault` are fully operational.

Visibility decisions for the new symbols (per CLAUDE.md §Visibility):

- `Prompter` trait, `DialoguerPrompter`, `validate_url` — `pub(crate)`
  initially. The only callers are inside `cbsbuild`. Promote to `pub` if a
  future cross-crate caller materialises.
- `ScriptedPrompter` — `pub(crate)` under `#[cfg(test)]`. Test-only.
- `PromptAnswer` enum (for `ScriptedPrompter`'s answer queue) — same.
- `ConfigCommand::InitVault` variant — `pub(crate)` like the existing `Init`,
  `Show`, `Check` variants.
- `config_init_vault` + `config_init` + sub-functions — `pub(crate)`, callable
  across the `cmds::config` submodule but not the crate.

## Out of scope

- **Full TUI** with multi-line forms, cursor navigation, live validation. Per
  design 003 §Non-Goals — `dialoguer` is prompt-at-a-time and that matches
  Python's flow.
- **Env-var fallback for prompted values.** Per design 003 §Non-Goals — anything
  not supplied via a flag is either prompted (interactive) or fails (bypass mode
  that didn't pre-fill it).
- **Replacing the flag-based bypass modes.** Per design 003 §Non-Goals and
  §Bypass Behaviour — the flags remain the canonical automation entry point.
  Interactive mode is for workstation onboarding only.
- **Real-pty integration tests.** Per design 003 §Testing — the `Prompter`
  seam + scripted prompter cover the prompt flow without the flakiness of
  pty-driving tests on CI.
- **`init-vault --interactive` flag or similar mode-switch.** The `init-vault`
  subcommand is always interactive (matches Python's `cmd_config_init_vault`).
  Operators who want non-interactive vault setup author the vault YAML by hand.
- **dialoguer Theme customisation.** Uses dialoguer's default theme (the
  colored-output `ColorfulTheme` when stdout is a TTY, plain otherwise). Matches
  Python `click`'s default; no special opt-in.

## Commit 1 — `cbsbuild config`: add prompts module (Prompter trait + dialoguer + validate_url)

Pure-infrastructure commit. The `Prompter` trait, `DialoguerPrompter` impl,
`ScriptedPrompter` test struct, and `validate_url` free function land here; no
Commits 2 / 3 callers are wired up yet.

**Files:**

- `cbsd-rs/cbsbuild/Cargo.toml` — add to `[dependencies]`:
  - `dialoguer = "0.11"` (no default features needed for the `Input` /
    `Password` / `Confirm` primitives we use; `fuzzy-select` and `history`
    features are not used).
  - `url = "2"` — `cbscore` already depends on this transitively, but `cbsbuild`
    doesn't pull it in directly; adding it explicitly keeps the `validate_url`
    call site honest about its dep graph.
- `cbsd-rs/cbsbuild/src/cmds/config.rs` → `cbsd-rs/cbsbuild/src/cmds/config/`
  (file-to-module conversion):
  - Move `config.rs` → `config/mod.rs` (preserves the existing `ConfigCommand`,
    `InitArgs`, `handle`, `handle_init`, `handle_show`, `handle_check`,
    `systemd_install_template`, `containerized_run_template`,
    `validate_config` + existing tests).
  - Per CLAUDE.md `mod_module_files = "warn"` clippy lint: prefer `config.rs` +
    `config/` submodules over `config/mod.rs`. Re-check the convention against
    the existing `cbsbuild/src/cmds/` layout (which uses `versions.rs` +
    `versions/` for its single submodule case — or actually doesn't, all of
    `versions`, `build`, `runner`, `advanced`, `config` are single files today).
    On this conversion: keep `config.rs` at the top level and add a new
    `config/` subdirectory only if the module path resolves cleanly under Rust
    2024's mod-file rules. Detail to confirm at implementation time; the
    existing single-file shape can stay if submodule splitting is fragile.
- `cbsd-rs/cbsbuild/src/cmds/config/prompts.rs` — new file:
  - `pub(crate) trait Prompter` with three methods:
    - `fn input(&mut self, label: &str, default: Option<&str>) -> Result<String, PromptError>`
      — bare-string input.
    - `fn password(&mut self, label: &str) -> Result<String, PromptError>` —
      hide-input password prompt.
    - `fn confirm(&mut self, label: &str, default: bool) -> Result<bool, PromptError>`
      — yes/no confirmation with explicit default.
  - `pub(crate) struct DialoguerPrompter;` —
    `impl Prompter for DialoguerPrompter` delegates each method to the
    corresponding `dialoguer` primitive
    (`Input::<String>::new().with_prompt(label) .default(default.to_owned()).interact()`,
    etc.).
  - `pub(crate) enum PromptError` — two variants:
    - `Io(std::io::Error)` — the only variant `DialoguerPrompter` ever produces.
      `dialoguer 0.11.0`'s `Error` enum has exactly one variant (`IO(IoError)`)
      and the impl maps it 1:1.
    - `EmptyScript` — produced only by `ScriptedPrompter` when its answer queue
      is exhausted. Unit variant (no payload). `Display` emits a fixed string
      such as `"ScriptedPrompter: answer queue exhausted"`. The label of the
      prompt the script was missing is recorded separately in
      `ScriptedPrompter`'s recorded-calls log (one entry per prompt the
      production code invoked, kept across the empty-queue point); tests inspect
      that log to identify which prompt was expected next. `DialoguerPrompter`
      never produces this variant. Documented in the variant's `///` doc-comment
      as "test-only signal; production paths do not surface this".
      `impl std::error::Error + std::fmt::Display` via the existing `thiserror`
      pattern. The original "Ctrl+C / EOF" framing was inaccurate — dialoguer
      0.11 does not intercept signals; Ctrl+C terminates the process at the OS
      level without coming back through `dialoguer::Error`.
  - `pub(crate) fn validate_url(s: &str) -> Result<(), String>` — free function
    calling
    `url::Url::parse(s).map(|_| ()).map_err(|e| format!("invalid URL: {e}"))`.
    Unit-tested directly per design 003 §URL validation; not threaded through
    the `Prompter` trait.
  - `#[cfg(test)] pub(crate) struct ScriptedPrompter` — `VecDeque<PromptAnswer>`
    answer queue + a recorded-calls log for test assertions. Three methods on
    `Prompter` for the trait impl; the scripted prompter pops the next answer
    from the queue and asserts the type matches (input / password / confirm).
  - `#[cfg(test)] pub(crate) enum PromptAnswer` — `Input(String)` /
    `Password(String)` / `Confirm(bool)`. The scripted prompter's queue is
    `VecDeque<PromptAnswer>`; mismatched type returns a panic-style error so
    tests fail loudly.

**Design constraints:**

- **`Prompter` trait stays minimal.** Three methods, no validator threading. Per
  design 003 §URL validation, threading `validate_with` through `input()` would
  widen the trait for three URL prompts and zero benefit for the ~20 other
  prompts. The trade-off (scripted prompter doesn't invoke `validate_url`) is
  explicitly accepted — URL validation has its own unit tests.
- **`DialoguerPrompter` has no state.** Empty struct. Each call constructs a
  fresh `dialoguer::Input` / `Password` / `Confirm`. Matches dialoguer's
  intended usage and keeps the impl trivially thread-safe (though the prompt
  flow is sync and single-threaded).
- **Theme.** Uses dialoguer's default theme (auto-detects TTY, picks
  `ColorfulTheme` or plain). No explicit theme construction; consistent with
  Python `click`'s default ANSI behaviour.
- **No async.** The trait + impls are all sync. The flow is prompt-at-a-time; no
  IO concurrency to exploit. The eventual `handle_init` caller is `async fn` but
  the prompt flow runs inside it synchronously (the awaits are around
  `Config::store` / `Config::load`, not around prompts).
- **`#[cfg_attr(not(test), allow(dead_code))]`** is **not** applied to
  `Prompter` / `DialoguerPrompter` / `validate_url`. The unit tests in this
  commit exercise them; the additional non-test callers land in Commits 2 + 3 —
  no intermediate dead-code state.

**Testable:**

- Unit test: `validate_url("https://example.com")` returns `Ok(())`.
- Unit test: `validate_url("https://localhost:9000")` returns `Ok(())`.
- Unit test: `validate_url("s3://my-bucket/prefix")` returns `Ok(())` (URL crate
  accepts non-http schemes).
- Unit test: `validate_url("htps://typo.example.com")` returns `Ok(())` — pins
  down the documented limitation that `url::Url::parse` accepts arbitrary scheme
  strings as syntactically valid (so scheme-typos pass this check and surface
  only at SDK connect time). See design 003 §URL validation "Documented
  limitation" block. If a future revision adds a scheme allowlist, flip this
  case to `Err(_)` in lockstep with the spec change.
- Unit test: `validate_url("not a url at all")` returns `Err(_)`.
- Unit test: `validate_url("")` returns `Err(_)`.
- Unit test: `validate_url("vault.local")` returns `Err(_)` (the no-scheme case
  that the design 003 §URL validation block names explicitly as the "missing
  scheme" trip-wire).
- Unit test: a `ScriptedPrompter` initialised with
  `[PromptAnswer::Input("foo".into()), PromptAnswer::Confirm(true)]` drives two
  `Prompter` calls correctly and the recorded-calls log contains both prompt
  labels in order.
- Unit test: a `ScriptedPrompter` with an empty queue returns
  `PromptError::EmptyScript` on the next call. The test then inspects the
  scripted prompter's recorded-calls log to confirm the production code invoked
  the expected prompts up to the exhaustion point — the log lets the test name
  which prompt was about to fire when the queue ran out, without the error
  variant having to carry a payload.
- Unit test: a `ScriptedPrompter` receiving a type mismatch (queue has
  `Input("...")` but caller invokes `confirm(...)`) panics — type mismatch is
  always a test-script bug, never a runtime concern, so panic-on-mismatch is the
  right shape (fail loudly).

## Commit 2 — `cbsbuild config`: add `init-vault` subcommand

Adds the `cbsbuild config init-vault` subcommand. Independent of `init`'s
interactive flow (Commit 3) — `init-vault` always runs interactive prompts to
populate a `cbs-build.vault.yaml`. After this commit,
`cbsbuild config init-vault --vault /path/to/vault.yaml` works end-to-end.

**Files:**

- `cbsd-rs/cbsbuild/src/cmds/config/mod.rs` (or the parent `config.rs` depending
  on Commit 1's module-layout choice):
  - Add `ConfigCommand::InitVault(InitVaultArgs)` variant.
  - Add
    `pub(crate) struct InitVaultArgs { vault_config_path: Option<Utf8PathBuf> }`
    (clap derive; `--vault PATH`, matches Python `cmd_config_init_vault`'s
    `--vault` option).
  - Add
    `ConfigCommand::InitVault(args) => handle_init_vault(args, config_path).await`
    arm to the `handle` dispatcher.
- `cbsd-rs/cbsbuild/src/cmds/config/init_vault.rs` — new file:
  - `pub(crate) async fn handle_init_vault(args: InitVaultArgs, config_path: &Utf8Path) -> Result<()>`
    — top-level handler. Constructs a `DialoguerPrompter`, calls
    `config_init_vault(&mut prompter, ...)`, prints the resulting path (or
    "vault configuration not initialized" if `None`).
  - `pub(crate) async fn config_init_vault<P: Prompter>(prompter: &mut P, cwd: &Utf8Path, vault_config_path: Option<&Utf8Path>) -> Result<Option<Utf8PathBuf>>`
    — the testable, prompter-driven flow. Mirrors Python `config_init_vault`
    (`cbscore/cmds/config.py` lines 40–105) step-by-step:
    - Step 0: if `vault_config_path.is_some() && vault_path.exists()`, return
      `Some(vault_path.to_owned())` unchanged. No prompts.
    - Step 1: `prompter.confirm("Configure vault authentication?", false)`. On
      false, return `Ok(None)`.
    - Step 2:
      `prompter.input("Vault config path", Some(&cwd.join("cbs-build.vault.yaml").to_string()))`.
    - Step 3: if the path exists,
      `prompter.confirm("Vault config path '{path}' already exists. Overwrite?", false)`.
      On false, return `Ok(Some(path))` unchanged.
    - Step 4:
      `prompter.input("Vault address (incl. scheme, e.g. https://...)", None)` —
      validated via `validate_url` AFTER receiving the input (since the
      `Prompter` trait doesn't thread validators per design 003 §URL
      validation). On invalid URL, surface a clear error and either re-prompt
      (in the `DialoguerPrompter` path via `validate_with`) or fail (in
      `ScriptedPrompter` tests where invalid URLs would indicate a bad test
      script).
    - Step 5: `prompter.confirm("Specify user/pass auth for vault?", false)`. If
      yes: `prompter.input("Username", None)`
      - `prompter.password("Password")`.
    - Step 6: if user/pass declined:
      `prompter.confirm("Specify AppRole auth for vault?", false)`. If yes:
      `prompter.input("Role ID", None)` + `prompter.password("Secret ID")`.
    - Step 7: if both declined: `prompter.password("Vault token")`. If empty,
      bail with `EINVAL`-style error matching Python.
  - Vault file write — assemble a `cbscore_types::config::vault::VaultConfig`
    from the prompted fields (`vault_addr: String`,
    `auth_user: Option<VaultUserPassConfig>`,
    `auth_approle: Option<VaultAppRoleConfig>`, `auth_token: Option<String>` —
    confirmed to exist at `cbscore-types/src/config/vault.rs`). Write via a new
    `cbscore::config::store_vault` function (added in this commit, see next
    bullet).
- `cbsd-rs/cbscore/src/config.rs` — add
  `pub async fn store_vault(vault: &cbscore_types::config::vault::VaultConfig, path: &Utf8Path) -> Result<(), ConfigError>`.
  Mirrors `store` exactly:
  - Wrap in `VersionedVaultConfig::new(vault.clone())` for the
    `schema-version: 1` marker (already exists at
    `cbscore-types/src/config/versioned.rs`).
  - Serialise via `serde_saphyr::to_string`.
  - Create parent directory via `tokio::fs::create_dir_all` if needed (same
    pattern as `store`).
  - Atomic write via the existing helper (whichever pattern `store` uses;
    extract a shared helper if not already shared).
- `cbsd-rs/cbsbuild/src/cli.rs` — no change; the existing `Command::Config`
  subcommand routes to the updated `ConfigCommand` enum.

**Design constraints:**

- **`config_init_vault` is generic over `Prompter`.** Same shape as the design's
  recommended signature (`<P: Prompter>` or `&mut dyn Prompter` — implementer's
  choice; generic is slightly faster, dyn is slightly more compile-friendly).
  Pick one and use it consistently across Commit 2 + 3.
- **Vault address validation runs through `validate_url`.** Per the design 003
  §URL validation note, the DialoguerPrompter route uses `validate_with` to
  re-prompt on invalid URL; the ScriptedPrompter route runs `validate_url` once
  after the input arrives and bails on error (no re-prompt loop in tests).
- **Token-empty exit code.** Python uses `sys.exit(errno.EINVAL)` (line 95).
  Rust port should `bail!` with a context message naming `errno.EINVAL` so the
  `main.rs` exit-code mapper (which knows the design 002 line 1247 mapping)
  renders the right exit code. Implementer confirms the exit-code mapping at
  wire-up time.
- **Vault file shape.** Use the existing
  `cbscore_types::config::vault::VaultConfig` + `VersionedVaultConfig` pair (no
  new type needed; both confirmed at `cbscore-types/src/config/vault.rs` +
  `cbscore-types/src/config/versioned.rs`). Write via the new
  `cbscore::config::store_vault` function added in this commit's §Files (mirrors
  `cbscore::config::store`'s shape exactly).

**Testable:**

- Unit test: `config_init_vault` with `vault_config_path = Some(p)` and `p`
  exists returns `Ok(Some(p))` after zero prompts (`ScriptedPrompter` with an
  empty queue verifies no calls fire).
- Unit test: `config_init_vault` with `vault_config_path = None` and
  `Confirm("Configure vault authentication?") → false` returns `Ok(None)`.
- Unit test: `config_init_vault` user/pass path —
  `[Confirm(true), Input("/tmp/v.yaml"), Input("https://vault.local"), Confirm(true), Input("user"), Password("pw")]`
  produces a vault file with the user/pass auth method and the provided creds.
- Unit test: `config_init_vault` AppRole path —
  `[Confirm(true), Input("/tmp/v.yaml"), Input("https://vault.local"), Confirm(false), Confirm(true), Input("role-id"), Password("secret-id")]`
  produces a vault file with AppRole auth.
- Unit test: `config_init_vault` token fallback —
  `[Confirm(true), Input("/tmp/v.yaml"), Input("https://vault.local"), Confirm(false), Confirm(false), Password("token")]`
  produces a vault file with token auth.
- Unit test: empty token bails — token fallback with `Password("")` returns
  `Err(_)` matching the `errno.EINVAL`-mapped exit code.
- Unit test: overwrite-declined path — `vault_config_path = Some(existing_path)`
  (file does NOT exist at Step 0 — Step 3 re-checks via `path.exists()` after
  operator types a path that collides) and
  `Confirm("...exists. Overwrite?") → false` returns `Ok(Some(path))` without
  writing.
- Clap-level test:
  `Cli::try_parse_from(["cbsbuild", "config", "init-vault", "--vault", "/path/to/vault.yaml"])`
  parses successfully with
  `InitVaultArgs.vault_config_path = Some("/path/to/vault.yaml")`.

## Commit 3 — `cbsbuild config init`: interactive flow + bypass `versions` pre-fill

The biggest commit. Replaces the M1 "no flags → error" branch with the
interactive flow, adds the matching sub-functions, flips the bypass templates'
`versions` pre-fill per OQ-A.1.

**Files:**

- `cbsd-rs/cbsbuild/Cargo.toml` — add `serde-saphyr = "0.0.24"` to
  `[dependencies]`. Required by the `config_init` flow's step 9 ("Print rendered
  YAML to stdout") which calls `serde_saphyr::to_string` directly. `cbsbuild`
  does not currently depend on `serde-saphyr` (only on `serde_json` for the
  existing `config show` JSON output); seq-003 keeps YAML for the
  rendered-preview output to match design 003 §Final confirmation step 3 and the
  Python flow's `yaml.safe_dump` call.
- `cbsd-rs/cbsbuild/src/cmds/config/mod.rs` (or `config.rs`):
  - `handle_init` becomes:
    - If `args.for_systemd_install || args.for_containerized_run`: compose
      template + per-field overrides + write (existing M1 behaviour, unchanged).
    - Else: construct `DialoguerPrompter`, call
      `config_init(&mut prompter, args, config_path).await`, write the result.
  - The "no flags → error" branch is removed (or kept only as a fallback when
    interactive mode is somehow unavailable — TBD; the plan recommends removal
    for cleanliness, matching the design's "no flags → interactive" claim in
    §Bypass Behaviour).
  - `InitArgs` gains:
    - `versions_dir: Option<Utf8PathBuf>` — per-field override matching
      seq-004's `cbsbuild versions create --versions-dir` flag shape. When
      supplied, skips the §config_init_paths "Versions path" prompt.
  - `systemd_install_template()`:
    `paths.versions: Some("/var/lib/cbsd/_versions".into())` per OQ-A.1. Drop
    the inline `// Pre-fill of versions ...is owned by seq-003...` comment (the
    work is done here).
  - `containerized_run_template()`:
    `paths.versions: Some("/cbs/_versions".into())` per OQ-A.1. Drop the
    matching inline comment.
  - Update the `InitArgs` clap doc comments to describe the interactive mode:
    the struct-level doc comment switches from "No interactive prompts — at
    least one of the `--for-*` mode flags is required" to "Interactive mode (no
    flags) or bypass mode (one of `--for-*`); per-field flags skip the
    corresponding prompts." This text drives `cbsbuild config init --help`
    output.
- `CHANGELOG.md` (or whatever cbsbuild's operator-facing release-notes file is —
  confirm at implementation time; if no CHANGELOG exists, defer this bullet and
  capture the change in the final commit message instead) — add an entry naming
  the no-flags-path behaviour change:
  ```
  ## Unreleased
  - `cbsbuild config init` with no flags now enters an interactive prompt
    flow (seq-003). Previously, no-flags exited with "one of
    --for-systemd-install or --for-containerized-run is required".
    Existing operators using `--for-*` bypass modes or per-field
    overrides see no behaviour change.
  - `--for-systemd-install` and `--for-containerized-run` now pre-fill
    `paths.versions` with a path matching the template's existing prefix
    (`/var/lib/cbsd/_versions` and `/cbs/_versions` respectively;
    previously the field was unset).
  ```
- `cbsd-rs/cbsbuild/src/cmds/config/init.rs` — new file:
  - `pub(crate) async fn config_init<P: Prompter>(prompter: &mut P, args: &InitArgs, config_path: &Utf8Path) -> Result<()>`
    — top-level interactive flow. Mirrors Python `config_init`
    (`cbscore/cmds/config.py` lines 251–309) step-by-step:
    1. Resolve `cwd` via `std::env::current_dir()`.
    2. Call `config_init_paths(prompter, cwd, args)?`.
    3. Call `config_init_storage(prompter)?`.
    4. Call `config_init_signing(prompter)?`.
    5. Call `config_init_secrets_paths(prompter, &args.secrets)?`.
    6. Build the `Config` struct.
    7. Suffix normalisation: if `config_path` doesn't end in `.yaml`, log the
       warning and switch to `.yaml`.
    8. Overwrite-confirm: if file exists,
       `prompter.confirm("Config file exists, overwrite?", false)`. On false,
       bail with `ENOTRECOVERABLE`.
    9. Print rendered YAML to stdout (`serde_saphyr::to_string`).
    10. Write-confirm: `prompter.confirm("Write config to '{path}'?", true)`. On
        false, bail with `ENOTRECOVERABLE`.
    11. `cbscore::config::store(&cfg, config_path).await?` (parent-dir creation
        is handled internally per design 002 and verified at
        `cbscore/src/config.rs:143–150`).
    12. Echo `"wrote config file to '{path}'"`.
  - `pub(crate) fn config_init_paths<P: Prompter>(prompter: &mut P, cwd: &Utf8Path, args: &InitArgs) -> Result<PathsConfig>`
    — follows design 003 §config_init_paths verbatim. Each prompt is suppressed
    when the corresponding flag in `args` is supplied.
  - `pub(crate) fn config_init_storage<P: Prompter>(prompter: &mut P) -> Result<Option<StorageConfig>>`
    — follows §config_init_storage. The two URL prompts (S3 + registry) call
    `validate_url` on the received input.
  - `pub(crate) fn config_init_signing<P: Prompter>(prompter: &mut P) -> Result<Option<SigningConfig>>`
    — follows §config_init_signing.
  - `pub(crate) fn config_init_secrets_paths<P: Prompter>(prompter: &mut P, args_secrets: &[Utf8PathBuf]) -> Result<Vec<Utf8PathBuf>>`
    — follows §config_init_secrets_paths. Returns `args_secrets.to_vec()` if
    non-empty (per the existing M1 per-field bypass), otherwise prompts.

**Design constraints:**

- **CLI UX parity for the bypass-mode + per-field path** (CLAUDE.md correctness
  invariant 2). Operators using `cbsbuild config init --for-systemd-install`
  (with or without per-field overrides) see byte-identical behaviour to today's
  M1, except that `paths.versions` now lands in the generated config file with
  the per-template pre-fill value (per OQ-A.1).
- **Behaviour change on the no-flags path.** Operators who previously got
  `cbsbuild config init: one of --for-systemd-install or --for-containerized-run is required`
  now enter an interactive prompt flow. This is the intentional consequence of
  seq-003; the operator-facing CHANGELOG entry names it explicitly.
- **`--versions-dir` per-field override.** New `InitArgs` field added per design
  003 §Bypass Behaviour. When supplied, suppresses the "Versions path" prompt
  and writes the value into `paths.versions`.
- **Storage / signing prompts default to "no".** Per design 003
  §config_init_storage Step 1 and §config_init_signing Step 1 — operators get a
  one-prompt opt-in for each subsystem; if they decline, the corresponding
  `Config` field stays `None` and no further prompts fire.
- **Suffix normalisation is non-interactive.** Per design 003 §Final
  confirmation Step 1 — a log line, not a prompt. Use `tracing::warn!` or a
  `println!` to stderr; mirror Python's `click.echo` shape.
- **Re-prompt on URL validation failure** lives inside `DialoguerPrompter`'s use
  of `validate_with`. Scripted-prompter tests must supply pre-validated URLs in
  their answer queue — invalid URLs in tests would not trigger a re-prompt loop
  (per design 003 §URL validation note).

**Testable:**

- Clap-level test: `Cli::try_parse_from(["cbsbuild", "config", "init"])` (no
  flags) parses successfully with `InitArgs` all-empty / all-`None`.
- Unit test: `config_init_paths` with all flags supplied (every `args.<field>`
  set) calls zero prompts (`ScriptedPrompter` with empty queue verifies no calls
  fire) and returns the supplied values verbatim.
- Unit test: `config_init_paths` no-flags path — scripts a sequence of
  `Confirm` + `Input` answers matching the design's prompt order, asserts the
  resulting `PathsConfig`.
- Unit test: `config_init_storage` declined — `[Confirm(false)]` returns
  `Ok(None)`.
- Unit test: `config_init_storage` accepted with both S3 and registry — scripts
  the full prompt sequence, asserts the resulting `StorageConfig`.
- Unit test: `config_init_signing` declined — `[Confirm(false)]` returns
  `Ok(None)`.
- Unit test: `config_init_signing` GPG-only — scripts
  `[Confirm(true), Confirm(true), Input("gpg-secret"), Confirm(false)]`, asserts
  the resulting `SigningConfig`.
- Unit test: `config_init_signing` neither GPG nor transit set —
  `[Confirm(true), Confirm(false), Confirm(false)]` returns `Ok(None)` with the
  "no signing methods specified" log line.
- Unit test: `config_init_secrets_paths` with
  `args_secrets = ["/path/a", "/path/b"]` returns those paths verbatim, zero
  prompts.
- Unit test: `config_init_secrets_paths` no flags, declined — `[Confirm(false)]`
  returns `Ok(vec![])`.
- Unit test: `systemd_install_template` now has
  `versions: Some("/var/lib/cbsd/_versions".into())`.
- Unit test: `containerized_run_template` now has
  `versions: Some("/cbs/_versions".into())`.
- Integration test: the existing M1 tests for bypass-mode handling stay green
  (regression guard).

## End-of-feature acceptance

After all three commits land:

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace --all-targets`, `cargo fmt --all --check` all pass
  with zero warnings.
- `cbsbuild config init` (no flags, in a TTY) walks the operator through the
  design 003 §Prompt-by-Prompt Mapping prompts and writes a config file.
- `cbsbuild config init --for-systemd-install` (no per-field flags) writes the
  systemd template with `versions: /var/lib/cbsd/_versions` (per OQ-A.1) and
  zero prompts.
- `cbsbuild config init --for-containerized-run` (no per-field flags) writes the
  containerized template with `versions: /cbs/_versions` (per OQ-A.1) and zero
  prompts.
- `cbsbuild config init --for-systemd-install --versions-dir /opt/v` writes the
  systemd template with `versions: /opt/v` (per-field override wins over the
  bypass pre-fill).
- `cbsbuild config init-vault` (no flags) prompts for vault address
  - auth method + creds, writes `./cbs-build.vault.yaml`.
- `cbsbuild config init-vault --vault /etc/cbsd/vault.yaml` (existing file)
  returns immediately with no prompts.
- README plans table entry for seq-003 updates: §"Related plans › seq-003"
  bullet flips from forward-tense to landed (same commit boundary as Commit 3 so
  the README state matches the on-disk reality).
- Plan progress table flips all three rows to `Done`.

## Verification before implementation starts

1. **OQ-A resolution confirmed.** OQ-A is resolved in its §Open Questions
   subsection above (per-template `versions` pre-fill matching the template's
   existing filesystem prefix — systemd → `/var/lib/cbsd/_versions`,
   containerized → `/cbs/_versions`).
2. **Design 004 §OQ7 amended to match.** Updated alongside this plan commit: the
   literal `/cbs/_versions` claim for both templates becomes "matches the
   template's existing prefix", with the per-template values named explicitly.
3. **Spawn a design-review pass on the amended plan + design.** Same cadence as
   the seq-004 / seq-005 v2 → v3 → v4 chains; expect "approve with MINOR
   cleanups" or cleaner.
4. **Then start Commit 1.**
