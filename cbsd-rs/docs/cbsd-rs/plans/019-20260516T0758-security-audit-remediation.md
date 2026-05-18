# Plan — Security Audit Remediation Implementation

| Field            | Value                                                                                                                                                                                                             |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Plan             | 019 — security-audit-remediation (sibling to `019-…-worker-control-plane-hardening`)                                                                                                                              |
| Design           | `cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md` (Draft v8)                                                                                                                          |
| Date             | 2026-05-16                                                                                                                                                                                                        |
| Status           | Draft v1                                                                                                                                                                                                          |
| Scope            | Commit-level implementation breakdown for design 019. Twelve independent commits + three Phase-E commits gated on the WCP design's implementation plan.                                                           |
| WCP prerequisite | The sibling WCP design (`019-20260426T1154-worker-control-plane-hardening.md`, Draft v11) has **no plan document yet**. Commits 13-15 below are blocked on the WCP plan and the WCP plan's commits landing first. |

## Overview

This plan covers the implementation of design 019 (security audit remediation)
at commit granularity. Each commit is described by the capability it delivers,
the affected packages, the LOC budget, and the notable implementation pitfalls.
**Code samples are not included** — they live in the design document where
relevant. The plan focuses on ordering, dependencies, and reviewer-facing
nuance.

Commit-message format follows the `git-commits` skill (see also the
`/git-commit-messages` skill referenced from `cbsd-rs/CLAUDE.md`):
`component: short description` with a 2-4 sentence body where needed. The
cbsd-rs project convention (per the existing git log) uses
`cbsd-rs/<crate-suffix>:` for crate-scoped work and `cbsd-rs:` for
workspace-spanning commits.

### Phasing recap (from the design)

The design defines five phases (A–E). The plan re-organises slightly to honour
the commit-granularity rules, in particular by relocating D11 into Phase E
(rationale below):

- **Phase A** (worker correctness) — D1, D5, D6 worker side. Phase A in the
  design also lists D11; the plan moves D11 to Phase E because D11 is a delta on
  the WCP supervisor model and cannot land before WCP exists. This is an
  explicit deviation from the design's phasing.
- **Phase B** (server auth / authz) — D2, D3, D4, D9, D10.
- **Phase C** (size limits) — D6 server side (folded into commit 3).
- **Phase D** (small fixes) — D7, D8.
- **Phase E** (WCP-dependent) — D11, D12, D13. Blocked on the WCP design's
  plan + implementation.

### `cbsd-proto` test commit

D13-T6 (the `no_deny_unknown_fields_on_server_message` regression test)
introduces the `strum` dev-dependency and the `ServerMessageTag` companion enum.
It is forward-protective for SI-18 regardless of whether D13's protocol changes
have landed (the test's `build_revoke` payload works against both pre- and
post-D13 wire shapes). The plan treats this as commit 12, ordered after the
other independent commits but before the Phase-E block, so the SI-18 invariant
is enforced from the moment any Phase-E work touches `ServerMessage`.

## Commit breakdown

LOC numbers are estimates, including tests but excluding auto-generated
artefacts (`.sqlx/` cache, `Cargo.lock`).

| #   | Component        | Subject                                                 | LOC  | Phase | Gated   |
| --- | ---------------- | ------------------------------------------------------- | ---- | ----- | ------- |
| 1   | `cbsd-rs`        | enforce strict `CBSD_DEV` parsing and loopback dev-mode | ~360 | A     |         |
| 2   | `cbsd-rs/worker` | enforce tarball containment and decompression cap       | ~380 | A     |         |
| 3   | `cbsd-rs`        | cap REST body and WebSocket message sizes               | ~250 | A+C   |         |
| 4   | `cbsd-rs/server` | reject OAuth callback when `email_verified` is false    | ~130 | B     | ⚠ small |
| 5   | `cbsd-rs/server` | split `periodic:manage` into `:own` and `:any` caps     | ~400 | B     |         |
| 6   | `cbsd-rs/server` | re-validate periodic task scopes at trigger time        | ~400 | B     |         |
| 7   | `cbsd-rs`        | redact bearer tokens from URI logging surface           | ~230 | B     |         |
| 8   | `cbsd-rs`        | wrap token material in `Secret<T>`                      | ~650 | B     |         |
| 9   | `cbsd-rs`        | redact token material from `tracing` call sites         | ~200 | B     | ⚠ small |
| 10  | `cbsd-rs/server` | index `api_keys.key_prefix` for O(log n) lookups        | ~50  | D     | ⚠ tiny  |
| 11  | `cbsd-rs/cbc`    | enforce HTTPS host and write config atomically          | ~250 | D     |         |
| 12  | `cbsd-rs/proto`  | add SI-18 regression test for `ServerMessage`           | ~250 | (B)   |         |
| 13  | `cbsd-rs/worker` | report `Building` during accepted-phase reconnect       | ~250 | E     | 🔒 WCP  |
| 14  | `cbsd-rs/server` | resolve dead workers by DB state and receipt            | ~400 | E     | 🔒 WCP  |
| 15  | `cbsd-rs`        | deliver migration revoke and drain terminal-pending     | ~700 | E     | 🔒 WCP  |

