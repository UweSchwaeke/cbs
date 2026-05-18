# Design Review: Security Audit Remediation

| Field          | Value                                                                                                                                                                            |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Review         | 019 — security-audit-remediation design v1                                                                                                                                       |
| Date (UTC)     | 2026-05-14 17:52                                                                                                                                                                 |
| Design         | `cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md`                                                                                                    |
| Sibling ref    | `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md` (WCP design v11)                                                                               |
| Scope          | All 13 decisions (D1–D13), state machines (SM-W, SM-S, SM-R), cross-cutting concerns, implementation phasing (A–E), and coverage claims for F1–F13 plus WCP v10 review items 1–3 |
| Reviewer       | Independent — no trust in implementer claims                                                                                                                                     |
| Recommendation | **Approve with conditions** — proceed to planning after addressing the two Critical items below                                                                                  |

## Summary

The design is a well-structured consolidation of the security audit remediation
work. It correctly identifies the scope boundary between this design and the WCP
sibling, the individual decisions are generally precise, and the test
expectations are specific enough to write tests against. Three findings require
attention before an implementation plan is written.

The most serious finding is **D13's `terminal-pending-report` discard**: a
subprocess that completed successfully has its result silently converted to
`revoked` in the DB, while side effects (S3, Harbor artifacts) remain committed.
The design calls this a "deliberate trade-off" but does not justify why
drain-then-accept would be worse. This is silent data loss under a realistic
operational scenario and should be resolved explicitly.

The second Critical finding is **D10's `Secret<T>` construction-time
guarantee**: the design promises redaction "by construction" but only specifies
`Debug` and `Display` impls. `serde::Serialize` is left unspecified. If
`Secret<T>` implements `Serialize` transparently, a JSON serialization path
exposes the inner value without any log involvement — and if it does not
implement `Serialize`, any code that tries to include a `Secret<T>` field in a
wire type will fail to compile with a confusing error. The design needs an
explicit stance.

The remaining High finding — **D5's PAX-extended-header gap and
`path-clean`-style vagueness** — is not a blocker if the implementation is
careful, but the design text as written gives an implementer insufficient
guidance to avoid a containment bypass.

Top findings ordered by severity:

1. **D13 `terminal-pending-report` discard** (Critical) — silent data loss: a
   successful build's outcome is discarded; S3/Harbor side effects already
   committed are orphaned.
2. **D10 `Secret<T>` serde gap** (Critical) — the "by construction" redaction
   claim does not cover `serde::Serialize`; a single `to_string(&secret)` call
   can leak the full secret.
3. **D5 PAX extended-header and path-normalization under-specification** (High)
   — the containment check is not applied to PAX-overridden entry paths;
   `path-clean` logical normalization is insufficient against chained symlinks.
4. **D1 loopback whitelist under-specification** (Significant) — the whitelist
   text does not cover IPv6 loopback or guard against URL-authority confusion.
5. **SM-S diagram missing transitions** (Significant) — `dispatched → queued`
   rollback and `dispatched → failure` (D12) are described in prose but absent
   from the state diagram.

## Findings

### F-R1 — D13 `terminal-pending-report` silent data loss

| Attribute | Value                                                                  |
| --------- | ---------------------------------------------------------------------- |
| Severity  | Critical                                                               |
| Location  | D13 "Worker-side handling during migration", `terminal-pending-report` |
|           | row; SM-W → D13 worker-side table row 4; State Machine section         |

**Problem.** When SM-W is in `terminal-pending-report` at the moment of
same-worker migration, D13 directs the supervisor to discard the pending
terminal result and report `build_finished(revoked)` on the new connection. The
stated rationale is "asserting the new connection's authority." The DB ends up
with `state = revoked` for a build whose subprocess ran to completion and whose
artifacts (tarballs, image tags, release metadata) are already committed to S3
and Harbor.

This is silent data loss. An operator watching the build dashboard sees
"revoked"; the build's artifacts exist in the artifact store under a build ID
that is now marked revoked. Downstream systems that gate on build state (release
pipelines, promotion workflows) will skip or reject these artifacts. No warning
is issued that a successful result was discarded; the "deliberate trade-off"
comment in the design is the only documentation.

A realistic trigger: the worker completes the subprocess while the network blip
that caused the reconnect is still ongoing. The supervisor holds the
`terminal-pending-report` state, the new connection arrives during that same
window, and D13 fires.

**Why the stated rationale is weak.** "Asserting the new connection's authority"
is not a correctness requirement — it is a simplicity preference. The
authoritative correct outcome is already known (the subprocess exited with a
definite success/failure code). The alternative is to drain the terminal result
on the new connection before applying the migration revoke for this build.
Specifically: if SM-W = `terminal-pending-report`, skip sending `BuildRevoke`
for that build ID (or defer it), let the supervisor report
`build_finished(actual_outcome)` on the new connection, and only if that report
never arrives within a grace window does D12's liveness path apply. The new
connection is still authoritative; no correctness property of the migration is
violated.

