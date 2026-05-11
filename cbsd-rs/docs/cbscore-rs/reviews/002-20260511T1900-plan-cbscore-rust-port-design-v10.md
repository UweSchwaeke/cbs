# Plan Review — cbscore Rust Port (v10)

**Scope:** Confirmation pass — verify the three v9 findings (V9-N1, V9-S1,
V9-S2) are cleanly closed by commit `0528fe0`, then fresh-eyes sweep of Phase 4
and the unchanged plans for new issues introduced by the fixes.

**Artefacts reviewed:**

- `002-20260508T1558-04-runner.md` — Phase 4 (M1.3, modified by `0528fe0`)
- `002-20260508T1558-01-types.md` — Phase 1 (M0, unchanged since v9)
- `002-20260508T1558-02-subprocess-and-shell-tools.md` — Phase 2 (M1.1,
  unchanged since v9)
- `002-20260508T1558-03-storage-and-secrets.md` — Phase 3 (M1.2, unchanged since
  v9)
- `plans/README.md` — index (unchanged since v9)

---

## 1. Summary Assessment

All three v9 findings are closed by `0528fe0`. The destructuring mechanism for
guard consumption is now unambiguous, the two-layer timeout architecture is
documented with correct wrapping rules, and both negative tests name their
concrete `VersionError` variants. The fresh-eyes sweep found no new blockers or
major concerns; one minor issue is noted in the RunnerError taxonomy note for
Timeout, and one observation on the outer-cancellation interaction is documented
as a suggestion. **Phase 4 is clear to proceed to implementation.**

---

## 2. v9 Finding Closures

### V9-N1 — CLOSED

Phase 4 Commit 1 §Testable now has two explicit negative tests (lines 112–113):

> Negative test: read a JSON file missing `schema_version` →
> `Err(VersionError::MissingSchemaVersion)`. A second negative test with
> `schema_version: 99` (above the compiled-in max) →
> `Err(VersionError::UnknownSchemaVersion { found: 99, max_supported: 1 })`.

Both variants are named precisely, matching the §Design constraints text and
Phase 1 Commit 2's `versions/errors.rs` spec. No residual hedge text
(`Err(VersionError::…)`) remains in Phase 4 C1 §Testable. Closed cleanly.

### V9-S1 — CLOSED

The "Guard-consumption ordering" subsection is present in §State machine §4
Cleanup. The mechanism is stated unambiguously: `cleanup` takes the guard by
value and destructures it on its first line via
`let Guard { tempfiles, components_dir, cidfile, container_name } = guard;`. The
text explicitly states that the guard binding no longer exists after
destructuring, so the `Drop` impl does not fire on subsequent `?`-early-returns
inside `cleanup`. The §Design constraints "Cleanup always runs" bullet
references the ordering rule with "see §State machine §4 above for the ordering
rule". Closed cleanly.

### V9-S2 — CLOSED

The "Two-layer timeout architecture, two distinct error variants" bullet is
present in §Design constraints. It names both variants (`RunnerError::Timeout`
for the outer `tokio::time::timeout` elapsed, `CommandError::Timeout` for the
inner `async_run_cmd`-owned timeout), explains the mapping (`Elapsed` →
`RunnerError::Timeout` directly), states the inner variant is surfaced via
`RunnerError::Command(CommandError::Timeout)` consistent with the `#[from]`
wrapping in Phase 1 Commit 2's `RunnerError` spec, and references "no default —
Phase 2 Commit 1 requires it to be supplied per-call" for the inner timeout. The
wrapping rule is consistent with Phase 1 C2's `runner/errors.rs` spec. Closed
cleanly.

---

## 3. Blockers

None.

---

## 4. Major Concerns

None.

---

## 5. Minor Issues

### V10-N1 — RunnerError::Timeout label slightly misleading in Phase 1 C2 spec

**What.** Phase 1 Commit 2's `runner/errors.rs` spec names the `Timeout` variant
with the parenthetical "the internal `tokio::time::timeout` elapsed" (line 175
of the Phase 1 plan). The v9 S2 fix in Phase 4 now correctly documents this
variant as the **outer** runner-level `tokio::time::timeout` (4-hour build
budget), not an internal one. The two descriptions are inconsistent: Phase 1
calls it "internal", Phase 4 calls it "the whole runner pass exceeded its
budget".

**Why it matters.** A future implementer reading Phase 1 in isolation may
mistakenly treat `RunnerError::Timeout` as an internal per-subprocess timeout,
which conflicts with Phase 4's two-layer architecture. The Phase 4 text is
authoritative; Phase 1's parenthetical just needs the adjective removed or
corrected.

