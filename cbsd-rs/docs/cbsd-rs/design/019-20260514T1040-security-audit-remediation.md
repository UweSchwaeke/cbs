# Security Audit Remediation

| Field    | Value                                                                                                                                                             |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Design   | 019 (sibling: `worker-control-plane-hardening`)                                                                                                                   |
| Date     | 2026-05-14                                                                                                                                                        |
| Status   | Draft v8                                                                                                                                                          |
| Packages | `cbsd-server`, `cbsd-worker`, `cbsd-proto`, `cbc`, `migrations/`, `ui/`                                                                                           |
| Inputs   | Prior security review `cbsd-rs/docs/000-20264026T1104-security-review.md`; audit reviews `cbsd-rs/docs/cbsd-rs/reviews/019-20260512T2339-…-v1.md` and `…-v1.1.md` |

## Position in the Security Work

This document is the **natural progression** of the earlier worker control plane
hardening design (seq 019, timestamp `20260426T1154`). The two designs are
siblings at the same sequence number and together constitute the full cbsd-rs
security remediation plan:

- The worker-control-plane-hardening design (referred to throughout this
  document as **"the WCP design"** for brevity) owns the worker control plane
  trust boundary: lifecycle ownership, dispatch rollback, descriptor validation,
  bounded log tailing, reconnect ownership, and the associated worker-side
  active-build state model.
- This design owns the cross-cutting remediation: authentication, RBAC, CLI
  transport, worker input safety, resource limits, token-material redaction. It
  also picks up unresolved open items from the WCP design's v10 review (see D11,
  D12, D13).

Neither design is sufficient on its own. References below to "the WCP design"
mean specifically
`cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md`.
References to "this design" or "design 019 audit-remediation" mean this file.

## Revision History

- **v8 (2026-05-16)** — addresses the v7 design review
  (`cbsd-rs/docs/cbsd-rs/reviews/019-20260516T0644-design-security-audit-remediation-v7.md`).
  Closes all three Minor findings substantively. NF-1-v7: removes the duplicate
  `use crate::arch::Arch;` and `use crate::build::{…}` imports from the v7
  sketch — verified against `cbsd-proto/src/ws.rs` lines 134-140 that the
  existing `mod tests` block already imports those via `use super::*` plus its
  own explicit lines, so the v8 sketch only adds
  `use serde_json::{Value, json};` and `use strum::IntoEnumIterator;`. NF-2-v7:
  removes the dead `case_tags: HashSet<&'static str>` allocation and the
  `let _ = &case_tags;` suppression; the v8 test loop iterates the new tag enum
  directly. NF-3-v7: closes the previously-acknowledged "witness updated, case
  forgotten" gap by introducing a `#[cfg(test)]` companion enum
  `ServerMessageTag` with `#[derive(strum::EnumIter)]` in
  `cbsd-proto/src/ws.rs::tests`.
  `ServerMessageTag::from_message(&ServerMessage)` is the compile-time witness
  (exhaustive on `ServerMessage`); `ServerMessageTag::as_wire(&self)` is the
  compile-time wire-tag mapping (exhaustive on `ServerMessageTag`);
  `ServerMessageTag::iter()` (from `strum::IntoEnumIterator`) gives runtime
  enumeration of all known tag enum variants. The test loop iterates `iter()`
  and asserts each tag has a sentinel and a case. Adding a `ServerMessage`
  variant cascades through three compile-time gates (witness, tag-enum, as_wire)
  before reaching the runtime sentinel/case gates — no manual list maintenance,
  no "known gap" remaining. Requires adding
  `strum = { version = "0.26", features = ["derive"] }` to a new
  `[dev-dependencies]` section of `cbsd-proto/Cargo.toml`. Verified:
  `cbsd-proto/Cargo.toml` currently has only three workspace dependencies
  (`serde`, `serde_json`, `chrono`) and no `[dev-dependencies]` section, so this
  is a small additive change scoped to test builds. All field shapes and imports
  in the v8 sketch are re-verified against the source.
- **v7 (2026-05-16)** — addresses the v6 design review
  (`cbsd-rs/docs/cbsd-rs/reviews/019-20260516T0626-design-security-audit-remediation-v6.md`).
  Closes one Critical, three Minor findings. NF-1-v6 (Critical): the v6 D13-T6
  sketch called `BuildDescriptor::default()` and
  `Box::new(BuildDescriptor::default())`, but `BuildDescriptor` and its nested
  types (`BuildSignedOffBy`, `BuildDestImage`, `BuildComponent`, `BuildTarget`)
  do **not** impl `Default` in `cbsd-proto/src/build.rs`. Same class of "test
  sketch fails to compile" defect as v5's NF-1. v7 replaces every `::default()`
  call with explicit field construction, mirroring the existing pattern at
  `cbsd-proto/src/ws.rs::tests::server_message_build_new_round_trip` (lines
  142-182). A new `test_descriptor()` helper performs the explicit construction;
  a `test_descriptor_json()` helper produces the JSON form via
  `serde_json::to_value`. NF-6 (Minor): the v6 sketch had a hardcoded tag list
  as a fourth maintenance point not described by the "three layers of
  protection" prose. v7 removes the hardcoded list — the runtime test iterates
  over `cases()` directly, leaving three coordinated lists (witness,
  `sentinel_for_tag`, `cases`) and three protection layers honestly described.
  NF-7 (Minor): the undefined `minimal_descriptor_json()` is replaced by the
  defined `test_descriptor_json()` helper. NF-8 (Minor): test placement is now
  specified — D13-T6 lives in `cbsd-proto/src/ws.rs::tests` (same module as the
  existing wire-shape tests), guaranteeing access to the full enum and
  same-crate exhaustive-match semantics. All field shapes in the v7 sketch are
  verified against the actual `cbsd-proto/src/{ws,build, arch}.rs` source rather
  than assumed.
- **v6 (2026-05-16)** — addresses the v5 design review
  (`cbsd-rs/docs/cbsd-rs/reviews/019-20260516T0447-design-security-audit-remediation-v5.md`).
  Closes one Critical, four Minor findings. NF-1: the v5 D13-T6 witness/cases
  sketch referenced a non-existent `ServerMessage::UnauthorizedBuildAction`
  variant (proposed in the WCP design but not yet in `cbsd-proto/src/ws.rs`) and
  omitted the actual `Error` variant; v6 rewrites the sketch against the real
  `cbsd-proto/src/ws.rs` (current variants: `BuildNew`, `BuildRevoke`,
  `Welcome`, `Error`) and explicitly notes that the WCP-proposed variants must
  be added to the witness+cases when they land. NF-2: the v5 SI-17 said
  `Option<Instant>` (std) while D13-T7 required `tokio::time::pause()` (which
  only controls `tokio::time::Instant`); v6 pins the supervisor flag's type to
  `Option<tokio::time::Instant>` in SI-17, with the `CLOCK_MONOTONIC` caveat
  unchanged because `tokio::time::Instant` wraps `std::time::Instant`. NF-3: the
  runtime exhaustiveness check in D13-T6 is now spelled out concretely (a
  per-variant sentinel constructor passes each variant through the witness; the
  returned tag is asserted to be in the cases-map keyset) rather than elided.
  NF-4: the `cfg(feature = "soft-delete-schema")` approach for the D3
  soft-delete test is reworked to be compatible with `sqlx::migrate!()`
  compile-time embedding — the test fixture runs an **inline**
  `ALTER TABLE users ADD COLUMN deleted_at TIMESTAMP NULL` after the standard
  migration set, gated at the test-function level rather than via a forked
  migrations directory. NF-5: the v5 history's attribution of the MF-3 fix is
  rephrased to "a prior revision-history entry" to sidestep the off-by-one
  argument.
- **v5 (2026-05-16)** — addresses the v4 design review
  (`cbsd-rs/docs/cbsd-rs/reviews/019-20260515T1059-design-security-audit-remediation-v4.md`).
  Closes both Significant findings and all three Minor findings. SF-1 expands
  D13-T6 from a single-variant test into a per-variant test loop guarded by an
  **exhaustive-match witness** function — adding a new `ServerMessage` variant
  without a corresponding test entry now fails to compile, turning SI-18's "no
  `deny_unknown_fields` on any variant" rule into a compile-time gate plus a
  runtime per-variant deserialization check. SF-2 resolves the soft-delete
  schema contradiction by rewriting the D3 contract as **conditional on schema
  state**: when the production schema lacks a soft-delete column (current
  state), the only "owner is no longer a valid identity" case is row absence;
  the soft-delete filter MUST be added only when a future migration introduces
  the column. The test fixture's `deleted_at` column and the
  `D3-T-owner-soft-deleted` test are now explicitly forward-protection, gated
  behind a `soft-delete-schema` `cfg` feature that is off by default; the
  default test suite runs only the hard-delete test. The three Minor findings
  are addressed: D13-T7 timing tests adopt `tokio::time::pause()` / `advance()`
  for deterministic boundary checks; SI-17 gains an explicit caveat that
  `Instant` does not advance during host suspension on Linux (`CLOCK_MONOTONIC`
  semantics), with operator guidance; the "SM-C transition" terminology drift in
  a prior revision-history entry is corrected to "new authenticated reconnect
  event" since SM-C is a trigger input, not a state machine with persistent
  state.
- **v4 (2026-05-15)** — addresses the v3 design review
  (`cbsd-rs/docs/cbsd-rs/reviews/019-20260514T2248-design-security-audit-remediation-v3.md`).
  Closes the three new findings introduced by v3's text: NF-1 adds an explicit
  `cbsd-proto` regression test (D13-T6) that pins the `deny_unknown_fields`
  prohibition on `ServerMessage`, turning a load-bearing convention into a CI
  gate; NF-2 makes the SM-C anti-coercion predicate concrete by defining a
  per-supervisor `last_authenticated_connect_at: Option<Instant>` flag with a
  `MIGRATION_RECENT_WINDOW = 30s`, plus test D13-T7 covering the false-predicate
  fallback; NF-3 adds test D3-T-owner-deleted to the D3 matrix and resolves the
  soft-delete-vs-hard-delete ambiguity by specifying that any soft-delete marker
  (`deleted_at IS NOT NULL` or equivalent) is treated as equivalent to row
  absence for trigger-time scope resolution.
- **v3 (2026-05-14)** — addresses the v2 design review
  (`cbsd-rs/docs/cbsd-rs/reviews/019-20260514T2227-design-security-audit-remediation-v2.md`).
  Closes all five new findings introduced by v2's text: N-1 pins the
  `BuildRevoke.reason` field's serde representation (`Option<BuildRevokeReason>`
  with `#[serde(default, skip_serializing_if = "Option::is_none")]`) so old
  workers on rolling upgrades treat the field as absent (Admin semantics) and
  v2's drain-then-revoke F-R1 closure is not silently undone at the rollout
  boundary; N-2 corrects the CI gate exemption marker from `# allow-expose` (not
  a Rust line comment) to `// allow-expose` throughout D10; N-3 rewrites the
  misleading chained-symlink test example and reframes Phase 2 of D5's
  containment check as defense-in-depth, with a clearer test set; N-4 adds an
  explicit normative invariant for the owner-deleted case in D3's trigger-time
  scope re-validation; N-5 documents the `secrecy::ExposeSecret` trait import
  requirement at every `.expose_secret()` call site.
- **v2 (2026-05-14)** — addresses the v1 design review
  (`cbsd-rs/docs/cbsd-rs/reviews/019-20260514T1752-design-security-audit-remediation-v1.md`).
  Resolves both Critical findings and the High finding plus all six Minor /
  Significant findings: D13 `terminal-pending-report` now drains-then-revoke
  rather than discarding the result; D10 makes the `Secret<T>` redaction
  guarantee construction-tight by forbidding `Serialize`/`Deserialize` and
  requiring `.expose_secret()`; D5 acknowledges PAX extended-header paths and
  replaces the logical-only containment check with a two-phase (logical +
  real-path) check against already-unpacked entries; D1 replaces the prose
  loopback list with a concrete `url::Host` algorithm; D3 adds trigger-time
  scope re-validation and an explicit custom-role migration spec; D4 requires
  `history.replaceState` after token extraction; D8 documents Windows rename
  atomicity; D9 broadens to a project-wide URI-logging policy; D2 documents the
  residual userinfo trust gap; Phase E gains an explicit WCP-prerequisite note;
  the SM-S diagram is extended with the previously-missing transitions.
- **v1 (2026-05-14)** — initial draft consolidating remediation for all open
  security-audit findings, including reclassified ones from v1.1 of the audit.
  In-flight revisions on the same day resolved the open questions in the draft
  based on maintainer feedback: D6 drops the per-log-line cap (free-form build
  output is trusted from authenticated workers), the CI grep gate is deferred
  behind a tooling-comparison roadmap item, and the WS message and tarball
  decompression caps are accepted at their proposed defaults. A subsequent
  in-flight revision added D11, D12, and D13 covering the three open items left
  over from the WCP design's v10 review (worker `accepted` phase reconnect,
  liveness/dead-worker policy for receipt-aware `dispatched`, and
  superseded-live-connection stop-work delivery). The WCP design's v11 policy is
  unchanged by this work; D11–D13 are additive and live in this design because
  the user opted to keep the WCP design frozen at v11 and let this design carry
  the remaining open items.

## Problem

The cbsd-rs implementation has two open security reviews:

1. The original audit `000-…-security-review.md` flagged four issues in the
   worker control plane. The WCP design covers the policy fix for those issues
   at v11 but is still in review (no v11 review document) and unimplemented.
2. The follow-up audit (review 019 v1 + v1.1) found additional issues across
   authentication, authorization, transport, worker input handling, and resource
   bounds.
3. The WCP design's v10 review left three open items unresolved at v11
   (accepted-phase reconnect, receipt-aware liveness, superseded-live stop
   path). This design carries them.

This design covers the **remediation** of every open finding that is not already
addressed in the WCP design at v11, **plus** the three v10-review items the WCP
design did not close.

Findings addressed here:

- F1 — Worker `CBSD_DEV` truthiness disables TLS verification (Critical)
- F2 — OAuth callback does not verify `email_verified` (High)
- F4 — Periodic-task descriptor privilege transfer (High)
- F5 — CLI login query-string leaks token to server access logs (High)
- F7 — Worker tarball unpack: symlink containment + decompression cap (Medium)
- F8 — No JSON/WebSocket/log-line size limits (Medium)
- F10 — `api_keys.key_prefix` lacks standalone index (Low)
- F11 — `cbc` accepts `http://` URLs; config-write TOCTOU window (High)
- F13 — Token material in logs (must-fix policy)
- WCP v10 review open items 1–3 (worker `accepted` reconnect; receipt-aware
  liveness; superseded-live stop). Covered by D11–D13 below.

Findings deferred / dismissed (reference only, no code change here):

- F3 — `worker_status(Building)` reconnect rewrites ownership: covered by the
  WCP design (`Same-worker connection migration`, `Reconnect Ownership`).
- F6 — server has no TLS support: reclassified to deployment policy. Roadmap
  item in `cbsd-rs/docs/ROADMAP.md`.
- F9 — `python3` resolved via `$PATH`: dismissed; superseded by `cbscore-rs`
  migration on the roadmap.
- F12 — Dev OAuth bypass arbitrary `dev_email`: dismissed; upstream systems (S3,
  Harbor) hold the real authorization boundary.
- The four original prior-review findings (cross-worker lifecycle spoofing,
  cross-worker log output spoofing, empty component lists, log tail full read):
  covered by the WCP design.

## Goals

- Eliminate every Critical and High finding that is not already covered by the
  WCP design.
- Address Medium and Low findings whose fixes are small or share code paths with
  the High fixes.
- Make token material un-loggable by construction, not by discipline.
- Reduce operator-visible footguns (URL scheme, dev-mode env handling,
  config-file permissions).
- Keep scope: this design does not redesign OAuth, RBAC, the worker protocol, or
  the build engine. It hardens what exists.

## Non-Goals

- Native TLS in `cbsd-server` (roadmap; F6).
- Replacing the Python `cbscore` wrapper (roadmap; F9, F12).
- Restating the WCP design's decisions.
- Rate limiting, audit trails, or anomaly detection beyond what is necessary to
  close the listed findings.

## Decisions

### D1 — Dev-mode env handling is strict and audit-logged (F1)

`CBSD_DEV` is currently treated as truthy when the env var is a non-empty
string. That makes `CBSD_DEV=0`, `CBSD_DEV=false`, and `CBSD_DEV=no` all enable
dev mode — including the worker's `NoVerifier` rustls bypass. This is a Critical
TLS-validation bypass on a single misconfiguration.

Decisions:

- Define a shared helper `cbsd_common::env::is_truthy_env(var: &str) -> bool`
  that returns true only for the case-insensitive values `1`, `true`, `yes`,
  `on`. Empty, absent, or any other value returns false.
- Replace every existing check on `CBSD_DEV` (and any other dev/test toggles)
  with the helper. Server, worker, and any future binaries must use the same
  helper.
- The worker must log a clear `WARN` line at startup when dev mode is active
  that names `CBSD_DEV` and the value observed (redacted to a fixed
  `true`/`false` boolean — do not echo the raw value because it may be a
  misconfigured secret).