**Impact.** Any network blip near build completion converts a successful build
to `revoked` in the database, with no user-visible warning and permanent
artifact-store inconsistency.

**Recommendation.** Introduce a `terminal-pending-report` exception in D13: when
the supervisor receives `BuildRevoke` but SM-W is `terminal-pending-report`, do
not discard the result. Report `build_finished(actual_outcome)` on the new
connection. If the actual outcome is `success`, the build is recorded as
`success`; D13's "reporter-directed revoke" goal is achieved for the other three
SM-W phases, which are the cases where the outcome is genuinely uncertain.
Update the SM-W → D13 worker-side table row 4 and the state machine section
accordingly.

---

### F-R2 — D10 `Secret<T>` `serde::Serialize` gap

| Attribute | Value                                                        |
| --------- | ------------------------------------------------------------ |
| Severity  | Critical                                                     |
| Location  | D10 "Add a `cbsd_common::secrets::Secret<T>` newtype"; State |
|           | Invariant 10; D10 tests                                      |

**Problem.** D10 specifies that `Secret<T>` has a redacting `Debug`/`Display`
impl, guaranteeing that `tracing::debug!(?secret, ...)` prints `<redacted>`. The
design is silent on `serde::Serialize`.

There are two failure modes:

1. **`Secret<T>` implements `Serialize` by forwarding to `T::serialize`**: a
   call to `serde_json::to_string(&token)` or embedding a `Secret<String>` in a
   response struct produces the raw token in JSON. This can happen in test
   output, error-response bodies, or any debug serialization helper without
   triggering a tracing call.
2. **`Secret<T>` does not implement `Serialize`**: any wire type or struct that
   accidentally includes a `Secret<T>` field will fail to compile with an opaque
   type error. Worse, a developer will be tempted to unwrap the secret to make
   it compile, defeating the entire abstraction.

The `secrecy` crate (the canonical Rust pattern for this) deliberately does NOT
implement `Serialize` for `Secret<T>`. Code that needs to transmit the inner
value over the wire calls `.expose_secret()` explicitly, making the leak visible
at the call site. The design neither references this pattern nor specifies the
stance on `Serialize`.

The PASETO case is directly relevant: `paseto::token_create` or equivalent will
need the raw token bytes. Without an explicit `.expose_secret()` accessor, all
PASETO creation paths either cannot use `Secret<T>` or must call `to_string`
through the secret, leaking through the Display impl's non-redacted path if
`Display` is accidentally passed to a formatter rather than the `Secret`
wrapper.

**Impact.** The "by construction" claim in D10 and State Invariant 10 is false
without an explicit Serialize policy. A single audit failure (forgetting to use
`tracing::debug!(?secret)` and instead using `tracing::debug!(%secret)` on a
field that is `serde_json::to_string`-d elsewhere) produces a regression
indistinguishable from the current codebase.

**Recommendation.** Add an explicit specification in D10:

- `Secret<T>` does NOT implement `Serialize` or `Deserialize`.
- The inner value is accessible only via a named accessor (e.g.,
  `.expose_secret()`) that is grep-able and auditable.
- Anywhere a raw token value must be passed to a function (PASETO creation, hash
  computation, Argon2 verify), use `.expose_secret()` explicitly.
- The D10 test should include a `cargo compile` negative test asserting that a
  struct containing `Secret<String>` cannot be serialized with
  `#[derive(Serialize)]` without an explicit annotation.

Reference the `secrecy` crate; adopting it directly removes the need to maintain
the newtype and its edge cases in-house.

---

### F-R3 — D5 PAX extended-header gap and path-normalization vagueness

| Attribute | Value                                                                   |
| --------- | ----------------------------------------------------------------------- |
| Severity  | High                                                                    |
| Location  | D5 "Replace the bare `Archive::unpack` call with a custom unpack loop"; |
|           | D5 tests                                                                |

**Problem — PAX extended headers.** The `tar` crate's `entry.path()` and
`entry.link_name()` already apply PAX extended-header overrides (GNU and POSIX
pax headers can override the POSIX-limited 100-byte filename with an arbitrary
UTF-8 path). The design's containment check operates on the resolved
`entry.path()` / `entry.link_name()` values, which is correct for UTF-8
filenames; however, the design does not acknowledge this surface or test it. A
maliciously crafted PAX header can override a harmless-looking POSIX name with a
path containing `../`, `//etc`, or a long absolute component that a naive
`path-clean` implementation might not handle. The `tar` crate does not filter
PAX-overridden names for `..` by default; the design's "defense in depth; `tar`
crate already does this" note applies to POSIX 100-byte names only, not PAX.

