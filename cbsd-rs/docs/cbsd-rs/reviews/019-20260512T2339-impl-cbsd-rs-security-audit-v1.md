# cbsd-rs security audit (v1)

| Field          | Value                                                                                                                                                                                                                     |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Review         | 019 — cbsd-rs implementation security audit                                                                                                                                                                               |
| Date (UTC)     | 2026-05-12 23:39                                                                                                                                                                                                          |
| Scope          | `cbsd-rs/` workspace: `cbsd-proto`, `cbsd-server`, `cbsd-worker`, `cbc`, `migrations/`, `scripts/cbscore-wrapper.py`                                                                                                      |
| Reviewer       | Independent (subagent), no trust in implementer claims                                                                                                                                                                    |
| Prior review   | `cbsd-rs/docs/000-20264026T1104-security-review.md` (4 findings, score 15/100). The hardening design (019 worker-control-plane-hardening) is still at draft v11 — none of the prior findings are fixed in the code today. |
| Recommendation | **NO-GO for production.** Multiple unfixed Critical/High issues across auth, RBAC, TLS, and the worker control plane. Significant rework required before this stack can be considered secure.                             |

## Summary

The single most exploitable issue is **F1 — the worker `CBSD_DEV`-truthiness TLS
bypass**: any non-empty value of the env variable on the worker silently
disables certificate verification, allowing a network attacker to MITM the
worker-to-server WebSocket and steal the API key. The four prior worker
control-plane findings remain present in the code and are now joined by a fifth
ownership-rewrite path (`worker_status(Building)` reconnect). Other
High-severity issues include the missing `email_verified` check on the OAuth
callback, the periodic-task privilege transfer, and bearer tokens being written
to server access/tracing logs via the CLI redirect query string. The server has
no TLS support at all — operators must terminate TLS at a reverse proxy, but
nothing in the binary enforces or even strongly encourages it.

Top findings, ordered by severity:

1. **F1 (Critical)** — Worker `CBSD_DEV` env variable truthiness silently
   disables TLS server-certificate verification.
2. **F2 (High)** — OAuth callback never checks `verified_email` on the Google
   user-info response.
3. **F3 (High)** — `worker_status(Building)` reconnect path rewrites
   `ActiveBuild.connection_id` without verifying the reporting worker was the
   originally-dispatched worker.
4. **F4 (High)** — Periodic-task `PUT` allows any holder of `periodic:manage`
   (without `*`) to rewrite the descriptor of another user's task; the task
   fires as the original owner.
5. **F5 (High)** — CLI login redirects with the PASETO token in the query string
   (not the fragment), leaking the bearer into the server's own request-tracing
   logs.

## Findings

Severity rubric used below:

- **Critical** — unauthenticated RCE, full secret leak, or authentication bypass
  with a realistic deployment configuration.
- **High** — authenticated privilege escalation, secret exposure in
  logs/storage, or persistent state corruption.
- **Medium** — DoS at request scope, information disclosure with limited impact,
  or pre-conditions that require an unusual config.
- **Low / Informational** — defense-in-depth, hardening.

Every finding cites `file:line` ranges in the worktree at `cbsd-rs/` rooted at
the repo working tree. Line numbers reflect the state at audit time.

### F1 — Worker `CBSD_DEV` truthiness disables TLS verification

| Field          | Value                                                                                                                                             |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity       | **Critical**                                                                                                                                      |
| Attacker model | On-path network attacker between worker and server (or anyone who can convince an operator to leave a stray `CBSD_DEV=…` value in the worker env) |
| New            | Yes                                                                                                                                               |

Evidence:

- `cbsd-rs/cbsd-worker/src/config.rs:153-155` sets `is_dev` to `true` whenever
  `CBSD_DEV` is set to any non-empty string.
- The same value is copied unchanged into `ResolvedWorkerConfig.dev_mode` at
  `cbsd-rs/cbsd-worker/src/config.rs:216` and `:252`.
- `cbsd-rs/cbsd-worker/src/ws/connection.rs:50-56` then selects a `Rustls`
  connector that uses `NoVerifier`
  (`cbsd-rs/cbsd-worker/src/ws/connection.rs:81-116`) which unconditionally
  returns success from `verify_server_cert`, `verify_tls12_signature`, and
  `verify_tls13_signature`.
- The worker sends `Authorization: Bearer <api_key>` over this unverified
  connection (`cbsd-rs/cbsd-worker/src/ws/connection.rs:44-48`).

Impact: a network adversary positioned between the worker and the server (e.g.
malicious LAN, compromised reverse proxy, hostile cloud networking) can present
any certificate and:

1. Capture the `cbsk_…` worker API key on the very first handshake.
2. With that key, the attacker is an authenticated registered worker and can
   exercise every prior-review finding (lifecycle spoof, log forgery, etc.).

The truthiness logic is itself the trap: an operator setting `CBSD_DEV=false` or
`CBSD_DEV=0` to "disable" dev mode actually **enables** it. There is no log
warning at startup that certificate verification is off. The same pattern exists
in `cbsd-rs/cbsd-server/src/config.rs:285-287` and
`cbsd-rs/cbsd-server/src/main.rs:63-65` for the server logging gate — same trap
shape, lower blast radius (only affects logging strictness), still worth fixing.