- The worker `dev_tls_config()` (`NoVerifier`) is reachable only when dev mode
  is active. The dev-mode entry point on the worker must additionally require
  that the parsed `server_url` host is loopback. The check is concrete:

  ```rust
  // After url::Url parsing of server_url.
  fn is_loopback_url(url: &Url) -> bool {
      match url.host() {
          Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
          Some(url::Host::Ipv4(addr)) => addr.is_loopback(), // 127.0.0.0/8
          Some(url::Host::Ipv6(addr)) => addr.is_loopback(), // ::1
          _ => false,
      }
  }
  ```

  Note: the check operates on the _parsed_ `url::Host`, not on a raw string
  prefix, which prevents authority-confusion bypasses such as
  `wss://localhost@evil.com/`. The Domain match is ASCII case-insensitive so
  `Localhost`, `LOCALHOST` etc. are accepted. The full IPv4 `127.0.0.0/8` range
  is accepted, as is `[::1]`. Port and path are not constrained. Connecting to
  any non-loopback host with `NoVerifier` is a configuration error and the
  worker refuses to start.

- The server applies the same strict parsing to its own dev toggles (notably the
  dev OAuth bypass; F12 is dismissed but the strict parse applies there too as
  defense in depth).

Tests:

- Unit tests for `is_truthy_env` covering `1`, `true`, `TRUE`, `yes`, `on`, `0`,
  `false`, `no`, ``, unset, and malformed values.
- Worker startup test: `CBSD_DEV=false` (any of `0`, `no`, `false`) must NOT
  install `NoVerifier`. Verifying this requires a fixture that asserts the
  active `ClientConfig` uses the default verifier set, not a mock returning
  `Ok(())` for any cert.
- Worker startup test: `CBSD_DEV=1` plus a non-loopback `server_url` causes the
  worker to refuse to start with a clear error message.
- `is_loopback_url` unit tests: `wss://localhost`, `wss://LOCALHOST`,
  `wss://127.0.0.1`, `wss://127.0.0.2`, `wss://[::1]`, `wss://[::1]:8443/x`
  return true; `wss://example.com`, `wss://localhost@evil.com`,
  `wss://127.0.0.1.evil.com`, `wss://10.0.0.1` return false.

### D2 — OAuth callback enforces verified email (F2)

Google's OIDC userinfo response includes a boolean `email_verified` field (or
`verified_email` for the older v1 endpoint). The current callback does not check
it. An attacker who controls a Google account with an arbitrary, unverified
email in an allowed domain can log in as that user.

Decisions:

- The OAuth callback handler decodes the userinfo response into a typed struct
  that includes `email_verified: bool` (with a serde alias for the legacy
  `verified_email` field).
- If `email_verified` is missing or false, the callback fails closed with
  `401 unauthorized` and a generic message. The server-side log records the
  email, the provider response shape, and the reason.
- This check runs **before** the allowed-domain check, so an attacker cannot
  probe domain allow lists with unverified accounts.
- The error message returned to the user is generic ("authentication failed;
  contact your administrator") to avoid revealing whether a domain is allowed or
  whether the email passed verification.

Residual trust gap (documented, not closed here):

- The `userinfo` endpoint response is a separate REST call made after the OAuth
  token exchange and is therefore **not** bound to the OAuth signature. A
  compromised or spoofed `userinfo` response could claim `email_verified: true`
  regardless of the underlying account state. This fix is adequate at the
  current security posture but is defense-in-depth, not end-to-end. A future
  hardening pass should consider ID-token introspection so the `email_verified`
  claim is bound to the OAuth signature itself, at which point the `userinfo`
  REST trust assumption can be dropped. This is noted explicitly so an
  implementer or auditor reading D2 understands the remaining trust dependency.

Tests:

- Mock userinfo response with `email_verified: false` → 401, user not created.
- Mock userinfo response with `email_verified: true` and an allowed domain →
  200, user created or refreshed.
- Mock userinfo response missing the field entirely → 401, treated as
  unverified.
- Mock response with legacy `verified_email` alias → accepted via serde alias.

### D3 — Periodic-task authorization splits into `:own` and `:any` (F4)

`periodic:manage` today is a single capability that lets any holder edit any
task. An owner of `periodic:manage` (without wildcard) can rewrite another
user's descriptor; subsequent triggers execute under the original owner's
identity and scope, so they can effectively launch builds in scopes they were
never granted.

The maintainer confirmed in 019 v1.1 follow-up Q1 that we can break the existing
capability without a migration concern.

Decisions:

- Replace the single `periodic:manage` capability with two:
  `periodic:manage:own` and `periodic:manage:any`. Defaults follow the existing
  `builds:list` pattern.
- `periodic_tasks` rows already record an owner (verify and add if missing). All
  mutating endpoints (`update_task`, `delete_task`, `enable_task`,
  `disable_task`) require:
  - `periodic:manage:any`, OR
  - `periodic:manage:own` AND `row.owner_email == user.email`.
- Read endpoints (`get_task`, `list_tasks`) continue to use the existing
  `periodic:view`; ownership filtering on list is added under the same `:own` /
  `:any` split if reads currently leak cross-owner rows. (Verify at
  implementation time; in scope for this design as a defense-in-depth pass.)
- Descriptor updates additionally re-validate scopes against the updating user's
  effective scopes, not the row's owner's scopes — so a `:any` admin who edits
  someone else's task still must hold the descriptor's required scopes
  themselves. This blocks scope smuggling via descriptor rewrite at write time.
- **Trigger-time descriptor re-validation against the owner's current
  capabilities.** The scheduler trigger (`scheduler/trigger.rs`) MUST
  re-validate the full stored descriptor (channel scope, repository scope, every
  component scope) against the task owner's **current** effective scopes before
  submitting the build. If the owner has lost any capability the descriptor
  relies on (role demotion, role removal, scope-reducing edit), the trigger:
  - logs the validation failure with the task ID, owner email, and the specific
    missing capability,
  - disables the periodic task and writes the validation message into
    `last_error` (consistent with the WCP design D5's fatal-disable rule for
    invalid stored periodic descriptors),
  - does NOT enqueue a build,
  - does NOT silently fall back to a reduced scope.

  This closes the privilege-persistence gap: once an owner loses a scope, no
  further builds will fire under that scope even from previously-valid stored
  descriptors. Re-enabling the task requires the owner (or an admin) to either
  re-acquire the capability or edit the descriptor to fit the new caps.

  **Owner-deleted case (normative).** When the periodic task's owner row is no
  longer a valid identity in the database, the trigger MUST treat the owner's
  effective scopes as the **empty set**, not as an error condition. With empty
  scopes, every descriptor scope check fails the re-validation step above, and
  the standard fatal-disable path applies: the trigger logs
  `owner_account_missing` plus the task ID, the task is disabled, and
  `last_error` records `owner account no longer exists`. No build is enqueued.
  The implementation MUST NOT panic, MUST NOT raise an unhandled error to the
  scheduler loop (which would stop other tasks from firing), and MUST NOT fall
  back to any cached or previously-resolved scope set. The owner-deleted case is
  a normal, expected lifecycle event handled by the same code path as
  scope-reduction, not an exceptional one.

  **Hard delete vs soft delete (normative — conditional on schema state).** "The
  owner row is no longer a valid identity" means the row cannot be located by
  `task.owner_email` using the canonical user-lookup query for the current
  schema. The contract is **conditional on whether the schema has a soft-delete
  marker**:
  - **Hard-delete schema (current production state).** The `users` table has no
    soft-delete column. The canonical lookup is
    `SELECT … FROM users WHERE email = ?`. "Owner row is no longer a valid
    identity" means the query returns zero rows. There is no filter to write;
    the absence of a soft-delete marker is itself the normative shape today.
    Phase B's implementation MUST NOT write a soft-delete filter against a
    schema that does not have the column — doing so would cause a runtime SQL
    error.
  - **Soft-delete schema (future, contingent on a separate migration that adds a
    soft-delete marker).** When/if a future schema migration adds a soft-delete
    column to `users` (e.g., `deleted_at`, `disabled_at`, or an explicit
    `is_deleted` flag), the canonical lookup MUST be extended to filter out
    soft-deleted rows. For a `deleted_at TIMESTAMP NULL` column the canonical
    lookup becomes `SELECT … FROM users WHERE email = ? AND deleted_at IS NULL`.
    The migration that introduces the soft-delete column is responsible for
    updating the canonical lookup in the same commit; lookups that pre-date the
    migration continue to work because they ran against the hard-delete schema.
    The trigger's behaviour for a row that the filter excludes is identical to
    the hard-delete absence case: empty effective scopes, fatal-disable path,
    `last_error = owner_account_missing`.

  Phase B implementers face no binary choice: the contract is schema-aware.
  Today (no soft-delete column), the trigger's lookup is the unfiltered query.
  The soft-delete clauses above are forward-protection that becomes binding only
  when the column exists. The soft-delete clause IS normative in the design, but
  the applicability condition (schema-has-soft-delete) gates when it takes
  effect.

  Other lookup paths that resolve a user's scopes elsewhere in the server (audit
  item for Phase B) inherit the same conditional rule: if the future migration
  adds the column, every canonical user-lookup must be updated. That migration
  is out of scope for D3 and out of scope for this design overall — D3 only pins
  what the trigger MUST do once the column exists.

  This invariant is restated in the State Invariants section so it is visible to
  implementers without reading the full D3 prose.

- Default roles in the seed data: `admin` gets `periodic:manage:any`; ordinary
  `developer` (or equivalent) gets `periodic:manage:own`.
- **SQL migration for the capability split.** The migration that introduces
  `periodic:manage:own` and `periodic:manage:any`:
  1. Adds the two new capability rows to the capability registry.
  2. Removes the legacy `periodic:manage` from every existing role's capability
     set. **No automatic mapping to `:own` or `:any` is performed.**
  3. The seed roles `admin` and `developer` (or equivalent) are updated in the
     same migration to grant `:any` and `:own` respectively, so the intended
     default still works post-migration.
  4. Any **custom** role that previously contained `periodic:manage` loses that
     capability and must be re-granted explicitly post-migration. The migration
     file's comment block must state this loudly so an operator reading the
     migration knows about the operational task.

  This is the deliberately conservative choice: silently mapping to `:any` would
  over-grant, and silently mapping to `:own` would under-grant for admin-style
  custom roles. Operators must make the call explicitly.

Tests:

- User A creates a periodic task. User B (with `:own` but not `:any`) attempts
  to PATCH/PUT/DELETE/enable/disable A's task → 403.
- User C with `:any` succeeds, but a descriptor update with a scope C lacks
  → 403.
- User A modifies their own task → 200.
- Database migration test: existing rows with no `owner_email` (if any in test
  fixtures) are backfilled or treated consistently.
- Custom-role migration test: seed a custom role containing legacy
  `periodic:manage`, run the migration, assert the legacy capability is removed
  and neither `:own` nor `:any` was auto-granted.
- Trigger-time scope re-validation test: User A creates a periodic task with a
  descriptor that requires `repo:foo`. Demote A so they no longer hold
  `repo:foo`. Fire the trigger; assert no build is enqueued, the task is
  disabled, and `last_error` contains the missing capability message.

### D4 — CLI login token uses URL fragment, not query string (F5)

`auth.rs:319-327` returns `Redirect::temporary("/?cli-token=…")` after OAuth.
The browser then issues `GET /?cli-token=…` to the server, which is logged by
`TraceLayer` (and by any reverse proxy in front). The existing code comment
claims fragment-based delivery, but the implementation uses a query string. RFC
3986 §3.5 forbids browsers from sending the fragment portion of a URL to the
server, so a fragment-based delivery moves the token out of every server-visible
log.

Decisions:

- Change `auth.rs:323-327` to emit `Redirect::temporary("/#cli-token=…")`
  (replace `?` with `#`).
- Update `ui/index.html:355-361` to:
  1. Read the token from `window.location.hash.slice(1)` (strip the leading
     `#`), parsed by `URLSearchParams`.
  2. **Immediately clear the fragment from the address bar and history**:
     `window.history.replaceState({}, '', '/');`. This is non-negotiable —
     without it, the PASETO token persists in `window.location.hash`, in the
     browser's history entry, and in anything that reads `document.URL`
     (analytics scripts, browser extensions with content-script access,
     third-party UI libraries loaded on the page). The clear must happen before
     the token value is displayed and before any other script has a chance to
     run on the page (i.e., at the top of the script block, in the same task as
     the read).
  3. Existing display logic (`tokenValue.textContent = cliToken`) is unchanged.
- Add an integration test that exercises the CLI login redirect and asserts the
  `Location` header contains `#cli-token=`, not `?cli-token=`.
- Independently of D4, audit every `tracing` call site (see D10) for inadvertent
  token logging — request-path logging at INFO level is the surface that made
  this bug exploitable, so reducing the path field's verbosity for the OAuth
  callback is an additional belt.

Tests:

- HTTP integration test: simulate the OAuth callback flow with `client=cli` and
  assert the response is `307` with `Location: /#cli-token=…`.
- Browser-level test (Playwright or curl + jq): the redirect URL's query
  component is empty; the fragment contains the encoded token.
- Browser-level test: after the page's JS has run, `window.location.hash` is
  empty and `window.history.state` reflects the cleared URL. A second test
  navigates back in the browser and asserts the prior history entry does not
  contain the token.
- Negative test: any request the server receives in the callback flow has its
  full path captured and asserted not to contain `cli-token=` in either query
  string or path segment.

### D5 — Worker tarball unpack: contained symlinks + decompression cap (F7)

`pack_component` uses `tar::Builder::append_dir_all`, which preserves symlinks.
The repo contains at least one legitimate symlink today
(`components/ceph/containers/v20.3 -> ./v20.2`, a relative same-directory
version alias). The worker uses `tar::Archive::unpack`, which already filters
absolute paths and `..`-bearing entry names but does **not** validate symlink
targets — an attacker who controls the tarball can ship `link -> /etc/passwd`
and a subsequent write through `link` writes to `/etc/passwd`. There is also no
cap on uncompressed total size; a small gzip can expand to many GB.

Decisions:

- Replace the bare `Archive::unpack` call with a custom unpack loop that:
  - Reads entry names and symlink targets via the `tar` crate's `entry.path()`
    and `entry.link_name()` accessors. These accessors **already apply PAX/GNU
    extended-header overrides**, so the containment check below operates on the
    effective (PAX-resolved) paths, not the on-wire POSIX 100-byte fields. The
    design states this explicitly so an implementer is not tempted to read the
    raw POSIX fields directly.
  - Rejects entries whose effective name has any absolute path component, any
    `..` component, or a drive-letter prefix (Windows defense, even though the
    worker is Linux-only today). This is a defense-in-depth rejection that runs
    on the PAX-resolved name, not only on the `tar` crate's built-in filter
    (which applies to the POSIX field only).
  - For symlink entries (`EntryType::Symlink` and `EntryType::Link`), applies a
    **two-phase containment check** against the resolved target:
    1. **Logical check** (primary defense): reject absolute symlink targets;
       logically normalize `unpack_root.join(entry_dir).join(link_target)`
       (without following any symlink on disk) and reject if it escapes
       `unpack_root`. This is the fast-fail for direct escapes and is sufficient
       against single-entry attacks: a strict `path-clean`-style normalization
       collapses `..` against the preceding component and treats popping past
       the root prefix as an escape.
    2. **Real-path check against already-unpacked entries** (defense in depth):
       for every entry (symlink, regular file, directory, or hardlink), walk
       every directory component of `entry_dir` from `unpack_root` down before
       writing. If any of those components on disk is a symlink (i.e., a symlink
       written by an earlier entry of the same tarball), `fs::read_link` it,
       recursively, and confirm the resolved real-path of `entry_dir` is still
       inside `unpack_root`. This phase exists as defense against:
       - **Implementation bugs in phase 1**: future maintainers porting the
         logical check to a different path-normalization library that handles
         `..`-past-root differently.
       - **TOCTOU during unpack**: if the unpack directory shares storage with
         other processes (e.g., a shared `/tmp` on a multi-tenant worker host),
         a concurrent symlink mutation by a non-tar agent could redirect a
         parent of `entry_dir` outside the root between phase 1 and the actual
         write. Phase 2's walk immediately before each write narrows this
         window.
       - **Changes to the `tar` crate's PAX or longlink handling** that alter
         what `entry.path()` returns in future versions. In the
         strict-`path-clean` implementation specified above, phase 1 alone
         catches every single-pass logical escape; phase 2 is not expected to
         fire in practice on a well-formed tarball, but its absence would be a
         regression risk worth more than its runtime cost.
  - Rejects device, fifo, char/block special, hardlinks (`EntryType::Link`) that
    resolve outside the unpack root, and any other
    non-regular-non-dir-non-symlink entry types.
  - Wraps the `GzDecoder` in a `Take<R>` limited reader with cap
    `MAX_UNCOMPRESSED_BYTES`. Default `MAX_UNCOMPRESSED_BYTES = 256 MiB`.
    Exceeding the cap fails the unpack with `ComponentError::Unpack` and a clear
    diagnostic.
  - Tracks per-entry sizes and rejects any single regular-file entry whose
    declared size exceeds the cap (defense against malformed entries).