**Problem — chained symlinks.** The design specifies: "Canonicalize
`unpack_root.join(entry_dir).join(link_target)` (without following the link) and
reject if the resolved path escapes `unpack_root`. Use `path-clean`-style
logical normalization." Logical normalization collapses `..` lexically but does
not account for symlinks that have already been unpacked into the archive.
Consider:

```
entry 1: dir/subdir → ../escape_dir        (symlink, escapes after normalization)
entry 2: dir/subdir/file                   (regular file)
```

Logical normalization of `dir/subdir` checks the link target `../escape_dir`
against `unpack_root` at link-entry time and sees an escape — so far so good.
But now consider:

```
entry 1: inner → .                         (symlink to current dir — passes check)
entry 2: inner/../../escape                (uses already-written symlink to escape)
```

The containment check for entry 2 resolves `inner/../../escape` logically (not
following the symlink on disk) to `escape` — which passes. The actual filesystem
write goes through `inner → .` and escapes. This is the classic symlink-chain
escape that requires a per-entry full-resolution check against already-unpacked
entries, not just logical path normalization.

**Impact.** A maliciously crafted component tarball (from a compromised server)
can escape the unpack root and overwrite arbitrary files on the worker host.
While the design's threat model notes the server is trusted, the whole point of
D5 is defense-in-depth against a compromised server.

**Recommendation.**

- State explicitly that `entry.path()` and `entry.link_name()` are used as
  returned by the `tar` crate (PAX-aware); add a test with a PAX-overridden path
  containing `../`.
- Replace the `path-clean` logical normalization with a two-phase check: (a)
  logical check to fast-fail obvious escapes, and (b) a real-path check that
  verifies no already-unpacked directory entry is a symlink pointing outside
  `unpack_root`. The `tar` crate's own test suite has examples of this. The
  `tar-rs` `set_unpack_xattr` feature and manual iteration examples in its docs
  provide guidance.
- Alternatively, adopt the `safer-unpack` crate or explicitly port the
  containment logic from `zip`/`tar` crate CVE responses.
- Add a test: a tarball where entry 1 is a symlink `inner → .` and entry 2 is a
  regular file `inner/../../escape` — unpack must fail.

---

### F-R4 — D1 loopback whitelist under-specified

| Attribute | Value                                                         |
| --------- | ------------------------------------------------------------- |
| Severity  | Significant                                                   |
| Location  | D1 "The worker `dev_tls_config()` (`NoVerifier`) is reachable |
|           | only when dev mode is active …"                               |

**Problem.** D1 specifies: "the server URL is `wss://localhost`,
`wss://127.0.0.1`, or matches a documented loopback whitelist." This is not
concrete enough for implementation. Missing cases:

- `wss://[::1]` (IPv6 loopback) — a worker configured with an IPv6 loopback
  address falls outside the whitelist as written.
- URL authority confusion: `wss://localhost@evil.com/` — a naive
  `starts_with("wss://localhost")` string check passes. The whitelist must
  operate on the parsed `Host` value (after `url::Url` resolution), not on the
  raw string.
- `wss://127.0.0.1:port/path` — the port and path must be allowed; only the host
  component must be loopback.
- The entire `127.0.0.0/8` range is loopback on Linux. A worker configured with
  `wss://127.0.0.2` is also loopback but not in the listed set.

**Impact.** An implementer who writes `url.host() == "localhost"` will silently
reject valid dev configurations (IPv6 loopback, alternate 127.x addresses) or
admit non-loopback hosts via authority confusion.

**Recommendation.** Replace the prose list with a concrete algorithm:

```
fn is_loopback_url(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(addr)) => addr.is_loopback(),     // 127.0.0.0/8
        Some(Host::Ipv6(addr)) => addr.is_loopback(),     // ::1
        _ => false,
    }
}
```

Reference the `url::Host` enum explicitly in D1 to remove ambiguity.

---

### F-R5 — SM-S state diagram missing transitions

| Attribute | Value                                       |
| --------- | ------------------------------------------- |
| Severity  | Significant                                 |
| Location  | "State Machines (D11 / D12 / D13)" section, |
|           | SM-S diagram                                |

**Problem.** The SM-S diagram omits several transitions that are specified in
the prose decisions:

- `dispatched → queued` (rollback for dispatch failure, transient reject,
  `AwaitingReceipt` liveness expiry). The caption mentions rollback paths but
  the arrow is absent.
- `dispatched → failure` (D12 liveness with `ReceivedByWorker`). This transition
  is the primary contribution of D12 to SM-S but it is not drawn.
- `dispatched → revoked` (admin revoke against a `dispatched` build — an
  operational path that presumably exists in the codebase today).