**Fix.** In Phase 1 C2's `runner/errors.rs` spec, change "the internal
`tokio::time::timeout` elapsed" to "the outer runner-level
`tokio::time::timeout` elapsed (the whole-build budget; see Phase 4 C3 §Design
constraints for the two-layer architecture)".

---

## 6. Suggestions

### V10-S1 — Confirm outer-cancellation child-kill relies on Phase 2 RAII guard, not cidfile cleanup

The two-layer timeout bullet states: "When the outer fires, the runner does NOT
re-wrap an inner `CommandError`; the future is simply dropped and the outer maps
`Elapsed` → `RunnerError::Timeout` directly." This is architecturally correct.
However, the next sentence in §Design constraints (the existing
`tokio::time::timeout` bullet, line 284–288) says: "On elapsed, read the cidfile
and call `podman_stop(name=cid, timeout=1s)`…. The internal-timeout contract on
`async_run_cmd` (Phase 2 Commit 1) handles the child kill."

Clarifying the sequencing would help the implementer: when the outer
`tokio::time::timeout` fires, the `async_run_cmd` future is dropped. Phase 2
Commit 1's RAII drop guard fires synchronously on that drop, calling
`Child::start_kill()` (non-blocking signal delivery). The runner's subsequent
cidfile read + `podman_stop` is the podman-side cleanup (so `--replace` works on
the next run), not the actual child kill — that has already been sent by the
drop guard. The two bullets together imply this but never state the sequencing
explicitly. Consider adding one sentence bridging them: "The child process kill
has already been initiated by Phase 2 C1's RAII drop guard firing synchronously
on the dropped future; the cidfile read + `podman_stop` that follows is for
podman-registry cleanup only."

This is purely a documentation gap; the design is architecturally sound.

---

## 7. Open Questions

None.

---

## 8. Fresh-Eyes Sweep

### Guard struct field consistency — no new issue

The destructuring pattern
`let Guard { tempfiles, components_dir, cidfile, container_name } = guard;`
(§State machine §4) uses four fields. The RAII guard is described as owning
"tempfile paths + components tempdir + container name + cidfile path" (line
234–235). The four fields map directly: `tempfiles` → tempfile paths,
`components_dir` → components tempdir, `cidfile` → cidfile path,
`container_name` → container name. The destructuring is field-consistent with
the struct description. No new issue.

### Outer-timeout + async_run_cmd RAII interaction — sound

When the outer `tokio::time::timeout` fires, the inner `async_run_cmd` future is
dropped. Phase 2 Commit 1 specifies a RAII drop guard that calls
`Child::start_kill()` in its `Drop` impl, best-effort. `start_kill()` is
non-blocking (signal delivery only; no wait). The drop guard fires synchronously
as part of the future-drop sequence, before the outer timeout branch proceeds.
The child process receives the kill signal. The subsequent `podman_stop` from
the runner then stops the podman-managed container. This chain is functionally
correct and does not require `async_run_cmd` to be polled further after the
outer timeout fires. No blocking issue; noted as V10-S1 above for clarity.

### RunOpts::timeout field type — consistent

Phase 4 C3 declares `pub struct RunOpts { pub timeout: Duration, … }`. The
two-layer timeout bullet says the inner timeout is "supplied per-call" and "no
default". Phase 2 C1's `RunOpts` lists "timeout / cwd / extra_env / out_cb"
without an `Option` qualifier, and Phase 2 C1's testable section passes a
concrete 100ms timeout. Both are consistent with `timeout: Duration` (mandatory,
no default) rather than `Option<Duration>`. No conflict.

### LOC arithmetic — estimate still valid

`0528fe0` added 42 lines and removed 12 lines of documentation prose (net +30
lines in the plan file). The ~700 LOC estimate for Commit 3 refers to
implementation code, not plan prose. The estimate is unchanged and valid. No
update needed.

### Phase 1/2/3 plans — no change, no new issues

Spot-check confirms Phase 1, Phase 2, Phase 3, and the README are byte-for-byte
unchanged since v9. No new issues were introduced in those files.

---

## Verdict

**All three v9 findings are closed. Phase 4 is ready for implementation start.**
One new minor (V10-N1): Phase 1 C2's `RunnerError::Timeout` parenthetical says
"internal `tokio::time::timeout` elapsed" where it should say "outer
runner-level `tokio::time::timeout`". Fix is a one-line edit to Phase 1, not a
blocker.

- **New findings by severity:** 0 blockers, 0 major, 1 minor, 1 suggestion.
- **Phase 4 is ready for implementation start.**