- The cap is a worker config field with a sensible default; operators with
  legitimately large components can raise it.
- Implementation note: consider adopting the `safer-unpack` crate (or porting
  equivalent containment logic from CVE responses in `tar-rs` and `zip-rs`)
  rather than maintaining the two-phase check in-house. The decision between
  adopting the crate and writing the loop directly is left to the implementer at
  plan time; the design's behavioural requirements above are the contract
  regardless.
- Server-side `pack_component` keeps using `append_dir_all` — it packs trusted
  local content. No server-side change required.

Tests:

- Pack a fixture component with a legitimate same-dir symlink, unpack on worker,
  assert success and that the symlink target is correct.
- Hand-crafted tarball with `link -> /etc/passwd` → unpack fails.
- Hand-crafted tarball with `link -> ../../etc/passwd` (relative escape) →
  unpack fails (caught by phase 1, logical check).
- **PAX-overridden path test**: hand-craft a tarball whose POSIX field is a
  benign name (e.g. `safe.txt`) and whose PAX extended header overrides the path
  to `../../escape.txt`. Unpack fails on the PAX-resolved name.
- **Symlink-chain happy-path test**: hand-craft a three-entry tarball where
  entries form a benign chain: `a → b`, `b → c`, and a regular file `c`. Each
  link's logical normalization stays inside the unpack root, phase 1 passes on
  every link entry. Phase 2 walks the chain on disk for any subsequent entry
  whose path traverses one of these links and confirms each `read_link`
  resolution stays inside the root. Assert unpack succeeds and `a/file` (if
  added as a later entry) writes to `unpack_root/c/file` via the chain.
- **Phase 2 fault-injection test**: this test exercises the real-path walk by
  injecting an out-of-band filesystem state between two tarball entries. The
  test fixture writes a tarball with entries `safe_dir/` (a directory) and
  `safe_dir/file` (a regular file), but the test harness, between unpack steps,
  atomically replaces `safe_dir` on disk with a symlink to a location outside
  `unpack_root`. When the unpacker reaches `safe_dir/file`, phase 1 passes (the
  entry path has no `..`), but phase 2's pre-write walk of `safe_dir` detects
  the symlink and confirms its target escapes; unpack fails. This test validates
  phase 2 as a TOCTOU defense even though phase 1 alone catches all attacks
  present in a well-formed tarball.
- **`path-clean` regression test**: a hand-crafted tarball whose symlink target
  is exactly `..` (relative). Phase 1 logical normalization of
  `unpack_root.join("").join("..")` resolves to one level above the unpack root
  and is rejected by phase 1. This is the case that the v2-draft test example
  mistakenly described as surviving phase 1; the v3 design clarifies that strict
  logical normalization catches it, and this test pins that behaviour.
- Hand-crafted tarball with a device-special entry → unpack fails.
- Hand-crafted tarball with a hardlink target outside the unpack root → unpack
  fails.
- Gzip-bomb fixture (e.g. 1 KiB gzip expanding to 1 GiB of zeros) → unpack fails
  with the cap-exceeded error.
- Tarball with the cap exactly reached → unpack succeeds at the boundary; one
  byte over → fails.

### D6 — Global request/message size limits (F8)

The server currently has no `RequestBodyLimit` middleware applied globally.
axum + `tower-http` ships a `RequestBodyLimitLayer`; without it, attackers can
submit JSON bodies of unbounded size to any POST/PUT endpoint. WebSocket frames
likewise have no per-message ceiling.

This design intentionally does NOT add a per-log-line cap. Build output is
free-form text (compiler errors, deserialized JSON, stack traces with embedded
blobs) and truncating or rejecting a long line risks losing the most useful
diagnostic signal exactly when an operator needs it most. The trust boundary for
log output is worker authentication: only an authenticated registered worker can
emit `BuildOutput`, and the WCP design's ownership rules constrain which build
it can write to. Defense against bulk log abuse therefore lives at the
authentication boundary, not at the per-line ingest path.

Decisions:

- Apply `tower_http::limit::RequestBodyLimitLayer::new(REQUEST_BODY_MAX)`
  globally to the axum router. Default `REQUEST_BODY_MAX = 1 MiB`.
- For endpoints that legitimately accept larger payloads (none today; build
  descriptors are small JSON), allow per-route overrides via layered middleware.
  The default applies everywhere unless overridden.
- WebSocket: configure
  `tokio_tungstenite::tungstenite::protocol::WebSocketConfig` with
  `max_message_size = Some(WS_MAX_MSG)` and
  `max_frame_size = Some(WS_MAX_FRAME)`. Defaults `WS_MAX_MSG = 8 MiB`,
  `WS_MAX_FRAME = 1 MiB`. Apply on both server-accept and worker-connect paths
  so the worker also enforces caps on server-sent frames (binary tarball frame
  budget governs the upper bound for the worker side).
- Component tarball binary frame is allowed to exceed the JSON-control message
  limit; the WS message-size cap is the operative ceiling. Set `WS_MAX_MSG` so
  that a reasonable component tarball fits — but match the worker's
  `MAX_UNCOMPRESSED_BYTES` cap (D5) so a single WS binary frame plus the gzip
  expansion ratio cannot exceed the worker's unpack budget.
- `BuildOutput` lines: no per-line cap. A `BuildOutput` message is still bounded
  indirectly by the WS message-size ceiling (`WS_MAX_MSG = 8 MiB`), which limits
  any single batched payload. The worker continues to stream output line-by-line
  as it does today.
- Total log size per build is already monotonic on disk; this design does not
  add a per-build cap (out of scope; a future build-quota design can address
  that).

Tests:

- POST a JSON body > `REQUEST_BODY_MAX` → 413 Payload Too Large.
- WS message > `WS_MAX_MSG` → connection closed by the protocol stack with a
  clear log entry.
- Worker emits a single very long log line (e.g. 1 MiB of one compiler error)
  within a batch that fits under `WS_MAX_MSG` → server stores the full line
  verbatim. No truncation, no sentinel.
- Worker emits 10,000 normal-sized lines back-to-back → all stored verbatim.
- Tarball binary frame just under `WS_MAX_MSG` → accepted; just over → rejected.

### D7 — `api_keys.key_prefix` standalone index (F10)

API-key lookup currently scans on a non-indexed `key_prefix` column, making
lookups O(n). At small scale this is harmless; at production scale it both slows
requests and gives a measurable timing side channel that partially defeats the
existing timing-parity sentinel.

Decisions:

- Add SQL migration creating a non-unique B-tree index on
  `api_keys(key_prefix)`. Non-unique because two different full keys could share
  a prefix (the prefix is a UX/lookup helper, not a unique identifier).
- `cargo sqlx prepare --workspace` after the migration to regenerate the offline
  query cache.
- Update query plans in any related sqlx query to confirm the index is used.

Tests:

- Migration applies cleanly forward.
- A query plan check (manual at migration time; document in the migration
  comment) confirms the index is used by `lookup_by_prefix`-style queries.

### D8 — `cbc` rejects non-HTTPS hosts and writes config with mode 0o600 atomically (F11)

`cbc` accepts any URL scheme that `url::Url` parses. An operator typing
`http://cbs.example.com` (or hand-editing the config) sends the bearer token in
plaintext on the first request. `--no-tls-verify`/`-k` is orthogonal (it only
toggles cert validation for HTTPS); it neither gates nor mitigates this.

`Config::save` writes the file then chmods to 0o600. Between the write and the
chmod, the file is world-readable (typical default umask `0o022` produces
0o644).

Decisions:

- `parse_base_url` gains an `insecure_http: bool` parameter (sourced from
  `ClientOpts::insecure_http`, see below) and validates the URL scheme: `https`
  is always accepted; `http` is accepted only when `insecure_http` is set; any
  other scheme — and `http` without the opt-in — is rejected with
  `Error::Config("host must be https; got: <scheme>")`.
- Add an explicit opt-in flag `--insecure-http` (long form only, no short alias)
  that allows `http://` hosts. It is a `global = true` flag on the top-level
  `Cli`, parallel to the existing `--no-tls-verify`/`-k` and independent of it
  (`-k` toggles cert validation for `https`; `--insecure-http` permits the
  `http` scheme). When set, `run()` emits a `WARN`-style message once per
  invocation, alongside the existing `-k` warning:
  `warning: --insecure-http is set; bearer tokens are sent in cleartext`.
- **Plumbing — `ClientOpts` (chosen over threading parallel bools):** the three
  client-construction flags (`debug`, `no_tls_verify`, `insecure_http`) are
  bundled into a single `client::ClientOpts` struct
  (`#[derive(Clone, Copy, Debug)]`; all-`bool`, no secret material — the token
  stays a separate `&SecretString` argument to `CbcClient::new`). `run()` builds
  one `ClientOpts` from the global `Cli` flags and threads that single value
  through every command function to `CbcClient::new` /
  `CbcClient::unauthenticated`, which unpack it (`opts.no_tls_verify` for cert
  validation, `opts.insecure_http` into `parse_base_url`, `opts.debug` stored on
  the client). This replaces the two parallel positional `bool`s (`debug`,
  `no_tls_verify`) currently threaded through ~70 command functions across the
  `cbc` crate. The alternative — adding `insecure_http` as a third positional
  `bool` — touches the same functions but leaves three transposable `bool`s at
  every call site; the struct is self-documenting and absorbs future flags as
  fields rather than as signature changes.
- The `Config` struct is unchanged on disk; the validation lives in the URL
  parsing path so legacy config files with `http://` either fail loudly or
  require the explicit flag.
- `Config::save` is rewritten to create the file atomically with restrictive
  permissions:
  - Build the target's parent directory if missing (existing).
  - Write the JSON to a sibling temp file in the same directory, created with
    `OpenOptions::new().write(true).create_new(true) .mode(0o600)` (Unix). On
    non-Unix, accept the platform default (this is a Unix-only concern in
    practice).
  - `fs::rename` the temp file over the target. Rename within the same directory
    is atomic on POSIX.
  - On any error after temp-file creation, the function MUST attempt
    `fs::remove_file` on the temp file in a best-effort cleanup before returning
    the original error. Log the cleanup failure (if any) at debug level. The
    original write/rename error is what the caller sees.
- **Windows atomicity caveat**: `std::fs::rename` is **not atomic on Windows**
  prior to Rust 1.86 (the version that stabilized `File::rename_no_replace`
  using `SetFileInformationByHandle` for atomic replace). The `cbc` binary is
  currently used primarily on Linux. The design accepts the following behaviour
  on Windows: a rename that fails because the target file is held open by
  another process surfaces as `Err(...)` from `Config::save`, the temp file is
  cleaned up by the error path above, and the user must retry. Silent corruption
  is not possible because the temp file's content is the only place the new
  config exists until the rename succeeds. If Windows becomes a first-class
  target, the implementer should switch to `File::rename_no_replace` on Rust ≥
  1.86 and document the MSRV bump in the cbc workspace `Cargo.toml`.
- Document the flag and the behavior change in `cbc --help` and any README.

Tests:

- `parse_base_url("http://x", false)` → error with the documented message.
- `parse_base_url("http://x", true)` → ok (the opt-in permits `http`).
- `parse_base_url("https://x", false)` → ok.
- `parse_base_url("ftp://x", true)` → error (the opt-in widens only to `http`,
  not to arbitrary schemes).
- CLI with `--insecure-http http://x` → command succeeds in test mode and emits
  the warning on stderr.
- `Config::save` test using a `tempfile` directory: verify the final file mode
  is `0o600`. Race test: spawn a reader thread that repeatedly stats the target
  while `save` runs in a tight loop; assert no observed state has mode !=
  `0o600` AFTER the file is visible by name (i.e., the file is never atomically
  swapped into place with permissive permissions).

### D9 — Project-wide URI-logging policy (F5 belt + D10 enabler)

The mainline `TraceLayer` configuration logs request URIs by default. That is
what made F5 a high-severity finding. Even with D4's query→fragment fix, the
request-path field is a foot-gun for any future endpoint that accepts secrets in
the URL. `TraceLayer` is not the only middleware layer that can log URIs: panic
handlers, error-reporting middleware (e.g., sentry-style integrations), custom
`on_request`/`on_response` callbacks, and reverse-proxy access logs all have to
be considered.

Decisions:

- **Project-wide policy**: no middleware, handler, panic handler, error
  reporter, or other logging surface within `cbsd-server` may log
  `Uri::to_string()`, `request.uri()` in its entirety, or any field that
  includes the query component. Only `Uri::path()` (path segments alone) is
  permitted at INFO or above. Query parameters may appear at DEBUG only, from
  explicitly annotated per-route handlers, and only when the route contract
  guarantees no secret can appear in the query (a route that accepts `?n=50` on
  `/logs/tail` is fine; any route that has _ever_ accepted a secret in the query
  is not).
- `TraceLayer::new_for_http().make_span_with(...)` is configured with a custom
  span builder that:
  - Logs `method`, `path` (path segments only — `Uri::path()`), and `status` at
    INFO.
  - **Does not log** `query` or full `Uri::to_string()` at any level.
- Panic handler / error reporter configuration: if `tower-http::catch_panic` or
  any sentry-style integration is enabled, its request-context formatter MUST be
  configured to elide the query string before reporting. This applies to any
  future addition of such middleware; the design's policy is the contract.
- Document the policy in `cbsd-server/src/main.rs` (near the router builder) and
  in `cbsd-rs/CLAUDE.md` under the "Correctness Invariants" section so
  contributors see it before adding new middleware.
- This is in addition to D10's `Secret<T>` construction-time defense for token
  material: D9 protects against URIs containing secrets (a class defense for the
  path-as-string surface); D10 protects against secret values themselves being
  formatted into log lines (a class defense for the value surface).

Tests:

- A request `GET /?cli-token=abc123` (simulating a regression of D4) is captured
  in `tracing-test` output; assert the captured logs contain `path=/` and do NOT
  contain `abc123`.
- A simulated panic in a handler triggers `catch_panic`; assert the captured
  panic-report does NOT contain a query string from the panicking request.
- A grep-style audit (manual at first; automated under D10's CI gate when that
  lands) on the cbsd-server source for `request.uri()`, `Uri::to_string`, and
  `request.url()` outside of explicitly-annotated DEBUG sites — none must exist
  in INFO-or-above logging call sites.

### D10 — Token-material redaction policy (F13)

No portion of any bearer token, PASETO raw token, session token, robot token, or
API key may be written to logs at any level.

**Lookup-prefix carve-out** (resolved during the commit-15 review): the
12-character API-key / robot-token lookup prefix is a non-secret routing index.
It is stored unencrypted in `api_keys.key_prefix`, returned in key-creation API
responses, and carried in URL paths, so it MAY appear in key-lifecycle log lines
(create / rotate / revoke). The prohibition above targets the raw token (the
full presented bearer string) and the Argon2 credential hash, which must never
appear in any log at any level; on the auth path the raw presented token is
redacted to a non-reversible per-process diagnostic identifier.

The "by construction" guarantee here is **construction-tight**: it must hold for
every formatting surface — `Debug`, `Display`, `serde::Serialize`,
`std::fmt::Pointer`, custom `tracing::Value` impls — not only the two that
`tracing::debug!` / `tracing::info!` use directly. A single
`serde_json::to_string(&token)` that bypasses `tracing` and lands the raw value
in an HTTP response body, a test snapshot, or a panic message would defeat the
entire abstraction. D10 therefore specifies all of these surfaces explicitly.

Decisions:

- Add a `cbsd_common::secrets::Secret<T>` newtype with the following
  construction-tight contract:
  - **`Debug` impl**: prints exactly `Secret<T>(<redacted>)`. Never delegates to
    `T`'s Debug.
  - **`Display` impl**: prints exactly `<redacted>`. Never delegates to `T`'s
    Display.
  - **NO `serde::Serialize` impl.** A struct that accidentally tries to
    `#[derive(Serialize)]` over a `Secret<T>` field will fail to compile. To
    serialize the inner value over the wire intentionally, callers must
    `.expose_secret()` first and serialize the resulting `&T` explicitly —
    making the leak surface visible and grep-able at every call site.
  - **NO `serde::Deserialize` impl** by default. If a particular `T` truly needs
    to be deserialized from a config or wire format, a per-call-site helper
    wraps the deserialized value into `Secret<T>` after read, again making the
    un-wrap surface visible.
  - **NO `std::fmt::Pointer` impl**, no `std::fmt::Octal`, no other formatter
    that could be triggered by a misformatted `tracing` value. Only
    `Debug`/`Display` exist, and both redact.
  - **Single named accessor**: `pub fn expose_secret(&self) -> &T`. This is the
    only way to obtain the inner value. The name is deliberately unsubtle so
    reviewers and grep can find every site that exposes secret material. Calls
    to `.expose_secret()` are flagged for review in code review and (eventually)
    by the CI grep gate.
  - **`Clone`** if `T: Clone`. **No `Copy`** even if `T: Copy` — copying a
    secret silently would defeat audit; cloning is explicit.
  - Constructor: `Secret::new(value: T) -> Self` — the only way to wrap.
  - The newtype is generic over `T` so it works for `String`, `Vec<u8>`, PASETO
    key bytes, Argon2 hashes, etc.

  **Implementation note**: the `secrecy` crate
  (https://crates.io/crates/secrecy) provides exactly this contract upstream.
  Adopting `secrecy` directly is the recommended path; the in-house newtype is
  described above only as the contract specification, not as a preferred
  implementation. The implementation plan should evaluate `secrecy` (license,
  MSRV, transitive deps) and prefer adoption over re-implementation unless there
  is a concrete reason not to.

  **Note on `secrecy`'s API shape**: `secrecy` exposes the inner value through
  an `ExposeSecret` _trait_, not an inherent method. Call sites must
  `use secrecy::ExposeSecret;` to bring the trait into scope before calling
  `.expose_secret()`. This is a small import-hygiene requirement that the plan
  document must mention so each Phase B commit that touches a secret-holding
  type adds the import where needed. The CI gate pattern (`\.expose_secret\(\)`
  followed by `// allow-expose`) is unaffected by the trait-vs-inherent
  distinction because the gate operates on the call-site syntax, not on the
  dispatch mechanism.

- **PASETO and Argon2 integration sites**: `paseto::token_create` and the Argon2
  verify path must consume `&[u8]` or `&str` directly. The call sites that hold
  a `Secret<String>` or `Secret<Vec<u8>>` use `.expose_secret()` at the call
  site (with `use secrecy::ExposeSecret;` in scope), passing the inner reference
  into the function. There is no `Display` path involved; the inner value flows
  through typed references, not formatted strings.

- Audit every existing `tracing::*!` macro call across the workspace for:
  - `Bearer` literal substrings in format strings.
  - Authorization header values being logged.
  - PASETO raw tokens being formatted.
  - API key prefix logging at debug (today's `F13` site): replaced with a stable
    per-process diagnostic identifier derived from the key hash, never the key
    bytes.

- Wire types in `cbsd-proto` that today contain raw token strings (`api_key`,
  the various `*Token` fields) must be migrated to `Secret<String>`. Any type
  that derives `Serialize` and contains a `Secret<T>` field will fail to
  compile, forcing the implementer to either (a) replace the derive with a
  custom Serialize that calls `.expose_secret()` explicitly at the wire
  boundary, or (b) restructure to separate the wire-format DTO from the
  in-memory secret holder. Both are acceptable; the design's contract is that
  any wire-boundary exposure is visible at the call site.

- Add a CI grep gate as a `cargo xtask` or a small CI script (deferred per
  ROADMAP roadmap item):
  - Reject any new staged file under `cbsd-rs/` whose diff contains one of:
    `\bBearer\b` in a `tracing::` argument; the literal substring `?token=` or
    `paseto_token =` outside of test code; `Authorization:` in a format-string
    argument; `\.expose_secret\(\)` without a Rust line-comment of the form
    `// allow-expose` (with optional follow-up text) on the same line for review
    tracking. The comment delimiter is **`//`** — the standard Rust line-comment
    marker. Earlier drafts of this document used `#`, which is not a valid Rust
    line comment and would have made the gate either flag every legitimately
    annotated call site or be trivially bypassed. False positives in the other
    patterns (Bearer literals, etc.) are handled with a similar `// allow-xyz`
    Rust comment near the call site.
- The `signed_off_by` field and similar non-secret identity fields are
  explicitly NOT in scope.

Tests:

- Compile-time: a `tracing::debug!(token = %my_secret, ...)` where
  `my_secret: Secret<String>` produces `token=<redacted>` in the captured log
  output. Use `tracing-test`.
- Compile-fail: a struct with `#[derive(Serialize)]` and a `Secret<String>`
  field fails to compile. Use `trybuild` or the equivalent.
- Compile-fail: `format!("{:?}", &my_secret_string_outside_wrapper)` where the
  value is a `Secret<String>` is captured and asserted to produce
  `Secret<String>(<redacted>)`, not the inner string.
- `.expose_secret()` is the only way to obtain `&T`: a separate trybuild test
  asserts that `&*my_secret` or `&my_secret.0` (no public field) fails.
- CI gate: a synthetic diff that introduces `tracing::info!(token = %raw_token)`
  (where `raw_token: &str`) is rejected by the grep gate.
- CI gate: a synthetic diff that introduces `.expose_secret()` without a
  `// allow-expose` Rust line comment on the same line is rejected by the grep
  gate. A second test asserts that
  `.expose_secret() // allow-expose: needed for paseto::token_create` is
  accepted (the gate's pattern is `.expose_secret()` AND no `// allow-expose` on
  the same line; longer comments after the marker are fine).
- Negative test: redaction wrapper is not used on identifiers that are
  intentionally logged (email, build_id, worker_id) — those remain visible in
  logs.

### D11 — Worker `accepted` phase is part of reconnect-`Building` status (WCP v10 #1)

The WCP design v11 reconnect rule reports `Building` only when the worker
supervisor has an active executor, an in-progress revoke, or a pending terminal
result. The supervisor model also tracks an `accepted` local phase (between
`build_accepted` and subprocess spawn), but the reconnect rule omits it. If a
websocket drops in that window, the worker reports `Idle` even though it still
holds the build's component working directory and is about to spawn the
subprocess. The server then treats the assignment as
`dispatched + AwaitingReceipt` (or `ReceivedByWorker` if `build_accepted`
already arrived) and rolls it back to `queued` — at which point the original
worker may continue to start the subprocess while the build is redispatched to a
different worker. The WCP v10 review tagged this as a High open item.

Decisions:

- The worker supervisor's reconnect-status rule reports `Building { build_id }`
  whenever the supervisor has **any non-terminal local assignment state**,
  including the `accepted` phase. Concretely, the rule reports `Idle` only when
  there is no active executor, no in-progress revoke, no pending terminal
  result, **and** no `accepted` assignment awaiting subprocess spawn.
- If the worker cannot determine whether the `accepted` phase has produced a
  spawned subprocess (race between OS spawn and reconnect), it follows the
  existing WCP v11 rule: stop and await any possible child, clean up local
  state, and only then report `Idle`. The `accepted`-only case (where no child
  has been spawned yet) reports `Building` after the supervisor confirms no
  executor exists; the server's reconnect handler treats this exactly like an
  authoritative receipt of `build_accepted` (mark `ReceivedByWorker`, cancel the
  dispatch-ack timer, keep the assignment under the new connection).
- The WCP design's same-worker reconnect migration is the only path that may
  rewrite `connection_id`. This decision does not change that; it only changes
  the **truthfulness** of the reconnect status the worker reports during
  migration.

Tests:

- Worker accepts a build (`build_accepted` sent), then the websocket is killed
  before subprocess spawn. Worker supervisor state is `accepted`. Worker
  reconnects and reports `Building { build_id }`. Server treats this as
  `ReceivedByWorker`, cancels the dispatch-ack timer if still armed, and leaves
  the assignment under the new connection. The original worker (now the new
  connection after migration) subsequently spawns the subprocess and sends
  `build_started`. The build proceeds normally; no rollback to `queued`; no
  redispatch.
- Same scenario but the original child process did spawn after reconnect was
  disconnected. The supervisor waits/kills the child before reporting status,
  per WCP v11. The reported status is consistent with whatever the supervisor
  confirms after that wait.

### D12 — Liveness/dead-worker resolution table for receipt-aware `dispatched` (WCP v10 #2)

The WCP design v11 splits idle-reconnect rollback by `AwaitingReceipt` vs
`ReceivedByWorker`, but never specifies what the liveness/grace-expiry monitor
does when the receipt-acknowledged worker never reconnects at all. The existing
`cbsd-server/src/ws/liveness.rs::handle_worker_dead` path requeues `dispatched`
rows without distinguishing receipt state. Requeuing a `ReceivedByWorker` build
can yield duplicate execution; failing an `AwaitingReceipt` build can drop work
that the worker never actually received. The WCP v10 review tagged this as a
High open item.

Decisions:

- Liveness/dead-worker resolution is **table-driven** and aware of
  `ActiveAssignmentReceipt`. The resolver runs when the worker liveness monitor
  declares a worker dead (no heartbeat within the grace window AND no live
  websocket connection for that registered worker ID) AND there are owned active
  assignments for that worker.
- Resolution table for owned active assignments at the moment of dead-worker
  declaration:

  | DB state     | Receipt            | Action                                                                                                                          |
  | ------------ | ------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
  | `dispatched` | `AwaitingReceipt`  | Roll back to `queued` using the WCP rollback-cleanup operation. Assumption: worker never received the assignment.               |
  | `dispatched` | `ReceivedByWorker` | Mark `failure` with reason `worker died after accepting assignment`. Finalize log, remove active entry/watcher. Do NOT requeue. |
  | `started`    | (any)              | Mark `failure` with reason `worker died during execution`. Finalize log, remove active entry/watcher.                           |
  | `revoking`   | (any)              | Mark `revoked` (revoke completed by worker death). Finalize log, remove active entry/watcher.                                   |

- `ReceivedByWorker` + `dispatched` is treated as work-in-flight that was
  potentially executed: failing rather than requeuing is the conservative choice
  because the worker may have written outputs to Harbor/S3 before dying.
  Duplicate execution would compound that side effect. Operators who need to
  retry a failed build can resubmit explicitly.
- The receipt state lives in process memory only (per WCP v11 invariant 25).
  After a server restart, no `ReceivedByWorker` rows exist; startup recovery
  uses the existing fail-in-flight policy. The new resolution table therefore
  applies only to dead-worker expiry during a single server-process lifetime.

Tests:

- Dispatch a build to a worker, simulate worker death **before** any owned
  message arrives (`AwaitingReceipt`) → assignment rolls back to `queued`;
  redispatch is eligible.
- Dispatch a build to a worker, receive `build_accepted` (now
  `ReceivedByWorker`), simulate worker death before `build_started` → assignment
  marked `failure` with the documented reason; no redispatch; log finalized.
- Dispatch and `build_started`, simulate worker death during execution →
  `failure`; log finalized.
- `revoking` plus worker death → `revoked`; log finalized.
- Server-process restart while a build is `dispatched + ReceivedByWorker`:
  startup recovery fails the build per existing policy (unchanged); the receipt
  state is not reconstructed.

### D13 — Superseded live same-worker connection receives a stop-work command (WCP v10 #3)

The WCP v11 same-worker reconnect migration declares the newest authenticated
connection the winner and removes the old sender. If the old connection's
subprocess is still running, the server has no specified way to tell that
subprocess to stop. The WCP v10 review called this a High open item: the new
connection's idle-reconnect resolution can mark the build `failure` or `revoked`
(per the WCP idle-reconnect table) while the old subprocess potentially still
runs to completion, producing side effects under a now-terminal build.

Server-side decisions:

- Same-worker reconnect migration **must send `BuildRevoke` to the old
  connection before removing its outbound sender**, for every owned active
  assignment that is being migrated. The revoke is sent synchronously on the old
  sender. The order is:
  1. Old connection identified for the authenticated registered worker ID.
  2. For each owned active assignment in `queue.active` whose `connection_id`
     points at the old connection, send `BuildRevoke { build_id }` to the old
     sender. Best-effort: if the old sender is already closed or `try_send`
     fails, log and continue.
  3. Run the existing WCP migration steps (DB-backed ownership checks,
     queue-lock-guarded swap of `connection_id`, removal of the old sender).
- This revoke is **reporter-directed cleanup**, not a state-mutating revoke: it
  does NOT set the DB state to `revoking`, does NOT cancel any timer, does NOT
  remove the log watcher, and does NOT rewrite `queue.active`. The new
  connection is now the authoritative owner and the WCP idle-reconnect rules
  decide the final DB transition.
- If the old connection's send fails, the operational warning ("a live
  same-worker connection was superseded") that the WCP v11 idle-reconnect table
  already requires is emitted with an additional `revoke_send_status` field so
  operators can correlate with downstream worker logs.

Worker-side handling during migration:

The two websocket connections briefly coexist (old and new) during migration.
The worker's process-level supervisor (per WCP "Worker-Side Active Build State")
owns the subprocess and reads messages from whichever websocket is currently
routing them. Behaviour depends on the supervisor's local phase for each
migrated build:

- **`accepted` (no subprocess yet)**: supervisor clears the local assignment
  state (component working directory, accepted-but-not-spawned marker). On the
  new connection it reports `build_finished(revoked)` for that `build_id` if the
  new connection is usable. No subprocess to kill.
- **`started` (subprocess running)**: supervisor kills + awaits the subprocess
  per WCP v11 worker-side revoke handling. On the new connection it reports
  `build_finished(revoked)` with the kill reason. If the kill takes longer than
  the server's reasonable wait, this surfaces only as the build remaining active
  under the new connection; D12's liveness path is the safety net if the new
  connection then also goes idle without reporting.
- **`revoking` (already mid-revoke)**: supervisor idempotently re-applies kill
  - await; the revoke is benign. The worker still reports
    `build_finished(revoked)` on the new connection.
- **`terminal-pending-report` (build completed locally, awaiting upload)**: the
  supervisor **drains the pending terminal result first**, then treats the
  migration revoke as a no-op for that build. The supervisor reports
  `build_finished(<actual_outcome>)` on the new connection — where
  `<actual_outcome>` is the real success / failure / revoked outcome the
  subprocess produced. The migration's authority is preserved (the new
  connection is the one carrying the report); the build's real outcome is
  preserved (no silent conversion of `success` to `revoked`); and the
  artifact-store state and DB state agree.

  This deviates from the generic WCP v11 worker-side revoke rule (which discards
  `terminal-pending-report` results on revoke). The deviation is scoped
  specifically to **migration-driven revokes**: a normal admin-initiated revoke
  that arrives at a `terminal-pending-report` build still discards the result
  per the WCP rule, because that is the operator's explicit intent. A migration
  revoke is not operator intent; it is a side-effect of the new connection
  winning the migration, and discarding a known-good outcome in that case is
  silent data loss for no correctness benefit.

  Implementation: the worker supervisor must distinguish a migration-context
  revoke from a normal admin revoke. The cleanest path is for the supervisor to
  detect that the revoke arrived **on the old, superseded websocket connection**
  (or via a server-side flag on the `BuildRevoke` message, e.g., a
  `reason: MigrationSupersede` enum variant added to `cbsd-proto`); see the
  protocol decision below.

  Protocol change to support the distinction: extend
  `ServerMessage::BuildRevoke` with a `reason` field carrying an enum. **The
  field's serde representation is pinned explicitly** to make the addition
  genuinely forward- and backward-compatible:

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum BuildRevokeReason {
      Admin,                  // operator/admin-initiated revoke
      MigrationSupersede,     // sent by D13 on the old connection during
                              // same-worker migration
      UnauthorizedAction,     // sent by the WCP unauthorized-action path
                              // (reporter-directed stray revoke)
  }
  ```

  And on the `ServerMessage` variant:

  ```rust
  pub enum ServerMessage {
      // … other variants …
      BuildRevoke {
          build_id: BuildId,
          #[serde(
              default,
              skip_serializing_if = "Option::is_none",
          )]
          reason: Option<BuildRevokeReason>,
      },
      // …
  }
  ```

  **Compatibility semantics — both directions:**
  - **New server → old worker** (rolling upgrade, server-first). The server
    serializes `BuildRevoke { build_id, reason: Some(MigrationSupersede) }`. The
    old worker's `BuildRevoke` variant has no `reason` field. Serde's default
    behavior on the worker side rejects unknown fields _only_ if the variant
    uses `#[serde(deny_unknown_fields)]`. The WCP wire types in `cbsd-proto`
    **must not** set `deny_unknown_fields` on `ServerMessage` variants. With
    ordinary (non-deny) serde derivation, the old worker silently ignores the
    unknown `reason` field and deserializes the `BuildRevoke` successfully with
    no reason information, applying the existing WCP v11 revoke semantics
    (discard `terminal-pending-report`). This degrades the F-R1 closure on
    rolling upgrade — but the build still terminates, no message is dropped, and
    the cluster recovers as soon as the worker is upgraded. Operators are
    advised in the deployment notes to upgrade workers before or alongside the
    server when the v3 change ships.
  - **Old server → new worker** (rare, but possible during a downgrade or a
    worker fleet that out-races a server downgrade). The old server emits
    `BuildRevoke { build_id }` with no `reason` field. The new worker's
    `Option<BuildRevokeReason>` deserializes to `None` via `#[serde(default)]`.
    `None` is treated as `Admin` semantics on the worker — the conservative
    interpretation: an unspecified revoke is an admin revoke, which uses WCP
    v11's existing discard-pending rules. This makes the absent-field case fully
    safe and explicit rather than relying on default-of-default semantics.
  - **`skip_serializing_if = "Option::is_none"`** means the server's wire output
    omits the field entirely when no reason is set, keeping the JSON payload
    identical to the pre-v3 shape. A v3 server that chooses to emit `None`
    (e.g., from a path that does not categorize its revokes) sends exactly the
    pre-v3 message; no compatibility surprise.

  The worker supervisor branches on `reason`:
  - `Some(Admin)` or `None` → existing WCP v11 behavior (discard
    `terminal-pending-report`, report `revoked`).
  - `Some(MigrationSupersede)` → if SM-W is `terminal-pending-report`, drain the
    real outcome and report `build_finished(<actual_outcome>)`; otherwise apply
    the kill-and-await path of the other phases.
  - `Some(UnauthorizedAction)` → existing WCP v11 reporter-directed stop
    behavior.

  D13's server-side send is updated to include
  `reason: Some(BuildRevokeReason::MigrationSupersede)` on every migration
  revoke. The protocol version stays at 2 (per WCP v11's pre-production
  no-compat-shim policy); the new optional field with explicit `serde(default)`
  is a genuinely forward-and-backward-compatible serde addition.

  **Worker-side anti-coercion predicate (concrete).** A malicious server could
  in principle emit `BuildRevoke { reason: MigrationSupersede }` outside an
  actual migration context to coerce a worker supervisor into drain-then-revoke
  semantics. The worker MUST NOT enforce migration semantics solely on the wire
  `reason` field. The worker confirms via a concrete local predicate that a
  migration is genuinely in progress before applying the drain-then-revoke
  deviation.

  The predicate is defined as follows:

  ```rust
  use tokio::time::{Duration, Instant};

  // Worker supervisor state (added to the WCP "Worker-Side Active
  // Build State" struct). The Instant type is tokio::time::Instant
  // (NOT std::time::Instant) so that test code using
  // tokio::time::pause() / tokio::time::advance() can drive the
  // predicate deterministically. tokio::time::Instant wraps
  // std::time::Instant in production and adds an alternative
  // test-controlled implementation under a paused runtime, so the
  // production behaviour and the CLOCK_MONOTONIC caveat are
  // unchanged.
  struct Supervisor {
      // ... existing fields ...

      /// Set every time the supervisor observes a successful
      /// authenticated websocket connection (i.e., post-Hello/Welcome
      /// handshake) for its registered worker ID. Cleared lazily —
      /// see predicate below.
      last_authenticated_connect_at: Option<Instant>,
  }

  /// Time window during which a `MigrationSupersede` revoke is
  /// considered plausibly tied to a recent reconnect. Chosen to
  /// comfortably exceed normal handshake+revoke RTT under load;
  /// smaller would risk false negatives on a stressed network,
  /// larger would unnecessarily widen the window for coercion.
  const MIGRATION_RECENT_WINDOW: Duration = Duration::from_secs(30);

  /// Predicate evaluated when the supervisor receives
  /// `BuildRevoke { reason: Some(MigrationSupersede) }`.
  fn migration_plausible(s: &Supervisor) -> bool {
      match s.last_authenticated_connect_at {
          Some(at) => at.elapsed() <= MIGRATION_RECENT_WINDOW,
          None => false,
      }
  }
  ```

  Semantics:
  - **Set**: `last_authenticated_connect_at = Some(Instant::now())` is set the
    moment the supervisor confirms a new authenticated websocket (after the WCP
    `Welcome` message is received and any same-worker migration handshake has
    completed). The flag is per _supervisor_ (process-level), not per _build_:
    it applies to every active build at the moment of reconnect, which is
    correct because a same-worker migration applies to every owned active
    assignment simultaneously per D13.
  - **Window**: 30 seconds. The server's migration revoke is sent synchronously
    during the new connection's handshake, so the gap between
    supervisor-observed-reconnect and revoke-receipt is typically sub-second. A
    30-second window is conservative against worker-side scheduling delay,
    websocket buffer queueing on the receive path, or stop-the-world GC on the
    worker host. Beyond 30 seconds, a migration revoke is implausible and the
    worker MUST fall back to `Admin` semantics.
  - **Cleared**: the flag is NOT actively cleared. It naturally decays past the
    window via the `elapsed() <= MIGRATION_RECENT_WINDOW` check. Each new
    authenticated connection overwrites the timestamp. This avoids any per-build
    bookkeeping for the flag.
  - **Predicate at revoke time**: when a
    `BuildRevoke { reason: Some(MigrationSupersede) }` arrives for a build, the
    supervisor evaluates `migration_plausible(self)`:
    - **True** → honour the `MigrationSupersede` semantics: if SM-W is
      `terminal-pending-report`, drain the result and report
      `build_finished(<actual_outcome>)`. Otherwise, the other phases apply as
      specified.
    - **False** → fall back to `Admin` semantics (discard
      `terminal-pending-report`, report `build_finished(revoked)`). Log a
      `WARN`-level diagnostic:
      `event = "migration_supersede_without_recent_reconnect"`, `build_id`,
      `last_authenticated_connect_at` (if any). The diagnostic does NOT include
      the wire `reason` field's decoded text, only the predicate's result, to
      avoid encoding attacker-supplied content in the log.
  - The supervisor also exposes a non-fatal counter for the false branch
    (`migration_supersede_implausible_count`) for operator observability without
    requiring log-parsing.

  This makes the anti-coercion defense **implementable**: every term in the
  predicate is a concrete worker-side state with a defined lifetime. It is also
  **testable**: the false-predicate fallback path is exercised by D13-T7 (see
  test summary).

