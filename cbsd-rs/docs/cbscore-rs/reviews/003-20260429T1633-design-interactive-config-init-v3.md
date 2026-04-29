# Design Review v3: Interactive `config init` for `cbsbuild`

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/003-20260427T1255-interactive-config-init.md`

**Prior reviews:** `003-20260428T1401-design-interactive-config-init-v1.md`,
`003-20260429T0929-design-interactive-config-init-v2.md`

**Commits reviewed since v2:** `fb395f7` (Step 6 in §config_init_paths;
`--versions-dir` in per-field flags; `/cbs/_versions` in `--for-systemd-install`
parenthetical).

---

## Summary

**Verdict: approve, no changes needed.**

The three edits from `fb395f7` are correct and slot cleanly into the existing
design structure. The new Step 6 mirrors the ccache prompt pattern exactly. The
`--versions-dir` addition to the per-field flags list is factually complete. The
`/cbs/_versions` addition to the systemd-install parenthetical is accurate. No
inconsistencies with the `Prompter` trait or `ScriptedPrompter` strategy are
introduced.

---

## Commit Verification (`fb395f7`)

### Step 6 in §config_init_paths

The new prompt:

> **Versions path (optional)** — `Confirm`: "Specify versions path?" then
> `Input::<String>`: "Versions path". The field is `Config.paths.versions`
> (added by design 004); when unset, cbscore-rs falls back at runtime to
> `<git-root>/_versions` (per design 004 OQ2). Setting this decouples
> `cbsbuild versions create` from being inside a git checkout.

The structure (`Confirm` then `Input`) is identical to the ccache prompt at
Step 5. The cross-references to design 004 OQ2 (fallback behaviour) are
accurate. The prompt is correctly marked optional (guarded by `Confirm`),
matching the `Option<Utf8PathBuf>` field type.

**Prompter trait compatibility:** The trait signature is
`fn input(...) -> Result<String, PromptError>` and
`fn confirm(...) -> Result<bool, PromptError>`. Step 6 uses exactly these two
methods in the same pattern as Step 5. `ScriptedPrompter` handles both; no trait
extension is needed. The new prompt slots in cleanly.

**`ScriptedPrompter` test impact:** Scripted tests that exercise
`config_init_paths` need two additional answers in their `VecDeque` for the new
step (one `bool` for the `Confirm`, one `String` for the path if `Confirm`
returns `true`). This is a normal consequence of adding a prompt and is handled
at implementation time, not a design issue.

### `--versions-dir` in the per-field flags list

The updated line reads:

> `--components`, `--scratch`, `--containers-scratch`, `--ccache`,
> `--versions-dir` (design 004), `--vault`, `--secrets`.

This is the correct flag name (matches design 004 §CLI flag:
`#[arg(long, value_name = "PATH")] versions_dir: Option<Utf8PathBuf>` which clap
renders as `--versions-dir`). The placement is alphabetically consistent with
the surrounding list. Factually complete.

### `/cbs/_versions` in the `--for-systemd-install` parenthetical

The updated line reads:

> `--for-systemd-install`: pre-fill paths for the systemd worker layout
> (`/cbs/components`, `/cbs/scratch`, `/cbs/_versions` for
> `Config.paths.versions` per design 004 OQ7, etc.) and write to
> `~/.config/cbsd/${deployment}/worker/cbscore.config.yaml`.

The value `/cbs/_versions` matches design 004 OQ7 exactly. The cross-reference
to OQ7 is accurate. The `etc.` elision is appropriate given the other paths are
unchanged and already covered by the M1 implementation.

---

## No New Issues Found

All five v1 findings remain closed (verified in v2). The design 004 edits
introduce no new inconsistencies with the `Prompter` trait, the
`ScriptedPrompter` strategy, the `DialoguerPrompter` implementation, the
§Testing layers, or the §Migration from M1 section.