- `started → revoked` (build_finished(revoked) without going through `revoking`
  — can the worker report directly? The WCP design should clarify, but the SM-S
  diagram should show the path if it exists).

A reviewer or implementer consulting the SM-S diagram to check whether a given
DB transition is specified cannot determine whether omitted paths are forbidden
or just undocumented. For a security-relevant state machine, every transition
and every explicitly-forbidden transition should appear.

**Impact.** Incomplete state diagrams introduce ambiguity that will be resolved
differently by different implementers. The `dispatched → failure` omission is
directly related to D12, the design's primary contribution to liveness handling.

**Recommendation.** Extend SM-S to include all specified transitions, with short
labels matching their trigger (liveness-dead-ReceivedByWorker, rollback,
admin-revoke). Mark transitions that are explicitly forbidden (e.g., no direct
`dispatched → revoked` if that is not specified) as crossed edges or prose
notes.

---

### F-R6 — D3 trigger-time scope re-validation underspecified for non-channel scopes

| Attribute | Value                                                    |
| --------- | -------------------------------------------------------- |
| Severity  | Significant                                              |
| Location  | D3 "Descriptor updates additionally re-validate scopes"; |
|           | D3 tests                                                 |

**Problem.** D3 states: "Descriptor updates additionally re-validate scopes
against the updating user's effective scopes, not the row's owner's scopes." The
WCP design's `resolve_and_rewrite` function (cited in audit F4's evidence)
verifies channel scope at trigger time. D3's scope re-validation at update time
is broader in theory but the design's test list covers only the case "a
descriptor update with a scope C lacks → 403."

The audit F4 explicitly called out that "repository scope is not re-checked at
trigger time (`scheduler/trigger.rs` only verifies channel scope via
`resolve_and_rewrite`), so a `repo`-override added by the attacker proceeds even
if the original task owner lacked that repository scope."

D3's descriptor re-validation at update time partially addresses this: if user C
cannot submit a build with `repo = attacker_repo`, they also cannot write that
descriptor to the periodic task. But the trigger-time validation gap in
`scheduler/trigger.rs` is independent: once a descriptor is stored, the trigger
fires it as the owner. If the owner later loses the relevant capability (role
change, token revoke), the trigger fires with the old descriptor under a now-
insufficient owner context. D3 does not add trigger-time re-validation of the
stored descriptor against the task owner's current scopes.

**Impact.** A descriptor stored legitimately by a user who later has their scope
reduced will continue to fire builds in the previously-authorized scope. This is
a privilege persistence gap, not a new privilege-escalation path, but it means
the `:own` / `:any` split alone does not close F4 completely.

**Recommendation.** Add an explicit statement in D3 about trigger-time
re-validation: either (a) the scheduler trigger re-validates the full descriptor
scope against the task owner's current capabilities before submitting, or (b)
the design accepts the privilege-persistence gap and documents it as a known
trade-off. If (a), update D3's test list with a test that demotes a user's role
after task creation and asserts the next trigger is rejected or the build is
submitted with the reduced scope. The WCP design's `resolve_and_rewrite` call is
not sufficient because it only checks channel scope.

---

### F-R7 — D2 trust boundary: userinfo REST call is a separate unbound request

| Attribute | Value                                                         |
| --------- | ------------------------------------------------------------- |
| Severity  | Minor                                                         |
| Location  | D2 "The OAuth callback handler decodes the userinfo response" |

**Problem.** D2 adds `email_verified: bool` to `GoogleUserInfo` and rejects
callbacks where it is `false`. This is the correct fix for F2 as stated.
However, the audit's F2 recommendation noted that `userinfo` is a separate REST
call made after the OAuth exchange, meaning the `email_verified` field arrives
outside the OAuth signature. A compromised or spoofed response to that REST call
could claim `email_verified: true`. The audit's preferred fix (switching to
ID-token introspection so the claim is bound to the OAuth signature) is more
robust but more complex.

D2 does not acknowledge this residual trust gap. The existing architecture uses
a separate `userinfo` REST call; D2 keeps that call and adds a field check on
it. That is correct and adequate for a defense-in-depth pass, but an implementer
or auditor should understand that the `email_verified` check only has value if
the `userinfo` endpoint is authentic.

**Impact.** No new exploit path beyond what already exists; the check is correct
given the existing userinfo approach. This is a defense-in-depth note, not a
blocking issue.

**Recommendation.** Add a comment in D2 noting: "The `userinfo` endpoint
response is a separate REST call made post-OAuth exchange and is not bound to
the OAuth signature. This fix is adequate at current security posture. A future
hardening pass should consider ID-token introspection to bind the
`email_verified` claim to the OAuth signature."

---

### F-R8 — D9 `TraceLayer` is not the only logging surface

