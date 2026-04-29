# Design Review v1: Interactive `config init` for `cbsbuild`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/003-20260427T1255-interactive-config-init.md`

**Prior reviews:** None (first pass).

**Python reference:**
`cbscore/src/cbscore/cmds/config.py`

---

## Summary

**Verdict: approve with two important fixes and three nice-to-have
corrections.**

The overall design is sound. The `Prompter` trait is well-calibrated, the
deferred-from-M1 framing is correctly articulated, and the two Resolved
Decisions (ccache default-no, URL validation via `url::Url::parse`) are
technically correct. The two-layer testing strategy (data-assembly + scripted-
prompter) is adequate given that the TTY-driving smoke test was dropped.

Two important issues need to be addressed before implementation: a vault-path
bypass gap that will produce an unexpected interactive prompt when `--vault`
is supplied pointing at an existing file, and a missing step in the final-
confirmation flow that will silently produce a file with a wrong extension.
Three minor gaps are worth fixing for completeness.

---

## Strengths

- The `Prompter` trait surface (`input`, `password`, `confirm`) maps cleanly
  to `click`'s three interactive primitives. No over-engineering.
- The `ScriptedPrompter` + `VecDeque<PromptAnswer>` pattern is the right
  approach for testing prompt-order regressions without spawning a TTY.
- Placing `prompts.rs` in `cbsbuild/src/cmds/config/` (rather than in
  `cbscore`) correctly keeps interactive IO in the binary crate.
- The `validate_with` hook approach for URL inputs is the right level of
  validation — catches syntax typos, defers semantic checks to the SDK.
- The Migration from M1 section is complete and coherent: no on-disk format
  change, no CLI flag deprecations, no breaking change for automation.

---

## Important Findings

### F1 — Vault bypass: `--vault` with an existing path triggers an
          unexpected "Configure vault?" confirm [IMPORTANT]

**Section:** `§ config_init_vault`, Step 1; `§ Bypass Behaviour`

The design's `config_init_vault` mapping (step 1: "Configure vault? — Confirm.
If no, return None.") implies that the confirm is always the first prompt, even
when the `--vault` flag was supplied. The Python implementation does not work
this way.

Python `config_init_vault` (lines 42-44 of `config.py`):

```python
if vault_config_path and vault_config_path.exists():
    return vault_config_path
```

If `vault_config_path` was pre-supplied by the `--vault` flag **and** it
already exists on disk, the function returns immediately — no "Configure
vault?" confirm, no overwrite prompt. The file is treated as already
configured and the function is a no-op.

The design's step 1 conflates two distinct paths:
- `--vault` not supplied, file not yet created → ask "Configure vault?"
- `--vault` supplied AND file already exists → return silently (no prompt)

The design's bypass section says "`--vault`: pre-fills the vault config path,
skipping its prompt." This only describes skipping the _path_ prompt (step 2).
It does not describe the silent-return behaviour when the pre-filled path
already exists. An operator who supplies `--vault /cbs/config/vault.yaml`
(which exists) will see an unexpected "Configure vault?" prompt that the Python
implementation never shows.

**Why it matters:** This is a UX parity break for automation-adjacent usage
(an operator who semi-automates setup via flags, as opposed to fully scripting
it). The `--vault` flag is specifically intended for the bypass modes
(`--for-systemd-install`, `--for-containerized-run`), which pre-fill all paths
including `vault_config_path` pointing at an existing file. A
`--for-systemd-install` invocation on a machine that has been set up before
will unexpectedly prompt the user.

**Resolution:** Add a pre-check step to `config_init_vault` before the
"Configure vault?" confirm:

> **Step 0 (pre-check):** If `vault_config_path` was supplied via flag AND
> `vault_config_path.exists()`, return that path immediately with no prompts.

Update the `§ Bypass Behaviour` section to document this case explicitly:

> `--vault <path>` where the path already exists: the entire vault init
> flow is skipped (no "Configure vault?" confirm, no overwrite check). This
> matches the Python `config_init_vault` short-circuit at lines 42-44.

---

### F2 — Final confirmation is missing the `.yaml` suffix
          normalisation step [IMPORTANT]

**Section:** `§ Final confirmation`

The Python `config_init` (lines 283-288) performs a path-suffix check before
writing:

```python
if config_path.suffix != ".yaml":
    new_config_path = config_path.with_suffix(".yaml")
    click.echo(
        f"config at '{config_path}' not YAML, use '{new_config_path}' instead."
    )
    config_path = new_config_path
```