The supervisor's response channel choice is always **the new connection**
because the old one is being torn down during the same migration step. The
worker's outbound queue is associated with the supervisor, not the websocket
loop; the websocket loops are transport clients that forward to/from the
supervisor.

If the supervisor never receives the `BuildRevoke` (e.g., old sender failed,
worker process restarted between connections so the supervisor has no in-memory
state for that build), no `build_finished(revoked)` arrives from the worker. The
build then remains under the new connection until either (a) the new
connection's worker_status arrives and is reconciled per the WCP idle/Building
rules, or (b) the new connection itself becomes inactive and D12's liveness path
resolves the build by DB state × receipt.

Tests:

Cover each subprocess phase explicitly at migration time. In every case the test
asserts: (i) `BuildRevoke` was sent to the old sender before removal; (ii) the
active entry was migrated to the new connection by the WCP migration steps;
(iii) no `revoking` DB transition occurred from the revoke itself; (iv) the new
connection eventually carries the terminal report.

- **Accepted phase**: Worker A receives `build_new`, sends `build_accepted`,
  then websocket drops before subprocess spawn. New connection arrives.
  Migration sends `BuildRevoke`; supervisor's `accepted` state is cleared; on
  the new connection the supervisor reports `build_finished(revoked)`.
- **Started phase**: Worker A is mid-build (`started`, subprocess running). New
  connection arrives. Migration sends `BuildRevoke`; supervisor kills + awaits
  subprocess; reports `build_finished(revoked)` on the new connection.
  Subprocess exit status confirms it was killed by the worker, not by its own
  completion.
- **Revoking phase**: Worker A is already handling a `BuildRevoke` (e.g., admin
  revoked the build a moment earlier). New connection arrives. Migration sends a
  second `BuildRevoke`; supervisor's kill is idempotent; reports
  `build_finished(revoked)` once.
- **Terminal-pending-report phase (drain-then-revoke)**: Worker A finished its
  subprocess successfully while disconnected; supervisor holds the terminal
  `success` result. New connection arrives. Migration sends
  `BuildRevoke { reason: MigrationSupersede }` on the old sender; supervisor
  detects the migration reason, **drains the terminal result** rather than
  discarding, and reports `build_finished(success)` on the new connection.
  Assert that the build is recorded as `success` in the DB, NOT `revoked`. No
  data loss. The corresponding test for an admin-initiated revoke (reason
  `Admin`) hitting `terminal-pending-report` is a regression test that the WCP
  v11 discard semantics still apply in the non-migration case.
- **Revoke send failure**: Old sender's channel is closed / full at migration
  time. Migration proceeds; operational warning logged with
  `revoke_send_status = "send_failed"`. Supervisor in `started` phase never
  receives the revoke. On the new connection the supervisor still reports
  `Building { build_id }` (D11) and continues the build to its natural terminal
  state; if instead the supervisor's local state was lost (worker process
  restart between connections), the new connection's `worker_status(idle)`
  triggers WCP idle reconciliation; if the new connection also becomes inactive,
  D12's liveness path is the safety net.
- **Concurrent unauthorized message**: Old connection sends an unauthorized
  build-scoped message at the same instant migration runs. The unauthorized
  message is rejected per WCP rules; the revoke send is independent and not
  blocked by the rejection.
- **No owned assignment**: Same-worker migration when the old connection has no
  active assignments. No `BuildRevoke` is sent; migration proceeds normally.

## State Machines (D11 / D12 / D13)

D11, D12, and D13 sit on top of three primary state machines plus two trigger
inputs. This section is a consolidated, normative summary: the individual state
machines are described in scattered prose across the WCP design, and this
section pins down their explicit form and the transitions that D11–D13 own.

### SM-W: Worker supervisor's local build phase

Lives in the **worker process**, outlives any single websocket connection (per
WCP's "Worker-Side Active Build State"). One instance per active build the
supervisor knows about.

```
          (server sends BuildNew)
                 │
                 ▼
           ┌──────────┐
           │ accepted │◀──── created when worker sends build_accepted
           └────┬─────┘      (subprocess not yet spawned)
                │
       spawn subprocess
                │
                ▼
           ┌──────────┐
           │ started  │      subprocess running, output streaming
           └────┬─────┘
                │
   ┌────────────┼──────────────────┐
   │            │                  │
recv BuildRevoke subprocess exits  websocket drops
   │            (success/failure)   │
   ▼            │                   ▼
┌──────────┐    │            (no state change here;
│ revoking │    │             supervisor keeps the
└────┬─────┘    │             phase it was already in)
     │          │
   killed       │
     │          │
     ▼          ▼
┌──────────────────────────┐
│ terminal-pending-report  │  result held until next usable
└────┬─────────────────────┘  websocket
     │
   send build_finished(...)
     │
     ▼
   (state cleared)
```

`accepted` is the phase D11 cares about: the gap between `build_accepted` send
and subprocess spawn.

### SM-S: Server-side build state (SQLite `builds.state`)

Every transition specified by this design or by the WCP design is shown.
Transitions are labelled with the trigger that drives them; transitions not
labelled `D12`/`D13`/`D11` originate in the WCP design or in pre-existing server
logic.

```
              ┌────────┐
              │ queued │◀──────────────────────────────────┐
              └───┬────┘                                   │
                  │                                        │
         server selects worker                  rollback paths:
                  │                                ├─ dispatch failure
                  ▼                                ├─ transient build_rejected
            ┌────────────┐                         └─ (D12) AwaitingReceipt
            │ dispatched │  ⚠ has SM-R sub-state       liveness-dead
            └─┬──┬───┬───┘                             │
              │  │   │                                 │
              │  │   └────────► (D12) ReceivedByWorker liveness-dead ─┐
              │  │                                                     │
              │  └────────────► admin revoke (server-initiated)        │
              │                       │                                │
              │                       ▼                                │
              │                  ┌──────────┐                          │
              │                  │ revoking │                          │
              │                  └────┬─────┘                          │
              │                       │                                │
              │                       ├─ build_finished(revoked)       │
              │                       │       │                        │
              │                       │       ▼                        │
              │                       │  ┌─────────┐                   │
              │                       │  │ revoked │                   │
              │                       │  └─────────┘                   │
              │                       │                                │
              │                       └─ (D12) liveness-dead ──────────┤
              │                                                        │
              ▼                                                        ▼
        build_started OR build_output owned by sender             ┌─────────┐
              │                                                   │ failure │
              ▼                                                   └─────────┘
         ┌─────────┐                                                   ▲
         │ started │                                                   │
         └────┬────┘                                                   │
              │                                                        │
              ├─ build_finished(success) ─► success                    │
              ├─ build_finished(failure) ─► failure ───────────────────┘
              ├─ build_finished(revoked) ─► revoked  (D13 path:
              │                              supervisor reports revoked
              │                              without server having set
              │                              SM-S to revoking)
              ├─ admin revoke ────────────► revoking
              └─ (D12) liveness-dead ─────► failure
```

Notes on D13-driven direct transitions:

- `dispatched → revoked` and `started → revoked` are reachable via D13: the
  supervisor on the worker receives a migration-revoke, kills (or drains) the
  local build, and reports `build_finished(revoked)` on the new connection. D13
  deliberately does NOT set SM-S to `revoking` beforehand (reporter-directed
  cleanup, not state-mutating revoke), so the worker's revoked report
  transitions SM-S directly to `revoked`.
- For the `terminal-pending-report` drain-then-revoke path (D13, v2), the worker
  may report `build_finished(success)` or `build_finished(failure)` instead of
  `revoked`. SM-S transitions to the corresponding terminal state, preserving
  the real outcome.
- The four terminal states (`success`, `failure`, `revoked`, re-entries to
  `queued`) are absorbing for non-rollback paths; `queued` is the only state any
  build can re-enter from a non-terminal state, and only via the WCP
  rollback-cleanup operation specified in the WCP design's D4.

### SM-R: Active-assignment receipt (in-memory, sub-state of `dispatched`)

Lives in `queue.active[build_id]`, server process only. Per WCP v11 invariant
25, never reconstructed after server restart.

```
  ┌──────────────────┐
  │ AwaitingReceipt  │◀── initial state after dispatch
  └────────┬─────────┘
           │
   any owned message
   from assigned worker
   (build_accepted, build_started
    from dispatched, owned build_output,
    accepted-phase reconnect Building per D11)
           │
           ▼
  ┌──────────────────┐
  │ ReceivedByWorker │
  └──────────────────┘
```

SM-R only matters while SM-S is `dispatched`. Once SM-S advances to
`started`/`revoking`/terminal, SM-R is no longer consulted.

### Trigger inputs (not state machines themselves)

- **SM-C, connection events** — `live(conn_id)`, `superseded(old, new)`,
  `absent`. Drives WCP same-worker migration and D13.
- **SM-L, liveness signal** — `healthy` / `stale` / `dead`. Drives D12.

These are observed events / health states, not first-class state machines that
other transitions read from.

### Which decision owns which transitions

#### D11 — Worker `accepted` phase in reconnect status

Operates on **SM-W ↔ server reconnect-status reporting**, and triggers an SM-R
transition on the server.

| Trigger                                                                          | Pre-state                              | Post-state                              | Notes                                                                                                                                                                     |
| -------------------------------------------------------------------------------- | -------------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worker reconnects with SM-W = `accepted`                                         | server: `dispatched + AwaitingReceipt` | server: `dispatched + ReceivedByWorker` | Worker reports `WorkerStatus(Building {build_id})`. Server cancels the dispatch-ack timer. SM-S stays `dispatched` until the subprocess spawns and sends `build_started`. |
| Worker reconnects with SM-W = `started` / `revoking` / `terminal-pending-report` | (any)                                  | unchanged                               | Existing WCP v11 rules apply.                                                                                                                                             |
| Worker reconnects with SM-W = no local state                                     | (any owned by this worker per DB)      | per WCP idle-reconcile table            | Existing WCP v11 rules; D11 changes only the _condition_ under which `Idle` is reportable (now requires no `accepted` phase either).                                      |

Net effect on SM-W: the **rules for what triggers a status report**, not the
SM-W states themselves. SM-W's `accepted` state existed in WCP v11 but wasn't
checked by the reconnect rule; D11 adds it to the check.

#### D12 — Liveness/dead-worker resolution table

Operates on **(SM-L = dead) × SM-S × SM-R → terminal SM-S**.

| SM-L | SM-S         | SM-R               | → resulting SM-S | Side effects                                                                                                                                                                           |
| ---- | ------------ | ------------------ | ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| dead | `dispatched` | `AwaitingReceipt`  | `queued`         | WCP rollback-cleanup (clears `worker_id`, `trace_id`, etc.); remove active entry / watcher. Eligible for redispatch.                                                                   |
| dead | `dispatched` | `ReceivedByWorker` | `failure`        | Finalize log; remove active entry / watcher. Reason: `worker died after accepting assignment`. **No requeue** — avoids duplicate side effects already produced by the now-dead worker. |
| dead | `started`    | (any)              | `failure`        | Finalize log; remove active entry / watcher.                                                                                                                                           |
| dead | `revoking`   | (any)              | `revoked`        | Finalize log; remove active entry / watcher. (Worker death completes the revoke.)                                                                                                      |

Operates orthogonally to SM-W: when the server declares the worker dead, the
worker's local SM-W state is irrelevant (the worker isn't there to report it).

#### D13 — Same-worker migration revoke

Operates on a **(SM-C: supersede) compound transition**, with side effects on
SM-W via the wire and (indirectly) on SM-S via subsequent owned messages.

Server-side sequence during migration:

```
state: live(old=A)
      │
new connection B authenticates for the same registered worker ID
      │
      ▼
state: superseded(old=A, new=B)
      │
   step 1: snapshot owned active entries where
           queue.active[bid].connection_id = A
   step 2: send BuildRevoke{bid} on A's sender for each
           (best-effort)
   step 3: run WCP migration steps:
            - DB-backed ownership check (builds.worker_id matches)
            - swap queue.active[bid].connection_id from A to B
            - remove A's sender from worker_senders
      │
      ▼
state: live(B)   ← single live connection
```

What does NOT change during this transition:

- SM-S — no DB transition.
- SM-R — no receipt-state change.
- Active-entry timers — not canceled here.
- Log watchers — not removed here.