| Attribute | Value                                                   |
| --------- | ------------------------------------------------------- |
| Severity  | Minor                                                   |
| Location  | D9 "`TraceLayer::new_for_http().make_span_with(...)` is |
|           | configured with a custom span builder"                  |

**Problem.** D9 configures `TraceLayer`'s `make_span_with` to omit the query
string. This is the right fix for the F5 exploit surface. However, `TraceLayer`
is not the only middleware layer that logs requests. Common sources of URI
logging outside of `TraceLayer`:

- Panic handlers (the default `tower-http` `CatchPanic` and axum's built-in
  recovery) may log the request URI.
- `tower-http::trace::on_request` and `on_response` callbacks beyond
  `make_span_with`.
- Any sentry / error-reporting integration that captures the full `Request`
  struct on error.
- Reverse-proxy access logs (not in-binary, but the D9 doc-comment
  recommendation should acknowledge this).

The single test in D9 captures the specific `/?cli-token=abc123` regression
against `tracing-test` output. This tests the `TraceLayer` span output but not
any of the other surfaces above.

**Impact.** A future contributor who adds a panic handler or an error-reporting
middleware without reading the D9 policy comment will unknowingly re-introduce
query-string logging. The fix is not architecture-wide; it is
middleware-specific.

**Recommendation.** Extend D9's documentation requirement to be a project-wide
logging policy (which is already implied by D10's CI gate goal). State
explicitly: "No middleware, handler, or error reporter may log
`Uri::to_string()` or `request.uri()` in their entirety. Only `Uri::path()` is
permitted at INFO or above; query strings may appear at DEBUG in explicitly
annotated handlers." Reference this policy in the CLAUDE.md correctness
invariants alongside D10.

---

### F-R9 — D8 `fs::rename` atomicity on Windows

| Attribute | Value                                                |
| --------- | ---------------------------------------------------- |
| Severity  | Minor                                                |
| Location  | D8 "`fs::rename` the temp file over the target.      |
|           | Rename within the same directory is atomic on POSIX" |

**Problem.** The design correctly notes POSIX rename atomicity. It adds "(this
is a Unix-only concern in practice)" for the 0o600 mode but does not address
`fs::rename` semantics on Windows. On Windows prior to Rust 1.86 / Windows 11
22H2, `std::fs::rename` is not atomic — it can fail if the target file is open.
Rust 1.86 stabilized `File::rename_no_replace` via `SetFileInformationByHandle`
which is atomic.

Since `cbc` is a cross-platform binary (the Rust workspace produces it for
multiple targets), a Windows user who experiences a file-open race during rename
may see a partially-written config. The design says "accept the platform
default" for non-Unix but does not acknowledge this means non-atomic rename on
Windows.

**Impact.** Low: `cbc` is used primarily on Linux. A token-in-config file that
fails to rename is a `Err(...)` from `fs::rename`, not a silent corruption. But
the intermediate temp file is not cleaned up if rename fails, and the user may
not realize their config was not saved.

**Recommendation.** Add a note: "On Windows, `std::fs::rename` is not atomic
before Rust 1.86. If Windows support is required, use `File::rename_no_replace`
(Rust 1.86+) or document the known limitation. For now, document the behavior
and ensure the error path always attempts `fs::remove_file` on the temp file."

---

### F-R10 — D3 existing custom-role migration gap

| Attribute | Value                                                       |
| --------- | ----------------------------------------------------------- |
| Severity  | Minor                                                       |
| Location  | D3 "Default roles in the seed data"; cross-cutting "Default |
|           | secrets / config seeding"                                   |

**Problem.** D3 splits `periodic:manage` into `periodic:manage:own` and
`periodic:manage:any`. The design covers the seed-data update for `admin` and
`developer` roles. However, a production deployment that has created custom
roles via the role-management API containing `periodic:manage` (a capability
that will no longer exist after the migration) is not addressed. The design says
"The maintainer confirmed in 019 v1.1 follow-up Q1 that we can break the
existing capability without a migration concern." That confirms no
backward-compatibility requirement, but it does not specify what happens to
existing custom roles:

- Do custom roles that contained `periodic:manage` silently lose the capability
  entirely?
- Are they migrated to `:own` (conservative) or `:any` (liberal) or both?

The SQL migration that implements D3 must make an explicit choice. If it
silently drops the capability, operators who had given non-admin users access to
periodic task management will not notice until runtime.

**Impact.** Operational surprise in existing deployments. No security
regression.

**Recommendation.** Add one sentence to D3: "The SQL migration drops the legacy
`periodic:manage` capability from all user-role assignments. Operators who
granted non-admin users `periodic:manage` must re-grant `periodic:manage:own` or
`periodic:manage:any` explicitly post-migration." Add a migration test that
seeds a custom role with `periodic:manage` and asserts post-migration state.

