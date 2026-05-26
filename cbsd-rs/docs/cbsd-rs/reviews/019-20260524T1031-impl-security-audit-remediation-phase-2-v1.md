# Implementation Review: Security Audit Remediation — Phase 2 Mid

- **Series:** 019
- **Type:** impl
- **Version:** v1
- **Date:** 2026-05-24
- **Reviewer:** Staff Engineer (adversarial — no trust in implementer claims)
- **Commits in scope:**
  - `79c281e5` — tarball containment + decompression cap (D5/F7)
  - `3ab98d04` — REST body + WebSocket message size caps (D6/F8)
  - `e0006457` — OAuth `email_verified` check (D2/F2)
  - `5a9fe991` — split `periodic:manage` into `:own` and `:any` (D3 part 1/F4
    write-path)
  - `27dd41f1` — re-validate periodic task scopes at trigger time (D3 part 2/F4
    trigger/SI-15)
  - `7434a738` — redact bearer tokens from URI logging surface (D4/D9/F5)
- **Design doc:**
  `cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md`
- **Plan doc:**
  `cbsd-rs/docs/cbsd-rs/plans/019-20260516T1033-security-audit-remediation.md`
- **Test run:** `SQLX_OFFLINE=true cargo test --workspace` — **255 tests, 0
  failures**

---

## 1. Summary Assessment

The six commits address real security weaknesses and the production logic is
largely correct. Tarball containment is well-designed, the `email_verified` gate
is correctly ordered, the periodic scope split routes ownership checks through
the right predicate, and the URI-logging surface is properly narrowed. However,
the test coverage gaps are severe: the trigger re-validation security claim (the
central objective of commit 12) has no integration test exercising the threat
model scenarios, the WebSocket size-cap is enforced but never verified by a
test, the D9 tracing assertions the plan explicitly required are absent, and the
OAuth rejection uses the wrong HTTP status code. Score: **35/100** —
block-merge; the plan-required tests are not optional and three of them guard
the central SI-15 security claim.

---

## 2. Strengths

**Tarball containment (commit 8).** The two-phase design — lexical normalization
first, then `verify_parent_realpath_under` after `unpack_root.join()` —
correctly catches both traversal-via-components attacks and symlink-redirect
attacks independently. The `LimitedReader` wrapping `GzDecoder` with
`remaining = cap + 1` allows exactly-at-cap archives to succeed while rejecting
over-cap ones, which is the correct boundary semantics. Hardlink targets are
resolved archive-root-relative via `logical_normalize_within`, not relative to
the CWD, which closes the hardlink-escape vector. The PAX extended header path
override is handled correctly because the `tar` crate applies PAX overrides
transparently through `entry.path()` and `entry.link_name()`; the implementation
does not need a separate PAX code path. The test corpus (10+ fixtures including
PAX override, phase-2 TOCTOU, at-cap boundary) is comprehensive for the paths it
covers.

**OAuth `email_verified` gate (commit 10).** `validate_user_info()` checks
`email_verified` before the domain allowlist check, so a
verified-but-wrong-domain user sees a domain rejection while an unverified user
sees the verification rejection regardless of domain. The `#[serde(default)]` on
the field means a missing `email_verified` key defaults to `false`, not a
deserialization error, and the `alias = "verified_email"` covers the legacy
Google field name. Both paths surface an identical generic error message to the
browser, preventing oracle-style information leakage about the server's domain
configuration. Seven targeted unit tests cover: missing field, legacy alias,
verified-but- wrong-domain, unverified-with-matching-domain, allow-any bypass
attempt.

**Periodic scope split logic (commits 11–12).** `can_manage_task()` uses the
correct short-circuit: `any` wins without touching the task record, `:own`
requires the equality check against `task.created_by`. All four mutating
endpoints (update, delete, enable, disable) fetch the task row before the
capability check, so the owner comparison can never race with a task that hasn't
been loaded. The trigger- time robot cap strip mirrors `load_authed_user` using
the same `ROBOT_FORBIDDEN_CAPS` constant, preventing cap-set drift.
`caps_satisfy_trigger_requirements()` honors the `*` wildcard correctly, which
is critical for the admin role.

**URI logging (commit 13).** The TraceLayer custom span builder emits
`path = %request.uri().path()` exclusively. `on_response` and `on_failure`
closures do not re-emit the URI. The UI IIFE runs outside `DOMContentLoaded`
(i.e., inline before any other script sees the DOM), so `history.replaceState`
clears the fragment synchronously before any asynchronous callback could observe
it. CLAUDE.md invariant #8 documents the policy so future contributors cannot
inadvertently regress it.