(These are the "reporter-directed cleanup, not state-mutating revoke" invariants
from D13.)

Worker-side reaction, dispatched per SM-W phase. The migration revoke carries
`BuildRevokeReason::MigrationSupersede` (per D13's protocol extension);
`terminal-pending-report` is the only phase that branches on this reason. The
other three phases behave identically regardless of reason.

| SM-W on receive of BuildRevoke during migration | Action                                                                                     | SM-W →                | Eventual outbound on new connection B                                                                                                    |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------ | --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `accepted`                                      | clear local assignment state (no subprocess)                                               | (cleared)             | `build_finished(revoked)`                                                                                                                |
| `started`                                       | kill + await subprocess                                                                    | `revoking` → terminal | `build_finished(revoked)`                                                                                                                |
| `revoking`                                      | idempotent re-kill (already in progress)                                                   | terminal              | `build_finished(revoked)` (once)                                                                                                         |
| `terminal-pending-report`                       | **drain the held terminal result** (do NOT discard, because `reason = MigrationSupersede`) | (cleared)             | `build_finished(<actual_outcome>)` — preserves the real `success` / `failure` / `revoked` outcome the subprocess produced. No data loss. |

The supervisor's outbound goes to **whichever websocket is active at the moment
of send** — which, after the migration completes, is B. If the BuildRevoke
send-on-A fails (channel closed, sender gone), the supervisor never receives it,
no `build_finished(revoked)` is generated from D13, and **D11 + D12 are the
safety nets**:

- If the supervisor still has SM-W non-terminal state, D11 has it report
  `Building` on B; the build continues naturally and concludes via
  `build_finished` or D12 if B then dies.
- If the supervisor lost local state (worker process restart between
  connections), it reports `Idle` on B; WCP idle-reconcile resolves the active
  entry; if B then also goes silent, D12 finalizes.

### How the three decisions compose

A single migration sequence can exercise all three:

```
        ┌─────────────────────────────────────────────────────┐
        │  Worker (host A)                Server              │
        │                                                     │
        │  SM-W: accepted   ◀── build_accepted ──▶ SM-R:      │
        │                                          AwaitingRx │
        │                          ▼                          │
        │                       (D11 input)                   │
        │                                          SM-R:      │
        │                          ▶ on reconnect ▶ ReceivedBy│
        │                                                     │
        │ ───────────── network blip ──────────────────       │
        │                                                     │
        │  New WS auth (B)         SM-C: live(A) →            │
        │                          superseded(A,B)            │
        │                          → live(B)                  │
        │                                                     │
        │                          (D13 fires here)           │
        │                          send BuildRevoke on A      │
        │                                                     │
        │  SM-W: accepted          ◀───── BuildRevoke         │
        │   → cleared                                         │
        │                          ─── build_finished(rev) ──▶│
        │                          SM-S: dispatched           │
        │                          → revoked                  │
        │                                                     │
        │ ──── alternative path: B never gets here ──────     │
        │                                                     │
        │                          SM-L: B stale → dead       │
        │                          (D12 fires)                │
        │                          SM-S × SM-R → terminal     │
        └─────────────────────────────────────────────────────┘
```

**D11** gates whether reconnect can correctly carry the receipt-acknowledgement
signal. **D13** gates whether stale subprocesses get stopped at migration time.
**D12** is the catchall safety net for both — it covers every case where neither
the migration revoke nor the new connection's `worker_status` arrives.

## Cross-Cutting Concerns

### Default secrets / config seeding

Default RBAC seed data (admin and developer roles) is updated to reflect the
periodic `:own` / `:any` split (D3). The seed migration adds both new
capabilities and removes the legacy `periodic:manage` where present.

### Documentation

Each decision lists a documentation touchpoint:

- D1: `cbsd-rs/CLAUDE.md` worker dev-mode section; `cbsd-worker --help`.
- D2: `cbsd-rs/docs/cbsd-rs/design/015-…-web-ui-authentication.md`
  cross-reference.
- D3: RBAC reference doc (if one exists; otherwise a short note).
- D4: `ui/index.html` inline comment explaining the fragment-based flow.
- D5: `cbsd-rs/docs/cbsd-rs/design/007-…-cbscore-wrapper.md` or wherever the
  component tarball flow is described.
- D6: `cbsd-rs/CLAUDE.md` "Correctness Invariants" section.
- D8: `cbc --help` output and the cbc README (if any).
- D10: `cbsd-rs/CLAUDE.md` logging policy section.

## Implementation Phasing

This design supports independent phasing because the decisions touch mostly
disjoint code paths:

1. **Phase A** (worker correctness): D1 (env hardening), D5 (tarball safety), D6
   worker side (WS limits), D11 (worker reconnect status). Worker-only; minimal
   server impact.
2. **Phase B** (server auth / authz): D2 (OAuth verify), D3 (periodic RBAC), D4
   (CLI redirect), D9 (logging), D10 (redaction). Server + UI changes; database
   migration for D3.
3. **Phase C** (size limits): D6 server side (request body, WS limits).
   Server-only.
4. **Phase D** (small fixes): D7 (DB index), D8 (cbc TLS + config). cbc and
   migration.
5. **Phase E** (worker-control-plane gap closers): D12 (liveness/dead-worker
   table), D13 (same-worker migration revoke + `BuildRevokeReason` protocol
   extension). Server-side, builds on the WCP design landing first.

**Phase E prerequisite tracking.** Phase E cannot land until:

- The WCP design has a plan document at
  `cbsd-rs/docs/cbsd-rs/plans/019-<timestamp>-worker-control-plane-hardening.md`
  (currently does not exist; needs to be authored before any WCP implementation
  commits).
- **All** WCP plan commits have landed in the codebase. Phase E reads from
  `queue.active[].connection_id`, the `ActiveAssignmentReceipt` field, the
  same-worker migration step boundary, and the reporter-directed revoke handler
  — all of which are introduced by the WCP implementation. Starting Phase E
  before these exist means implementing against shapes that do not yet match the
  codebase.

The plan document for this design (forthcoming, named
`019-<timestamp>-security-audit-remediation-01-foundation.md` etc.) must record
the WCP plan-document path and its commit-completion checkpoint as explicit
prerequisites for Phase E's first commit. Phase E's plan must also reflect the
protocol extension to `cbsd-proto` (`BuildRevokeReason`) and its rollout
coordination with the worker.

**Phase C / Phase E ordering note.** D6's "no per-log-line cap" trust argument
relies on the WCP ownership rules being enforced (only the assigned worker may
emit `BuildOutput` for a given build). Phase C (server-side size limits) is
independent of WCP, but if Phase C lands **before** WCP's ownership rules are in
force, there is a window in which log output is size-bounded at the message
level but not ownership-gated at the build level. This is acceptable: an
unauthenticated attacker still cannot emit `BuildOutput`, and an authenticated
worker is already a trusted principal even before the WCP rules tighten
ownership. The plan document should note this window explicitly so a reader
understands that the D6 trust argument is fully in force only after both Phase C
and the WCP implementation have landed.

Phases A and B are the priority. Phase C builds on B's logging changes. Phase D
is independent. Phase E follows the WCP design's implementation. A future plan
document (seq 019, `019-…-security-audit-remediation-01-foundation.md` etc.)
will sequence the commits.

## State Invariants Updated By This Design

The following invariants are added or reinforced:

1. Dev mode requires `CBSD_DEV` to be one of `1`, `true`, `yes`, `on`
   (case-insensitive). Any other value, including `0`, `false`, `no`, empty, or
   unset, disables dev mode.
2. The worker's `NoVerifier` rustls config is reachable only when dev mode is
   active AND the server URL is loopback.
3. OAuth login requires `email_verified == true` in the userinfo response.
   Unverified or missing-field responses fail closed.
4. Periodic-task mutating actions require either `periodic:manage:any` or
   `periodic:manage:own` plus row-owner match. Descriptor updates re-validate
   scopes against the updating user's effective scopes.
5. The CLI-login redirect uses a URL fragment. The token never appears in any
   server-side log.
6. Worker tarball unpack rejects entries whose symlink target resolves outside
   the unpack root and entries that exceed the uncompressed-size cap.
7. Every authenticated REST endpoint inherits the global
   `RequestBodyLimitLayer`. WebSocket connections cap message and frame sizes on
   both sides. Worker log-line ingestion has no per-line cap: authenticated
   workers are trusted to emit free-form text, and the WS message-size ceiling
   provides the indirect upper bound.
8. `api_keys.key_prefix` is indexed; prefix lookup is O(log n).
9. `cbc` rejects non-HTTPS hosts unless `--insecure-http` is explicitly set, and
   persists its config atomically with mode `0o600`.
10. No token material appears in any log line. `Secret<T>` does not implement
    `Serialize` or `Deserialize`; the only way to obtain the inner value is
    `.expose_secret()`. The CI gate prevents regressions once shipped.
11. Worker reconnect status reports `Building { build_id }` when the supervisor
    has any non-terminal local assignment state, including the `accepted` phase.
    `Idle` is reported only when no executor, no in-progress revoke, no pending
    terminal result, and no `accepted` assignment exists.
12. Liveness/dead-worker resolution distinguishes `dispatched + AwaitingReceipt`
    (roll back to `queued`) from `dispatched + ReceivedByWorker` (fail).
    `started` and `revoking` are resolved to `failure` and `revoked`
    respectively.
13. Same-worker reconnect migration sends
    `BuildRevoke { reason: MigrationSupersede }` to the superseded old
    connection (for every owned active assignment) before removing its sender.
    The revoke is reporter-directed cleanup; it has no DB, timer, watcher, or
    queue side effects.
14. A worker supervisor that receives
    `BuildRevoke { reason: MigrationSupersede }` while in SM-W
    `terminal-pending-report` drains the real outcome and reports
    `build_finished(<actual_outcome>)` rather than discarding the result.
    Admin-initiated revokes (reason `Admin`) against `terminal-pending-report`
    continue to discard per WCP v11.
15. Periodic-task triggers re-validate the stored descriptor against the task
    owner's current effective capabilities before submitting the build. Lost
    capabilities disable the task and persist `last_error`; no build is enqueued
    under outdated authorization. When the owner row is no longer a valid
    identity (canonical lookup returns zero rows), the owner's effective scopes
    are the empty set; the trigger follows the same fatal-disable path and
    records `owner_account_missing` in `last_error`. The trigger MUST NOT panic,
    MUST NOT raise to the scheduler loop, and MUST NOT fall back to a cached
    scope set. The canonical lookup is schema-dependent: today (hard-delete
    schema) it is the unfiltered query `WHERE email = ?`; if a future migration
    adds a soft-delete marker, the canonical lookup is extended with the
    appropriate filter (e.g., `WHERE email = ? AND deleted_at IS NULL`) at the
    same time as the migration lands. The "no longer valid identity" behaviour
    is identical under either schema state.
16. The cbsd-server URI-logging policy permits only `Uri::path()` at INFO or
    above; query strings appear at DEBUG only in explicitly annotated handlers.
    No middleware, panic handler, or error reporter may bypass this policy.
17. The worker supervisor honours
    `BuildRevoke { reason: Some(MigrationSupersede) }` only when its local
    `last_authenticated_connect_at` is within `MIGRATION_RECENT_WINDOW = 30s` of
    the present moment; otherwise it falls back to `Admin` semantics and emits a
    `WARN`-level `migration_supersede_without_recent_reconnect` diagnostic plus
    a non-fatal counter increment. The flag is set on every successful
    authenticated websocket reconnect for the registered worker ID and is never
    actively cleared. The flag's type is `tokio::time::Instant` (not
    `std::time::Instant`) so test code using `tokio::time::pause()` /
    `tokio::time::advance()` can drive the predicate deterministically;
    `tokio::time::Instant` wraps `std::time::Instant` under the real-clock
    runtime, so production semantics are unchanged. **Caveat — `Instant` and
    host suspension.** Under the real-clock runtime, the flag inherits
    `std::time::Instant`'s monotonic-clock semantics (`CLOCK_MONOTONIC` on
    Linux). On Linux, `CLOCK_MONOTONIC` does NOT advance during host suspension
    (suspend-to-RAM, suspend-to-disk); `CLOCK_BOOTTIME` does. A worker host that
    suspends for hours and resumes will compute `elapsed()` against a frozen
    reading, which can return a value below the 30 s window even though
    wall-clock time has moved well past it. This produces a **false positive**
    (migration plausible when it shouldn't be). The risk is bounded: a malicious
    server would have to time its coercion attempt to coincide with a victim's
    suspend-resume cycle, and worker hosts in production are typically
    bare-metal or always-on VMs that do not suspend. Operator guidance: do not
    run `cbsd-worker` on a laptop or any host with active power-saving
    suspension; if such a deployment becomes necessary, switch the supervisor's
    clock to a `CLOCK_BOOTTIME`-backed type (e.g., via the `nix` crate's
    `clock_gettime`) in a follow-up commit. This caveat is intentional in v5;
    the alternative (using `SystemTime`) sacrifices monotonicity for
    suspension-awareness, and the trade-off is worse for the typical worker
    deployment.
18. `cbsd-proto` wire types — specifically `ServerMessage` and any of its
    variants — MUST NOT carry `#[serde(deny_unknown_fields)]`. This is a
    load-bearing rolling-upgrade invariant: without it, old workers reject
    forward-compatible additions like `BuildRevoke.reason`. The invariant is
    enforced by test D13-T6 (an unknown-field round-trip on `BuildRevoke`),
    which fails if a future contributor adds the attribute.

## Open Questions

None at this revision. The v1-draft open questions and the v1 review findings
were resolved as follows:

**v1-draft open questions (maintainer feedback, 2026-05-14):**

1. WS message cap value (D6) — accepted at 8 MiB as initial default. Real
   component tarballs today are ~2 KiB compressed / ~20 KiB uncompressed, so the
   8 MiB ceiling has ~4,000× headroom. Revisit only if a future component class
   requires it.
2. CI grep gate location (D10) — deferred. A tooling-comparison (Lefthook vs.
   `pre-commit` vs. alternatives) is on the roadmap and will produce a follow-up
   decision document before the gate is implemented. The interim policy is the
   `Secret<T>` newtype plus targeted review.
3. `Secret<T>` newtype (D10) — confirmed. Implement now as the durable
   construction-time defense; the CI gate (Q2) will layer on top later.
4. Log-line policy (D6) — resolved. No per-line cap. Authenticated workers are
   trusted to emit free-form text; truncation or rejection would lose diagnostic
   content (large compiler errors, embedded JSON, stack traces) exactly when an
   operator needs it most. The trust boundary is worker authentication and the
   WCP design ownership rules, not per-line filtering.
5. Decompression cap (D5) — accepted at 256 MiB. Current real components
   uncompress to ~20 KiB, so the cap has ~13,000× headroom. Operators with
   legitimately larger components can raise the cap via worker config.

**v1 review findings (closed in v2):**

- **F-R1 (Critical)** — D13 `terminal-pending-report` silent data loss: resolved
  by drain-then-revoke semantics on `MigrationSupersede` revoke; protocol
  extension `BuildRevokeReason` added to `cbsd-proto` to disambiguate from admin
  revokes.
- **F-R2 (Critical)** — D10 `Secret<T>` Serialize gap: resolved by forbidding
  `Serialize` / `Deserialize` impls and requiring `.expose_secret()`.
  Compile-fail tests added.
- **F-R3 (High)** — D5 PAX-header + chained-symlink containment: resolved by
  explicit PAX-aware `entry.path()` use and a two-phase (logical + real-path)
  containment check against already-unpacked entries. Tests added.
- **F-R4 (Significant)** — D1 loopback algorithm: resolved by replacing the
  prose list with a concrete `url::Host` algorithm covering IPv4 `127.0.0.0/8`,
  IPv6 `::1`, and `localhost` (ASCII case-insensitive).
- **F-R5 (Significant)** — SM-S diagram: extended to include all rollback and
  D12 / D13 transitions, with explicit notes on D13's direct-to-terminal paths.
- **F-R6 (Significant)** — D3 trigger-time scope re-validation: added as an
  explicit decision; scheduler trigger disables tasks on lost capability.
- **F-R7 (Minor)** — D2 userinfo trust gap: documented as a residual known
  limitation pointing at a future ID-token-introspection hardening pass.
- **F-R8 (Minor)** — D9 broadened to a project-wide URI-logging policy covering
  all middleware, panic handlers, and error reporters.
- **F-R9 (Minor)** — D8 Windows rename atomicity: documented with the current
  platform stance; error-path temp-file cleanup is now an explicit MUST.
- **F-R10 (Minor)** — D3 custom-role migration: explicit migration spec; legacy
  `periodic:manage` is dropped without auto-mapping, operators must re-grant.
- **F-R11 (Minor)** — D4 `history.replaceState`: required after fragment
  extraction. Tests added.
- **F-R12 (Minor)** — Phase E prerequisites: explicit WCP plan-document
  - commit-completion checkpoint required before Phase E begins; Phase C / Phase
    E ordering note added.

**v2 review findings (closed in v3):**

- **N-1 (Significant)** — `BuildRevokeReason` serde representation unspecified:
  closed by pinning the `BuildRevoke.reason` field as
  `Option<BuildRevokeReason>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. The worker
  treats `None` as `Admin` semantics, making old-server → new-worker safe. The
  worker also confirms migration via its recent-reconnect signal (SM-C is a
  trigger input, not a state machine with persistent state) before honouring
  `MigrationSupersede`, so a malicious server cannot coerce drain-then-revoke
  semantics via the wire field alone. Five serde compatibility tests added to
  the test summary.
- **N-2 (Significant)** — CI gate uses `#` not `//`: closed by switching every
  exemption marker to the standard Rust line-comment form `// allow-expose`. The
  gate spec, the test examples, and the test description all use the corrected
  form.
- **N-3 (Minor)** — D5 chained-symlink test example: closed by reframing Phase 2
  of the containment check as defense-in-depth (against Phase 1 implementation
  bugs, TOCTOU during unpack, and future `tar` crate changes) rather than as the
  primary defense against a specific attack vector. The misleading test example
  is replaced by three tests: a happy-path symlink chain, a fault-injection test
  that exercises Phase 2 under a TOCTOU-style mutation, and a regression test
  pinning that strict logical normalization in Phase 1 catches `..` targets.
- **N-4 (Minor)** — D3 owner-deleted edge case: closed by adding the normative
  invariant that a missing owner row is treated as empty effective scopes,
  following the same fatal-disable path as scope-reduction. The State Invariants
  section was updated to mirror the new requirement.
- **N-5 (Informational)** — `secrecy::ExposeSecret` import: documented in D10 as
  a small import-hygiene requirement that the plan must surface at each
  `.expose_secret()` call site.

**v3 review findings (closed in v4):**

- **NF-1 (Significant)** — `deny_unknown_fields` prohibition without a
  regression test: closed by adding test D13-T6, a `cbsd-proto` round-trip test
  asserting that an unknown `future_field` on `BuildRevoke` deserializes
  successfully. The test would fail if any `ServerMessage` variant gains a
  `#[serde(deny_unknown_fields)]` attribute. State Invariant 18 records the
  prohibition explicitly so the test's purpose is visible without reading D13.
- **NF-2 (Significant)** — SM-C anti-coercion predicate unspecified: closed by
  adding a concrete supervisor-level
  `last_authenticated_connect_at: Option<Instant>` flag, a
  `MIGRATION_RECENT_WINDOW = 30s` constant, the
  `migration_plausible(&Supervisor) -> bool` predicate, and concrete semantics
  for both branches (honour the reason / fall back to Admin
  - WARN). Test D13-T7 exercises the false-predicate fallback plus positive- and
    boundary-case sub-tests. State Invariant 17 records the predicate's
    normative form.
- **NF-3 (Minor)** — D3 owner-deleted test missing + soft-delete ambiguity:
  closed by adding two tests (D3-T-owner-deleted for hard delete,
  D3-T-owner-soft-deleted for the `deleted_at` marker case), and by extending
  D3's "Owner-deleted case" prose to specify explicitly that any soft-delete
  marker is treated as equivalent to row absence. State Invariant 15 reflects
  the soft-delete clause.

**v4 review findings (closed in v5):**

- **SF-1 (Significant)** — D13-T6 covered only `BuildRevoke` while SI-18 applies
  to every `ServerMessage` variant: closed by rewriting D13-T6 as a per-variant
  test loop guarded by an **exhaustive-match witness** function. Adding a new
  `ServerMessage` variant without a matching test entry fails to compile,
  turning SI-18 into a compile-time gate plus a runtime per-variant
  deserialization check.
- **SF-2 (Significant)** — D3 soft-delete MUST contradicted the production
  schema: closed by rewriting the D3 contract as **conditional on schema
  state**. Today (no soft-delete column), the trigger's canonical lookup is
  `WHERE email = ?` with no filter; if/when a future migration adds a
  soft-delete marker, the canonical lookup is extended at the same time and the
  soft-delete clause becomes binding. The `D3-T-owner-soft-deleted` test is
  feature-gated behind `cfg(feature = "soft-delete-schema")`; the default test
  run exercises only the hard-delete contract.
- **Minor 1** — D13-T7 boundary tests flaky with real `Instant::now()`: closed
  by adopting `tokio::time::pause()` / `advance()` for deterministic timing. The
  supervisor's clock source is wired through a small trait/wrapper so the test's
  paused runtime controls the predicate.
- **Minor 2** — `Instant` does not advance during host suspension on Linux:
  closed by documenting the caveat in SI-17 with explicit operator guidance
  (don't run `cbsd-worker` on a suspending host; if needed, switch to
  `CLOCK_BOOTTIME`-backed clock in a follow-up commit).
- **Minor 3** — "SM-C transition" terminology drift in v3 history entry: closed
  by rewording to "recent-reconnect signal (SM-C is a trigger input, not a state
  machine with persistent state)".