This step normalises a user-supplied path with a wrong extension (e.g.,
`cbs-build.config.json` → `cbs-build.config.yaml`) before asking the user to
confirm the write destination. The design's `§ Final confirmation` section
omits this step entirely: it goes directly from "print rendered config" to
"confirm write" to "write file".

The omission means the Rust port will write to the user-supplied path
regardless of extension, which produces a config file named e.g.
`cbs-build.config.json` even though the content is YAML. The schema_version
dispatch in `Config::load` uses the extension to pick the deserialiser (YAML
vs. JSON) — a `.json` file containing YAML will fail to parse.

**Why it matters:** The normalisation is a silent operator-protection guard
that prevents a hard-to-diagnose load failure downstream. Omitting it from the
design means the Rust port will not preserve it, and operators who historically
passed `--config cbs-build.config` (no extension) will get a file with the
wrong extension and a confusing load error on the next `cbsbuild` invocation.

**Resolution:** Add a step 0 to `§ Final confirmation`:

> **Step 0 (extension normalisation):** If `config_path` does not have a
> `.yaml` or `.yml` suffix, replace the suffix with `.yaml` and echo
> `"config at '<original>' not YAML, using '<new>' instead."` This matches the
> Python `config_init` behaviour at lines 283-288.

---

## Nice-to-Have Findings

### F3 — Final confirmation is missing the `mkdir -p` for the
          config parent directory [NICE-TO-HAVE]

**Section:** `§ Final confirmation`, Step 3

Python `config_init` (line 302): `config_path.parent.mkdir(exist_ok=True,
parents=True)`. The Rust `Config::store` implementation will need to handle
the case where the parent directory does not exist. Design 003 says "call
`Config::store(path)`" but does not specify whether `store` handles missing
parent directories or the caller is expected to create them.

**Resolution:** Add one line to Step 3 clarifying that the store call
(or a preceding step) creates the parent directory with `mkdir -p` semantics
(`fs::create_dir_all`) before writing. This is a three-line implementation
detail but it is worth documenting to preserve the Python behaviour.

---

### F4 — ScriptedPrompter does not exercise URL validation
          [NICE-TO-HAVE]

**Section:** `§ Prompter trait`, `§ Testing`

The `Prompter` trait's `input` method signature is:

```rust
fn input(&mut self, label: &str, default: Option<&str>) -> Result<String, PromptError>;
```

URL validation is implemented inside `DialoguerPrompter::input` (or via a
separate `input_url` method — the design doesn't specify) using
`dialoguer::validate_with`. When `ScriptedPrompter` provides a URL answer, it
bypasses validation entirely — the scripted answer is returned as-is without
calling `url::Url::parse`.

This is acceptable for the test strategy as-is: unit tests with scripted
answers control their inputs and can trivially supply valid URLs. But there is
a gap: a test cannot verify that the validator fires and re-prompts on an
invalid URL, because `ScriptedPrompter::input` never rejects the answer.

Two options:

1. Accept the gap (current design). Unit tests verify prompt order and config
   assembly; the validation logic is tested separately as a pure function
   (`url_validator(s: &str) -> Result<(), String>`).
2. Add a `validate` parameter to `Prompter::input`:

```rust
fn input(
    &mut self,
    label: &str,
    default: Option<&str>,
    validate: Option<Box<dyn Fn(&str) -> Result<(), String>>>,
) -> Result<String, PromptError>;
```

`ScriptedPrompter` would then call `validate` on the scripted answer and
return an error if it fails, which lets tests assert that invalid URLs are
rejected.

Option 2 makes the trait heavier (and complicates the `DialoguerPrompter`
impl slightly). Option 1 is fine as long as the URL validator is a standalone
function tested independently. **Prefer option 1** unless the team wants the
full seam test. Either way, add a note in the Testing section to document the
decision.

---

### F5 — `config_init_vault` is called from `init-vault`, not from
          `config init`; the prompt mapping should clarify this
          distinction [NICE-TO-HAVE]

**Section:** `§ Prompt-by-Prompt Mapping`, `§ config_init_vault`

The section heading reads "config_init_vault (separate `init-vault`
subcommand)". This is correct — `config_init_vault` is called from
`cmd_config_init_vault`, not from `cmd_config_init`. However, the `§ Bypass
Behaviour` section lists `--vault` as a per-field flag for `config init`, and
the `config_init` function in Python does accept `vault_config_path` as a
parameter (storing the path reference in the resulting `Config`, not running
the vault auth flow).