Recommendation:

- Gate the `NoVerifier` connector behind a config field
  (`dev.dangerous_disable_tls_verification`) **read from the worker YAML**, not
  from the environment.
- Refuse to start with an explicit error if both a `wss://` server URL and the
  disable-verification flag are set without an additional
  `--i-know-what-i-am-doing` flag.
- Print a `tracing::warn!` at startup whenever cert verification is off,
  including the resolved value and source (env vs config).
- Replace the `CBSD_DEV` truthiness check with a strict `"1" | "true"` parse and
  reject every other non-empty value with a fatal config error. Apply this
  everywhere the env var is read (`cbsd-rs/cbsd-worker/src/config.rs:153`,
  `cbsd-rs/cbsd-server/src/config.rs:285`,
  `cbsd-rs/cbsd-server/src/main.rs:63`).

### F2 — OAuth callback does not verify `verified_email`

| Field          | Value                                                                                                               |
| -------------- | ------------------------------------------------------------------------------------------------------------------- |
| Severity       | **High**                                                                                                            |
| Attacker model | Anonymous internet attacker who controls a Google account with an unverified email address inside an allowed domain |
| New            | Yes                                                                                                                 |

Evidence:

- `cbsd-rs/cbsd-server/src/auth/oauth.rs:42-47` defines the `GoogleUserInfo`
  struct as containing only `email` and `name`. Google's `userinfo` endpoint
  returns a `verified_email` boolean (and the OpenID `email_verified` claim)
  that the code never requests and never checks.
- `cbsd-rs/cbsd-server/src/routes/auth.rs:237-265` performs only
  domain-allowlist matching on the returned email, not a verification check.
- `cbsd-rs/cbsd-server/src/routes/auth.rs:272-289` then calls
  `db::users::create_or_update_user` which creates a new user row on first
  sight.

Impact: when `oauth.allowed_domains` is non-empty (typical production config),
Google permits OAuth users to authenticate with self-asserted email addresses
under any non-Google-Workspace-managed domain (this is a documented Google quirk
that requires explicit `verified_email` checking on the relying party). An
attacker who adds an unverified `@<allowed-domain>` email to a Google account
they control can complete OAuth and gain whatever default capabilities new users
receive — and, if an admin later assigns them a role matching the email, full
privilege escalation. The `oauth.allow_any_google_account` config makes this
worse: any self-asserted Google email is accepted.

This is also a Google-specific spec deviation (D8) — the OpenID Connect Core 1.0
§5.7 requires relying parties to consider `email_verified` before treating an
`email` claim as identity.

Recommendation:

- Extend `GoogleUserInfo` with `verified_email: bool` and reject the callback
  when it is `false` (or missing).
- Document the requirement in the OAuth flow doc 015.
- Consider switching from `userinfo` to ID-token introspection so the
  verified-email claim is bound to the OAuth signature, not a separate REST
  call.

### F3 — `worker_status(Building)` reconnect rewrites build ownership

| Field          | Value                                                                                                                                                          |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity       | **High**                                                                                                                                                       |
| Attacker model | Any authenticated registered worker (i.e. any holder of a `cbsk_…` key bound to a worker row)                                                                  |
| New            | Yes — distinct from the prior review's lifecycle-spoof finding and **not** addressed by any code yet; design 019 v11 specifies the fix but it is unimplemented |

Evidence:

- `cbsd-rs/cbsd-server/src/ws/handler.rs:638-714`: when the worker reports
  `WorkerStatus { state: Building, build_id }`, the server looks up the build's
  DB state and acts on it.
- For the `"dispatched"` branch (`:656-667`) the server calls
  `dispatch::handle_build_started(state, build_id.0)` and then rewrites
  `queue.active[build_id].connection_id` to the **reporting connection**,
  without verifying that this worker was the dispatch target.
- The `"started"` branch (`:668-677`) does the same overwrite for a build that
  was already running.

Impact: worker B (which holds any valid registered API key) connects and sends
`worker_status` claiming to be running build 42, which was in fact dispatched to
worker A. The server silently transfers ownership of build 42 to worker B, after
which every subsequent build_output/build_started/build_finished/build_rejected
sent by worker B is accepted as authoritative — and the legitimate worker A
finds itself locked out (the ownership check on lifecycle messages will reject
it because the active connection_id has moved).

This is exactly the same vulnerability class as the prior review's "lifecycle
spoof" but reached via a different code path (`worker_status` instead of
`BuildStarted/BuildOutput`). It is **not** patched by any current commit and is
enumerated in the v11 design (`019-…-worker-control-plane-hardening`) under D1
as a required boundary, but the design is still in review.

Recommendation: implement the build-scoped authorization required by design 019
D1 across **all** worker-originated messages including
`worker_status(Building)`. Reject and log mismatches; never use a worker claim
to migrate active ownership.

### F4 — Periodic-task privilege transfer

| Field          | Value                                                                                                                                |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Severity       | **High**                                                                                                                             |
| Attacker model | Authenticated user with `periodic:manage` + `builds:create` (no wildcard), targeting a task owned by another user with broader scope |
| New            | Yes                                                                                                                                  |