Legend: ⚠ flagged as below the 200-LOC guideline (justification under "Requested
exceptions" below); 🔒 blocked on WCP implementation.

### Independent ordering

Commits 1-12 are independent of WCP and may land in the order shown. There is
one strict order constraint within Phase B:

- **Commit 7 (D4 + D9) before commit 8 (D10 wrap)**: D9 establishes the
  project-wide URI-logging policy that D10's CI gate work will extend later (the
  CI gate is deferred per `cbsd-rs/docs/ROADMAP.md`). Out of order is not a hard
  correctness failure, just a review-clarity concern.
- **Commit 8 (D10 wrap) before commit 9 (D10 audit)**: commit 9 audits call
  sites against the contract commit 8 introduces. Without 8, the audit has
  nothing to enforce.

Other commits have no ordering constraints relative to each other.

## Per-commit details

### Commit 1 — `cbsd-rs: enforce strict CBSD_DEV parsing and loopback dev-mode`

**Closes** D1 (and reverses F1 from the audit).

**Capability**: setting `CBSD_DEV` to `false`, `0`, `no`, or any value that is
not `1` / `true` / `yes` / `on` (case-insensitive) no longer silently enables
the worker's `NoVerifier` rustls bypass. The worker also refuses to start when
dev mode is active AND the configured `server_url` is non-loopback.

**Packages**: a new `cbsd-common` crate is introduced for the shared
`is_truthy_env` helper, used by both `cbsd-server` and `cbsd-worker`. The crate
is the natural home for future shared, IO-free helpers that don't belong in
`cbsd-proto`.

**Notable pitfalls**:

- The `is_loopback_url(url: &url::Url) -> bool` predicate MUST operate on the
  parsed `url::Host`, not on a raw string prefix. A naive
  `starts_with("wss://localhost")` admits `wss://localhost@evil.com/`. Use the
  algorithm shown in D1 (`url::Host::Domain`/`Ipv4`/`Ipv6` arms,
  `addr.is_loopback()` for v4 and v6).
- The startup `WARN` log MUST NOT echo the raw `CBSD_DEV` value (the value could
  be a misconfigured secret); only a boolean `true`/`false`.
- Add `cbsd-common` to the workspace members list in `cbsd-rs/Cargo.toml`.

**Tests**: 8 unit tests for `is_loopback_url` (localhost, `127.0.0.0/8`, `::1`,
authority-confusion, non-loopback rejections); unit tests for `is_truthy_env`
covering `1`/`true`/`TRUE`/`yes`/`on`/ `0`/`false`/`no`/empty/unset/malformed;
integration test that `CBSD_DEV=false` does not install `NoVerifier`;
integration test that `CBSD_DEV=1` + non-loopback URL causes the worker to
refuse to start.

---

### Commit 2 — `cbsd-rs/worker: enforce tarball containment and decompression cap`

**Closes** D5 (reverses F7).

**Capability**: the worker's tar unpack no longer follows symlinks whose target
escapes the unpack root, rejects PAX-overridden paths with `..` components,
rejects device/fifo/hardlink entries that escape, and aborts unpacks that exceed
`MAX_UNCOMPRESSED_BYTES = 256 MiB`.

**Packages**: `cbsd-worker`.

**Notable pitfalls**:

- The two-phase containment check is **defense in depth**; phase 1 (strict
  `path-clean`-style logical normalization) catches every single-pass logical
  escape, and phase 2 (real-path walk against already-unpacked entries) protects
  against TOCTOU and future `tar`-crate semantic changes. The design's "two
  phases" prose acknowledges that phase 2 is not expected to fire on well-formed
  tarballs.
