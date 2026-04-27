# Design Review: cbscore Rust Port — Architecture & Subsystem Design

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`

**Companion review:**
`cbsd-rs/docs/cbscore-rs/reviews/001-20260420T1132-design-cbscore-project-structure-v1.md`

---

## Summary

**Verdict: approve-with-changes.**

The subsystem design is thorough and well-grounded in the Python
source. The capability mapping is accurate, the error taxonomy is
principled, and the runner state-machine section is the clearest
part of the whole pair of documents. Several findings require
attention before implementation starts: two are correctness issues
(the runner mounts the wrong path and SIGKILL semantics differ from
the Python), and several design decisions are made by omission in
ways that will create load-bearing ambiguity for implementors.

---

## Strengths

- **Runner state machine is complete.** The diagram plus mount-path
  table make it possible to implement the runner from the design
  alone without re-reading the Python source. The "Cleanup (always)"
  edge from every terminal state is the right discipline.
- **`SecureArg` trait design is correct.** Moving from the Python
  ABC pattern to a Rust trait + `CmdArg` enum is a genuine
  improvement: the type system now enforces that passwords reach
  `tokio::process::Command::arg` only as plaintext and reach
  `tracing` only as redacted. The `_sanitize_cmd` carry-over for
  the `--passphrase` bare-string case is the right conservative
  approach.
- **Error taxonomy is principled.** One enum per subsystem, all
  in `cbscore-types`, with `#[from]` for propagation and
  `anyhow` restricted to the binary boundary. This will make
  match exhaustiveness cheap across crate boundaries.
- **Migration milestones are independently revertable.** M0–M2 are
  entirely additive; nothing breaks if M2 is reverted to the Python
  wrapper. That is the right safety property for a production system.

---

## Findings

### F1 — Runner mounts the wrong path for the Rust binary

**Section:** "Runner Subsystem — In-container mount layout" table.

The design shows:

| `cbsbuild` binary (self) | `/runner/cbsbuild` |

But the entrypoint script (`cbscore-entrypoint.sh`) in the current
Python source invokes `cbsbuild` at:

```bash
cbsbuild --config "${RUNNER_PATH}/cbs-build.config.yaml" ${dbg} \
  runner build $*
```

i.e., it expects `cbsbuild` on `$PATH`, not at an absolute path. The
script prepends `${RUNNER_PATH}/bin:$PATH`, so to be found, the
binary must be mounted at `/runner/bin/cbsbuild`, not
`/runner/cbsbuild`.

The design also says the entrypoint script's "final line changes
from `exec python -m cbscore "$@"` to `exec /runner/cbsbuild "$@"`",
but the actual script does not end with `exec python -m cbscore "$@"`.
The final lines install cbscore via `uv tool install`, then call
`cbsbuild` by name via `$PATH`. The Rust port must either update the
entrypoint script to call `/runner/cbsbuild` directly (which is
simpler and removes the `$PATH` dependency) or mount the binary at
`/runner/bin/cbsbuild`. Either is fine, but the design must be
explicit about which choice is made.

**Action:** Re-read `cbscore/_tools/cbscore-entrypoint.sh` (current
content downloads uv, installs cbscore wheel, then calls `cbsbuild`
via `$PATH`). The Rust port's bundled entrypoint will be significantly
simpler — it just needs to call the binary. Specify the exact new
script content and the exact mount path in the design, replacing the
current handwavy "final line changes" description.

---

### F2 — Timeout cancellation calls SIGKILL; Python raises to caller

**Section:** "Subprocess & Secret Redaction — `async_run_cmd`".

The design says:

> On timeout or cancellation, calls `Child::start_kill()` (SIGKILL
> on unix) and waits — matches Python's `p.kill(); await p.wait()`.

But the Python `async_run_cmd` does **not** catch the exception:

```python
except (TimeoutError, asyncio.CancelledError):
    logger.error("async subprocess timed out or was cancelled")
    p.kill()
    _ = await p.wait()
    raise        # ← re-raises, caller must handle
```