---

### F-R11 — D4 token visible in `window.location` after redirect

| Attribute | Value                                                     |
| --------- | --------------------------------------------------------- |
| Severity  | Minor                                                     |
| Location  | D4 "Update `ui/index.html:355-361` to read the token from |
|           | `window.location.hash.slice(1)`"                          |

**Problem.** Using the URL fragment moves the token out of server logs (correct
per RFC 3986 §3.5). However, after the redirect, the token sits in
`window.location.hash` and in the browser's history entry for `/#cli-token=...`.
Any JavaScript on the page (including analytics scripts, browser extensions with
content-script access, or third-party UI libraries) can read the full
`window.location.hash` before the client code extracts it.

The design does not specify `history.replaceState({}, '', '/')` after
extraction, which would remove the token from the visible URL, browser history,
and `window.location`. Without it, a user who copies the URL from the browser's
address bar after the CLI login redirect includes the PASETO token.

**Impact.** Low: only the user's own browser session is at risk, and the token
is long-lived (6-month TTL per the audit). A shared-machine user copy-pasting
the post-redirect URL exposes the token to whoever receives it.

**Recommendation.** Add to D4: "After extracting the token from
`window.location.hash`, call `history.replaceState({}, '', '/')` to clear the
fragment from the address bar and browser history. Add a test asserting that the
hash is cleared immediately after extraction."

---

### F-R12 — Phase E dependency not tracked

| Attribute | Value                             |
| --------- | --------------------------------- |
| Severity  | Minor                             |
| Location  | "Implementation Phasing", Phase E |

**Problem.** Phase E (D12 + D13) "cannot land until the WCP design v11 is
implemented." The WCP design (the sibling document) has no plan document under
`cbsd-rs/docs/cbsd-rs/plans/`; the only plan documents for seq 019 are in the
reviews directory. Phase E's dependency is thus a verbal contract on a document
that has not produced an implementation plan.

If the WCP design produces a plan and its implementation is tracked separately,
Phase E has no mechanism to record or check its dependency on Phase
(WCP-implementation). An implementation agent picking up Phase E without
consulting the WCP implementation status may proceed prematurely.

**Impact.** Operational/process risk; no security impact.

**Recommendation.** Add an explicit note in Phase E: "The WCP design must have a
plan document at `cbsd-rs/docs/cbsd-rs/plans/019-…-worker-control-plane-…` AND
all WCP plan commits must have landed before Phase E begins. The plan document
for this design should record the WCP plan document path as a prerequisite."

---

## Prior Review Coverage

The design claims to address findings F1, F2, F4, F5, F7, F8, F10, F11, F13, and
the three WCP v10 open items. This section evaluates each claim.

| Finding                                     | Claimed status      | Assessment                                                                                                                                                                                                     |
| ------------------------------------------- | ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F1                                          | Addressed (D1)      | Substantially correct. Strict parse + loopback guard + startup WARN are all present. Gap: loopback whitelist text is under-specified (see F-R4).                                                               |
| F2                                          | Addressed (D2)      | Correct and sufficient at current security posture. The userinfo REST-call trust gap is a residual known risk (see F-R7).                                                                                      |
| F3                                          | Deferred to WCP     | Correctly deferred. WCP v11 "Same-worker connection migration" + "Reconnect Ownership" cover F3 in the sibling design.                                                                                         |
| F4                                          | Addressed (D3)      | Partially. The `:own` / `:any` RBAC split and update-time scope re-validation close the lateral privilege-transfer path. Trigger-time scope validation against current owner caps is not specified (see F-R6). |
| F5                                          | Addressed (D4 + D9) | Correct. Fragment fix + TraceLayer query-omission. Belt-and-suspenders approach is sound. Minor gap: D9 does not cover non-TraceLayer logging surfaces (see F-R8).                                             |
| F6                                          | Dismissed           | Correctly dismissed per v1.1 reclassification to deployment policy.                                                                                                                                            |
| F7                                          | Addressed (D5)      | Partially. Symlink containment and decompression cap are specified. PAX-header and chained-symlink gaps remain (see F-R3).                                                                                     |
| F8                                          | Addressed (D6)      | Correct. 1 MiB REST body, 8 MiB WS message, no per-line log cap. Internally consistent. The no-per-log-line-cap trust argument is sound given WCP ownership rules will be required before Phase E.             |
| F9                                          | Dismissed           | Correctly dismissed per v1.1.                                                                                                                                                                                  |
| F10                                         | Addressed (D7)      | Correct. B-tree index on `api_keys(key_prefix)`. Non-unique index is the right choice; prefix is a lookup helper, not a unique key. Migration is straightforward.                                              |
| F11                                         | Addressed (D8)      | Correct. HTTPS enforcement + `--insecure-http` opt-in + atomic config write. The Windows rename atomicity caveat is noted (see F-R9).                                                                          |
| F12                                         | Dismissed           | Correctly dismissed per v1.1.                                                                                                                                                                                  |
| F13                                         | Addressed (D10)     | Addressed for `Debug`/`Display`. The `serde::Serialize` gap is Critical (see F-R2). The CI grep gate is deferred, which is acceptable given the `Secret<T>` newtype provides the construction-time defense.    |
| Prior F1 (cross-worker lifecycle spoofing)  | Deferred to WCP     | Correctly deferred.                                                                                                                                                                                            |
| Prior F2 (cross-worker log output spoofing) | Deferred to WCP     | Correctly deferred.                                                                                                                                                                                            |
| Prior F3 (empty component lists)            | Deferred to WCP     | Correctly deferred.                                                                                                                                                                                            |
| Prior F4 (log tail full read)               | Deferred to WCP     | Correctly deferred.                                                                                                                                                                                            |