- Use `entry.path()` and `entry.link_name()` from the `tar` crate — both are
  PAX-aware. Do NOT read raw POSIX 100-byte fields.
- The legitimate `components/ceph/containers/v20.3 -> ./v20.2` same-directory
  symlink in the repo MUST keep working. The happy-path fixture should pack the
  real ceph component dir and unpack it.
- Consider adopting the `safer-unpack` crate or porting equivalent containment
  logic from `tar-rs` / `zip-rs` CVE responses instead of maintaining the loop
  in-house. The decision is left to the implementer at commit time; the design's
  behavioural requirements are the contract regardless.

**Tests**: ~10 tarball fixtures — good same-dir symlink (happy path), absolute
symlink target rejected, relative-escape symlink rejected (phase 1),
PAX-overridden path with `..` rejected, chained-symlink attack rejected (phase 2
TOCTOU fault-injection), `path-clean` regression test, device-special entry
rejected, escaping hardlink rejected, gzip-bomb fixture rejected at the byte
cap, boundary test at exactly the cap.

---

### Commit 3 — `cbsd-rs: cap REST body and WebSocket message sizes`

**Closes** D6 (reverses F8).

**Capability**: every authenticated REST endpoint rejects bodies above
`REQUEST_BODY_MAX = 1 MiB` with 413 Payload Too Large. WebSocket connections
(both server-accept and worker-connect paths) cap message size at 8 MiB and
frame size at 1 MiB.

**Packages**: `cbsd-server` (REST router + WS accept), `cbsd-worker` (WS
connect).

**Notable pitfalls**:

- The tarball binary frame MUST fit within `WS_MAX_MSG = 8 MiB`. Real components
  today pack to ~2 KiB; the 8 MiB ceiling is ~4000× headroom (verified in design
  D6 + the ROADMAP analysis).