The comment on that block even says "FIXME: evaluate all callers for
whether they are properly handling these exceptions." The Python
re-raises `TimeoutError` / `CancelledError` after killing the child;
the Rust design says the function returns `Result<RunOutcome,
CommandError>` — it is silent on what happens on timeout. If Rust
returns `Err(CommandError::Timeout)` (which is correct), that is a
behaviour change from the Python which re-raises `asyncio.TimeoutError`
(not a `CommandError`).

More concretely: `runner::run` calls `async_run_cmd` inside a
`tokio::time::timeout`. If `timeout` elapses, the `async_run_cmd`
future is dropped (cancellation). The design says the runner "reads
the cidfile and calls `podman_stop`" on cancellation — but if
`async_run_cmd` eats the cancellation and returns `Err`, the runner
never sees the `tokio::time::timeout` expiry as a `Future::drop`.
These two flows are mutually exclusive: either `async_run_cmd`
handles cancellation internally (and callers do not see it), or it
propagates via drop (and callers must use `select!`/`timeout`).

**Action:** Specify the exact contract:

1. `async_run_cmd` completes with `Err(CommandError::Timeout)` on
   timeout, or
2. `async_run_cmd` propagates via drop (the outer `timeout` wrapper
   cancels the future).

Then specify how `runner::run` observes the timeout and executes the
`podman_stop` cleanup in each case. The two must be consistent.

---

### F4 — `release_python_env` uses `which::which` but Python uses
         `shutil.which` — subtly different

**Section:** "Subprocess & Secret Redaction — `reset_python_env`".

The Python implementation (`utils/__init__.py` line 178):

```python
python3_loc = shutil.which("python3")
```

`shutil.which` respects `PATH` in the current environment. The
design says:

> the implementation detects the `python3` resolution via
> `which::which("python3")` and rewrites `PATH` the same way.