This creates a potential confusion: is the vault path prompt (step 2 of
`§ config_init_vault`) fired from `config init` or only from `init-vault`?
The answer is only from `init-vault`. Within `config init`, the `--vault`
flag simply provides a pre-filled value for the `Config.vault` field (the
path), bypassing the prompt in `init.rs`; no vault authentication is
configured interactively through `config init`.

**Resolution:** Add a one-sentence clarification at the top of
`§ config_init_vault`:

> Note: this flow runs only under `cbsbuild config init-vault`. The main
> `cbsbuild config init` command accepts `--vault <path>` to pre-fill
> `Config.vault` (the path reference only); it does not run the vault
> authentication prompts. The `init-vault` subcommand is separate.

---

## Resolved Decisions — Pushback Assessment

### ccache default: no

Correct. Python parity is the right call for the interactive cohort.
The workstation users who want ccache are likely already familiar with the
flag-based bypass. Default-yes would create a surprising prompt for the
majority who use the bypass modes for automation.

### URL validation via `url::Url::parse`

Correct. Syntactic validation at prompt time is the right scope. The
validator correctly catches missing-scheme errors (`vault.local` without
`https://`) — the most common typo — without false-positiving on
unreachable-but-syntactically-valid URLs. The scheme constraint is worth
documenting: `url::Url::parse("vault.local")` returns an error because it
cannot identify a scheme, but `url::Url::parse("custom://vault.local")`
succeeds. If the intent is to restrict to `http://` and `https://` schemes,
that should be an additional check on top of the parse call.

This is not a blocker — "any syntactically valid URL" is a reasonable
acceptance criterion for a setup command. But a note in the design about
scheme-checking being out of scope would pre-empt future debate.

---

## Two-Layer Testing Strategy Assessment

Adequate given the dropped TTY smoke test. The reasoning:

1. **Data-assembly tests** (no prompting): covers `Config` construction from
   pre-filled `Init` structs — the same path M1's flag-based modes exercise.
   This catches regressions in the flag-bypass-to-`Config`-field mapping.

2. **Scripted-prompter tests**: covers prompt order, skip logic, and default
   values. A `VecDeque` that drains in order provides unambiguous assertion
   that the prompts fire in the expected sequence, and that conditional branches
   (e.g., "skip ccache prompt if --ccache was supplied") work.

The TTY-driving smoke test was dropped for good reason: it requires a pty,
is inherently flaky on CI, and the `Prompter` seam already covers everything
the smoke test would have verified. No concern here.

---

## Cross-Document Consistency

**`dialoguer` and `url` in `cbsbuild` Cargo sketch:** Design 001 lists neither
in `cbsbuild`'s Cargo sketch, which is correct — both are post-M1 additions.
Design 003 `§ Migration from M1 to This Design` step 1 correctly identifies
them as additions at implementation time. The integration plan is coherent.

**`cbsbuild`'s role and runner container PID 1:** Design 003 does not
contradict designs 001 or 002 on these points. The config init subcommand is
a workstation-side subcommand that does no podman interaction, so there is no
runner-container surface here.

**OQ resolutions:** Design 003's Resolved Decisions (ccache default-no, URL
validation) are consistent with design 002's Open Questions (OQ8: deferred;
no OQ constrains these two decisions).

---

## Summary of Action Items

| ID | Severity      | Action                                                                                                                            |
| -- | ------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| F1 | IMPORTANT     | Add Step 0 to `config_init_vault`: if `--vault` was supplied AND the path exists, return silently with no prompts. Update bypass section. |
| F2 | IMPORTANT     | Add Step 0 to Final confirmation: normalise config path to `.yaml` suffix; echo warning if changed. Matches Python lines 283-288. |
| F3 | NICE-TO-HAVE  | Clarify that `Config::store` or the caller creates the parent directory (`create_dir_all`) before writing. Matches Python line 302. |
| F4 | NICE-TO-HAVE  | Document that `ScriptedPrompter::input` bypasses URL validation; add a note on testing the validator as a standalone function.    |
| F5 | NICE-TO-HAVE  | Add a one-sentence clarification that `config_init_vault` runs only from `init-vault`, not from `config init`.                   |
