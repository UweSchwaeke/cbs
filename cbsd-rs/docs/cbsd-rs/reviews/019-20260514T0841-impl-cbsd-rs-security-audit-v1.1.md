# cbsd-rs implementation security audit — follow-up addendum (v1.1)

| Field          | Value                                                                                                            |
| -------------- | ---------------------------------------------------------------------------------------------------------------- |
| Review         | 019 v1.1 (addendum to v1)                                                                                        |
| Date (UTC)     | 2026-05-14 08:41                                                                                                 |
| Parent review  | `cbsd-rs/docs/cbsd-rs/reviews/019-20260512T2339-impl-cbsd-rs-security-audit-v1.md`                               |
| Scope          | Reclassification of selected findings (F6, F9, F11, F12, F13) based on maintainer feedback and code verification |
| Reviewer       | Independent (follow-up after maintainer feedback)                                                                |
| Recommendation | Unchanged at the top level: NO-GO for production pending the remaining unfixed Critical/High items               |

## Purpose

This addendum **does not** restate the full audit. It records the outcome for
the five findings the maintainer asked to re-evaluate after v1. Findings not
listed here are unchanged from v1.

## Items

### F6 — Server has no TLS support — reclassified to Informational

- v1 severity: **High**
- v1.1 severity: **Informational / operational**
- Verdict: **Accepted as deployment policy.**

Maintainer feedback: production deployments will always place `cbsd-server`
behind a TLS-terminating reverse proxy. With that policy in place, the finding
loses its in-band exploitability and becomes a deployment constraint rather than
a code defect.

The underlying observation in v1 is still true: `cbsd-server` itself binds plain
HTTP. No code-level change is required to close v1.1 of this finding, but the
constraint must be documented:

- Operators MUST deploy a TLS-terminating reverse proxy in front of
  `cbsd-server`. There is no in-process TLS today.
- A "no reverse proxy" deployment is unsupported and would re-expose every
  authenticated request, including bearer tokens, in cleartext.

Action: a roadmap entry for native TLS support has been added to
`cbsd-rs/docs/ROADMAP.md` under the `cbsd-rs` section. Picking it up becomes
appropriate when operators want to remove the reverse-proxy requirement.

Confidence-score impact: the −20 deduction from v1 is reversed in v1.1.

### F9 — `python3` resolved via `$PATH` — dismissed, superseded by roadmap item

- v1 severity: **Low**
- v1.1 severity: **N/A (dismissed)**
- Verdict: **Dismissed**, with replacement plan.

Maintainer feedback: the current Python wrapper at `scripts/cbscore-wrapper.py`
will be obsoleted by migrating `cbscore` itself from Python to Rust. The Rust
port becomes a crate consumed in-process by `cbsd-worker`, removing the `$PATH`
resolution surface entirely. There is no need to harden the wrapper now if it is
going to be deleted.

Action: roadmap entry "Migrate `cbscore` from Python to Rust (`cbscore-rs`
crate)" added to `cbsd-rs/docs/ROADMAP.md` under the `cbsd-rs` section. F9 is
closed pending that migration.

Confidence-score impact: the −5 deduction from v1 is reversed in v1.1.

### F11 — `cbc` accepts `http://` URLs and config-file TOCTOU — VERIFIED, stands as High

- v1 severity: **High**
- v1.1 severity: **High (unchanged)**
- Verdict: **Confirmed. Not gated or mitigated by `--no-tls-verify` / `-k`.**

Maintainer asked whether `--no-tls-verify` / `-k` mitigates F11. Direct code
reading shows the two are orthogonal. Both sub-issues stand.

#### Sub-issue A: `http://` host acceptance

Path of evidence:

- `cbc/src/main.rs:50-51`: `-k, --no-tls-verify: bool` clap flag.
- `cbc/src/main.rs:96-99`: the only behavior driven by `k` is a warning print;
  `k` is then forwarded into every command.
- `cbc/src/main.rs:111-117` (`cmd_login`): the raw `url` argument is passed
  verbatim into `CbcClient::unauthenticated(url, debug, no_tls_verify)`. There
  is no scheme check.