- No per-log-line cap on `BuildOutput`. The design's trust argument rests on the
  WCP ownership rules; commit 3 lands before WCP, so the D6 trust-boundary
  justification is fully in force only after Phase E also lands (documented in
  the design's Phasing section).
- `tower_http::limit::RequestBodyLimitLayer` is the layer to add.

**Tests**: 4 limit-enforcement tests — REST body over limit returns 413, WS
message over `WS_MAX_MSG` triggers protocol-level close, tarball binary frame
just under `WS_MAX_MSG` is accepted, just over is rejected.

---

### Commit 4 — `cbsd-rs/server: reject OAuth callback when email_verified is false`

**Closes** D2 (reverses F2).

**Capability**: a user who controls a Google account with
`email_verified: false` can no longer log in. The userinfo response is
deserialized into a typed struct (with a serde alias for the legacy
`verified_email` field), and the check runs **before** the allowed-domain check
so an attacker cannot probe the domain allow-list with unverified accounts.

**Packages**: `cbsd-server`.

**Notable pitfalls**:

- The error returned to the user MUST be generic ("authentication failed;
  contact your administrator") — do NOT leak whether the domain is allowed or
  whether the email passed verification.
- Add a server-side log with the email + provider response shape + the failure
  reason for operator debugging.
- The userinfo endpoint is a separate REST call after OAuth token exchange and
  is NOT bound to the OAuth signature. D2 explicitly documents this as a
  residual trust gap to be closed by a future ID-token-introspection pass; the
  commit message body should not re-litigate this.

**Tests**: 4 mocked-userinfo tests — `email_verified: false` →
401/user-not-created; `email_verified: true` + allowed domain → 200; missing
field → 401; legacy `verified_email` alias → accepted.

---

### Commit 5 — `cbsd-rs/server: split periodic:manage into :own and :any caps`

**Closes** D3 part A (RBAC split + endpoint ownership + migration).

**Capability**: ordinary users with `periodic:manage:own` can mutate only their
own periodic tasks. `periodic:manage:any` is the admin-level capability for
cross-owner management. Cross-owner mutation by `:own` holders is rejected
with 403.

**Packages**: `cbsd-server` (route handlers + permission constants + seed
migration).

**Notable pitfalls**:

- The migration drops the legacy `periodic:manage` capability from every
  existing role's capability set. **No automatic mapping** to `:own` or `:any`
  is performed. Operators with custom roles must re-grant explicitly. The
  migration's SQL comment block MUST call this out loudly.
- The four mutating endpoints (`update_task`, `delete_task`, `enable_task`,
  `disable_task`) each need the `:any` OR `:own` + row-owner-match check.
- Descriptor updates additionally re-validate scopes against the **updating
  user's** effective scopes (not the row owner's). This blocks scope-smuggling
  at update time. The trigger-time check that enforces the same constraint at
  scheduler-fire time is commit 6.

**Tests**: 6 RBAC tests — :own + own task → success; :own + other's task → 403;
:any + other's task → success; :any with descriptor scope they lack → 403;
:own + own task with descriptor scope they lack → 403; migration test that seeds
a custom role with `periodic:manage` and asserts post-migration the legacy cap
is removed.

---

### Commit 6 — `cbsd-rs/server: re-validate periodic task scopes at trigger time`

**Closes** D3 part B (trigger-time scope check) + SI-15.

**Capability**: scheduled triggers re-validate the stored descriptor against the
task owner's **current** effective capabilities before submitting the build. If
the owner has lost any capability the descriptor relies on (role change, role
removal, account deleted), the task is fatally disabled with
`last_error = owner_account_missing`.

**Packages**: `cbsd-server` (scheduler `trigger.rs` + user-lookup helper).

**Notable pitfalls**:

- The canonical user-lookup is schema-dependent (D3 conditional contract).
  Today's hard-delete schema → `WHERE email = ?`; the future soft-delete schema
  → `WHERE email = ? AND deleted_at IS NULL`. **Do not** write the soft-delete
  filter against today's schema — the column does not exist; it would be a
  runtime SQL error.
- "No longer a valid identity" includes both row absence (hard delete) and the
  soft-delete marker once the column lands. The trigger MUST NOT panic, MUST NOT
  raise to the scheduler loop, and MUST NOT fall back to a cached scope set.
- The `D3-T-owner-soft-deleted` test is feature-gated behind
  `cfg(feature = "soft-delete-schema")`. The test fixture's setup runs an inline
  `ALTER TABLE users ADD COLUMN deleted_at TIMESTAMP NULL` after
  `sqlx::migrate!()`. Do NOT fork the migrations directory.

**Tests**: trigger-time scope-reduction test (role demoted → task disabled);
`D3-T-owner-deleted` (hard delete); feature-gated `D3-T-owner-soft-deleted`;
scheduler-loop-continuity test that a second valid task fires alongside a
disabled one.

---

### Commit 7 — `cbsd-rs: redact bearer tokens from URI logging surface`

**Closes** D4 + D9 (reverses F5).

**Capability**: the CLI login flow no longer leaks the PASETO token to server
access logs. The server's project-wide URI-logging policy prevents any future
endpoint from leaking secrets through the path / query field.

**Packages**: `cbsd-server` (auth.rs redirect, TraceLayer config, panic-handler
hygiene) + `cbsd-rs/ui` (`index.html` fragment extraction +
`history.replaceState`) + `cbsd-rs/CLAUDE.md` (correctness invariants section).

**Notable pitfalls**:

- The redirect change is a one-character fix: `?cli-token=…` → `#cli-token=…`.
  The browser does not send fragments to the server.
- `ui/index.html` MUST call `window.history.replaceState({}, '', '/')`
  **immediately** after reading the token from `window.location.hash`,
  **before** any other script has a chance to read `document.URL`. Order
  matters.
- The TraceLayer span builder logs `method`/`path`/`status` at INFO; never
  `query` or full `Uri::to_string()`. The policy applies to every middleware in
  the stack, including panic handlers and any future error-reporting integration
  (CLAUDE.md note records this).

**Tests**: integration test asserting `Location: /#cli-token=…` (not
`?cli-token=…`); negative log-capture test that `GET /?cli-token=abc123` does
not produce `abc123` in captured logs; 2 browser-level tests (Playwright or
equivalent) — hash cleared after extraction, no token in browser history.

---

### Commit 8 — `cbsd-rs: wrap token material in Secret<T>`

**Closes** D10 part A (wrapping). Reverses F13's wrap-by-construction clause for
token material.

**Capability**: every in-memory token (PASETO raw tokens, API keys, robot
tokens, worker tokens) is wrapped in `secrecy::Secret<T>`. A struct that derives
`Serialize` and accidentally includes a `Secret<T>` field fails to compile,
forcing callers to use `.expose_secret()` explicitly at the wire boundary.

**Packages**: `cbsd-proto` (`WorkerToken.api_key` → `Secret<String>`),
`cbsd-server` (PASETO creation/storage, OAuth callback flow, robot token paths),
`cbsd-worker` (stored API key), `cbsd-rs/cbc` (persisted bearer in `Config`).

**Notable pitfalls**:

- Add `secrecy = "0.10"` (or current stable) to workspace dependencies.
  Per-crate inheritance via `secrecy.workspace = true`.
- `secrecy::ExposeSecret` is a **trait**, not an inherent method. Every
  `.expose_secret()` call site MUST `use secrecy::ExposeSecret;` to bring the
  trait into scope.
- Wire types that today derive `Serialize` and include token fields will fail to
  compile. Either replace the derive with a custom `Serialize` that calls
  `.expose_secret()` at the boundary, or separate the wire-format DTO from the
  in-memory secret holder. Both are acceptable per D10.
- The CI grep gate is **deferred** to a roadmap item; this commit ships the
  `Secret<T>` wrapper and its users, not the gate.

**Tests**: 1 `tracing-test` redaction test
(`tracing::debug!(token = %my_secret)` → `<redacted>`); 1 `trybuild`
compile-fail test for `#[derive(Serialize)]` over a struct with a
`Secret<String>` field; 1 `trybuild` compile-fail test for inner-field access
without `.expose_secret()`.

---

### Commit 9 — `cbsd-rs: redact token material from tracing call sites`

**Closes** D10 part B (audit).

**Capability**: every existing `tracing::*!` macro call across the workspace
that previously emitted token material (bearer literal, authorization header,
PASETO raw bytes, API key prefix) is updated to either route the value through a
`Secret<T>` (covered in commit 8) or to emit a non-reversible per-process
diagnostic identifier instead of the key bytes.

**Packages**: `cbsd-server`, `cbsd-worker`, `cbsd-rs/cbc`, `cbsd-proto`
(grep-and-fix across the workspace).

**Notable pitfalls**:

- The API key prefix logging at debug (audit finding F13's original site) is
  replaced with a stable per-process diagnostic identifier derived from the key
  hash (not the key bytes). This identifier must not be reversible by an
  attacker with log access.
- `signed_off_by` and similar non-secret identity fields are explicitly NOT in
  scope.
- This commit is small (~200 LOC) and looks like a clean-up pass. It is
  meaningful by itself because it closes the audit-defined policy contract:
  SI-10 ("No token material appears in any log line").

**Tests**: targeted `tracing-test` assertions per fixed site, ensuring the
redacted form is produced; manual grep audit during review.

---

### Commit 10 — `cbsd-rs/server: index api_keys.key_prefix for O(log n) lookups`

**Closes** D7 (reverses F10).

**Capability**: API-key lookups by prefix are O(log n) rather than O(n), and the
timing-parity side-channel that the missing index exposed is closed.

**Packages**: `cbsd-server` (`migrations/` + `.sqlx/` offline cache
regeneration).

**Notable pitfalls**:

- The index is **non-unique** — two different full keys could share a prefix;
  the prefix is a UX/lookup helper, not a unique key.
- After the migration is added, run
  `DATABASE_URL=sqlite:///tmp/cbsd- dev.db cargo sqlx prepare --workspace` and
  commit the resulting `.sqlx/` changes. The skill counts `.sqlx/` as
  auto-generated and excludes it from the LOC budget.
- Document the query-plan check in the migration's comment block so future
  reviewers can confirm the index is used.

**Tests**: migration apply test (forward + idempotent rerun); the query-plan
check is a manual review step documented in the migration comment.

---

### Commit 11 — `cbsd-rs/cbc: enforce HTTPS host and write config atomically`

**Closes** D8 (reverses F11).

**Capability**: `cbc` rejects `host` URLs whose scheme is not `https`. The new
`--insecure-http` flag explicitly opts into plain HTTP and emits a per-command
warning. `Config::save` writes the config file atomically with mode `0o600` via
temp-file + rename, closing the TOCTOU window where the bearer token was briefly
world-readable.

**Packages**: `cbsd-rs/cbc` (client.rs URL parsing, main.rs flag, config.rs
save).

**Notable pitfalls**:

- `--insecure-http` is **independent** of `--no-tls-verify` / `-k`. The two
  flags address different problems and must not be conflated.
- `parse_base_url` rejects every scheme that is not `https` (or `http` only when
  `--insecure-http` is set). The check operates on the parsed `Url::scheme()`,
  not on string prefix.
- `Config::save` writes to a sibling temp file with
  `OpenOptions::new().write(true).create_new(true).mode(0o600)` on Unix, then
  `fs::rename`s over the target. On any error, attempt best-effort
  `fs::remove_file` on the temp file.
- Windows non-atomic rename caveat: documented in D8. `std::fs:: rename` is not
  atomic on Windows before Rust 1.86; today's cbc is Linux-primary, so accept
  the platform limitation for now.

**Tests**: scheme-rejection unit tests (https accepted; http/ftp rejected);
`--insecure-http` permits http with the documented warning; `Config::save`
mode-0o600 race test (a reader thread stat-ing the path observes 0o600 once the
file is visible by name).

---

### Commit 12 — `cbsd-rs/proto: add SI-18 regression test for ServerMessage`

**Closes** D13-T6 (forward-protection for SI-18, ahead of the rest of D13).

**Capability**: `cbsd-proto` has a regression test that catches any addition of
`#[serde(deny_unknown_fields)]` on `ServerMessage` or any of its variants.
Adding a new `ServerMessage` variant cascades through four automated gates
(witness, tag-enum, as_wire, sentinel_for_tag) before reaching the runtime
case-coverage gate.

**Packages**: `cbsd-proto` (`Cargo.toml` `[dev-dependencies]` section +
`src/ws.rs::tests` additions).

**Notable pitfalls**:

- Add `strum = { version = "0.26", features = ["derive"] }` to a new
  `[dev-dependencies]` section of `cbsd-rs/cbsd-proto/Cargo.toml`. The crate
  currently has no `[dev-dependencies]` section.
- The companion enum `ServerMessageTag` lives **inside the existing
  `#[cfg(test)] mod tests` block** in `ws.rs`. Same-crate placement is
  load-bearing: it preserves exhaustive-match semantics if `ServerMessage` ever
  gains `#[non_exhaustive]`.
- The existing `mod tests` block already imports `use super::*;` plus `Arch` and
  a set of build types. The D13-T6 sketch's preamble adds ONLY
  `use serde_json::{Value, json};` and `use strum::IntoEnumIterator;`. Do NOT
  re-import types already in scope — `-D warnings` will reject duplicates.
- `Hash` derive on `ServerMessageTag` is not needed at this revision (the v8
  design review NF-2-v8 flagged it). Drop `Hash` from the derive list when
  implementing.
- The "build_revoke" case payload omits `reason` — works against both pre- and
  post-D13 wire shapes.

**Tests**: this commit IS the test. No further tests required.

---

### Commit 13 — `cbsd-rs/worker: report Building during accepted-phase reconnect` 🔒

**Closes** D11 (WCP v10 review open item #1).

**Capability** (post-WCP): when the worker has accepted a build
(`build_accepted` sent) but the subprocess has not yet spawned, a websocket
drop + reconnect now reports `Building { build_id }` rather than `Idle`. The
server treats the report as authoritative receipt and does not roll the build
back to `queued`.

**Packages**: `cbsd-worker` (supervisor reconnect rule).

**Dependency**: WCP design must have a plan document at
`cbsd-rs/docs/cbsd-rs/plans/019-…-worker-control-plane-hardening.md` and **all**
WCP plan commits must have landed. The WCP supervisor model is the anchor that
D11 extends; without it, this commit has nothing to modify.

**Notable pitfalls**:

- The supervisor's reconnect status rule reports `Building` whenever the
  supervisor has any non-terminal local assignment state (executor, in-progress
  revoke, pending terminal result, **or** `accepted` phase).
- For the spawn-race case (supervisor cannot determine whether a child process
  exists), follow the WCP v11 rule: stop and await any possible child, clean up
  local state, only then report `Idle`.

**Tests**: 2 reconnect tests — accept + drop + reconnect ⇒ `Building`; spawn
race (kill child, then reconnect ⇒ correct state).

---

### Commit 14 — `cbsd-rs/server: resolve dead workers by DB state and receipt` 🔒

**Closes** D12 (WCP v10 review open item #2).

**Capability** (post-WCP): when the liveness monitor declares a worker dead, the
resolution is table-driven over the (DB state × `ActiveAssignmentReceipt`)
combinations: `dispatched + AwaitingReceipt` → rollback to `queued`;
`dispatched + ReceivedByWorker` → `failure` (no requeue); `started → failure`;
`revoking → revoked`.

**Packages**: `cbsd-server` (`ws/liveness.rs`, `ws/handler.rs`
`handle_worker_dead`).

**Dependency**: WCP plan + implementation (the `ActiveAssignmentReceipt` field,
the `queue.active` shape, and the liveness monitor scheduling are all WCP
territory).

**Notable pitfalls**:

- `ReceivedByWorker + dispatched + dead` → `failure`, **not** requeue. The build
  may have produced upstream side effects (S3, Harbor); requeuing would
  duplicate them.
- `AwaitingReceipt + dispatched + dead` uses the existing WCP rollback-cleanup
  operation (D4 in the WCP design), which clears `worker_id`/`trace_id`/etc.
- Receipt state is in-memory only; server-restart recovery uses the existing
  fail-in-flight policy and does NOT reconstruct `ActiveAssignmentReceipt`.

**Tests**: 5 dead-worker resolution tests — one per row of the table plus a
server-restart interaction test.

---

### Commit 15 — `cbsd-rs: deliver migration revoke and drain terminal-pending` 🔒

**Closes** D13 (WCP v10 review open item #3) + serde compat extension to
`BuildRevoke`.

**Capability** (post-WCP): same-worker reconnect migration sends
`BuildRevoke { reason: Some(MigrationSupersede) }` on the old sender for every
owned active assignment **before** removing the old sender. The worker
supervisor confirms migration via a `last_authenticated_connect_at` predicate
within `MIGRATION_RECENT_WINDOW = 30s`. If `terminal-pending-report` is the
local state at migration time, the supervisor **drains** the real outcome and
reports `build_finished(<actual_outcome>)` instead of discarding — preserving
the build's real success / failure result.

**Packages**: `cbsd-proto` (`BuildRevokeReason` enum, `BuildRevoke` gains
`reason: Option<BuildRevokeReason>` with
`#[serde(default, skip_serializing_if = "Option::is_none")]`), `cbsd-server`
(migration revoke send), `cbsd-worker` (supervisor predicate + drain-then-revoke
semantics + `tokio::time::Instant` clock injection).

**Dependency**: WCP plan + implementation (same-worker migration flow,
supervisor state, websocket sender removal protocol).

**Notable pitfalls**:

- The `reason` field MUST be `Option<BuildRevokeReason>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. The `Option<>` +
  `default` makes the addition forward- and backward-compatible. An old server
  emitting no `reason` field deserializes to `None` on a new worker (treated as
  `Admin` semantics). A new server emitting `Some(MigrationSupersede)` is
  silently ignored by an old worker WITHOUT `deny_unknown_fields` — which SI-18
  enforces.
- Worker MUST NOT use `std::time::Instant` for `last_authenticated_connect_at`.
  Use `tokio::time::Instant` so test code with `tokio::time::pause()` can drive
  the predicate.
- Anti-coercion: the worker checks `migration_plausible()` (recent reconnect
  within window) before honouring `MigrationSupersede`. A malicious server
  emitting `MigrationSupersede` without a real migration falls through to
  `Admin` semantics + WARN log.
- The drain-then-revoke deviation from WCP v11's generic revoke rule is scoped
  to `reason = MigrationSupersede` only. Admin revokes against
  `terminal-pending-report` continue to discard.
- `Instant` does NOT advance during host suspension on Linux (`CLOCK_MONOTONIC`
  semantics). Documented in SI-17; operator guidance: don't run `cbsd-worker` on
  a suspending host.

**Tests**: 7 same-worker migration tests (4 SM-W phases + 3 edge cases); 5 serde
compatibility tests on `BuildRevoke.reason` (absent-field deserialize, None
round-trip, Some round-trip, unknown-variant rejection, advisory-fallback);
D13-T7 timing tests using `tokio::time::pause()` + `advance()` for deterministic
boundary checks.

This is the largest commit in the plan (~700 LOC). It is at the upper end of the
400-800 budget but is internally coherent (one feature: migration revoke
semantics with anti-coercion). Splitting would create commits that don't compile
(the worker-side predicate references the `BuildRevokeReason` field).

## Phase E — blocked on WCP

Commits 13, 14, and 15 cannot land until:

1. The WCP design (`019-20260426T1154-worker-control-plane- hardening.md`) has a
   plan document at
   `cbsd-rs/docs/cbsd-rs/plans/019-…-worker-control-plane-hardening.md`.
2. All WCP plan commits have landed in the workspace.

This plan does **not** define the WCP commit breakdown. Writing the WCP plan is
a separate effort. The WCP plan author should cross-reference this plan's Phase
E section so the dependency is captured from both sides.

## Test strategy

Tests land with their implementing commit. The design's test count (per the
D-decision in the design's "Test Expectations Summary" section) aligns with the
per-commit tests listed above. No test-only commits other than commit 12 (D13-T6
is a forward- protective regression test that justifies its own commit because
of the `strum` dev-dependency addition).

Per the cbsd-rs CLAUDE.md pre-commit policy, every commit MUST pass
`cargo fmt --all`, `cargo clippy --workspace`, and `cargo check --workspace`
(with `SQLX_OFFLINE=true` if needed) before staging.

## Open questions / requested exceptions

The git-commits skill requires that exceptions to the 400-800 LOC guideline are
justified and surfaced to the user. The plan asks the user to confirm the
following:

1. **Commit 4 (D2 OAuth) at ~130 LOC** — below the 200-LOC floor.
   - Justification: the change is a focused, high-severity security fix that
     closes audit finding F2. Combining with another commit would dilute review
     focus. The capability ("anonymous attacker cannot log in via unverified
     Google account") is clear and standalone.
   - Recommended action: accept as-is.

2. **Commit 9 (D10 audit) at ~200 LOC** — at the floor.
   - Justification: targeted audit/cleanup of existing tracing sites to enforce
     SI-10. Not architecturally splittable. The capability ("no log line
     contains token material") is the codified policy.
   - Recommended action: accept as-is.

3. **Commit 10 (D7 DB index) at ~50 LOC** — well below the floor.
   - Justification: a single SQL migration adding a non-unique index plus the
     `.sqlx/` cache regeneration. Combining with D8 (commit 11) is possible but
     the topics are unrelated (DB index vs CLI client transport).
   - Recommended action: accept standalone. Alternative: combine with commit 11
     under a "operational hygiene" umbrella — please indicate preference.

4. **Commit 15 (D13) at ~700 LOC** — at the upper end of 400-800.
   - Justification: protocol change + server-side revoke send + worker-side
     predicate + supervisor state machine all form one feature. Splitting would
     create non-compiling intermediates (e.g., adding `BuildRevokeReason` to the
     enum without any users).
   - Recommended action: accept as-is. Alternative: pre-split into "protocol +
     server send" and "worker predicate + drain" — please indicate preference.

5. **D11 placement — Phase A in design, Phase E in plan**.
   - Justification: D11 is a delta on the WCP supervisor model. Without the WCP
     supervisor existing in the codebase, D11 has nothing to modify. Treating
     D11 as Phase E (WCP-gated) is more accurate than the design's "Phase A:
     D11" claim.
   - Recommended action: accept the plan's reclassification. Alternative: have
     the design re-issue a vN+1 to relocate D11 to Phase E — please indicate
     preference.

6. **`cbsd-common` crate introduction in commit 1**.
   - The design's D1 says the helper lives in `cbsd_common::env::is_truthy_env`.
     There is no `cbsd-common` crate today; commit 1 would create it.
     Alternatives: (a) keep the helper in `cbsd-proto` (which is shared); or (b)
     inline the helper in each binary (`cbsd-server` and `cbsd-worker`).
   - Recommended action: create the `cbsd-common` crate as the design specifies
     — it is a natural home for future shared IO-free helpers. Alternative paths
     above; please indicate preference if you'd prefer (a) or (b).

7. **`secrecy` crate version pin**.
   - The design's D10 doesn't specify a version. The current stable `secrecy` is
     `0.10`. Recommended action: add `secrecy = "0.10"` to workspace deps. If
     you have a version-policy preference (e.g., always pin to MSRV-
     compatible), please indicate.

## References

- Design:
  `cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md`
  (Draft v8)
- Sibling design:
  `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md`
  (Draft v11, no plan yet)
- v8 review:
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260516T0715-design-security-audit-remediation-v8.md`
- Audit history:
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260512T2339-impl-cbsd-rs-security-audit-v1.md`,
  `…-v1.1.md`
- Original security review: `cbsd-rs/docs/000-20264026T1104-security-review.md`
- Roadmap: `cbsd-rs/docs/ROADMAP.md`