**Shared limits constants.** `cbsd_common::limits` is the single source of truth
for all three size thresholds. The static assert
`WS_MAX_FRAME_BYTES <= WS_MAX_MESSAGE_BYTES` catches an obvious misconfiguration
at compile time. Both server-side `ws_upgrade` and client-side `ws_connect` read
from the same constants, eliminating the risk of asymmetric caps causing
premature protocol closures.

---

## 3. Blockers

### B1 — Trigger re-validation is untested at integration level (commit 12)

The plan required three integration tests for the SI-15 threat model
(`trigger_revalidation_scope_loss`, `trigger_revalidation_owner_deleted`,
`trigger_loop_continuity`). None exist. The security claim of commit 12 cannot
be verified from the test suite. See S1 in §4 for full detail and remediation
direction — promoted here because the missing tests are plan-required and guard
the primary security objective of this phase.

### B2 — WebSocket size-cap enforcement is untested (commit 9)

The plan required a test exercising the server's enforcement of
`WS_MAX_MESSAGE_BYTES` at the protocol layer. No such test exists. See S2 in §4
for remediation direction.

### B3 — D9 tracing-test log-capture assertions absent (commit 13)

The plan required `tracing-test`-based assertions that log output does not
contain query-string tokens. No such tests exist. See S3 in §4 for remediation
direction.

### B4 — OAuth callback returns 403, design specifies 401 (commit 10)

`StatusCode::FORBIDDEN` (403) is wrong for an authentication failure. The design
document (D2 section) specifies 401 Unauthorized, which is the correct HTTP
semantic for "authentication failed." This is a one-line fix
(`StatusCode::FORBIDDEN` → `StatusCode::UNAUTHORIZED`) and requires adding a
status-code assertion to the existing `oauth_callback_unverified_email_rejected`
test.

---

## 4. Serious Concerns

_B1–B4 above are the four blockers. The detail sections below expand on B1–B3's
remediation directions. B4 (OAuth 403 vs 401) is self-contained in §3._

### S1 — Trigger re-validation: remediation detail (B1)

