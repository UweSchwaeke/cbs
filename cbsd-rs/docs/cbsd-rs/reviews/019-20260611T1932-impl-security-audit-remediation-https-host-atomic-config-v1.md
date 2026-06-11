# Commit 17 — HTTPS enforcement and atomic config write

**Commit:** `025a1ed0`\
**Subject:** `cbsd-rs/cbc: enforce HTTPS host and write config atomically`\
**Closes:** audit-rem D8 (atomic config write) / F11 (bearer tokens over HTTP)\
**Phase:** 2, Commit 17 of 18\
**Reviewer:** Claude Sonnet 4.6 (adversarial; no trust in implementer claims)\
**Date:** 2026-06-11

---

## Verdict: GO

No blockers. No major issues. Two minor findings, one nit. All security-critical
paths verified from first principles. The commit is safe to autosquash and
merge.

---

## Confidence Score

| Item                                                 | Points | Description                                                                                                                                                                                                                                                                                                               |
| ---------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Starting score                                       | 100    |                                                                                                                                                                                                                                                                                                                           |
| D5: CLI warning emission test absent                 | -5     | Design doc lists "CLI with `--insecure-http http://x` → emits warning on stderr" as a required test; no such test exists — the `warnings()` unit test verifies the return value but not that `run()` calls `eprintln!`. Partial deduction: the wiring is clearly visible in `main.rs:107-109` and is trivially auditable. |
| D8: Design says "debug level"; code uses `eprintln!` | -5     | The D8 section of design doc 019 says cleanup failures "should be logged at the debug level". `cbc` has no `tracing` dependency; `eprintln!` is correct and arguably better (a stray-temp warning deserves stderr visibility). The deviation is in the design doc, not the code.                                          |
| **Total**                                            | **90** |                                                                                                                                                                                                                                                                                                                           |

---

## Summary Assessment

Commit 17 correctly closes both audit-rem targets. The scheme gate
(`parse_base_url`) runs in both `CbcClient::new` and
`CbcClient::unauthenticated` and returns an error before any token material is
formatted or attached to a request, eliminating the bearer- over-HTTP exposure
(F11). The config write rewrites the save path from a write-then-chmod sequence
to an `OpenOptionsExt::mode(0o600)` + `create_new(true)` + `fs::rename` atomic
sequence, eliminating the world-readable window (D8 / F11). The `ClientOpts`
refactor is mechanical and uniform across all thirteen call-site files. Tests
pass clean (16/16), clippy is silent, and format is clean.

---

## Strengths

**Scheme gate runs before token attachment in both constructors.** In
`CbcClient::new` (`client.rs:68`), `parse_base_url` is called and `?`-returns
before `format!("Bearer {}", token.expose_secret())` at line 71.
`CbcClient::unauthenticated` also calls `parse_base_url` immediately. There is
no third `parse_base_url` call site and no bypass path — an adversarial search
of the crate confirmed both constructors are the only two build points.

**Scheme logic is correct and complete.** `https` is always accepted; `http` is
accepted only with `insecure_http`; all other schemes are rejected even with
`insecure_http`. The error string matches the design verbatim:
`"host must be https; got: {scheme}"`. Four scheme-coverage tests plus a
`warnings()` matrix test verify these branches.

**`--insecure-http` is genuinely independent of `--no-tls-verify`.** The flags
map to separate `bool` fields in `ClientOpts`. `insecure_http` gates
`parse_base_url`; `no_tls_verify` gates `danger_accept_invalid_certs`. There is
no cross-gating and no implicit coupling.

**Mode `0o600` is set at creation time, never after.**
`OpenOptionsExt ::mode(0o600)` combined with `create_new(true)` means the inode
is born with the correct mode — there is no instant where the temp file is
world-readable. The subsequent `fs::rename` atomically replaces the target, so a
concurrent reader sees either the previous `0o600` file or the new one, never a
transitional state. The race test
(`save_never_exposes_permissive_mode_to_concurrent_readers`, 200 saves + reader
thread) cannot false-fail against the atomic implementation; it would have
caught the old write-then-chmod pattern probabilistically over 200 iterations,
making it a genuine regression guard.

**`ClientOpts` is safely `Copy` and `Debug`.** All three fields are `bool`;
`Debug` output exposes no secret material. The token remains a separate
`&SecretString` argument, never a field of `ClientOpts`.

**Cleanup is exhaustive.** The write-result and rename-result funnel through a
shared `if result.is_err()` block. Both write failure and rename failure trigger
cleanup. The original error is preserved — the cleanup path only `eprintln!`s
its own secondary errors and never returns them. `NotFound` is silently
swallowed (temp may have been partially created before the write, or never
created), which is correct.

**Commit passes the smell test.** One-sentence purpose; previous commit compiles
and is unaffected; the commit is safely revertable; every added symbol
(`ClientOpts`, `warnings()`, `parse_base_url`, `temp_path_for`, `insecure_http`
flag) has at least one caller within the same commit.

---

## Blockers

None.

---

## Major Concerns

None.

---

## Minor Issues

**M1 (D5) — Design-specified CLI warning emission test not implemented.**

The D8 section of design doc 019 lists the following test as required: "CLI with
`--insecure-http http://x` → command succeeds in test mode and emits the warning
on stderr." The existing `warnings()` unit test verifies that
`ClientOpts::warnings()` returns the expected strings; it does not verify that
`run()` calls `eprintln!` on them. The wiring in `main.rs:107-109` is plainly
visible and trivially correct, but the design asked for an E2E wire test and it
was not written. A future refactor that moved or removed the
`for warning in opts.warnings()` loop would not be caught by the existing test
suite.

Resolution: add a test (or document in the design that this test was
intentionally deferred to a subsequent CLI integration test harness).

**M2 (D8) — Design doc says "debug level" for cleanup logging; code uses
`eprintln!`.**

The D8 section of design doc 019 says cleanup failures on the temp file "should
be logged at the debug level." The implementation uses `eprintln!`, which writes
to stderr unconditionally. This is actually correct — `cbc` has no `tracing`
dependency and `eprintln!` is the appropriate mechanism for a best-effort
warning that a stray temp file may remain on disk. The deviation is imprecision
in the design doc, not an error in the code. The design doc should be updated to
read "logged to stderr" rather than "at the debug level."

Resolution: update design doc 019 D8 section wording in a follow-on fixup.

---

## Nit

**N1 — PID recycling can cause `create_new(true)` to fail spuriously on stale
temp files.**

`temp_path_for` constructs names as `config.json.<pid>.<counter>.tmp`. In the
normal case (clean exits, cleanup on error paths) this is fine. In the narrow
case where a previous `cbc` invocation crashed (SIGKILL, power loss) after
creating the temp file but before cleanup, the temp file persists on disk. If
the OS later recycles the same PID and the new process starts its `AtomicU64`
counter at 0, `create_new(true)` returns `AlreadyExists`, and `save()` fails
with a config error. This is not a security issue and is recoverable by
re-running `cbc login`.

Mitigation options: append a short random suffix (e.g., 4 hex bytes), or retry
on `ErrorKind::AlreadyExists` with an incremented counter. Neither is urgent.

---

## Open Questions

None. All design items for Commit 17 are accounted for.