Evidence:

- `cbsd-rs/cbsd-server/src/routes/periodic.rs:301-313` (PUT handler) requires
  only `periodic:manage`; there is no `created_by == user.email` ownership
  check.
- `cbsd-rs/cbsd-server/src/routes/periodic.rs:329-337` validates scope **against
  the caller**, not the task owner.
- The scheduler trigger at `cbsd-rs/cbsd-server/src/scheduler/trigger.rs:60-127`
  then uses `task.created_by` for the user identity, `signed_off_by`, and the
  channel-scope check, but the **descriptor body** is whatever the attacker
  wrote.
- The same hole exists for `delete_task`
  (`cbsd-rs/cbsd-server/src/routes/periodic.rs:449-485`),
  `enable_task`/`disable_task`
  (`cbsd-rs/cbsd-server/src/routes/periodic.rs:491-572`).
- `cbsd-rs/cbsd-server/src/db/seed.rs:93-135` only assigns `periodic:*` caps via
  the `admin` role (with `*` wildcard), but the permissions/role-management API
  allows operators to create custom roles that grant `periodic:manage` without
  the wildcard. That is the realistic path for non-admin holders to exist.

Impact: an attacker with `periodic:manage` can:

1. Locate a periodic task owned by a higher-privileged user (e.g. an admin who
   scheduled a nightly release build).
2. PUT a new `descriptor` that points at attacker-controlled components/repos.
3. When the cron fires, the build is submitted as the original owner: their
   `signed_off_by`, their channel scope, their image destination namespace.
   Build provenance is fully corrupted and the attacker's payload runs under the
   higher-privileged identity's authorization context.

Repository scope is **not** re-checked at trigger time (`scheduler/trigger.rs`
only verifies channel scope via `resolve_and_rewrite`), so a `repo`-override
added by the attacker proceeds even if the original task owner lacked that
repository scope.

Recommendation:

- Enforce `task.created_by == user.email || user.has_cap("*")` on every periodic
  task mutation route.
- At trigger time, re-run the full scope-validation set against the task owner
  before submitting the build (or accept the design's position that updates
  require equivalent scopes and reject any update that the caller could not
  perform as a `submit_build`).
- Add tests that simulate a non-wildcard `periodic:manage` user rewriting
  another user's task.

### F5 — CLI login redirect places PASETO token in the query string

| Field          | Value                                                                                                                           |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Severity       | **High**                                                                                                                        |
| Attacker model | Anyone with read access to the server's tracing/access logs (operators, log-aggregation pipelines, log-shipping infrastructure) |
| New            | Yes                                                                                                                             |

Evidence:

- `cbsd-rs/cbsd-server/src/routes/auth.rs:319-327`:

  ```rust
  let token_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
      .encode(raw_token.as_bytes());
  // ...
      // CLI: redirect to UI with token in fragment for copy-paste.
      // The fragment is client-side only — never sent to the server.
      let redirect_url = format!("/?cli-token={token_b64}");
  ```

  The comment claims a fragment is used; the code uses a query string
  (`?cli-token=`). The browser follows the redirect with a GET to
  `/?cli-token=...`.

- `cbsd-rs/cbsd-server/src/app.rs:97-110` wraps the router in a `TraceLayer`
  whose `make_span_with` callback logs `uri = %request.uri()` for every request
  — query string included.
- `axum::http::header::AUTHORIZATION` is marked sensitive via
  `SetSensitiveRequestHeadersLayer` at `cbsd-rs/cbsd-server/src/app.rs:132-133`,
  but query strings are not subject to that filter. There is no axum/tower
  middleware in the stack that scrubs the URI of `cli-token` values.

Impact: every CLI login leaves a complete base64-encoded PASETO token in the
server's structured INFO-level request logs. Anyone with log read (the
legitimate population is large — operators, SREs, the log-aggregation/SIEM
pipeline) can replay that token for up to `max-token-ttl-seconds` (default: 6
months). PASETO tokens are also stored hashed in `tokens` table with
`expires_at` matching that TTL, so the server-side revocation list will accept
them as valid.

Recommendation:

- Use the URL fragment as the comment intended:
  `format!("/#cli-token={token_b64}")`. Fragments are not sent with the HTTP
  request line and never appear in server logs.
- Add a regression test that asserts the redirect target starts with `/#` for
  `client=cli` and `/` for `client=web`.
- Audit the rest of the codebase for similar comment-vs-code drift ("fragment"
  or "never sent to the server" comments).
- For extra defense in depth, scrub `cli-token` from the URI in the
  `make_span_with` closure even after the fix lands.

### F6 — Server binary has no TLS support; all bearer tokens are HTTP

| Field          | Value                                                                                                                                       |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity       | **High** (Medium if the deployment doc unambiguously requires a TLS-terminating reverse proxy in front; no such enforcement exists in code) |
| Attacker model | On-path attacker between any client (browser, `cbc`, worker) and the server when TLS termination is missing or misconfigured upstream       |
| New            | Yes                                                                                                                                         |

Evidence:

- `cbsd-rs/cbsd-server/src/config.rs:33-38` declares `tls_cert_path` and
  `tls_key_path` but tags them `#[allow(dead_code)]`.
- `cbsd-rs/cbsd-server/src/main.rs:289-295` calls `axum::serve(listener, ...)` —
  plain TCP, no `axum_server::bind_rustls`, no `tokio-rustls` integration in
  `Cargo.toml`.
- `cbsd-rs/cbsd-server/src/main.rs:159` sets the session cookie's `Secure` flag
  based on `!config.dev.enabled`. The flag is honored by the cookie attribute,
  but the server itself never serves over TLS, so cookies still travel over
  plaintext unless a reverse proxy is doing TLS termination — and the binary has
  no way to detect or enforce that.

Impact: every PASETO token, API key, robot token, OAuth callback, session
cookie, and worker bearer authorization is sent in cleartext unless an operator
independently configures TLS termination upstream. There is no startup check or
warning that informs the operator they are running on plaintext HTTP.

Recommendation:

- Either implement TLS termination in-binary via `axum-server` + `rustls`,
  **or** make TLS-fronting explicit:
  - Reject `with_secure(true)` cookies when the server's bind address is not
    loopback and no `X-Forwarded-Proto: https` is detected (configurable).
  - Document a hard requirement in `cbsd-rs/README.md` and in the deployment
    doc.
  - Emit a startup `warn!` when listening on a non-loopback address without TLS.

### F7 — Worker tar unpack permits symlinks; no decompression-size cap

| Field          | Value                                                                                                                                                |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Severity       | **Medium** (defense-in-depth — bytes are validated by SHA-256 before unpack and the SHA comes from a trusted server, but the trust chain is fragile) |
| Attacker model | Compromised cbsd-server, or a maliciously-prepared `components_dir` on the server, against the cbsd-worker process                                   |
| New            | Yes                                                                                                                                                  |

Evidence:

- `cbsd-rs/cbsd-worker/src/build/component.rs:79-84` calls
  `tar::Archive::unpack(&unpack_dir)` directly. The `tar` crate version is `0.4`
  (`cbsd-rs/cbsd-worker/Cargo.toml:22`) and filters paths with `..` and absolute
  roots, but it does **not** by default reject symlinks/hardlinks whose targets
  point outside the unpack root, and it does not enforce a decompression cap.
- No `take` or size budget wraps the
  `flate2::read::GzDecoder::new(tarball_bytes)` decoder
  (`cbsd-rs/cbsd-worker/src/build/component.rs:80`). A 10 MiB gzip can
  decompress to gigabytes.
- The SHA-256 check at `cbsd-rs/cbsd-worker/src/build/component.rs:64-73`
  authenticates that the bytes match what the server intended to send. It does
  **not** authenticate that the server-side components_dir is benign.
- Server-side packing at `cbsd-rs/cbsd-server/src/components/tarball.rs:30-49`
  uses `tar::Builder::append_dir_all` which (in `tar 0.4` defaults) follows
  symlinks when reading the source tree — so an attacker who can place a symlink
  in `components_dir/<name>/` causes the server to embed the symlink target in
  the tarball.

Impact: in the current trust model the cbsd-server is fully trusted by the
cbsd-worker, so this is defense-in-depth. The configuration matters: if
`components_dir` ever holds attacker- writable content (e.g. populated from a
Git repo with weak review), a malicious component can:

1. Symlink `payload -> /etc/shadow` on the server filesystem at the time of
   `pack_component`; the server reads `/etc/shadow` and ships it to the worker.
2. Symlink at unpack time on the worker that overwrites a file outside the
   unpack dir.
3. Ship a decompression bomb that exhausts worker memory or disk.

Recommendation:

- Use `tar::Entry::header().entry_type()` to reject symlinks, hardlinks, and
  device files during unpack. Iterate entries manually instead of calling
  `Archive::unpack`.
- Cap decompression: read up to a config-controlled budget (e.g. 512 MiB) via
  `Read::take` before passing to the tar parser, and abort with a clear error.
- On the server side, walk the source tree and refuse symlinks during
  `append_dir_all` (or use `follow_symlinks(false)` on the `Builder`).

### F8 — No request body, WebSocket message, or log-line size limits

| Field          | Value                                                                                                                             |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Severity       | **Medium**                                                                                                                        |
| Attacker model | Any authenticated user (REST submit), any authenticated registered worker (WS messages), or any subprocess that the worker spawns |
| New            | Yes                                                                                                                               |

Evidence:

- `cbsd-rs/cbsd-server/src/app.rs:83-92` builds the router with no
  `axum::extract::DefaultBodyLimit` configuration. The framework default (2 MiB)
  is applied per-extractor and only on Json/Form bodies; it is easy to opt out
  of and provides no enforcement on alternative paths.
- `grep` over `cbsd-server/src/ws/` shows no `WebSocketConfig` with
  `max_message_size` or `max_frame_size`; the `tokio-tungstenite` default
  (`64 MiB` per message) applies. A malicious worker can send a 64 MiB
  `BuildOutput` and the server will allocate accordingly inside the WS receiver.