**v5 review findings (closed in v6):**

- **NF-1 (Critical)** — D13-T6 sketch referenced non-existent
  `ServerMessage::UnauthorizedBuildAction` variant (proposed in WCP design, not
  yet in `cbsd-proto/src/ws.rs`) and omitted the actual `Error` variant: closed
  by rewriting the witness/cases/sentinel sketches against the **real**
  `cbsd-proto/src/ws.rs` enum (`BuildNew`, `BuildRevoke`, `Welcome`, `Error`),
  with correct field shapes (`Welcome` has
  `protocol_version`/`connection_id`/`grace_period_secs`; `Error` has
  `reason`/`min_version`/`max_version`), and an explicit note that WCP-proposed
  variants must be added to the witness, the sentinel constructor, AND the cases
  map when they land.
- **NF-2 (Minor)** — `std::time::Instant` in SI-17 vs `tokio::time::pause()` in
  D13-T7: closed by pinning the supervisor flag's type to
  `Option<tokio::time::Instant>` in SI-17 and in the D13-T7 sketch.
  `tokio::time::Instant` wraps `std::time::Instant` under the real-clock
  runtime, so the SI-17 `CLOCK_MONOTONIC` caveat is unchanged.
- **NF-3 (Minor)** — runtime exhaustiveness check elided with `// …`: closed by
  adding a concrete `sentinel_for_tag` function that constructs one
  `ServerMessage` per tag, plus an explicit test-side loop that passes each
  sentinel through the witness and asserts the returned tag matches the input.
  Three layers of protection are now spelled out (compile-time, runtime
  exhaustiveness, runtime deserialization).
- **NF-4 (Minor)** — `cfg(feature = "soft-delete-schema")` on `sqlx::migrate!()`
  is incompatible with compile-time embedding: closed by switching to a
  **test-function-level** feature gate plus an **inline `ALTER TABLE`** in the
  test fixture's setup function (after `sqlx::migrate!()` runs the standard
  production migrations). This keeps `sqlx::migrate!` compile-time-clean and
  scopes the schema divergence to one test.
- **NF-5 (Minor)** — v5 history attributed the MF-3 fix to "v3 entry" while the
  v4 review didn't specify which entry: closed by rephrasing to "a prior
  revision-history entry" to sidestep the off-by-one argument. The actual fix
  location is unchanged.

**v6 review findings (closed in v7):**

- **NF-1-v6 (Critical)** — `BuildDescriptor::default()` and other `::default()`
  calls in the v6 sketch do not compile because the underlying types don't impl
  `Default` in `cbsd-proto/src/build.rs`: closed by reading
  `cbsd-proto/src/{ws,build,arch}.rs` directly, documenting every type's actual
  API in the prose preceding the sketch (BuildId tuple struct, Priority's
  Default impl, the no-Default types), and rewriting `sentinel_for_tag` with
  explicit field construction via a new `test_descriptor()` helper that mirrors
  the existing test pattern at
  `cbsd-proto/src/ws.rs::tests::server_message_build_new_round_trip` (lines
  142-182). The v7 sketch is verified to compile against the real crate API.
