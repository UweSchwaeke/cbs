# Plan Review — cbscore Rust Port (v8)

**Scope:** Phase 4 (new — primary focus) + sanity recheck of Phases 1–3 and the
README against Phase 4's additions. All v1–v7 findings are closed; this review
does not re-litigate them.

**Artefacts reviewed:**

- `002-20260508T1558-04-runner.md` — Phase 4 (M1.3 runner subsystem)
- `002-20260508T1558-01-types.md` — Phase 1 (M0, sanity only)
- `002-20260508T1558-02-subprocess-and-shell-tools.md` — Phase 2 (M1.1, sanity
  only)
- `002-20260508T1558-03-storage-and-secrets.md` — Phase 3 (M1.2, sanity only)
- `plans/README.md` — index and dependency graph
- Design 002 §Runner Subsystem (lines 741–864), §Wire-Format Versioning, §Error
  Taxonomy, §Version Descriptors & Creation

---

## 1. Summary Assessment

Phase 4 is well-structured and the three-commit decomposition is sound. The
state-machine alignment with design 002, the cross-phase dependency citations,
the `camino-tempfile` usage, the `dump_to_runner` mount-contract closure, and
the commit-size rationales all check out. Two issues prevent implementation
start: the RAII cleanup guard description is impossible as written (async
functions cannot be awaited inside `Drop`), and the vault-config mount
description is wrong in both mechanism and API (no `VaultConfig::store` function
exists, and none is needed — `Config.vault` is a path, not an inline struct).
Fix those two before coding begins. Three minors cover undeclared error-variant
names that will surface at compile time; they belong in Phase 1's spec, not in
the implementer's lap.

**Verdict: requires revision. Phase 4 is NOT ready for implementation start.**

---

## 2. Strengths

- **Design alignment — state machine.** The Idle → Preparing → Spawning →
  Running → Finished/Failed/Stopped → Cleanup sequence matches design 002 lines
  746–772 exactly, including the "Cleanup always" edge.
- **Mount table — 9 rows, all present.** All nine design 002 rows (lines
  779–789) appear in the plan table, with optional vault and ccache entries
  correctly gated on `is_some()`.
- **`CBS_DEBUG` and `HOME=/runner` both captured.** Design 002 lines 816–819 and
  821–831 are each a dedicated bullet under Design constraints — neither is
  buried or implied.
- **Two-layer timeout faithful.** The `tokio::time::timeout` +
  `podman --timeout` dual-layer contract (design 002 lines 848–856) is stated
  precisely, including the cidfile-read-then-`podman_stop` cleanup on elapsed.
- **Cross-phase citations all verified.** `async_run_cmd` → Phase 2 C1 ✓;
  `Config::store` → Phase 3 C4 ✓; `SecretsMgr::dump_to_runner` → Phase 3 C3 ✓.
  Phase 3 §Out of scope mount-contract handoff is explicitly closed by Phase 4
  C3 ✓.
- **`camino-tempfile` used, not `tempfile`.** Phase 4 C3 names
  `camino_tempfile::NamedUtf8TempFile`, consistent with the v7 N-M1 fix that
  added `camino-tempfile` to Phase 1 C1's Cargo.toml spec.
- **`gen_run_name` matches design exactly.**
  `rand::seq::IteratorRandom:: choose_multiple` over `'a'..='z'`, length 10,
  `"ces_"` default — matches design 002 lines 833–844 and Python
  `random.choices(ascii_lowercase, k=10)`.
- **`stop` → `podman stop --time 1 --all` correct.** `Option<&str>` with `None`
  → `--all` matches Python `utils/podman.py` line 134
  (`name if name else "--all"`).
- **Commit-size rationales present.** All three below-floor commits (C1 ~150
  LOC, C2 ~200 LOC) include the rationale paragraph the pattern established in
  Phase 2.
- **Integration tests follow `#[ignore]` convention.** Both runner-run and
  SIGTERM smoke tests are `#[ignore]`-able, consistent with the P3-S1 pattern.

---

## 3. Blockers

### P4-B1 — RAII cleanup guard: `Drop` cannot await async functions

**What.** Phase 4 C3 §Design constraints says:

> Implement via an RAII guard struct that owns the tempfile paths + components
> tempdir + container name, with a `Drop` impl that calls `tokio::fs::remove_*`
> and `podman_stop` best-effort.

Both `tokio::fs::remove_file` and `podman_stop` are `async fn`. Rust's `Drop`
trait is synchronous — `.await` is a compile error inside `Drop`. The
cleanup-on-panic test in §Testable also relies on this guard working at
panic-time. As described, the guard does not compile.

**Why it matters.** This is not an implementation detail — the implementer
cannot resolve it without a design decision that changes the public cleanup
contract. Getting it wrong produces either a guard that silently no-ops on drop
(defeating the entire cleanup-always guarantee) or a panic inside `Drop` (which
aborts the process).