- `cbsd-rs/cbsd-server/src/logs/sse.rs:201-231` reads log lines with
  `BufReader::read_line` and no per-line bound. The same unbounded reading
  appears on the worker at `cbsd-rs/cbsd-worker/src/build/output.rs:91-103`. A
  build subprocess that emits a 1 GiB line without newline will OOM the worker;
  the worker will then forward the line to the server (subject only to the WS
  message size cap above).
- `cbsd-rs/cbsd-server/src/routes/builds.rs:475-492` and `:495` (`logs_tail`)
  read the **entire log file** into memory before slicing the tail. This was the
  prior review's Low finding and is still unfixed.

Impact: a single authenticated worker can trivially cause the server to allocate
tens of megabytes per WS frame; sustained traffic can exhaust server memory. A
REST caller can probably submit very large descriptors with embedded unbounded
strings (no `max_len` on `BuildDescriptor` fields in
`cbsd-rs/cbsd-proto/src/build.rs`).

Recommendation:

- Pin a router-level `DefaultBodyLimit::max(64_000)` (or similar) on REST
  routes; raise per-route where needed.
- Use a `WebSocketUpgrade::max_frame_size(...)` / `max_message_size(...)` in
  `ws::handler::ws_upgrade`.
- Use `AsyncBufReadExt::take(N).read_line` or `read_until(b'\n', limit)` to
  bound log line size on both worker output (1 MiB cap recommended) and server
  SSE read.