- **NF-6 (Minor)** — hardcoded tag list was a fourth maintenance point not
  described by the "three layers of protection" prose: closed by removing the
  hardcoded list. The runtime exhaustiveness loop now iterates over `cases()`
  directly, leaving exactly three coordinated lists (witness, sentinel_for_tag,
  cases) and three honestly-described protection layers. The known gap ("witness
  updated, case forgotten") is documented explicitly with its mitigation
  (witness source comment + PR review) and a forward pointer to closing it via
  `strum` or similar.
- **NF-7 (Minor)** — undefined `minimal_descriptor_json()` referenced by the
  sketch: closed by replacing it with `test_descriptor_json()`, defined inline
  as `serde_json::to_value(test_descriptor()).unwrap()`.
- **NF-8 (Minor)** — D13-T6 test placement unspecified: closed by specifying
  placement in `cbsd-proto/src/ws.rs::tests` (same module as existing wire-shape
  tests), with an explicit note that same-crate placement is load-bearing for
  the exhaustive-match semantics — required if `ServerMessage` later gains
  `#[non_exhaustive]`.

**v7 review findings (closed in v8):**

- **NF-1-v7 (Minor)** — duplicate `use crate::arch::Arch;` and
  `use crate::build::{…}` imports in the v7 sketch preamble would trigger
  `unused_imports` under `-D warnings` because the existing `mod tests` block
  already imports them (`cbsd-proto/src/ws.rs:134-140`): closed by removing the
  duplicates. The v8 sketch's preamble only adds
  `use serde_json::{Value, json};` and `use strum::IntoEnumIterator;`.
- **NF-2-v7 (Minor)** — dead `case_tags: HashSet<&'static str>` allocation
  suppressed with `let _ = &case_tags;` in the v7 test loop: closed by removing
  the allocation. The v8 test uses a `HashMap<&'static str, Value>` of cases
  keyed by wire-tag for the lookup assertion, which serves the same purpose with
  no dead code.
- **NF-3-v7 (Minor)** — "witness updated, case forgotten" gap unautomated
  (acknowledged in v7 as a known gap with PR-review mitigation): closed
  substantively rather than deferring. v8 introduces a `#[cfg(test)]` companion
  enum `ServerMessageTag` with `#[derive(strum::EnumIter)]` in
  `cbsd-proto/src/ws.rs::tests`. `ServerMessageTag::from_message` is the
  compile-time witness (exhaustive on `ServerMessage`);
  `ServerMessageTag::as_wire` is the compile-time wire-tag mapping (exhaustive
  on `ServerMessageTag`); `iter()` provides runtime enumeration of every
  tag-enum variant. The test loop iterates `iter()` and asserts each tag has a
  sentinel (compile-forced via `sentinel_for_tag`'s exhaustive match on the tag
  enum) and a case (runtime-asserted via `cases_map.contains_key`). Four
  protection layers, all automated, no manual list maintenance. Adds
  `strum = { version = "0.26", features = ["derive"] }` to a new
  `[dev-dependencies]` section of `cbsd-proto/Cargo.toml` (test-only dep, not a
  production dependency).

## Test Expectations Summary

A future plan document will catalog tests by phase. At minimum:

- D1: 6 unit tests + 2 integration tests for env handling and worker startup
  safety, plus 8 `is_loopback_url` unit tests covering localhost, 127.0.0.0/8,
  ::1, and the authority-confusion / non-loopback negative cases.
- D2: 4 OAuth-callback integration tests with mocked userinfo responses.
- D3: 8 RBAC tests for cross-owner mutation, the trigger-time scope-revalidation
  path, and the custom-role migration; 1 migration schema test.
- D4: 1 redirect-target integration test, 1 negative log-capture test, 2
  browser-level tests for the fragment clear (post-extract hash empty and
  history not retaining the token).
- D5: ~10 tarball fixtures (good symlink, bad symlinks, device, size-bomb,
  boundary cases, PAX-overridden path, chained-symlink escape, hardlink outside
  root).
- D6: 4 limit-enforcement tests across REST, WS, and tarball binary frame
  ceiling.
- D7: migration apply test; 1 query-plan confirmation.
- D8: scheme-rejection tests; atomic-write race test; error-path
  temp-file-cleanup test.
- D9: 3 log-capture tests covering TraceLayer span, panic handler URI elision,
  and a manual grep-style audit gate.
- D10: 4 tests: `tracing-test` redaction, `trybuild` compile-fail for
  `#[derive(Serialize)]` over `Secret<T>`, `trybuild` compile-fail for
  inner-field access, CI-gate negative test for `.expose_secret()` without
  allow-comment.
- D11: 2 reconnect tests covering the `accepted`-phase status reporting.
- D12: 5 dead-worker resolution tests covering the four (DB state × receipt)
  rows in the table plus a server-restart interaction test.
- D13: 7 same-worker migration tests covering the four subprocess phases at
  migration time (accepted, started, revoking, terminal-pending-report with
  drain-then-revoke), plus revoke-send failure, concurrent unauthorized message,
  and no-owned-assignment cases. Includes a regression test asserting an admin
  revoke (reason `Admin`) against a `terminal-pending-report` build still
  discards per WCP v11.
- D13 serde compatibility tests for `BuildRevoke.reason`:
  - **Absent-field deserialize**: a JSON `BuildRevoke { build_id }` with no
    `reason` key deserializes successfully on a v3 worker and resolves to
    `reason: None`; the worker handler treats `None` as `Admin` semantics. (Pins
    backward compatibility: old-server → new-worker.)
  - **Round-trip with `None`**: serializing a
    `BuildRevoke { build_id, reason: None }` MUST NOT emit a `reason` key in the
    JSON output (per `skip_serializing_if = "Option::is_none"`), producing a
    byte sequence identical to a pre-v3 server's wire format. (Pins new-server →
    old-worker: pre-v3 deserializers see no unknown field.)
  - **Round-trip with `Some(MigrationSupersede)`**: serializing emits
    `"reason": "migration_supersede"`; an old worker without
    `deny_unknown_fields` silently ignores the extra key and deserializes
    normally with no reason information, falling back to WCP v11 semantics.
  - **Unknown-variant rejection**: an old-server emitting
    `"reason": "future_reason"` against a v3 worker fails the
    `BuildRevokeReason` deserializer (because the enum is closed) and surfaces
    as a `cbsd-proto` deserialization error rather than silently misinterpreted.
    The error path must be captured and logged.
  - **Migration-context advisory check (D13-T7) — deterministic clock**: test
    cases use `tokio::time::pause()` to freeze the runtime clock and
    `tokio::time::advance(Duration)` to step it deterministically, instead of
    relying on real `Instant::now()` and sleeping. This avoids CI flake at the
    boundary. The supervisor's clock source for `last_authenticated_connect_at`
    is wired through a small trait or a `tokio::time::Instant` so the test's
    paused clock controls the predicate. Test cases:
    - **Negative case**: with no recent authenticated reconnect
      (`last_authenticated_connect_at = None`, or elapsed past the window), the
      supervisor receives
      `BuildRevoke { build_id, reason: Some(MigrationSupersede) }` for a build
      in SM-W `terminal-pending-report`. Assert: the supervisor discards the
      terminal result (`Admin`-semantics fallback), reports
      `build_finished(revoked)` on the available connection, emits a
      `WARN`-level log with
      `event = "migration_supersede_without_recent_reconnect"`, and increments
      the `migration_supersede_implausible_count` counter.
    - **Positive case**: set `last_authenticated_connect_at`, then
      `tokio::time::advance(Duration::from_secs(1))`, then deliver the migration
      revoke. Assert the drain-then-revoke path executes per D13's
      `terminal-pending-report` rule.
    - **Boundary just-inside**: set the flag,
      `tokio::time::advance(MIGRATION_RECENT_WINDOW - Duration::from_millis(1))`,
      deliver. Assert plausible (drain-then-revoke).
    - **Boundary just-outside**: set the flag,
      `tokio::time::advance(MIGRATION_RECENT_WINDOW + Duration::from_millis(1))`,
      deliver. Assert implausible (`Admin`-fallback).
    - Note: real wall-clock arithmetic with `Instant::now() - 30s ± 1ms` is
      non-deterministic under CI scheduling jitter; the paused-runtime pattern
      is the only reliable way to test boundary semantics. The supervisor's
      production code path uses real `Instant::now()`; the test harness
      substitutes a paused clock via the trait/wrapper.
  - **`deny_unknown_fields` regression (D13-T6) — per-variant with
    exhaustive-match witness**: SI-18 prohibits `#[serde(deny_unknown_fields)]`
    on **every** `ServerMessage` variant, not just `BuildRevoke`. The test
    enforces this with a two-layer guard: a compile-time exhaustive match that
    catches new variants missing from the test list, and a runtime loop that
    deserializes each listed variant with an unknown field. Sketch:

    The sketch below is written against the **current** `ServerMessage` variants
    in `cbsd-proto/src/ws.rs` (`BuildNew`, `BuildRevoke`, `Welcome`, `Error`)
    and the related types in `cbsd-proto/src/{build,arch}.rs`. Every field
    shape, constructor, and default-availability claim in this sketch has been
    verified against the source as of this revision. Specifically:
    - `BuildId` is `pub struct BuildId(pub i64)`; `BuildId(0)` is valid; no
      `Default` impl.
    - `Priority` derives `Default` with `#[default]` on `Normal`;
      `Priority::default()` returns `Normal`; serde rename_all `"lowercase"`, so
      it serializes as `"normal"`.
    - `BuildDescriptor` and its nested types (`BuildSignedOffBy`,
      `BuildDestImage`, `BuildComponent`, `BuildTarget`) do **NOT** impl
      `Default` — explicit field construction is required. `test_descriptor()`
      below mirrors the existing pattern at
      `cbsd-proto/src/ws.rs::tests::server_message_build_new_round_trip`.
    - `Arch` does not impl `Default` either; the sketch uses `Arch::X86_64`
      (matching the `default_arch()` helper used by serde's
      `default = "default_arch"` annotation on `BuildTarget::arch`).
    - `ServerMessage::Welcome` carries `protocol_version: u32`,
      `connection_id: String`, `grace_period_secs: u64`.
    - `ServerMessage::Error` carries `reason: String`,
      `min_version: Option<u32>`, `max_version: Option<u32>`.

    When the WCP design's `UnauthorizedBuildAction` (or any other future
    variant) is added to the crate, the developer MUST add a corresponding match
    arm to `variant_tag_witness`, a sentinel constructor to `sentinel_for_tag`,
    and a JSON case to `cases`. The compile-time witness gate forces the witness
    arm; the runtime exhaustiveness loop forces the sentinel arm; adding the
    case requires no automated gate but is documented in the witness's source
    comment.

    **Test placement.** D13-T6 lives in the existing `#[cfg(test)] mod tests`
    block in `cbsd-proto/src/ws.rs`, alongside the
    `server_message_build_new_round_trip` and
    `server_message_welcome_includes_grace_period` tests. Placing it inside the
    `cbsd-proto` crate (rather than a downstream consumer) is load-bearing:
    rust's match exhaustiveness check on `ServerMessage` requires that the
    witness live in the same crate when the enum is not marked
    `#[non_exhaustive]`. The witness function thus automatically extends its
    exhaustiveness guarantee across any future addition of `#[non_exhaustive]`
    to `ServerMessage`.

    **Cargo.toml change required**: `cbsd-proto/Cargo.toml` gains a new
    `[dev-dependencies]` section (verified non-existent today):

    ```toml
    [dev-dependencies]
    strum = { version = "0.26", features = ["derive"] }
    ```

    `strum` is a small, widely-adopted enum-utility crate with derives. Used
    here only for `EnumIter` on a test-only companion enum; `strum` is NOT a
    production dependency.

    ```rust
    // File: cbsd-proto/src/ws.rs, inside #[cfg(test)] mod tests.
    //
    // The existing `mod tests` block already imports `use super::*`,
    // plus `use crate::arch::Arch;` and
    // `use crate::build::{BuildComponent, BuildDestImage,
    //                     BuildSignedOffBy, BuildTarget, VersionType};`
    // (verified at cbsd-proto/src/ws.rs:134-140). The v8 sketch only
    // ADDS the imports below; it does NOT re-import types already in
    // scope. Adding existing imports would trigger `unused_imports`
    // under `-D warnings` (v7 review NF-1-v7).
    use serde_json::{Value, json};
    use strum::IntoEnumIterator;

    /// Build a valid `BuildDescriptor` for SI-18 test payloads.
    /// Mirrors `server_message_build_new_round_trip`'s explicit
    /// construction because `BuildDescriptor` does not impl `Default`
    /// in `cbsd-proto/src/build.rs:121-132`.
    fn test_descriptor() -> BuildDescriptor {
        BuildDescriptor {
            version: "test".to_string(),
            channel: None,
            version_type: None,
            signed_off_by: BuildSignedOffBy {
                user: "test".to_string(),
                email: "test@example.com".to_string(),
            },
            dst_image: BuildDestImage {
                name: "test-image".to_string(),
                tag: "test-tag".to_string(),
            },
            components: vec![BuildComponent {
                name: "test-component".to_string(),
                git_ref: "v0".to_string(),
                repo: None,
            }],
            build: BuildTarget {
                distro: "rockylinux".to_string(),
                os_version: "el9".to_string(),
                artifact_type: "rpm".to_string(),
                arch: Arch::X86_64,
            },
        }
    }

    /// JSON form of `test_descriptor()`. Always succeeds because
    /// `BuildDescriptor` derives `Serialize`.
    fn test_descriptor_json() -> Value {
        serde_json::to_value(test_descriptor()).unwrap()
    }

    /// Test-only companion enum mirroring `ServerMessage`'s variants
    /// without their associated data. `strum::EnumIter` derives a
    /// `Self::iter()` method that yields every variant. This is the
    /// runtime-enumeration mechanism that closes the v7 review's
    /// NF-3-v7 "witness updated, case forgotten" gap: the test
    /// iterates `iter()` and asserts every tag has a sentinel and a
    /// case.
    #[derive(strum::EnumIter, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum ServerMessageTag {
        BuildNew,
        BuildRevoke,
        Welcome,
        Error,
    }

    impl ServerMessageTag {
        /// Compile-time witness. Exhaustive on `ServerMessage` — `rustc`
        /// rejects this with a non-exhaustive-match error if a new
        /// variant is added to `ServerMessage` without a corresponding
        /// arm here. Each arm must map to an existing
        /// `ServerMessageTag` variant, so a missing tag-enum variant
        /// surfaces as an "unknown variant" compile error in the arm
        /// body — forcing the tag-enum update too.
        fn from_message(msg: &ServerMessage) -> Self {
            match msg {
                ServerMessage::BuildNew { .. } => Self::BuildNew,
                ServerMessage::BuildRevoke { .. } => Self::BuildRevoke,
                ServerMessage::Welcome { .. } => Self::Welcome,
                ServerMessage::Error { .. } => Self::Error,
            }
        }

        /// Wire-format tag (the serde `"type"` discriminator) for
        /// this variant. Exhaustive on `Self` — `rustc` forces an arm
        /// when a new variant is added to `ServerMessageTag`.
        fn as_wire(&self) -> &'static str {
            match self {
                Self::BuildNew => "build_new",
                Self::BuildRevoke => "build_revoke",
                Self::Welcome => "welcome",
                Self::Error => "error",
            }
        }
    }

    /// Construct a sentinel `ServerMessage` for a given tag. Panics
    /// on unknown tags so that "tag enumerated but no sentinel" trips
    /// a loud runtime failure. Every field is explicitly constructed
    /// because the underlying types do not impl `Default` (see the
    /// verified-facts list preceding this sketch).
    fn sentinel_for_tag(tag: ServerMessageTag) -> ServerMessage {
        match tag {
            ServerMessageTag::BuildNew => ServerMessage::BuildNew {
                build_id: BuildId(0),
                trace_id: String::new(),
                priority: Priority::default(),
                descriptor: Box::new(test_descriptor()),
                component_sha256: String::new(),
            },
            ServerMessageTag::BuildRevoke => ServerMessage::BuildRevoke {
                build_id: BuildId(0),
            },
            ServerMessageTag::Welcome => ServerMessage::Welcome {
                protocol_version: 2,
                connection_id: String::new(),
                grace_period_secs: 0,
            },
            ServerMessageTag::Error => ServerMessage::Error {
                reason: String::new(),
                min_version: None,
                max_version: None,
            },
        }
    }

    /// JSON payloads for the SI-18 deserialization check, keyed by
    /// wire-format tag. Each payload's shape matches the
    /// corresponding variant's field schema in
    /// `cbsd-proto/src/ws.rs`, plus an injected `future_field` to
    /// exercise the unknown-field path.
    ///
    /// Maintenance: `cases()` is the third coordinated list (alongside
    /// the `ServerMessageTag` enum and the `from_message`/`as_wire`
    /// matches). Adding a `ServerMessageTag` variant without adding a
    /// case here trips the runtime assertion in
    /// `no_deny_unknown_fields_on_server_message`.
    fn cases() -> Vec<(&'static str, Value)> {
        vec![
            ("build_new", json!({
                "type": "build_new",
                "build_id": 42,
                "trace_id":
                    "00000000-0000-0000-0000-000000000000",
                "priority": "normal",
                "descriptor": test_descriptor_json(),
                "component_sha256":
                    "0000000000000000000000000000000000000000\
                     000000000000000000000000",
                "future_field": "x",
            })),
            ("build_revoke", json!({
                "type": "build_revoke",
                "build_id": 42,
                "future_field": "x",
                // D13's BuildRevoke.reason field is Option<...> with
                // serde(default), so this payload works against both
                // the pre-D13 and post-D13 wire shapes. The separate
                // serde-compatibility tests above pin the
                // reason-field handling specifically.
            })),
            ("welcome", json!({
                "type": "welcome",
                "protocol_version": 2,
                "connection_id": "test-conn-id",
                "grace_period_secs": 60,
                "future_field": "x",
            })),
            ("error", json!({
                "type": "error",
                "reason": "test",
                "min_version": null,
                "max_version": null,
                "future_field": "x",
            })),
        ]
    }

    #[test]
    fn no_deny_unknown_fields_on_server_message() {
        let cases_map: std::collections::HashMap<&'static str, Value> =
            cases().into_iter().collect();

        // Layer 2: runtime exhaustiveness over ALL ServerMessageTag
        // variants. `ServerMessageTag::iter()` is derived by strum
        // and automatically extends when a variant is added to the
        // tag enum. For each tag, construct a sentinel (the
        // sentinel match is exhaustive on ServerMessageTag, so
        // adding a tag variant is compile-forced), verify the
        // witness round-trips the sentinel, and confirm a case
        // exists. This is the gate that closes the
        // "case forgotten" gap.
        for tag in ServerMessageTag::iter() {
            let wire = tag.as_wire();
            let sentinel = sentinel_for_tag(tag);
            let witnessed = ServerMessageTag::from_message(&sentinel);
            assert_eq!(
                tag, witnessed,
                "sentinel/witness drift for tag `{}`: from_message \
                 returned a different ServerMessageTag",
                wire,
            );
            assert!(
                cases_map.contains_key(wire),
                "ServerMessageTag::{:?} (wire `{}`) has no entry in \
                 cases() — SI-18 is not enforced for this variant. \
                 Add a case to cases() in cbsd-proto/src/ws.rs.",
                tag, wire,
            );
        }

        // Layer 3: per-variant deserialization check.
        // The SI-18 contract: confirm each cases() payload accepts
        // an unknown field. Fails if any variant gains
        // `#[serde(deny_unknown_fields)]`.
        for (wire, payload) in cases_map {
            let result: Result<ServerMessage, _> =
                serde_json::from_value(payload);
            assert!(
                result.is_ok(),
                "ServerMessage variant `{}` rejected unknown field \
                 — likely caused by `#[serde(deny_unknown_fields)]` \
                 being added; this violates SI-18 and breaks \
                 rolling upgrades. See design 019 D13-T6. \
                 Error: {:?}",
                wire, result.err(),
            );
            // Confirm the deserialized variant matches its expected
            // wire tag. Catches "case payload has wrong type
            // discriminator".
            let msg = result.unwrap();
            let witnessed = ServerMessageTag::from_message(&msg).as_wire();
            assert_eq!(
                wire, witnessed,
                "case payload tagged `{}` deserialized to wire tag \
                 `{}` — case-tag/payload-type drift",
                wire, witnessed,
            );
        }
    }
    ```

    Four layers of protection — all automated, no manual list maintenance:
    - **Compile-time (witness on ServerMessage)**: adding a new `ServerMessage`
      variant fails `ServerMessageTag::from_message`'s exhaustive match.
      Same-crate placement guarantees this fires even if `ServerMessage` later
      gains `#[non_exhaustive]`.
    - **Compile-time (witness on ServerMessageTag)**: `from_message`'s arms map
      to `ServerMessageTag` variants; adding a `ServerMessage` variant without
      also adding a `ServerMessageTag` variant fails compilation because the
      `from_message` arm's body references an unknown enum variant. `as_wire`'s
      exhaustive match on `Self` (tag enum) provides the same gate from the
      other direction: adding a tag variant without a wire mapping fails
      compilation.
    - **Runtime (every tag has a sentinel + a case)**: `iter()` automatically
      yields every `ServerMessageTag` variant. `sentinel_for_tag` is exhaustive
      on the tag enum, so missing sentinel arms fail compilation (not runtime).
      `cases_map` lookup fails the test if a tag has no case — closing NF-3-v7's
      "case forgotten" gap.
    - **Runtime (SI-18 deserialization)**: each cases() payload must deserialize
      with the unknown `future_field`. Fails if any variant gains
      `#[serde(deny_unknown_fields)]`.

    There is no "known gap" remaining: every list update is either
    compile-forced or runtime-asserted. Adding a `ServerMessage` variant
    cascades through `from_message` (compile), then `ServerMessageTag` (compile,
    via the arm body), then `as_wire` (compile), then `sentinel_for_tag`
    (compile, via the `for tag in iter()` loop using a tag value that has no
    arm), then the runtime `cases_map.contains_key` assertion. Every step has an
    automated gate.

D3 owner-deleted lifecycle tests:

- **D3-T-owner-deleted (hard delete)**: User A creates a periodic task with a
  valid descriptor. Operator hard-deletes A's row from the `users` table
  (`DELETE FROM users WHERE email = ?`). The scheduler fires the trigger.
  Assert: no build is enqueued, the periodic task row is marked disabled,
  `last_error` contains `owner_account_missing` plus the task ID, a `WARN`-level
  log event is emitted, and the scheduler loop continues firing other tasks
  (assert by triggering a second periodic task in the same test that has a valid
  owner and confirming its build is enqueued).
- **D3-T-owner-soft-deleted (forward-protection, feature-gated)**: this test
  exercises the soft-delete clause of the D3 conditional contract. Because the
  production schema does not include a soft-delete column today, the test is
  gated at the **test-function level** by
  `#[cfg(feature = "soft-delete-schema")]`. The default `cargo test` run does
  NOT execute this test; CI runs an additional pass with
  `--features soft-delete-schema` to exercise it.

  Schema setup approach (compatible with `sqlx::migrate!()`): the
  `sqlx::migrate!()` macro embeds the production migrations directory at compile
  time and does NOT support feature-gated forks of the migration set. Instead,
  this test's setup function runs the standard migrations (via
  `sqlx::migrate!()`) and then executes an **inline `ALTER TABLE`** to add the
  soft-delete column for the scope of this test only:

  ```rust
  #[cfg(feature = "soft-delete-schema")]
  async fn setup_soft_delete_fixture(pool: &SqlitePool) {
      sqlx::migrate!("./migrations").run(pool).await.unwrap();
      sqlx::query("ALTER TABLE users \
                   ADD COLUMN deleted_at TIMESTAMP NULL")
          .execute(pool)
          .await
          .unwrap();
  }
  ```

  This avoids forking the migrations directory, keeps `sqlx::migrate!`
  compile-time-clean, and is bounded to the single test that needs the column.
  The test then runs the soft-delete trigger logic against this schema state.

  Implementation expectation under the feature: the trigger's user-lookup
  function detects the column's presence (either at compile time via the same
  `cfg(feature = ...)` annotation, or at runtime via a one-time
  schema-introspection query at startup) and applies the
  `WHERE … AND deleted_at IS NULL` filter accordingly. The test asserts the
  filter is applied; the canonical lookup question is whether the implementation
  uses compile-time or runtime detection, which is a Phase B implementation
  choice documented in the plan.

  Test body: User A creates a periodic task with a valid descriptor. Operator
  sets `deleted_at = NOW()` on A's row. The scheduler fires the trigger. Assert:
  identical outcome to the hard-delete case (no build enqueued, task disabled,
  `last_error` records `owner_account_missing`, scheduler loop continues). This
  pins the conditional contract: **when the schema has a soft-delete marker**,
  the trigger's lookup MUST filter it. The test does NOT pin any schema-shape
  requirement on production; it pins behaviour for the hypothetical future
  migration that would add the column. When that migration lands, the inline
  `ALTER TABLE` is replaced by a proper migration file, and the test moves out
  of the feature-gated section into the default test run.

## References

- Audit v1:
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260512T2339-impl-cbsd-rs-security-audit-v1.md`
- Audit v1.1:
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260514T0841-impl-cbsd-rs-security-audit-v1.1.md`
- Design v1 review (closed in v2):
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260514T1752-design-security-audit-remediation-v1.md`
- Design v2 review (closed in v3):
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260514T2227-design-security-audit-remediation-v2.md`
- Design v3 review (closed in v4):
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260514T2248-design-security-audit-remediation-v3.md`
- Design v4 review (closed in v5):
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260515T1059-design-security-audit-remediation-v4.md`
- Design v5 review (closed in v6):
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260516T0447-design-security-audit-remediation-v5.md`
- Design v6 review (closed in v7):
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260516T0626-design-security-audit-remediation-v6.md`
- Design v7 review (closed in this v8):
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260516T0644-design-security-audit-remediation-v7.md`
- Prior security review: `cbsd-rs/docs/000-20264026T1104-security-review.md`
- The WCP design (seq 019, sibling):
  `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md`
- Roadmap: `cbsd-rs/docs/ROADMAP.md`
- Auth design: `cbsd-rs/docs/cbsd-rs/design/001-20260318T1801-…` (web UI auth
  design 015) — cross-reference for D2 / D4.
- Dev OAuth bypass design 009 — cross-reference for D1.