- `cbc/src/client.rs:29-44` (`CbcClient::new`) and `cbc/src/client.rs:54-66`
  (`unauthenticated`): both call `parse_base_url(host)` and then build a
  `reqwest::Client` with `.danger_accept_invalid_certs(no_tls_verify)`.
- `cbc/src/client.rs:217-227` (`parse_base_url`): `Url::parse(&s)` with no
  `https://` enforcement. Any scheme that `url::Url` accepts is accepted.
- Bearer token is then attached unconditionally in `cbc/src/client.rs:32-38`:
  `Authorization: Bearer <token>` is set in default headers regardless of
  scheme.

Concretely:

- `--no-tls-verify` (`-k`) only sets `danger_accept_invalid_certs`, which is a
  **TLS certificate-validation toggle for HTTPS connections**. It has no effect
  on whether HTTP is permitted.
- An operator configuring `host = "http://cbs.example.com"` (or running
  `cbc login http://cbs.example.com`) will send the bearer token in the
  `Authorization` header in plaintext over HTTP on the very first request. If
  the reverse proxy responds with a 301/302 redirect to HTTPS, the redirect is
  followed — but the token has already left the client in cleartext on the wire.
- The `-k` flag's warning ("warning: TLS certificate verification is disabled",
  `main.rs:98`) is accurate and unrelated. The CLI emits no warning when the
  configured `host` is plain HTTP.

`--no-tls-verify` therefore neither gates nor mitigates this issue. They are two
independent code paths.

Recommended fix (unchanged from v1):

- Reject non-`https://` schemes in `parse_base_url` by default. Allow an
  explicit opt-in (`--insecure-http` or similar) that is distinct from
  `--no-tls-verify` and emits a stronger warning.
- Emit a startup warning whenever `host` resolves to plain HTTP, even if opt-in
  is granted.

#### Sub-issue B: config-file TOCTOU window

`cbc/src/config.rs:49-71` (`Config::save`):

```rust
std::fs::write(path, &json)?;          // step 1: file created, default umask
#[cfg(unix)] {
    std::fs::set_permissions(path,
        std::fs::Permissions::from_mode(0o600))?;  // step 2: chmod 0o600
}
```

Window analysis:

- Step 1 creates the file with the OS default umask. On a typical Linux system
  that is `0o022`, producing a file with mode `0o644` (world-readable). The
  token JSON content is already on disk.
- Step 2 narrows the mode to `0o600`. Between steps 1 and 2, any local reader on
  the same host can `cat` the file and obtain the bearer token.

This is a real, if narrow, TOCTOU window. It is completely independent of any
TLS flag.

Recommended fix (unchanged from v1):

- Use `std::fs::OpenOptions::new().write(true).create_new(true)` plus
  `.mode(0o600)` on Unix when creating the file, so the file is created with
  restrictive permissions atomically. On overwrite of an existing file, set
  permissions before writing the new content (or write to a temp path with 0o600
  and rename into place).

#### Confidence-score impact

Unchanged. The −20 D7 deduction and the −5 D10 convention-violation deduction
from v1 both stand.

### F12 — Dev OAuth bypass accepts arbitrary `dev_email` — dismissed

- v1 severity: **Low / Informational**
- v1.1 severity: **N/A (dismissed)**
- Verdict: **Dismissed.**

Maintainer feedback: in dev mode a forged identity cannot publish anything
relevant because the build pipeline relies on `cbscore` to interact with S3,
Harbor, and other upstream systems. Those systems require their own credentials.
A user who can name themselves `victim@example.com` via the dev bypass cannot
use that identity to do anything meaningful against the real publishing
infrastructure. The future `cbscore-rs` migration is expected to keep this
property.

Caveats noted but not raised as findings:

- Dev mode must remain strictly opt-in (e.g. an explicit `CBSD_DEV=1` or
  equivalent) and must never default-on. Misconfiguration that enables dev mode
  in production is its own deployment concern and is already visible from the
  existing F1 (worker `CBSD_DEV` truthiness) follow-up work.