`which::which` in Rust also respects `PATH` by default. This is
correct. However, the Python code uses `Path.full_match`, a
glob-based matcher, to check whether the python3 binary lives under
`/usr/bin`. The Rust port must replicate the same rule ("strip the
directory containing the non-system python3 from PATH"). The design
does not state what "non-system" means (is `/usr/local/bin` system?
`/opt/homebrew/bin`?) and leaves the exact rewrite rule to the
implementor, which risks a subtle parity gap.

**Action:** State the exact PATH rewrite rule in prose: "strip any
path component that is a strict prefix of the resolved `python3`
binary path, unless that component is `/usr/bin`." Ideally, also
note that this flag is expected to become dead code once all
consumers stop running cbscore from inside a uv venv (per the Open
Questions section, which says the flag should be deleted in a later
cleanup — make that lifecycle explicit here).

---

### F5 — `cbsbuild build` drops the `--cbscore-path` flag with no
         documented UX break

**Section:** "CLI Surface" tree.

The Python `cbsbuild build` requires `--cbscore-path PATH` (type
`click.Path`, `required=True`). The Rust CLI surface shows no such
flag. As discussed in the `001` review (F2), this is correct for the
end-state but constitutes a CLI UX parity break that violates
Correctness Invariant 2 in CLAUDE.md.

The design does not note this as an intentional deviation or provide
a migration plan (e.g. deprecation period, notify callers). Any
operator script passing `--cbscore-path` will fail immediately when
the Rust binary ships.

**Action:** Identical to `001` F2: record this as a deliberate,
design-approved UX break in the "CLI Surface" section, with a note
on who needs to be notified and when the break lands (M1).

---

### F6 — Secrets model serde discriminator will not round-trip

**Section:** "Configuration & Secrets Subsystem — Secrets".

The design proposes:

```rust
#[serde(tag = "creds")]
enum GitCreds {
    #[serde(rename = "plain")]  Plain(GitPlainCreds),
    #[serde(rename = "vault")]  Vault(GitVaultCreds),
}

#[serde(untagged)]
enum GitPlainCreds {
    Ssh   { username: String, ssh_key: String },
    Token { username: String, token: String },
    Https { username: String, password: String },
}
```

The `#[serde(untagged)]` variant on `GitPlainCreds` is fragile:
serde tries each variant in declaration order and picks the first
that deserialises without error. `Token` and `Https` have the same
shape (`username: String, x: String`); only the field name
distinguishes them. Untagged deserialisation with `serde_yaml_ng`
deserialises YAML as a stringly-typed map and will match `Ssh` for
any struct that has a `username` field if `ssh-key` happens to be
absent (or defaults to empty). The Python uses shape inspection
explicitly, not discriminated unions.

**Action:** Either (a) use an explicit discriminator field on
`GitPlainCreds` (e.g. a `type: "ssh" | "token" | "https"` field),
or (b) add a golden-file round-trip test as a hard gate at M0 for
all three `GitCreds` variants. Note the fragility in the design as
a known risk and the mitigation chosen.

---

### F7 — `VersionDescriptor` serde key names are not validated

**Section:** "Version Descriptors — Descriptor".

The Python `VersionDescriptor` uses pydantic defaults — field names
are snake_case (`signed_off_by`, `el_version`) with no aliases. The
JSON output uses snake_case keys. The Rust struct in the design has
no `#[serde(rename_all)]` attribute:

```rust
pub struct VersionDescriptor {
    pub signed_off_by:  VersionSignedOffBy,
    pub el_version:     u32,
    ...
}
```

Rust field names default to their declared identifier on serde. As
long as the Rust fields match the Python JSON keys exactly
(snake_case), this is correct. However, the `Config` struct uses
`rename_all = "kebab-case"` (correct, matching Python's aliases),
and the design does not explicitly state that `VersionDescriptor`
must use snake_case. An implementor applying `rename_all =
"kebab-case"` uniformly (a natural mistake given the surrounding
context) would produce `signed-off-by` and `el-version` in the JSON,
breaking wire format.

**Action:** Add a note to the `VersionDescriptor` section:
"No `rename_all` — serde defaults to field names as declared
(snake_case), matching the Python JSON output directly." Mirror this
note for `ReleaseDesc` and `ReleaseComponentVersion`.

---

### F9 — `normalize_version` and `get_major_version` are missing
         from the port

**Section:** "Version Descriptors — `VersionType` and parsing".

`cbscore/versions/utils.py` exports five public functions:
`parse_version`, `get_version_type`, `parse_component_refs`,
`normalize_version`, and `get_major_version`. The design lists
only three:

```rust
pub fn parse_version(...)
pub fn get_version_type(...)
pub fn parse_component_refs(...)
```

`normalize_version` and `get_major_version` are not in the import
tables for any current consumer, but they are public and part of the
Python API surface. Omitting them from `cbscore-types::versions::utils`
now means they would need to be added later.

**Action:** Add `normalize_version` and `get_major_version` to the
`cbscore-types::versions::utils` module listing. If they are
intentionally excluded, say so.

---

## Cross-document notes

- **F1 (mount path)** contradicts `001`'s Runner Container section,
  which also describes the binary as mounted at `/runner/cbsbuild`.
  Both documents need the same correction.
- **F5 (CLI flag drop)** is the same issue as `001` F2. The decision
  must be recorded in exactly one document; the other should
  cross-reference it.
- The `cbscore-types` pulling `serde_json`/`serde_yaml_ng` (`001`
  F4) affects which crate owns the `VersionDescriptor::read/write`
  and `Config::load/store` impls described here. If those crates
  move out of `cbscore-types`, the "Types" vs "IO" boundary in this
  document remains clean; if they stay, the boundary blurs.

---

## Suggested follow-ups

- Fix the entrypoint mount path and rewrite the script content
  description to match the actual current script (§ Runner, F1).
- Specify the exact async cancellation / timeout contract for
  `async_run_cmd` and the runner cleanup path (§ Subprocess, F2).
- Write out the exact `PATH` rewrite rule for `reset_python_env`
  (§ Subprocess, F4).
- Record the `--cbscore-path` drop as an intentional UX break in
  the CLI surface section (§ CLI, F5).
- Add a note or test gate for the `GitPlainCreds` untagged serde
  fragility (§ Secrets, F6).
- Add explicit `#[serde]` annotations (or their absence) for
  `VersionDescriptor`, `ReleaseDesc`, `VersionComponent` to prevent
  an implementor applying `rename_all = "kebab-case"` uniformly
  (§ Version Descriptors, F7).
- Add `normalize_version` and `get_major_version` to the
  `cbscore-types::versions::utils` listing (§ Version Descriptors,
  F9).
