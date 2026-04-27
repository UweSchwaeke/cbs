# Design Review v3: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Prior reviews:** `002-20260420T1132-design-cbscore-rust-port-design-v1.md`,
`002-20260420T1512-design-cbscore-rust-port-design-v2.md`

> **Post-review correction (2026-04-27):** When applying the N3 fix, the
> reviewer's identification of which half of the merge conflict was
> authoritative was inverted. The reviewer described lines 1–1410 as the live
> content and lines 1412–2844 as debris; in fact lines 1–1455 were a stale
> revert (the original Open Questions text without resolutions) and lines
> 1456–2843 were the up-to-date content (with all eight Open-Question
> resolutions and the relaxed cross-language compatibility framing). The N3 fix
> kept lines 1456–2843 and dropped lines 1–1455 + line 2844. All other findings
> in this review (N4–N8, ADV) apply against the kept content and remain valid as
> written.

---

## Summary

**Verdict: one blocking structural defect (unresolved merge conflict), otherwise
approve-with-minor-fixes.**

The eight Open Question resolutions are technically sound and the design is
coherent. Two v2 findings (N1 `$*` → `"$@"`, N2 `set -e` comment) are closed.
However, the file contains an unresolved `diff3`-style merge conflict that
doubles roughly half the document — the content from line 1456 to the end is the
"other" side of a conflict that was never resolved. This must be fixed before
any implementation begins, because readers cannot tell which copy of each
section is authoritative.

Beneath that structural issue there are six substantive findings: one blocking
(the merge conflict), two important (a path-type inconsistency in the struct
code sketches, and a stale forward reference), and three that are minor or
advisory.

---

## v2 Finding Verification

**N1 — `$*` replaced with `"$@"`:** CLOSED. The bundled entrypoint script at
line 778 now reads `runner build "$@"`. Correct.

**N2 — `set -e` comment:** CLOSED. The script now includes the comment
explaining why `set -e` is safe (line 759-761), matching the "keep it with a
comment" resolution from N2.

---

## New Findings

### N3 — Unresolved `diff3` merge conflict doubles the document [BLOCKING]

**Location:** Lines 1454–2844 (entire second half of the file)

The file contains a partially-resolved `diff3`-style merge conflict:

- Line 1454: `||||||| parent of af8e520 (wip)` — the conflict base marker.
- Line 1455: `=======` — start of the "theirs" side.
- Lines 1456–2844: A second full copy of the document from `## Overview` onward,
  corresponding to the `af8e520 (wip)` commit.
- Line 2844: `>>>>>>> af8e520 (wip)` — the closing conflict marker.

The `<<<<<<< HEAD` opening marker is absent from the file, which means the
"ours" side was accepted without removing the base-and-theirs block. The result
is that the file presents the Open Questions section twice: once at lines
1301–1388 (the resolved version, which is the correct one) and again at lines
2756–2844 (also the resolved version, from the wip branch). The two copies of
the Open Questions happen to agree on all resolutions in this case, but every
other section in the second copy (§ Wire-Format Versioning, § Configuration &
Secrets, § Secrets, § Runner Subsystem, etc.) is duplicated verbatim. Any future
edit to the document is ambiguous about which copy to update.

The authoritative content is lines 1–1388 (through `### Rollback`). Lines
1412–1453 (old unresolved Open Questions) and lines 1454–2844 (base marker,
separator, entire duplicate body, closing marker) are the debris of the merge
and must be deleted.