- The internal cbsd-server RBAC surface is still reachable via the bypass. That
  is acceptable in dev because the upstream-side enforcement remains in place.

Confidence-score impact: the −20 deduction from v1 (D7 framing of the dev bypass
acceptance gap) is reversed in v1.1.

### F13 — Bearer token prefix logged at debug — confirmed, action required

- v1 severity: **Informational**
- v1.1 severity: **Informational, but action required** (raised to explicit
  "must fix" status by maintainer policy)
- Verdict: **Confirmed.**

Maintainer policy: no portion of any bearer token, session token, robot token,
or API key may be written to logs at any log level. This is stricter than v1,
which only flagged the prefix-logging at debug as a mild defense-in-depth
concern.

Action required:

- Audit every `tracing` call in `cbsd-server`, `cbsd-worker`, and `cbc` for
  emission of token material. Common patterns to grep for:
  - `tracing::debug!(.*token`
  - `tracing::info!(.*token`
  - `Bearer` literal in format strings
  - `Authorization` header keys appearing in field lists
  - PASETO raw token variables passed to formatters
- Remove or redact all such sites. If diagnostic visibility is needed, prefer a
  deterministic non-reversible identifier (e.g. a content hash prefix or a
  per-process session ID generated separately from the token).
- Add a CI grep gate (or a small `clippy::disallowed_methods` / `tracing` filter
  test) to prevent regressions.

This work pairs naturally with the F5 fix from v1 (CLI login query-string leaks
token to request logs) because both need the access-log surface to be reviewed
for token material end-to-end.

Confidence-score impact: unchanged from v1 (the −20 deduction stands). The
reclassification is policy-level, not numeric.

## Revised confidence score (after v1.1 adjustments)

Starting from the v1 subtotal of −230, the v1.1 changes are:

| Change                                       | Delta |
| -------------------------------------------- | ----- |
| F6 reclassified to operational/informational | +20   |
| F9 dismissed (superseded by roadmap item)    | +5    |
| F11 confirmed (no change)                    | 0     |
| F12 dismissed                                | +20   |
| F13 confirmed, policy raised (no number)     | 0     |
| **v1.1 subtotal**                            | −185  |
| **Floor per scoring rubric**                 | 0     |
| **Total reported**                           | 0/100 |

The reported score is still at the floor: the math improved by 45 points but
remains 85 points below the floor. The interpretation does not change: "Major
rework needed. Block merge."

## Go / No-Go

Unchanged from v1: **NO-GO for production.**

The blocking-list contracts on three items but does not flip the verdict:

- F6 is no longer blocking (operational, accepted) — track via `ROADMAP.md`.
- F9 is no longer tracked individually (folded into `cbscore-rs` migration
  roadmap item).
- F12 is no longer tracked individually (dev bypass is acceptable given upstream
  enforcement).

Remaining blockers from v1 (unchanged):

1. F1 Critical — worker `CBSD_DEV` truthiness installs `NoVerifier`.
2. F2 High — OAuth callback does not verify `email_verified`.
3. F3 High — `worker_status(Building)` reconnect rewrites `connection_id` with
   no ownership check.
4. F4 High — periodic-task descriptor privilege transfer.
5. F5 High — CLI login query-string leaks token to request logs; coordinate with
   F13 redaction policy.
6. F11 High — `cbc` accepts `http://` and has a config-write TOCTOU window. Not
   mitigated by `--no-tls-verify`.
7. F13 elevated to explicit redaction policy (must fix; pairs with F5).

Plus the prior security review's four unfixed findings; design 019 v11 is still
draft.

## References

- v1 review:
  `cbsd-rs/docs/cbsd-rs/reviews/019-20260512T2339-impl-cbsd-rs-security-audit-v1.md`
- Roadmap: `cbsd-rs/docs/ROADMAP.md`
- Prior security review: `cbsd-rs/docs/000-20264026T1104-security-review.md`
- Design 019 (latest draft):
  `cbsd-rs/docs/cbsd-rs/design/019-20260426T1154-worker-control-plane-hardening.md`
