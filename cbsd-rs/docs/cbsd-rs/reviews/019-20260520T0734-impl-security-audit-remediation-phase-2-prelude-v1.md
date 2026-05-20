# Implementation Review — Security Audit Remediation Phase 2 Prelude v1

**Reviewed commits:**

- `113fe04` —
  `cbsd-rs/server: test try_dispatch send-failure rollback end-to-end` (Phase 1
  carry-over NB1)
- `0d6eafe` — `cbsd-rs: enforce strict CBSD_DEV parsing and loopback dev-mode`
  (Phase 2 Commit 7)

**Design:**
`docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` (§D1)

**Plan:** `docs/cbsd-rs/plans/019-20260516T1033-security-audit-remediation.md`
(Phase 1 carry-over NB1; Phase 2 Commit 7)

**Reviewer:** independent adversarial pass, no trust in implementer claims

---

## 1. Summary Assessment

Both commits are production-ready with minor reservations. The Phase 1
carry-over test (`113fe04`) is correct and sound: it forces a real send failure,
exercises the full rollback path, and asserts all seven WCP D4 SI-6/SI-13
postconditions. The Phase 2 Commit 7 (`0d6eafe`) correctly introduces
`cbsd-common`, migrates all five CBSD_DEV call sites, and closes the
authority-confusion gap with a parsed-URL loopback check. The implementation is
functionally complete for the security objective. Two plan-specified tests are
absent — the worker startup integration tests for `CBSD_DEV=false` NoVerifier
assertion and dev+non-loopback refusal — and four design-enumerated
`is_loopback_url` test cases are missing. These are non-blocking for the
security goal (the production guard itself is correct) but are explicit test
commitments in both plan and design that must be honoured.

Recommendation: **accept with fixups** before the next phase lands. Required
fixups are limited to test additions; no production code changes are needed.

---

## 2. Strengths

**`is_loopback_url` operates on parsed `url::Host`.** The three-way match over
`url::Host::Domain` / `Ipv4` / `Ipv6` is exactly what the design specifies. It
correctly rejects authority confusion (`wss://localhost@evil.com/` — `url` crate
identifies the host as `evil.com`), `wss://localhost.evil.com/`, path-level
confusion (`wss://user:pass@evil.com/localhost`), and non-loopback public IPs. A
naive `starts_with` check would have admitted several of these.

**Migration is complete and verifiable.** All five CBSD_DEV check points across
`cbsd-server` (config.rs, main.rs) and `cbsd-worker` (config.rs twice, main.rs)
now delegate to `cbsd_common::is_truthy_env`. No raw `std::env::var("CBSD_DEV")`
remnants exist in any `.rs`, `.toml`, `.yaml`, or `.sh` file. The
`TRUTHY_VALUES` closed set is enforced in a single place; `CBSD_DEV=0` and
`CBSD_DEV=false` can no longer silently enable the NoVerifier bypass.

**Loopback check fires at resolve-time, before TLS config.**
`WorkerConfig::resolve()` returns `Err(ConfigError::Validation(...))` for
dev+non-loopback before any `ResolvedWorkerConfig` is produced. The `connect()`
function in `ws/connection.rs` receives a `&ResolvedWorkerConfig` that can only
exist if the loopback check passed. There is no window where `dev_mode = true`
is propagated to `dev_tls_config()` / `NoVerifier` for a non-loopback host.

**NB1 test exercises the full dispatch-rollback pipeline.** The test constructs
a real `AppState` with a real SQLite pool and a real component tarball
directory. It registers a closed-receiver sender, calls `try_dispatch`, and
asserts all seven postconditions: `state = queued`, `worker_id = NULL`,
`trace_id = NULL`, `error = NULL`, `started_at = NULL`, `finished_at = NULL`,
`build_report = NULL`, active map entry removed, build re-enqueued, log watcher
removed. This would catch a regression in any of the six provenance columns or
any omission in `rollback_active_to_queued`.

**`cbsd-common` crate is correctly scoped.** The new crate has no IO or async
dependencies (only stdlib). Both server and worker depend on it. The
`is_loopback_url` predicate is correctly kept in `cbsd-worker` as a per-binary
concern. The crate boundary is clean.

**`temp_component_dir` helper is correctly implemented.** It creates a real
`TempDir` with a named subdirectory and a `placeholder` file, which is the
minimum structure required by `try_dispatch`'s tarball assembly path. The helper
is gated behind `#![cfg(test)]` and will not appear in production binaries.

**Compose file compatibility.** `podman-compose.cbsd-rs.yaml` uses
`CBSD_DEV: "1"` for both server and worker containers. `"1"` is in the accepted
truthy set; existing dev deployments are not broken by the semantics tightening.

**WARN log does not echo the raw value.** `main.rs` emits
`"CBSD_DEV is set — TLS certificate verification is disabled"` without including
the raw env var value. This satisfies the plan's security note that the value
might be a misconfigured secret.

---

## 3. Blockers

None.