**What:** `trigger_periodic_build()` in `scheduler/trigger.rs` is the
security-critical path that enforces the SI-15 claim ("a scope-reduced user
cannot trigger builds they should no longer be able to trigger"). The plan
explicitly required three integration test scenarios:

- `trigger_revalidation_scope_loss` — owner loses `periodic:create` or
  `builds:create` cap after task creation; trigger must reject.
- `trigger_revalidation_owner_deleted` — owner account deleted or deactivated;
  trigger must reject and disable the task.
- `trigger_loop_continuity` — one task failing with `OwnerAccountMissing` must
  not halt the scheduler loop; other tasks in the same tick must still fire.

**Only** `caps_satisfy_trigger_requirements` unit tests exist. These verify the
predicate function in isolation. No test exercises the full call path: scheduler
tick → `trigger_periodic_build` → `get_user` / `is_user_active` →
`caps_satisfy_trigger_requirements` → DB write / build dispatch / task disable.

**Why it matters:** The entire security claim of commit 12 is untested at the
integration level. If `is_user_active` returns the wrong value (e.g., a future
migration adds a soft-delete that `get_user` does not filter), or if the
`disable_with_error` call fails silently, the claim is broken and there is no
test to catch it. The scheduler loop `continue`-on-error behavior has the same
gap.

**Direction:** Add three `#[tokio::test]` integration tests in
`scheduler/trigger.rs` or a dedicated `scheduler/tests.rs` file that use a real
in-memory SQLite pool:

1. Create a user + task, strip a required cap, call `trigger_periodic_build`,
   assert it returns `Err(TriggerError::CapabilityInsufficient)` (or whichever
   variant maps to scope loss).
2. Create a user + task, deactivate the user (`UPDATE users SET active = 0`),
   assert the trigger returns the owner-missing error and the task row has
   `enabled = false`.
3. Create two tasks (one with a broken owner, one healthy), run one scheduler
   tick, assert the healthy task fires despite the broken task failing.

### S2 — WebSocket size-cap enforcement: remediation detail (B2)

**What:** The plan required two test scenarios for the WebSocket message size
cap:

- Server rejects a frame/message over `WS_MAX_MESSAGE_BYTES` with a
  protocol-level close (not a panic or silent drop).
- A frame just under `WS_MAX_FRAME_BYTES` is accepted normally.

No such tests exist. The `limits_match_design` test in `cbsd-common` only
asserts the constant values; it does not exercise axum's enforcement of them.

**Why it matters:** The constants are wired in, but there is no test verifying
that axum actually enforces them at the protocol layer for the specific
`/api/ws/worker` upgrade path. A future refactor that accidentally moves the
`ws.max_message_size()` call to the wrong scope (e.g., inside a conditional
branch) would not be caught.

**Direction:** Add an integration test in `cbsd-server/src/ws/` that establishes
a real WebSocket connection to a test server instance (using
`axum::test::TestClient` or a `TcpListener`-based approach), sends a message
whose serialized size exceeds `WS_MAX_MESSAGE_BYTES`, and asserts the connection
receives a Close frame with protocol error rather than the message being
processed.

### S3 — D9 tracing-test log-capture assertions: remediation detail (B3)

**What:** The plan explicitly required `tracing-test`-based assertions capturing
actual log output and asserting it does not contain the literal query string for
two scenarios:

- A `GET /?cli-token=abc123` request: log output must contain the path `/` but
  must NOT contain `cli-token` or the token value.
- A request that triggers a panic-handler or 5xx path: same assertion on the
  error log.

The implementation correctly narrows the span to `Uri::path()`, but this is only
verified by reading the code. No test captures the actual `tracing` output and
asserts absence of the sensitive string.

**Why it matters:** A future contributor could add a
`tracing::error!("{:?}", request)` line in a middleware or handler (where
`request` includes the full URI) and break the invariant without any test
failing. The plan identified this exact risk and required the assertions to
guard against it.

**Direction:** Add
`tracing-test = { version = "...", features = ["no-env-logger"] }` to
dev-dependencies and write two tests using `#[tracing_test::traced_test]`:

1. Issue a request to any endpoint with a recognizable query string token value;
   assert `logs_contain("?")` is false, assert `logs_contain` the path is true.
2. Issue a request that triggers a `on_failure` path (e.g., a connection reset);
   assert the error log does not contain the URI.

---

## 5. Minor Issues

- **M1 — Migration 008 missing step 3 (seed role cap grants for new installs).**
  The design (D3, migration step 3) states: "seed roles `admin` and `developer`
  are updated in the same migration to grant `:any` and `:own` respectively."
  Migration `008_periodic_manage_split.sql` contains only
  `DELETE FROM role_caps WHERE cap = 'periodic:manage'` and does not INSERT the
  new caps.

  Factual note: the `builder` role (the closest match to the design's
  "developer") never held `periodic:manage` in `db/seed.rs` — its cap list is
  `builds:create`, `builds:revoke:own`, `builds:list:own/any`,
  `apikeys:create:own`, `workers:view`, `channels:view`, with no periodic cap.
  This means the migration's DELETE is a no-op for all three builtin seed roles
  and **there is no production data loss on upgrade.** The gap is instead for
  new installs: the design intended `builder` accounts to have
  `periodic:manage:own` out of the box, but neither the migration nor the seed
  grants it. Builder-role users on a fresh install cannot manage their own
  periodic tasks without manual admin intervention. The fix is to add
  `periodic:manage:own` to the `builder` cap list in `db/seed.rs`.

- **M2 — `docs/rbac.md` not updated.** Three occurrences of `periodic:manage`
  remain in `docs/rbac.md` (lines 96, 347, and the capability table at lines
  449–452). These are stale: the cap no longer exists after migration 008.
  Operators reading this document to configure role caps will be misled into
  granting a cap that is a no-op on any system running migration 008 or later.

- **M4 — Directory entries skip phase-2 real-path check.** The design (D5)
  states the real-path check applies "for every entry (symlink, regular file,
  directory, or hardlink)." The `validate_and_unpack` match arm for
  `EntryType::Directory` calls only `create_dir_all` without calling
  `verify_parent_realpath_under`. In practice, directories do not write file
  contents, so a directory traversal cannot plant a file; but a symlink-followed
  directory creation could land outside the unpack root if a prior symlink entry
  was not checked. The existing phase-2 TOCTOU test covers a
  symlink-then-regular-file sequence, not a symlink-then-directory sequence. The
  implementation is lower-risk than for regular files but deviates from the
  stated invariant.

---

## 6. Suggestions