**Resolution direction.** Choose one strategy and name it explicitly in the
plan:

1. **Sync std fallback (recommended for simplicity).** In `Drop`, use
   `std::fs::remove_file` / `std::fs::remove_dir_all` (blocking, ~microseconds)
   and
   `std::process::Command::new("podman").args(["stop", "--time", "1", name]).status().ok()`
   (blocking, ~1 s). Acceptable best-effort because `Drop` is already
   best-effort and the blocking duration is bounded by `podman stop --time 1`.
2. **Explicit `async fn cleanup()` + sync `Drop`.** Call `cleanup().await` on
   every explicit return path (success, error, cancellation). The `Drop` impl
   handles only the panic / future-drop case and falls back to the sync approach
   for async work it cannot await.
3. **`Handle::block_on` in `Drop`.** `Handle::current().block_on(async { … })`.
   Requires a tokio runtime to be live, which is guaranteed during normal
   execution but fragile in test contexts.

Option 1 or 2 are both safe choices. Pick one, state it in the plan, and align
the cleanup-on-panic test description with the chosen mechanism.

---

## 4. Major Concerns

### P4-M1 — Vault mount: wrong mechanism, nonexistent API

**What.** Phase 4 C3 §State machine, Preparing step says:

> `cbs-build.vault.yaml` (write via the same `Config::store`-style YAML helper
> if `config.vault.is_some()`; otherwise skip the mount)

Two problems:

1. **Wrong type.** Design 002 line 449 shows `Config.vault: Option<Utf8PathBuf>`
   — a path to an existing vault YAML file on disk, not an inline `VaultConfig`
   struct. No serialisation is needed. The Python runner (runner.py line
   268–270) mounts the file by its existing path directly:
   `config.vault.resolve() .as_posix()`. The Rust runner should do the same:
   mount `config.vault` (the `Utf8PathBuf`) directly as the host side of the
   `/runner/cbs-build.vault.yaml` volume.

2. **Nonexistent API.** Phase 3 does not expose `VaultConfig::store` (or any
   standalone function to write a `VaultConfig` as YAML). `Config::store`
   serialises the full `Config`, not `VaultConfig` independently. No such
   function is planned in any phase.

**Why it matters.** An implementer following the plan will spend time looking
for an API that does not exist, and if they build one ad hoc they deviate from
both design 002 and Python semantics. The vault file is not a generated temp
file — it is operator-supplied and must be mounted as-is.

**Resolution.** Replace the Preparing-step bullet with:

> - `cbs-build.vault.yaml`: if `config.vault` is `Some(path)`, add
>   `path → /runner/cbs-build.vault.yaml` to the mount table directly (no temp
>   copy; the existing file is mounted read-only, matching Python runner.py line
>   268–270). If `None`, omit the mount.

No Phase 3 API change needed. The mount is unconditionally read-only (`:ro`)
since the container never writes back to the vault config.

---

## 5. Minor Issues

### P4-N1 — `RunnerError::Cancelled` variant not declared in Phase 1

**What.** Phase 4 C3 §Testable SIGTERM smoke test expects
`Err(RunnerError::Cancelled)`. This variant name does not appear in Phase 1 C2's
`runner/errors.rs` spec, in design 002's error taxonomy (which names only
`RunnerError::Timeout` and `RunnerError::Command(e)` at lines 1046–1048), or
anywhere else in the plan corpus.

**Why it matters.** The implementer of Phase 1 C2 will not know to add this
variant. When Phase 4 C3 references it, it may get an ad hoc name that does not
match the test, or the variant may be missing entirely.

**Fix.** Add `Cancelled` to Phase 1 C2's `runner/errors.rs` variant list with a
brief note: "`Cancelled` — SIGTERM received while the container was running;
`podman_stop` was called, container exited gracefully."

### P4-N2 — `VersionError` missing-schema-version variant is unnamed

**What.** Phase 4 C1 §Design constraints says reads without `schema_version`
produce:

> `VersionError::InvalidDescriptor(MissingSchemaVersion { … }) or the equivalent variant from Phase 1's VersionError`

The hedge "or the equivalent variant" signals that the actual variant name is
undefined. Phase 1 C2 lists `VersionError::InvalidDescriptor` and
`VersionError::NoSuchDescriptor` but no missing-schema-version variant. The
`VersionedVersionDescriptor::into_latest()` deserialisation path must produce a
typed error for absent `schema_version`; `VersionError::InvalidDescriptor` alone
does not encode the "schema_version absent" case unambiguously (it would be
confused with a malformed field value).

**Fix.** Add `MissingSchemaVersion` to Phase 1 C2's `versions/errors.rs` spec,
parallel to `ConfigError::MissingSchemaVersion`. Update Phase 4 C1 §Design
constraints to reference it by the exact name.

### P4-N3 — Phase 1 §Out of scope wording implies descriptor IO lands in Phase 3

**What.** Phase 1 §Out of scope opens with:

