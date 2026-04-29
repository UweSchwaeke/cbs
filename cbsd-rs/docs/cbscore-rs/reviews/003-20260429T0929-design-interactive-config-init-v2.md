# Design Review v2: Interactive `config init` for `cbsbuild`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/003-20260427T1255-interactive-config-init.md`

**Prior reviews:** `003-20260428T1401-design-interactive-config-init-v1.md`

**Commits reviewed since v1:** None. Design 003 was not modified by `6064814` or
`970fb3a`.

---

## Summary

**Verdict: approve, no changes needed.**

All five v1 findings are confirmed closed in the current document text. The
cbscommon lift-out work in commits `6064814` and `970fb3a` does not touch design
003 and introduces no inconsistencies with it — design 003 operates entirely at
the `cbsbuild` binary layer and has no dependency surface on `utils::git` or
`utils::subprocess`. No new issues found.

---

## v1 Finding Verification

**F1 (vault bypass pre-check):** CLOSED. `§ config_init_vault` step 0 reads: "If
`vault_config_path` was supplied … AND the file already exists on disk, return
that path unchanged with no prompts." The `§ Bypass Behaviour` section documents
the same case explicitly: "`-- vault <path>` is special: when the supplied path
already exists on disk, the entire vault flow (the `config_init_vault` Step 0
short- circuit) is skipped — no prompts, no overwrite confirmation."

**F2 (yaml suffix normalisation):** CLOSED. `§ Final confirmation` step 1 now
reads: "If the target `config_path` does not end in `.yaml`, rename it to use
the `.yaml` extension (`Path::with_extension("yaml")` /
`Utf8PathBuf::with_extension`) and echo a warning: …". The design explains why
this is load-bearing (`Config::load` picks the deserialiser by extension).

**F3 (mkdir parent directory):** CLOSED. `§ Final confirmation` step 5 now
reads: "`Config::store` creates the parent directory if needed (mirrors Python
`config_path.parent.mkdir(...)` on line 302 — see design 002 § Configuration &
Secrets / IO for the contract)."

**F4 (ScriptedPrompter URL validation gap documented):** CLOSED.
`§ URL validation` now contains: "The trade-off: `ScriptedPrompter` (used in
prompt-flow unit tests) does not invoke `validate_url`, so scripts must supply
pre-validated answers — invalid URLs in test scripts won't trigger a re-prompt
loop. That is acceptable because the URL validator is exercised directly by its
own tests; the prompt-flow tests cover ordering / defaults / skip logic, not URL
parsing." The `§ Testing` section confirms the same in layer 2 vs. layer 3.

**F5 (init-vault vs config init clarification):** CLOSED. A dedicated paragraph
at the top of `§ config_init_vault` reads: "The prompts in this subsection fire
from `cbsbuild config init-vault` — a separate subcommand — **not** from
`cbsbuild config init`. The primary `config init` flow only records the vault
config file _path_ (it stores `Config.vault = <path>` and writes the main
config); the vault address, auth method, and credentials are gathered when the
operator runs `init-vault` to populate the vault file."

---

## Cross-Document Consistency

The cbscommon lift-out invariants (`970fb3a`) affect `utils::git` and
`utils::subprocess` in `cbscore`. Design 003 lives entirely in
`cbsbuild/src/cmds/config/` and depends on `dialoguer`, `url`, and the
`Prompter` trait — none of which are in the lift-out scope. No inconsistency
introduced.