- **SG1 — Consider `DECOMPRESS_CAP` documentation in `limits.rs`.** The
  decompression cap constant in `cbsd_common::limits` is present but has no
  inline comment linking it to the attack scenario it defends against (zip-bomb
  / decompression-bomb). Adding a one-line comment ("Prevents decompression-bomb
  attacks; see design doc 019 §D5") would help future maintainers understand why
  the cap exists and what raising it would cost.

- **SG2 — `disable_with_error` string should be a constant.** `scheduler/mod.rs`
  passes the string `"owner_account_missing"` as a literal to
  `disable_with_error`. If this string is matched anywhere (monitoring,
  alerting, operator documentation), a typo in a future call site would silently
  produce a different disable reason with no compile-time error. Promoting it to
  a `const` in `trigger.rs` or `scheduler/mod.rs` would eliminate that risk.

---

## 7. Open Questions

- **OQ1 — What is the intended behavior when a periodic task's owner has
  `periodic:manage:own` but not `periodic:create` or `builds:create`?** The
  trigger re-validation checks `TRIGGER_REQUIRED_CAPS` (`periodic:create` +
  `builds:create`) but not `periodic:manage:own`. A user could therefore retain
  the ability to trigger builds after losing management access. Is this
  intentional? The design is silent on this edge case.

- **OQ2 — Is `docs/rbac.md` considered authoritative?** If operators use this
  document to configure role caps, the M2 stale references to `periodic:manage`
  could cause them to grant a cap that no longer exists. Clarifying whether this
  file is auto-generated or hand-maintained would inform who is responsible for
  keeping it current.

---

## 8. Confidence Score

| Item                                                           | Points | Description                                                                                                      |
| -------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------- |
| Starting score                                                 | 100    |                                                                                                                  |
| D5: No trigger integration tests                               | -15    | `trigger_periodic_build` untested at integration level for the three SI-15 threat scenarios required by the plan |
| D5: No WS size-cap enforcement test                            | -15    | Plan required WS over-cap close test; only constant-value assertions exist                                       |
| D5: No D9 tracing-test assertions                              | -15    | Plan required `tracing-test` log-capture assertions; absent entirely                                             |
| D8: OAuth callback returns 403, design says 401                | -5     | Spec deviation; 403 incorrect HTTP semantics for authentication failure                                          |
| D8: Directory entries skip phase-2 real-path check             | -5     | Design says "every entry"; Directory arm omits `verify_parent_realpath_under`                                    |
| D10: Migration 008 missing step 3 cap grants                   | -5     | Design step 3 says seed roles get new caps in the migration; not implemented                                     |
| D10: `docs/rbac.md` stale — still references `periodic:manage` | -5     | Convention violation: doc update required by the commit that splits the cap                                      |
| **Total**                                                      | **35** |                                                                                                                  |

### Interpretation: 35/100 — Block Merge

Per the confidence-scoring rubric, scores below 50 require major rework and
block merge. The production security logic is correct for every commit in scope,
but the score reflects four distinct plan-required test categories that are
entirely absent (three D5 deductions at -15 each) plus three additional
deductions for spec deviations and convention violations. The missing tests are
not optional: the plan identified them as required, and three of them guard the
central SI-15 security claim of this phase. The blockers are all additive (test
additions + a one-line status-code fix); no production code needs to be
reverted.

---

## 9. Recommendation

**Block merge.** Score 35/100 falls in the "major rework needed" band of the
confidence-scoring rubric. All blockers are additive — no production code needs
to be reverted or redesigned. The required actions before this branch can merge:

1. **(B1) Required:** Add the three trigger integration tests (`scope_loss`,
   `owner_deleted`, `loop_continuity`). The SI-15 claim that this phase exists
   to deliver is unverifiable without them.
2. **(B2) Required:** Add a WebSocket over-cap integration test verifying the
   protocol-level Close frame is sent for a message exceeding
   `WS_MAX_MESSAGE_BYTES`.
3. **(B3) Required:** Add `tracing-test`-based log-capture assertions confirming
   query-string token values do not appear in trace output.
4. **(B4) Required:** Change `StatusCode::FORBIDDEN` to
   `StatusCode::UNAUTHORIZED` in the OAuth callback rejection path and add a
   status-code assertion to the existing
   `oauth_callback_unverified_email_rejected` test.

After B1–B4 are addressed, the following should be fixed before the next review
cycle:

- **(M1)** Add `periodic:manage:own` to the `builder` role cap list in
  `db/seed.rs` (new-install gap).
- **(M2)** Update `docs/rbac.md` to replace `periodic:manage` references with
  `:own` / `:any` variants.
- **(M3)** Add `verify_parent_realpath_under` to the `Directory` arm in
  `validate_and_unpack` (spec conformance).

**Blockers: 4 (B1–B4). Minor findings to track: 3 (M1–M3).**