- Replace `tokio::fs::read_to_string` in `logs_tail` with a reverse line scan
  over a bounded budget (the prior review's recommendation).
- Add per-field length caps in `cbsd_proto::BuildDescriptor` via custom
  deserialization or in the `submit_build` handler.

### F9 — Worker `python3` path resolves through `$PATH`

| Field          | Value                                                                                                    |
| -------------- | -------------------------------------------------------------------------------------------------------- |
| Severity       | **Low**                                                                                                  |
| Attacker model | Local attacker with the ability to write to a directory listed earlier in `$PATH` than the system Python |
| New            | Yes                                                                                                      |

Evidence:

- `cbsd-rs/cbsd-worker/src/build/executor.rs:141` invokes
  `Command::new("python3")`. The OS resolves the binary through `$PATH`.
- The systemd unit and container files do not (as audited) pin `PATH=` to a
  known-clean value; the worker inherits whatever the operator's environment
  provides.

Impact: standard PATH injection. Requires the attacker to already have a local
foothold, so this is hardening rather than a primary exposure.

Recommendation: invoke a fully-qualified interpreter path (`/usr/bin/python3`)
or accept the interpreter path as a config field with the same validation as
`cbscore_wrapper_path`.

### F10 — `api_keys.key_prefix` has no standalone index

| Field          | Value                                                                 |
| -------------- | --------------------------------------------------------------------- |
| Severity       | **Low**                                                               |
| Attacker model | Any unauthenticated attacker who can send bearer tokens to the server |
| New            | Yes                                                                   |

Evidence:

- `cbsd-rs/migrations/001_initial_schema.sql` defines
  `UNIQUE (owner_email, key_prefix)` on `api_keys` but no index on `key_prefix`
  alone. SQLite cannot use a composite UNIQUE index for a query whose `WHERE`
  clause leads with the second column.
- `cbsd-rs/cbsd-server/src/db/api_keys.rs:60-72` (`find_api_keys_by_prefix`)
  issues `WHERE key_prefix = ? AND revoked = 0` — a full table scan.
- For contrast, `migrations/007_robot_accounts.sql` does declare
  `CREATE INDEX idx_robot_tokens_prefix ON robot_tokens(token_prefix)`, so robot
  tokens are O(log n) lookup. API keys are O(n).

Impact: the prefix-enumeration **timing** attack is well-defended by the
dummy-Argon2 sentinel at
`cbsd-rs/cbsd-server/src/auth/token_cache.rs:42-45,300-310`. The remaining
concern is DoS scaling: as the `api_keys` table grows (every web login does
**not** create a row — only explicit `api-keys` POST does, so growth is bounded
by user behavior), every bearer authentication burns one full table scan.

Recommendation: add `CREATE INDEX idx_api_keys_prefix ON api_keys(key_prefix);`
in a new migration. Cheap to fix.

### F11 — `cbc` accepts `http://` server URLs; saves config file with a TOCTOU window

| Field          | Value                                                                                                                                 |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Severity       | **Low / Medium** (Medium if used against an `http://` server, Low against `https://` with TOCTOU only)                                |
| Attacker model | (a) On-path attacker against a `cbc` user who configured `http://…`. (b) Local attacker with read access to the user's home directory |
| New            | Yes                                                                                                                                   |

Evidence:

- `cbsd-rs/cbc/src/client.rs:217-227` parses the host URL via `Url::parse`
  without enforcing `https`. An operator who configures
  `host: "http://cbs.example.com"` will send bearer tokens in cleartext.
- `cbsd-rs/cbc/src/client.rs:29-44` accepts a `no_tls_verify` boolean from the
  CLI and forwards it to
  `reqwest::Client::builder().danger_accept_invalid_certs(...)`. No
  warn-on-stderr, no record of the choice in the config file.
- `cbsd-rs/cbc/src/config.rs:49-71` (`Config::save`): writes the JSON file first
  (`std::fs::write`, which uses the umask-derived default mode, typically
  `0644`), then chmods to `0o600`. Between those two syscalls the token is
  briefly readable by every local user.

Impact:

- (a) When users configure plain HTTP, bearer tokens (PASETO, potentially API
  keys when generated and copied) leak to the network on every request.
- (b) Local users who win the race after `write` and before `set_permissions`
  can read the freshly-written token.

Recommendation:

- Default `cbc login` to require `https://`. Allow `http://` only with an
  explicit `--insecure` flag and warn loudly on stderr.
- Persist the config file via an atomic create flow: open with
  `O_CREAT | O_EXCL`, mode `0o600`, write, fsync, rename into place. The
  `tempfile` crate (`NamedTempFile` + `persist`) handles this idiomatically.

### F12 — Dev OAuth bypass accepts arbitrary `dev_email`

| Field          | Value                                                                                                                          |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Severity       | **Low / Informational** (dev-mode is intentionally permissive, but the design comments imply it is restricted to `seed_admin`) |
| Attacker model | Anyone with HTTP access to a `cbsd-server` running with `dev.enabled = true`                                                   |
| New            | Yes                                                                                                                            |

Evidence:

- `cbsd-rs/cbsd-server/src/routes/auth.rs:170-179` (`login`): in dev mode it
  redirects to the callback with `dev_email = seed_admin`.
- `cbsd-rs/cbsd-server/src/routes/auth.rs:228-231` (`callback`): in dev mode it
  accepts the **`dev_email` query parameter from the request**, with no check
  that it matches the configured `seed_admin`. Any client can pass any value.
- A user row is then created on demand at
  `cbsd-rs/cbsd-server/src/routes/auth.rs:272-289`, attaching whatever effective
  caps the email already has — which is none for a freshly invented address.

Impact: dev mode is intentionally an auth bypass, but the **implementation lets
any callback set any `dev_email`**, which means in dev a tester can authenticate
as `victim@example.com` without anyone noticing. New rows get default zero caps
so the escalation potential is limited, but the surprise factor is real.

Recommendation: in dev mode, reject any `dev_email` that does not exactly equal
`config.seed.seed_admin`. The redirect from `login` already supplies the right
value — the callback should mirror that constraint.

### F13 — Bearer token logged via `tracing` debug

| Field          | Value                                                                       |
| -------------- | --------------------------------------------------------------------------- |
| Severity       | **Informational**                                                           |
| Attacker model | Anyone with read access to debug-level logs (developers, CI, log pipelines) |
| New            | Yes                                                                         |

Evidence:

- `cbsd-rs/cbsd-server/src/auth/extractors.rs:273-277` (debug-level) and
  `:213-219` (warn-level on decode failure) log
  `token_prefix = &token_str[..token_str.len().min(20)]`. PASETO bodies are
  encrypted so 20 chars of `v4.local.<base64>` leak only the version label; for
  `cbsk_…` and `cbrk_…` the first 17 chars include the literal prefix (`cbsk_` +
  12 hex chars) used by the DB lookup, plus three more. That is the same prefix
  the prefix-enumeration sentinel is defending against — printing it at debug
  level partly negates the timing-parity work.

Impact: informational only — a debug-log reader can list existing API-key
prefixes, which is the precondition the timing-parity sentinel was designed to
deny. Still bounded by log-access and Argon2-on-the-rest, so not a direct
compromise.

Recommendation: drop `token_prefix` from the debug log entirely, or restrict it
to the first 5 chars (`cbsk_` literal, no random material). The warn-on-reject
case can keep the truncated prefix because reject-by-format never reveals real
prefix data.

## Positive Observations

Defenses that work as intended:

- **Session cookie hardening** — `cbsd-rs/cbsd-server/src/main.rs:153-162` sets
  `HttpOnly`, `SameSite=Lax`, `Secure` (outside dev mode), `Path=/`, signed key
  derived via HKDF from the token secret, 10-minute base TTL with web-session
  bump to 7 days. Session ID is cycled after OAuth completes
  (`cbsd-rs/cbsd-server/src/routes/auth.rs:314-317`) which prevents
  session-fixation.
- **PASETO revocation list** — every issued token is hashed and stored in
  `tokens`; unknown hashes are treated as revoked at
  `cbsd-rs/cbsd-server/src/auth/extractors.rs:230-247`. This is the right
  defense against arbitrary PASETO replay.
- **Timing-parity sentinel** —
  `cbsd-rs/cbsd-server/src/auth/token_cache.rs:42-45` and `:300-310` keep Argon2
  verify cost on the no-match path. Combined with prefix-indexed lookup for
  robot tokens, this is a solid prefix-enumeration defense (the missing api-keys
  index is separate — see F10).
- **Robot cap stripping** —
  `cbsd-rs/cbsd-server/src/auth/extractors.rs:61-78,194-199` defines
  `ROBOT_FORBIDDEN_CAPS` and strips at auth time, with a defense-in-depth
  assignment-time reject elsewhere. The `audit_identity_lint` regression test at
  `cbsd-rs/cbsd-server/src/routes/audit_identity_lint.rs` ensures
  log-attribution code never embeds `user.email` where robots could pass
  through.
- **Last-admin guard** —
  `cbsd-rs/cbsd-server/src/routes/admin.rs:770-784,985,1127` and the inline
  guard at `:88-117` prevent deactivating the final wildcard holder inside a
  transaction.
- **`PRAGMA foreign_keys = ON`** — set per-connection at
  `cbsd-rs/cbsd-server/src/db/mod.rs:39-45`. The `tower-sessions-sqlx-store`
  reuses the same pool so it inherits the pragma.
- **sqlx compile-time queries** — virtually every query in the `db/` module uses
  `sqlx::query!` macros, eliminating string-concatenation SQL injection by
  construction. No dynamic-SQL surface was observed in this audit.
- **Robot tokens partial unique index** — `migrations/007_robot_accounts.sql`
  has `idx_robot_tokens_active ... WHERE revoked = 0` closing the
  concurrent-create race.
- **Robot display-name forgery guard** —
  `cbsd-rs/cbsd-server/src/routes/auth.rs:268-289` rejects OAuth sign-in where
  the Google display name starts with `robot:`, preventing identity confusion in
  audit logs.
- **`AUTHORIZATION` header marked sensitive** —
  `cbsd-rs/cbsd-server/src/app.rs:132-133` uses
  `SetSensitiveRequestHeadersLayer` so the header is redacted in tower-http
  traces. (Note: query strings are _not_ covered — see F5.)
- **Rate limiting on OAuth endpoints** —
  `cbsd-rs/cbsd-server/src/routes/auth.rs:39-67` applies tower-governor (10 req
  / 60 s / IP) to `/login`, `/callback`, `/logout`.
- **Subprocess process-group isolation** —
  `cbsd-rs/cbsd-worker/src/build/executor.rs:155-163` runs `setsid()` in
  `pre_exec` so SIGTERM/SIGKILL target the entire build process group, and the
  escalation timer at `:236-251` reliably reaps stuck builds.
- **SSE FD held open** — `cbsd-rs/cbsd-server/src/logs/sse.rs:73-83` opens the
  log file once and keeps the descriptor for the stream lifetime, which lets the
  GC unlink without breaking active followers (a deliberate design invariant per
  the project's CLAUDE.md correctness notes).
- **Drain shutdown** — `cbsd-rs/cbsd-server/src/main.rs:388-482` revokes active
  builds with timeout fallback to failure marking on SIGQUIT, preserving
  DB/queue consistency across decommission.

## Areas Reviewed

The following paths were read end-to-end:

- `cbsd-rs/cbsd-server/src/auth/{extractors,oauth,paseto,token_cache,mod}.rs`
- `cbsd-rs/cbsd-server/src/routes/{auth,builds,periodic,test_support,audit_identity_lint,mod}.rs`
  (excerpts of `admin.rs`, `robots.rs`, `workers.rs`, `permissions.rs`,
  `channels.rs`)
- `cbsd-rs/cbsd-server/src/ws/{handler,dispatch}.rs`
- `cbsd-rs/cbsd-server/src/logs/sse.rs`
- `cbsd-rs/cbsd-server/src/db/{mod,seed,api_keys}.rs` (sample of `users.rs`,
  `roles.rs`, `tokens.rs`, `robots.rs`, `builds.rs`, `periodic.rs`)
- `cbsd-rs/cbsd-server/src/{app,main,config}.rs`
- `cbsd-rs/cbsd-server/src/components/tarball.rs`
- `cbsd-rs/cbsd-server/src/scheduler/trigger.rs`
- `cbsd-rs/cbsd-worker/src/build/{executor,component,output}.rs`
- `cbsd-rs/cbsd-worker/src/ws/connection.rs`
- `cbsd-rs/cbsd-worker/src/config.rs`
- `cbsd-rs/cbc/src/{client,config}.rs`
- `cbsd-rs/cbsd-proto/src/build.rs`
- All seven SQL migration files under `cbsd-rs/migrations/`
- `cbsd-rs/scripts/cbscore-wrapper.py`

## Areas NOT Reviewed (coverage limits)

- `cbsd-rs/cbsd-server/src/ws/liveness.rs` (worker liveness state machine —
  assumed to behave as documented in design 019)
- `cbsd-rs/cbsd-server/src/queue/recovery.rs` startup recovery — only checked
  that it is invoked; behavior not audited.
- `cbsd-rs/cbsd-server/src/logs/{writer,gc}.rs` — log write path was inspected
  only enough to corroborate the prior review's output-spoof finding.
- `cbsd-rs/cbsd-server/src/db/{roles,users,workers,channels,robots,tokens,periodic,builds}.rs`
  in full (only excerpts looked at).
- `cbsd-rs/cbc/src/admin/*.rs` — CLI admin surface only briefly scanned.
- `cbsd-rs/ui/` (web UI assets) — out of scope per task.
- `cbsd-rs/cbsd-server/src/routes/components.rs` — only confirmed it does not
  handle uploads (server reads components from the filesystem `components_dir`).
- The Python `cbsd/` codebase (explicit out-of-scope).

## Confidence Score

Starting score 100. Each distinct new finding above is a separate deduction; the
prior review's four open findings are also counted, since the design 019 v11
that would address them is not yet implemented. Score floors at 0 — the table
below reports the math honestly even when the running total goes below zero.

| Item                                                                                | Points   | Description                                                                                                              |
| ----------------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------ |
| Starting score                                                                      | 100      |                                                                                                                          |
| D7: prior review — worker lifecycle messages lack ownership                         | -20      | Unchanged from prior review; design 019 v11 not implemented.                                                             |
| D7: prior review — worker log output lacks ownership                                | -20      | Unchanged.                                                                                                               |
| D7: prior review — empty component list accepted                                    | -20      | Unchanged.                                                                                                               |
| D5: prior review — websocket ownership tests missing                                | -15      | Unchanged.                                                                                                               |
| D3: prior review — log tail reads full file                                         | -5       | Unchanged.                                                                                                               |
| D9: prior review — tarball pack failure lacks rollback                              | -5       | Unchanged.                                                                                                               |
| D7: F1 — worker `CBSD_DEV` truthiness disables TLS verification                     | -20      | Critical net-attacker → key compromise.                                                                                  |
| D7: F2 — OAuth callback does not verify `verified_email`                            | -20      | High auth-bypass with allowed-domain set.                                                                                |
| D8: F2 — OpenID Connect spec deviation (`email_verified` requirement)               | -5       | Concurrent with D7.                                                                                                      |
| D7: F3 — `worker_status(Building)` reconnect rewrites ownership                     | -20      | High; same class as prior review but unaddressed in design's implemented state.                                          |
| D7: F4 — periodic-task descriptor privilege transfer                                | -20      | High; descriptor injection to fire under another user's identity.                                                        |
| D7: F5 — CLI login query-string leaks token to request logs                         | -20      | High; bearer in INFO logs for the full token TTL.                                                                        |
| D10: F5 — comment ("fragment") contradicts code ("?cli-token=")                     | -5       | Concurrent convention violation.                                                                                         |
| D7: F6 — server has no TLS; bearers ride plaintext unless reverse-proxied           | -20      | High; deployment-dependent but enforcement is absent.                                                                    |
| D7: F7 — tar unpack allows symlinks and has no decompression cap                    | -20      | Medium-as-defense-in-depth but D7 applies because the validation gap is on a security-relevant path.                     |
| D7: F8 — no JSON / WS / log-line size limits                                        | -20      | DoS via authenticated WS or REST.                                                                                        |
| D4: F9 — `python3` resolved through `$PATH`                                         | -5       | Low; PATH-injection hardening.                                                                                           |
| D3: F10 — `api_keys.key_prefix` lacks standalone index                              | -5       | DB index missing; lookups are O(n).                                                                                      |
| D7: F11 — `cbc` accepts `http://` URLs and has a config-file TOCTOU window          | -20      | Bearer cleartext + briefly world-readable.                                                                               |
| D10: F11 — convention violation (file written then chmod'd, not atomic)             | -5       | Concurrent.                                                                                                              |
| D7: F12 — dev OAuth bypass accepts arbitrary `dev_email`                            | -20      | Low/Informational under D7 because dev mode is intentionally permissive, but a clear deviation from documented behavior. |
| D7: F13 — bearer token prefix logged at debug                                       | -20      | Informational; partial negation of timing-parity defense via log access.                                                 |
| **Subtotal (would be)**                                                             | **-230** |                                                                                                                          |
| **Floor (per scoring rubric: "score floors at 0 — do not report negative scores")** | **0**    |                                                                                                                          |
| **Total reported**                                                                  | **0**    |                                                                                                                          |

Interpretation per the rubric: `0–49 → Major rework needed. Block merge.` The
score is at the floor; the math is well below the floor. This is **not** a
calibration error — the codebase carries the prior review's four unfixed
findings plus eight new High/Critical findings and several Medium/Low ones. The
floor is the message.

## Go / No-Go

**NO-GO for production deployment.**

Before this stack can be considered for any production use, the following
changes are required (Critical/High blockers):

1. Fix the worker `CBSD_DEV` truthiness TLS bypass (F1). Same pattern fix
   everywhere `CBSD_DEV` is read.
2. Verify `verified_email` in the OAuth callback (F2).
3. Land the design 019 boundary on `worker_status(Building)` (F3) and ship the
   prior review's four unaddressed worker control-plane findings as well.
4. Add ownership enforcement on all periodic-task mutation routes (F4) and
   re-validate scope at trigger time against the task owner.
5. Move the CLI token from query string to fragment, and scrub any residual
   occurrences from `TraceLayer` URI logging (F5).
6. Pick a TLS posture (in-binary rustls termination or require-reverse-proxy
   with hard checks) and enforce it (F6).
7. Apply tar-unpack hardening + decompression caps on the worker (F7); apply
   size limits to REST bodies, WS frames, and log lines (F8).

After those, the remaining Medium/Low items (F9 – F13) should be ground out with
a small migration + small refactor pass and a single dedicated commit for the
cbc client.