> Any IO. `cbscore::config::Config::load`, secrets-manager IO, descriptor-store
> walks — all land in Phase 3.

The phrase "all land in Phase 3" is inaccurate:
`versions::desc::read_descriptor` and `write_descriptor` land in Phase 4 C1, not
Phase 3. Phase 3 §Goal and §Out of scope do not mention descriptor IO. An
implementer reading Phase 1 §Out of scope in isolation would assume descriptor
IO belongs in Phase 3 and be surprised when Phase 4 introduces it.

**Fix.** Add a bullet to Phase 1 §Out of scope:

> `versions::desc` IO (`read_descriptor`, `write_descriptor`) — lands in Phase 4
> C1, where the runner is the first consumer of the write side.

Optionally update the "all land in Phase 3" sentence to "all non-descriptor IO
lands in Phase 3."

---

## 6. Suggestions

### P4-S1 — Document `current_exe()` symlink resolution behaviour

Phase 4 C3 says: "The `cbsbuild` binary self-mount uses
`std::env::current_exe()` to find the running binary's host-side path." On
Linux, `current_exe()` resolves through symlinks and returns the real binary
path (via `/proc/self/exe`). This is exactly the correct behaviour for the
bind-mount use case — mounting the real binary, not the symlink. A brief note
prevents implementers from second-guessing and adding a redundant
`canonicalize()` call (which would be a no-op but adds noise):

> Note: `std::env::current_exe()` follows symlinks on Linux and returns the
> absolute, realpath of the running binary. No additional `canonicalize()` is
> needed.

### P4-S2 — Add `:ro` to the `cbsbuild` binary mount

The plan's mount table lists `/runner/cbsbuild` without a read-only flag. Adding
`:ro` to the binary bind-mount is a security hardening that prevents the
container from accidentally (or maliciously) overwriting the host binary. The
same applies to the descriptor, config, and secrets mounts (all of which are
read by the in-container `cbsbuild`, never written back). Consider listing mount
options in the table for the three non-scratch, non-ccache mounts.

### P4-S3 — Note `/var/lib/containers:Z` SELinux conditionality explicitly

The `:Z` suffix is present in both Python (`runner.py` line 264) and the plan
(mount table). On non-SELinux hosts, `:Z` is silently ignored by podman. This is
intentional and correct; a brief note in the plan's mount table or §Design
constraints confirming "`:Z` is a no-op on non-SELinux hosts; unconditional here
matches Python" prevents future readers from proposing a config-gated
conditional.

---

## 7. Open Questions

None new. All prior open questions are confirmed closed.

---

## 8. Cross-Phase Sanity — Phases 1 + 2 + 3

**No regressions introduced by Phase 4.** Detailed recheck:

- **Phase 1 §Out of scope** — see N3 above; wording is imprecise but not
  blocking. The actual phase boundaries in the plan corpus are consistent.
- **Phase 2 §End-of-phase acceptance lift-out grep** — the grep covers
  `utils::subprocess`, `utils::git`, `utils::git/errors`. Phase 4 adds
  `runner/{mod,run}`, which are not lift-out candidates. No impact.
- **Phase 3 §Out of scope mount contract** — correctly closed by Phase 4 C3:
  tempfile created via `camino-tempfile::NamedUtf8TempFile` with mode 0600,
  `dump_to_runner(path)` called, path passed to podman mount table. Verified.
- **`nix` dep (Phase 1 C1, added in v5)** — needed for Phase 2 C1 RAII smoke
  test. Phase 4 C3 SIGTERM smoke test sends SIGTERM to the test process; `nix`
  is the natural tool (`nix::sys::signal::kill(Pid::this(), Signal::SIGTERM)`)
  and is already listed in Phase 1's cbscore Cargo.toml. No new dep needed.
- **README** — Phase 4 filename `002-20260508T1558-04-runner.md` exists on disk;
  the link resolves. Dependency graph remains strictly linear (Phase 4 depends
  on Phase 3 depends on Phase 2 depends on Phase 1). Consistent.
- **`camino-tempfile` (v7 N-M1)** — closed in `809f43a`; Phase 1 C1 Cargo.toml
  spec now includes it. Phase 4 C3 uses it. Consistent.

---

## Verdict

**Phase 4 requires revision before implementation start.**

- **1 blocker** (P4-B1): async `Drop` is not implementable as described. Must
  specify the concrete cleanup strategy (sync std fallback recommended).
- **1 major** (P4-M1): vault mount mechanism is wrong and references a
  nonexistent API. `Config.vault: Option<Utf8PathBuf>` — mount the existing file
  directly.
- **3 minors** (P4-N1, P4-N2, P4-N3): undeclared `RunnerError::Cancelled`
  variant, unnamed `VersionError` missing-schema-version variant, imprecise
  Phase 1 §Out of scope wording. All require small edits to Phase 1 C2 and Phase
  1 §Out of scope; they surface at compile time if not fixed.

Phases 1, 2, and 3 are free of regression.