---

## WCP v10 Open Items Coverage

The WCP v10 review (2026-04-27) raised three High-severity open items. The
design claims D11, D12, D13 address them.

| WCP v10 open item                                                                           | D-number | Assessment                                                                                                                                                                                                                                |
| ------------------------------------------------------------------------------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. `accepted` phase omitted from reconnect `Building` / `Idle` rules                        | D11      | **Fully addressed.** D11 adds `accepted` to the reconnect `Building` condition; the server treats reconnect-`Building` from an `accepted`-phase worker as `ReceivedByWorker`; SM-R transition is specified.                               |
| 2. Liveness/dead-worker resolution for `dispatched + AwaitingReceipt` vs `ReceivedByWorker` | D12      | **Fully addressed.** The resolution table covers all four SM-S × SM-R combinations with explicit actions and side effects. The rationale for `ReceivedByWorker → failure` (conservative, avoids duplicate S3/Harbor) is correctly stated. |
| 3. Superseded live same-worker connection has no concrete stop-work path                    | D13      | **Mostly addressed.** Stop-work delivery is specified, the best-effort send order is correct, and the safety-net (D11 + D12) is defined. The `terminal-pending-report` discard is a critical residual gap (see F-R1).                     |

---

## Strengths

**Scope discipline.** The design rigorously distinguishes what it owns from what
the WCP sibling owns. The overlap on D11–D13 is handled by keeping the WCP v11
policy unchanged and treating these decisions as additive. This is the right
architecture for two concurrent designs at the same sequence number.

**D12 resolution table.** The four-row table (SM-S × SM-R → terminal SM-S) is
precise, complete for the defined cases, and the conservative choice
(`ReceivedByWorker → failure`) is well-justified by the side-effect concern.

**D6 trust argument.** The explicit decision NOT to add a per-log-line cap, with
the stated trust boundary (authenticated workers + WCP ownership rules), is
sound and documented. It also correctly scopes the decision to the
authentication boundary rather than trying to enforce syntactic constraints on
free-form text.

**D1 strict-parse set.** The truthy set (`1`, `true`, `yes`, `on`) is a
well-known convention for boolean env vars and the comparison is specified as
case-insensitive, which avoids `TRUE` / `True` surprises. The startup WARN
redacting the raw value to `true`/`false` (not echoing the original string) is a
useful defense against secrets accidentally passed as env var values.

**D3 scope-smuggling close.** The requirement that a `:any` admin who edits
another user's descriptor must personally hold the descriptor's required scopes
is the correct fix for the privilege-transfer attack described in F4. This is
non-obvious and gets it right.

**D10 CI gate acknowledgment.** Deferring the CI grep gate to a tooling
comparison rather than shipping a half-baked solution is pragmatic. The
`Secret<T>` newtype gives the construction-time defense immediately; the gate is
a process defense that should be added but is not blocking.

**State machine section.** The consolidated normative summary of SM-W, SM-S,
SM-R, and the trigger inputs (SM-C, SM-L) is genuinely useful as a reference.
Even with the SM-S diagram gap called out above, having this section at all is
well above average for design documents of this kind.

**D5 legitimate-symlink acknowledgment.** The design explicitly notes the
existing legitimate symlink in the repository (`v20.3 → ./v20.2`) and asserts it
must continue to work. This is the hallmark of a design that was actually tested
against real data.

---

## Open Questions

1. **D13 `terminal-pending-report` drain (from F-R1).** Is the trade-off
   (discard success, force revoked) the right behavior, or should the supervisor
   drain the pending terminal result before the migration revoke applies? If the
   maintainer accepts the discard, that policy should be documented explicitly
   rather than described as a side effect of the migration.