**Why it matters:** A reader following a section reference (e.g. "see §
Secrets") could land on either copy. The two copies differ in one material
detail (see N5 below). The git conflict marker at line 2844 will also cause
`markdownlint` and `prettier` to fail.

**Resolution:** Delete lines 1412 (old Open Questions header) through 2844 (the
`>>>>>>>` marker) and replace them with the resolved Open Questions block that
currently lives at lines 1301–1388 of the first copy. Concretely, after
`### Rollback` (line ~1410), the file should end with `## Open Questions`
followed by the eight resolved entries. The resolved Open Questions block is
identical in both copies, so either can be used.

Alternatively: use `git checkout --theirs` on the conflict section, confirm the
resolved text matches, then delete the conflict markers.

---

### N4 — Config struct code sketches use `PathBuf` instead of

        `Utf8PathBuf` [IMPORTANT]

**Section:** `§ Configuration & Secrets Subsystem — Types` (lines ~429–452)

The `Config`, `PathsConfig`, and `VaultConfig` struct sketches declared in
`cbscore-types::config` use `PathBuf`:

```rust
pub secrets:  Vec<PathBuf>,
pub vault:    Option<PathBuf>,
// PathsConfig:
pub components:         Vec<PathBuf>,
pub scratch:            PathBuf,
pub scratch_containers: PathBuf,
pub ccache:             Option<PathBuf>,
```

The Capability Mapping table (line ~201) resolves OQ3 as:

> `camino` (`Utf8Path` / `Utf8PathBuf`) at all cbscore API boundaries.

`PathsConfig` and `Config` are explicitly API boundary types — they are public,
consumed by `cbsbuild` and `cbsd-worker`. Using `PathBuf` here means the UTF-8
guarantee breaks at the first call site that passes a path from one of these
structs to a subprocess argument or a log message — exactly the problem OQ3 was
resolved to prevent.

The same inconsistency appears in the `Config::load` / `Config::store` function
signatures, which use `&Path` rather than `&Utf8Path`.

**Why it matters:** The design establishes `Utf8PathBuf` as the invariant for
path types at API boundaries. Code written against the struct sketches will use
`PathBuf`, and the first attempt to pass `.scratch` to a
`tokio::process::Command::arg` will require an explicit `.to_str().ok_or(...)?`
— the exact unwanted pattern the OQ3 resolution was designed to eliminate.

**Resolution:** Update all path field types in the struct sketches to
`Utf8PathBuf`:

```rust
// In Config:
pub secrets:  Vec<Utf8PathBuf>,
pub vault:    Option<Utf8PathBuf>,

// In PathsConfig:
pub components:         Vec<Utf8PathBuf>,
pub scratch:            Utf8PathBuf,
pub scratch_containers: Utf8PathBuf,
pub ccache:             Option<Utf8PathBuf>,
```

Update `Config::load` / `Config::store` signatures to use `&Utf8Path`. Add a
note that `camino = { version = "1", features = ["serde1"] }` must appear in
`cbscore-types/Cargo.toml` (not just `cbscore/Cargo.toml`) because the path
fields participate in `#[derive(Serialize, Deserialize)]` on the types crate
side.

---

### N5 — Stale forward reference "see Open Questions" in

        `§ Subprocess & Secret Redaction` preamble [IMPORTANT]

**Section:** `§ Subprocess & Secret Redaction`, preamble bullet for
`async_run_cmd` (line ~929)

The bullet reads:

> `async_run_cmd(cmd, outcb, timeout, cwd, extra_env)` — async wrapper with
> line-granular stream callback (the Python `reset_python_env` flag is
> intentionally **not ported** — see Open Questions)

`reset_python_env` is no longer an open question — it is resolved in the
`## Open Questions` section at the bottom of the document (OQ5). The
parenthetical forward reference is now a dead pointer.

This is low-risk — the resolution is right there in the same document — but a
reader encountering this sentence for the first time will scroll to
`## Open Questions` looking for an entry labelled "open" and find only a
resolved entry. The sentence should reference the resolution directly.

**Resolution:** Replace the parenthetical with a back-reference:

> (the Python `reset_python_env` flag is intentionally **not ported** — see
> `## Open Questions`, OQ5 for rationale)

Or simply incorporate the one-sentence rationale inline and drop the reference
entirely:

> `async_run_cmd` — async wrapper with line-granular stream callback. The Python
> `reset_python_env` flag is not ported: cbsbuild runs directly with no venv on
> `PATH`, so the workaround is moot.

---

### N6 — OQ7 resolution has a path inconsistency: `/bin/cbsbuild`

        vs `/runner/cbsbuild` [MINOR]

**Section:** `## Open Questions`, OQ7 resolution (line ~1374)

The OQ7 resolution text says:

> The new entrypoint mounts the `cbsbuild` static binary at
> `${RUNNER_PATH}/bin/cbsbuild` and invokes it directly.

But the mount table (line ~733), the entrypoint script (line ~776), and the
`Target Architecture` diagram all use `/runner/cbsbuild` (no `/bin/`
subdirectory). The entrypoint script uses:

```bash
exec "${RUNNER_PATH}/cbsbuild" ...
```

where `RUNNER_PATH="/runner"`, so the binary is at `/runner/cbsbuild`.

The `/bin/cbsbuild` path in OQ7 is the one inconsistent value. Since this is in
the second copy of the document (the merge-conflict debris described in N3),
fixing N3 will automatically remove the offending sentence. If OQ7 is retained
in the cleaned-up document, update the path to `/runner/cbsbuild`.

---

### N7 — `crt` is not a consumer of `ReleaseDesc`; OQ6 claim is

        incorrect [MINOR]

**Section:** `## Open Questions`, OQ6 resolution (line ~1358)

The OQ6 resolution states:

> this assumes `crt` is rewritten or retired as part of that cutover, since it
> is currently the only remaining Python consumer of `ReleaseDesc`.

A grep of the `crt/` tree confirms that `crt` imports only `parse_version` from
`cbscore` (two files: `crtlib/manifest.py` and `crtlib/utils.py`). `crt` does
not import `cbscore.releases.desc` or `ReleaseDesc` anywhere. The only Python
consumers of `ReleaseDesc` are within `cbscore` itself (`releases/s3.py`,
`builder/builder.py`, `containers/build.py`).

The practical consequence of the incorrect claim is that OQ6's migration
precondition ("crt must be rewritten or retired before deploying Rust cbscore in
a deployment that uses release descriptors") is unsupported. `crt` only consumes
version parsing; it can continue to run unchanged after a Rust cbscore cutover.

**Resolution:** Remove or correct the claim in OQ6. The sentence should read:

> The only Python consumers of `ReleaseDesc` are internal to `cbscore` itself
> (`builder/builder.py`, `releases/s3.py`, `containers/build.py`), all of which
> are replaced by the Rust implementation. No external Python package imports
> `ReleaseDesc`.

---

### N8 — `rand 0.9` API incompatibility in `gen_run_name` sketch [MINOR]

**Section:** `§ Runner Subsystem — Running name generation` (line ~817)

The code sketch uses `rand::thread_rng()`:

```rust
use rand::{seq::IteratorRandom, thread_rng};
let mut rng = thread_rng();
let suffix: String = ('a'..='z').choose_multiple(&mut rng, 10)
    .into_iter().collect();
```

`thread_rng()` was removed in `rand` 0.9. The replacement is `rand::rng()` (a
free function returning a thread-local RNG). The Capability Mapping table pins
`rand` at `0.9`.

With `rand` 0.9 the sketch should read:

```rust
use rand::seq::IteratorRandom as _;
let mut rng = rand::rng();
let suffix: String = ('a'..='z').choose_multiple(&mut rng, 10)
    .collect();
```

(The `.into_iter()` is also redundant since `choose_multiple` in rand 0.9
returns a `Vec`, whose `IntoIterator` impl produces an owned iterator;
`collect()` works directly on the `Vec`'s `into_iter()`.)

This is a sketch, not production code, but incorrect sketches in design
documents become incorrect copy-paste in implementation. Since the Capability
Mapping table already pins rand 0.9, the sketch should be consistent with that
pin.

**Resolution:** Update `thread_rng()` to `rand::rng()` in the sketch and drop
the now-redundant `.into_iter()`.

---

## Cross-Document Notes

**001 / 002 consistency:** Three issues in design 001 stem directly from the OQ
resolutions in design 002. See the companion design 001 v2 review for details:

- `## Python Coexistence` item 1 (contradicts no-file-interchange position).
- `## Runner Container` Python-removal sentence (overstates removal).
- `## Crate Dependencies` (missing `camino`, `camino-tempfile`, `regex`).

**N4 (PathBuf → Utf8PathBuf):** This inconsistency must be fixed in design 002
first; the corresponding note to add `camino` to `cbscore-types` Cargo.toml is
in the 001 review (F3).

**N3 (merge conflict):** Once the duplicate block is removed, verify that the
surviving `## Open Questions` section (lines 1301–1388 in the current file)
contains all eight resolutions and the OQ7 path fix from N6. The second copy
(lines 2756–2844) is authoritative for OQ7 wording but should be discarded as a
body — use the first copy and apply the N6 and N7 fixes to it.

---

## Specific Concern Responses (per brief §C)

### C1 — `Prompter` trait in design 003: right level of abstraction?

The `Prompter` trait in design 003 is well-calibrated. Three methods (`input`,
`password`, `confirm`) with a `ScriptedPrompter` that drains a `VecDeque` is
exactly what is needed to unit-test prompt order and default values without
spawning a TTY. The trait is not premature — interactive code without this kind
of seam is untestable by construction, and the cost (one trait, two impls, one
`VecDeque`) is minimal.

One note: `config_init` takes `&mut dyn Prompter`. If the function ends up
deeply nested (e.g. `config_init` calls `config_init_paths` which calls
`config_init_storage`), all sub-functions must also take `&mut dyn Prompter` or
a `&mut impl Prompter` bound. Make sure the design's module layout
(`prompts.rs`, `init.rs`) is consistent with passing the prompter through the
call chain without cloning or re-borrowing across an `await` boundary. Since
`config init` is sync-only (dialoguer is sync), there is no async borrow issue;
the call chain is straightforward.

### C2 — Camino interop clarity: "bridge at FFI points"

The guidance "bridge to `std::path::Path` only at FFI points where third-party
crates require it" is operationally clear enough for a senior implementor but
may leave too much to interpretation for someone newer to the codebase. Two
practical clarifications would help:

1. State explicitly which crates are known FFI points today: `tempfile`
   (resolved by `camino-tempfile`), `std::fs` calls (use `.as_std_path()`
   inline), `tokio::fs` calls (same adapter), OS `PermissionsExt` calls.
   Everything else should use `Utf8Path`/`Utf8PathBuf` natively.
2. Add a note that `camino-tempfile` resolves the biggest friction point
   (runner's tempfile creation) and that `Utf8TempPath` / `Utf8NamedTempFile`
   should be used throughout the runner and builder subsystems.

This is a clarification for the implementation plan, not a change to the design.

### C3 — `reset_python_env` removal: is there an edge case with

         developer-activated venvs?

The concern is valid but the design's conclusion is correct. The scenario: a
developer who has activated a personal venv before invoking `cbsbuild` from a
workstation will have that venv's `bin/` on `PATH`. Child processes spawned by
`cbsbuild` (rpmbuild, git, skopeo, podman) will all inherit this `PATH`. The
question is whether any of those child processes inadvertently run the venv's
`python3` in a way that breaks a component build.

The answer is: most likely not, but the risk is not zero. The key difference
from the Python scenario is that the Python cbscore `reset_python_env` stripped
the cbscore venv's `python3` because it was guaranteed to be present. The Rust
binary makes no such guarantee about any particular venv. Child processes that
need Python (rpmbuild's `%build` scriptlets, cmake's Python probes) will find
whatever `python3` is first on `PATH` — if a developer's venv provides a
different Python version than the expected system `python3`, a build could
behave differently on the developer's workstation than in CI. This is a
pre-existing hazard on the Python side too, because the Python cbscore stripped
only its own venv, not a developer's personal venv.

The design's "don't port the flag" decision is reasonable because:

- The Rust binary cannot know which venv entries on `PATH` are "ours" vs the
  developer's — the Python flag only worked because the cbscore venv had a
  predictable location.
- Reproducing the behaviour would require specifying a well-known path to strip,
  which is a policy decision (`CBSCORE_VENV_PATH` env var or similar) that is
  not worth the complexity.
- The developer scenario is already broken by the Python cbscore in the same way
  if the developer activates a venv that Python cbscore does not recognise.

The design should add a one-sentence note: "Developers who activate a personal
Python venv before invoking `cbsbuild` on a workstation should deactivate it
first to avoid propagating the venv's `python3` into component build scripts."
This is documentation, not code.

### C4 — No cross-language file interchange: is the deployment

         assumption realistic?

The position "a given deployment runs either Python cbscore or Rust cbscore,
never both against the same on-disk files" is defensible for **cbsd-worker**
deployments (one worker, one `secrets.yaml`, atomic cutover) and for
**standalone cbsbuild workstation** use (one user, one config file).

It becomes fragile for **rolling k8s / systemd-unit upgrades** where multiple
workers share a single secrets store. In that deployment model, a rolling
upgrade means some workers are on Python cbscore and some are on Rust cbscore
simultaneously, and a single `secrets.yaml` (potentially a mounted ConfigMap or
a shared NFS path) is read by both. Rust cbscore after M1 will reject any
`secrets.yaml` that lacks `schema_version` (absent = hard error). A shared file
that has not been re-tagged will block the Rust workers from starting while
Python workers keep running.

The risk is bounded in practice: the `secrets.yaml` file has a well-defined
format and the migration is mechanical (add `schema_version: 1`, add `type:` to
git entries). Operators who understand their deployment can do this atomically
before the first Rust worker starts. But the design should acknowledge this
scenario explicitly rather than assuming all deployments are single-worker.

**Suggested addition to `§ Migration Strategy` — M1:** Add a note:

> Rolling deployments (multiple workers sharing a secrets store): apply the
> `secrets.yaml` migration before upgrading any worker. Rust workers will refuse
> to start if the file lacks `schema_version`; Python workers will continue to
> function (pydantic ignores the new field). The recommended migration order is:
> (1) migrate `secrets.yaml`, (2) upgrade workers one by one.

This does not change the design's position; it just makes the assumption visible
and gives operators a path.

---

## Summary of Action Items

| ID  | Severity  | Action                                                                                                                                                                               |
| --- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| N3  | BLOCKING  | Resolve merge conflict: delete lines 1412–2844 (old Open Questions + duplicate body + conflict markers); retain and finalize the resolved Open Questions at lines 1301–1388          |
| N4  | IMPORTANT | Update all `PathBuf` → `Utf8PathBuf` in Config / PathsConfig struct sketches; update `load`/`store` signatures to `&Utf8Path`; add note that `camino` dep belongs in `cbscore-types` |
| N5  | IMPORTANT | Replace "see Open Questions" forward reference with inline rationale or back-reference to OQ5                                                                                        |
| N6  | MINOR     | Fix `/bin/cbsbuild` → `/runner/cbsbuild` in OQ7 resolution (auto-fixed if N3 is done correctly)                                                                                      |
| N7  | MINOR     | Correct OQ6 claim: `crt` does not import `ReleaseDesc`                                                                                                                               |
| N8  | MINOR     | Update `gen_run_name` sketch: `thread_rng()` → `rand::rng()`                                                                                                                         |
| C4  | ADVISORY  | Add rolling-deployment migration note to `§ Migration Strategy — M1`                                                                                                                 |