---

## 4. Major Concerns

None. The security objective (preventing NoVerifier from being enabled for
non-loopback hosts, and preventing `CBSD_DEV=false` from enabling dev mode) is
correctly implemented in production code.

---

## 5. Minor Issues

### F1 — Two plan-required worker startup integration tests are absent

**Criterion:** D5 (untested critical path).

The plan (§Commit 7, lines 524–527) explicitly enumerates four test groups:

1. `is_truthy_env` unit tests — implemented in `cbsd-common`.
2. `is_loopback_url` unit tests — partially implemented (see F2).
3. Worker startup test: `CBSD_DEV=false` does NOT install `NoVerifier`.
4. Worker startup test: dev mode + non-loopback `server_url` causes startup
   refusal.

Tests 3 and 4 are absent. This is not a theoretical omission:

- Test 3 verifies that the `NoVerifier` bypass is unreachable when dev mode is
  off. Without it, a future refactor that re-introduces a non-empty check
  (`!var.is_empty()`) in `config.rs` would pass all existing tests.
- Test 4 verifies the loopback guard in `resolve()` end-to-end. Without it, a
  typo in the guard condition (e.g., negation inversion) would be silently
  missed.

Both tests can be written as synchronous unit tests in `config.rs` because
`resolve()` is synchronous: set env vars, construct a `WorkerConfig`, call
`resolve()`, assert `Ok` or `Err`. The `CBSD_DEV` env var must be set and
cleared carefully (or use a wrapper that scopes the mutation) to avoid test
interaction in parallel test runs.

**Resolution:** Add `resolve_refuses_dev_with_non_loopback_url()` and an
additional test exercising `resolve()` with `CBSD_DEV=false` and a non-loopback
URL, asserting the result is `Ok` (not refused) and
`resolved.dev_mode == false`.

### F2 — Four design-specified `is_loopback_url` cases are untested

**Criterion:** D5/D8 (untested critical path, spec deviation).

Design §D1 lines 316–319 enumerate exactly ten `is_loopback_url` test cases. The
implementation tests seven of the ten true/false pairs:

Implemented:

- `wss://localhost` → true
- `wss://LOCALHOST` → true
- `wss://127.0.0.1` → true (via `accepts_ipv4_loopback`)
- `wss://[::1]` → true (via `accepts_ipv6_loopback`)
- `wss://example.com` → false
- `wss://localhost@evil.com` → false (authority confusion)
- `wss://10.0.0.1` is implicitly covered by `rejects_public_ipv4` via
  `wss://192.168.1.1`

Missing from the design-specified set:

- `wss://127.0.0.2` → true (confirms the full `127.0.0.0/8` range is accepted,
  not just `127.0.0.1`)
- `wss://[::1]:8443/x` → true (confirms port+path do not affect result for IPv6)
- `wss://127.0.0.1.evil.com` → false (confirms DNS subdomain crafted to look
  like a dotted-quad is rejected — currently `rejects_public_ipv4` tests only
  numeric IPs)

The `wss://10.0.0.1` case is debatable: `rejects_public_ipv4` covers
`wss://192.168.1.1`, which hits the same `!is_loopback()` path. That one is
acceptable as covered. The three listed above are distinct enough in the failure
mode they guard against to warrant explicit cases.

**Resolution:** Add three tests to `config.rs`: `accepts_full_127_0_0_range`
(using `wss://127.0.0.2`), `accepts_ipv6_with_port_and_path` (using
`wss://[::1]:8443/x`), and `rejects_localhost_subdomain_crafted_as_ip` (using
`wss://127.0.0.1.evil.com`).

---

## 6. Nits

### N1 — Module path deviation: `cbsd_common::is_truthy_env` vs design's `cbsd_common::env::is_truthy_env`

Design §D1 line 268: `cbsd_common::env::is_truthy_env(var: &str) -> bool`. The
implementation places `is_truthy_env` at the crate root
(`cbsd_common::is_truthy_env`), not under an `env` submodule.

This is not a correctness issue. The decision to omit the `env` submodule is
defensible for a two-function crate. The gap is noted for awareness; if
`cbsd-common` grows additional modules in later commits, the design's `env`
submodule namespace may still be worth honouring for consistency.

### N2 — WARN log lacks structured `value = true` field

Design §D1 lines 274–277 specify that the WARN log names `CBSD_DEV` "and the
value observed (redacted to a fixed `true`/`false` boolean)." The implementation
emits a plain prose string:

```rust
tracing::warn!("CBSD_DEV is set — TLS certificate verification is disabled");
```

The intent of the design's structured field (e.g.,
`tracing::warn!(cbsd_dev = true, "...")`) is to make the fact machine-searchable
in structured log pipelines. The current log line communicates the information
in prose, which is operationally sufficient. The deviation is minor.

### N3 — `drop(tempdir)` after assertions is dead code with misleading comment

In `try_dispatch_send_failure_rolls_back_db_end_to_end`, the final line is:

```rust
drop(tempdir); // keep the component dir alive until after assertions
```

`TempDir` is already dropped at end-of-scope. The explicit `drop` is a no-op
that slightly post-dates the assertions, not at the start of the function. The
comment implies the caller intended to ensure the directory was not cleaned up
prematurely, but the natural drop order already guarantees this. The line is
harmless and should be removed to avoid misleading future readers.

---

## 7. Open Questions

None that block progression.

---

## 8. Plan / Design Fidelity Checklist

| Requirement                                                         | Source               | Status                                           |
| ------------------------------------------------------------------- | -------------------- | ------------------------------------------------ |
| `cbsd_common` crate introduced with `is_truthy_env`                 | Plan §C7             | Implemented                                      |
| Closed truthy set `{1, true, yes, on}` (case-insensitive)           | Design §D1           | Implemented                                      |
| All CBSD_DEV call sites migrated (server + worker)                  | Plan §C7             | Implemented — 5/5 sites                          |
| `is_loopback_url` operates on parsed `url::Host`                    | Design §D1, Plan §C7 | Implemented                                      |
| Dev mode + non-loopback → startup refused                           | Design §D1           | Implemented                                      |
| WARN log does not echo raw CBSD_DEV value                           | Plan §C7 pitfalls    | Implemented                                      |
| WARN log structured boolean field                                   | Design §D1           | Partial — prose string, no structured field (N2) |
| `is_truthy_env` unit tests (accepted + rejected sets, unset)        | Plan §C7             | Implemented                                      |
| `is_loopback_url` 8 tests incl. authority-confusion negatives       | Plan §C7             | Partial — 7/10 design cases; 3 missing (F2)      |
| Worker startup test: `CBSD_DEV=false` → no NoVerifier               | Plan §C7             | Missing (F1)                                     |
| Worker startup test: dev + non-loopback → refused                   | Plan §C7             | Missing (F1)                                     |
| NB1: `try_dispatch_send_failure_rolls_back_db_end_to_end`           | Phase 1 carry-over   | Implemented                                      |
| All 7 WCP D4 SI-6/SI-13 postconditions asserted in NB1              | Phase 1 review NB1   | Implemented                                      |
| `test_app_state_with_components_dir` + `temp_component_dir` helpers | Phase 1 review NB1   | Implemented                                      |
| `cbsd_common` path: `cbsd_common::env::is_truthy_env`               | Design §D1           | Nit — no `env` submodule (N1)                    |

---

## 9. Confidence Score

| Item                                                                                         | Points | Description                                                                                                                            |
| -------------------------------------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| Starting score                                                                               | 100    |                                                                                                                                        |
| D5: `CBSD_DEV=false` → no NoVerifier startup test absent                                     | −15    | Plan §C7 explicitly requires this test                                                                                                 |
| D5: dev+non-loopback startup refusal test absent                                             | −10    | Plan §C7 explicitly requires this test (combined with above finding — one integration test covers both directions, so split deduction) |
| D5/D8: `wss://127.0.0.2`, `wss://[::1]:8443/x`, `wss://127.0.0.1.evil.com` test cases absent | −10    | Design §D1 lines 316-319 enumerate these explicitly                                                                                    |
| D8: `cbsd_common::env` submodule omitted                                                     | −5     | Design §D1 line 268 specifies `cbsd_common::env::is_truthy_env`                                                                        |
| **Total**                                                                                    | **60** |                                                                                                                                        |

**Interpretation:** Significant issues. Addresses the most critical correctness
gaps but leaves plan-committed tests unwritten. Must address before the next
phase lands.

---

## 10. Recommendation

**Accept with fixups.** The production security controls introduced by these two
commits are correct and complete: `CBSD_DEV=false` no longer enables dev mode,
authority-confusion is blocked by parsed-URL host matching, and the NoVerifier
bypass is unreachable for non-loopback servers. The NB1 regression guard is
sound.

The required fixups, in priority order:

1. **(F1, -25 pts combined)** Add two integration tests in
   `cbsd-worker/src/config.rs`:
   - `resolve_with_dev_false_does_not_set_dev_mode()` — call `resolve()` with
     `CBSD_DEV` absent or set to `"false"`, assert `Ok(r)` and
     `r.dev_mode == false`.
   - `resolve_refuses_dev_mode_with_non_loopback_url()` — call `resolve()` with
     `CBSD_DEV=1` and a non-loopback server URL, assert
     `Err(ConfigError::Validation(_))` containing `"loopback"` or
     `"non-loopback"`.

2. **(F2, -10 pts)** Add three `is_loopback_url` unit tests covering
   `wss://127.0.0.2`, `wss://[::1]:8443/x`, and `wss://127.0.0.1.evil.com`.

3. **(N3, nit)** Remove the dead `drop(tempdir)` line and its misleading comment
   from `try_dispatch_send_failure_rolls_back_db_end_to_end`.

None of these require changes to production code. The security objective of
audit-rem D1 / audit F1 is met.