2. **D10 Serialize stance (from F-R2).** What is the explicit `Serialize` /
   `Deserialize` policy for `Secret<T>`? Should the team adopt the `secrecy`
   crate or maintain the newtype in-house?

3. **D3 trigger-time scope re-validation (from F-R6).** Is the scheduler trigger
   expected to re-validate the stored descriptor against the task owner's
   current capabilities? Or is the update-time scope check sufficient, and the
   privilege-persistence case is accepted?

4. **Phase E WCP prerequisite tracking (from F-R12).** When will the WCP design
   produce a plan document? Phase E should not begin until that plan's commits
   have landed.

5. **D6 interaction with Phase E ordering.** The no-per-log-line-cap trust
   argument rests on the WCP ownership rules being in force. If Phase C (server
   size limits) lands before Phase E (WCP ownership), there is a window where
   log output is size-bounded but not ownership-gated. Is that acceptable, or
   does Phase C have an ordering dependency on Phase E?

---

## Confidence Score

| Item                                                                  | Points | Description                                                                                                       |
| --------------------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------- |
| Starting score                                                        | 100    |                                                                                                                   |
| D7: D13 `terminal-pending-report` silent data loss                    | -20    | A build that completed successfully is recorded as `revoked`; S3/Harbor artifacts committed and orphaned.         |
| D7: D10 `Secret<T>` missing `Serialize` policy                        | -20    | "By construction" redaction claim is false without explicit `serde` stance; `to_string(&secret)` leaks raw value. |
| D7: D5 PAX extended-header and chained-symlink containment gap        | -20    | Containment check does not account for PAX-overridden paths or previously unpacked symlinks in the same archive.  |
| D11: D1 loopback whitelist not concrete                               | -5     | IPv6 loopback and URL-authority confusion not addressed; implementers must guess.                                 |
| D11: SM-S diagram missing `dispatched → failure` (D12) transition     | -5     | Primary contribution of D12 to SM-S is absent from the normative state diagram.                                   |
| D11: D3 trigger-time scope re-validation not specified                | -5     | Privilege persistence after role demotion is an unacknowledged residual gap.                                      |
| D11: D3 custom-role migration not specified                           | -5     | Existing deployments with custom `periodic:manage` roles will silently lose the capability; no migration spec.    |
| D11: D4 `history.replaceState` not specified after token extract      | -5     | Token remains in `window.location.hash` and browser history after CLI login redirect.                             |
| D9: D9 covers only TraceLayer; other URI-logging surfaces unaddressed | -5     | Panic handlers, error reporters, and reverse-proxy logs are not covered by the D9 policy.                         |
| **Total**                                                             | **10** |                                                                                                                   |

Interpretation: 10/100 — "Major rework needed. Block merge." This score applies
to the design document only. The number is depressed primarily by the two
Critical design gaps (D13 data loss, D10 Serialize). If those two items are
resolved with a concrete specification addendum, the score would rise above 50;
with all Significant items addressed, above 75. Recommend a v2 design pass
addressing at minimum F-R1 and F-R2 before an implementation plan is written.

---

## Go / No-Go

**No-go for implementation planning as written.**

Required before producing an implementation plan:

1. **(Blocking)** D13: specify the `terminal-pending-report` behavior
   explicitly. Either adopt the drain-then-accept alternative or document the
   discard as a known operational risk with operator guidance. The current
   framing ("deliberate trade-off") is insufficient.

2. **(Blocking)** D10: specify the `serde::Serialize` stance for `Secret<T>`
   explicitly. The construction-time redaction guarantee is the core of D10; it
   must hold for all serialization paths, not just `Debug`/`Display`.

After those two items are resolved in a v2 design pass, the remaining findings
(F-R3 through F-R12) can be addressed in implementation notes or during code
review without blocking planning:

- F-R3 (D5 PAX / chained-symlink) should be addressed in the implementation
  plan's commit spec for D5.
- F-R4 (D1 loopback algorithm) is a two-line change to D1's implementation spec.
- F-R5 (SM-S diagram) is a documentation fix; update the state machine section
  in v2.
- F-R6, F-R10 (D3 gaps) require explicit answers but do not block Phase B
  implementation if answered in the plan.
- F-R7 through F-R12 are minor observations that can be addressed inline.

---

## References

- Primary design under review:
  `cbsd-rs/docs/cbsd-rs/design/019-20260514T1040-security-audit-remediation.md`
- WCP sibling design (v11):
  `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md`
- WCP v10 design review:
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260427T0818-design-worker-control-plane-hardening-v10.md`
- Security audit v1:
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260512T2339-impl-cbsd-rs-security-audit-v1.md`
- Security audit v1.1:
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260514T0841-impl-cbsd-rs-security-audit-v1.1.md`
- Original security review: `cbsd-rs/docs/000-20264026T1104-security-review.md`
