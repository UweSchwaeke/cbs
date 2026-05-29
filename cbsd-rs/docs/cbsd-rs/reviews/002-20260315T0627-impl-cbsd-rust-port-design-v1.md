# Implementation Review: cbsd-rs Commits 0–3 (Phase 0, Phase 1, Phase 2 partial)

**Commits reviewed:**

- `c158ee4` — Phase 1 Commit 1: cbsd-proto crate with shared types
- `68c35b2` — Phase 1 Commit 2: SQLite schema, server scaffold, config loading
- `2ac5383` — Phase 2 Commit 3: PASETO tokens, user/token DB, AuthUser extractor

**Evaluated against:**

- Design documents in `cbsd-rs/docs/cbsd-rs/design/`
- Implementation plans in `cbsd-rs/docs/cbsd-rs/plans/`

---

## Summary

The implementation is high quality and closely tracks the design documents and
plans. All four commits compile, the schema matches the design with zero
divergences, the PASETO token implementation correctly freezes the canonical
payload form, and the AuthUser extractor follows the right check sequence
(decode → revocation → active → user load). One minor plan deviation (missing
`config.rs` in cbsd-proto) and several idiomatic improvements are noted. No
blockers.

**Verdict: Implementation is sound. Proceed to Commit 4.**

---

## Per-Commit Assessment

### Commit 0 (`331fdc5`) — Phase 0: CLAUDE.md + Plans

**Plan compliance: Complete.**

All Phase 0 requirements fulfilled:

- `cbsd-rs/CLAUDE.md` contains all 7 correctness invariants, skill references,
  build commands, git conventions, architecture pointers, and sqlx offline cache
  instructions
- All 7 phase plan files created with progress tracking tables
- Plan README with dependency graph and status table

No issues.

---

### Commit 1 (`c158ee4`) — Phase 1: cbsd-proto crate

**Plan compliance: 95%. One deviation.**

All shared types are correctly implemented:

- `Arch` enum with `arm64` serde alias ✓
- `BuildDescriptor` preserving Python nesting (version, channel, version_type,
  signed_off_by, dst_image, components[], build.BuildTarget) ✓
- `BuildState` (7 states), `Priority`, `BuildId(i64)` newtype ✓
- All WS messages (4 ServerMessage variants, 8 WorkerMessage variants) ✓
- `Welcome` includes `grace_period_secs` ✓
- `BuildOutput` uses `start_seq` (per-line seq) ✓
- `BuildFinished.error` uses `skip_serializing_if = "Option::is_none"` ✓
- `WorkerStatus.build_id` similarly ✓
- 20 serde round-trip tests ✓

**Deviation: `config.rs` missing from cbsd-proto.**

The plan specifies: "`config.rs` — Shared config types (server URL, TLS CA
bundle path)." This file was not created. The shared config types will likely be
needed when the worker crate is implemented (Commit 10).

Severity: Low. The types can be added in a later commit without retroactive
changes. The worker crate is a stub at this point.

**Code quality notes:**

- `BuildDescriptor::registry_host()` helper is a nice addition not required by
  the plan — useful for registry scope extraction in Commit 5.
- `BuildComponent.git_ref` correctly uses `#[serde(rename = "ref")]` to match
  the Python JSON field name.
- `BuildTarget` defaults (`artifact_type = "rpm"`, `arch = X86_64`) match the
  design doc exactly.

---

### Commit 2 (`68c35b2`) — Phase 1: Schema, server scaffold, config

**Plan compliance: Complete.**

**Schema (001_initial_schema.sql):** All 9 tables present and correct (users,
tokens, api_keys, roles,

role_caps, user_roles, user_role_scopes, builds, build_logs).

Key schema elements verified against design doc:

- `builds.descriptor_version INTEGER NOT NULL DEFAULT 1` ✓
- `builds.trace_id TEXT` (nullable) ✓
- `builds.queued_at INTEGER NOT NULL DEFAULT (unixepoch())` ✓
- `api_keys UNIQUE(name, owner_email)` ✓
- `api_keys UNIQUE(owner_email, key_prefix)` ✓
- `user_role_scopes CHECK(scope_type IN ('channel','registry','repository'))` ✓
- `builds.state CHECK(...)` includes all 7 states including `revoking` ✓
- All `ON DELETE CASCADE` FKs correct ✓
- All 4 indexes: `idx_tokens_user`, `idx_builds_state`, `idx_builds_user`,

  `idx_builds_state_queued` ✓

- All timestamps are `INTEGER` (Unix epoch) ✓

**Zero schema divergences from design doc.**

**Server scaffold:**

- `create_pool()` sets all 4 pragmas (WAL, foreign_keys=ON, busy_timeout=5000,
  synchronous=NORMAL) per-connection ✓
- `max_connections = 4` (deadlock prevention) ✓

- `create_if_missing(true)` on SqliteConnectOptions ✓
- `tower-sessions-sqlx-store` initialized with `.migrate().await` ✓
- 10-minute session TTL ✓
- Expired session deletion background task ✓
- `GET /api/health` returns `{"status": "ok"}` ✓

**Config loading:**

- All config fields from design present ✓
- Validation: `allowed_domains` empty guard ✓
- Validation: `backoff_ceiling >= grace_period` guard ✓
- `--drain` CLI flag present ✓ **Shutdown signal handling:**

- SIGTERM, SIGQUIT, Ctrl+C all handled ✓
- Signal-specific log messages ✓
- Correct `#[cfg(unix)]` guards ✓

**sqlx offline cache:** Not committed (`.sqlx/` absent). This is **acceptable**
because Commit 2 contains no `sqlx::query!` macros — only

pool setup and migration embedding. The cache is needed starting Commit 3.

---

### Commit 3 (`2ac5383`) — Phase 2: PASETO, user/token DB, AuthUser

**Plan compliance: Complete.**

**PASETO implementation (`auth/paseto.rs`):**

- `CbsdTokenPayloadV1` with frozen field order: `expires` then `user`
  (alphabetical) ✓

- Fields: `expires: Option<i64>`, `user: String` (epoch integers) ✓
- Canonical JSON verified by test:
  `{"expires":1710412200,"user":"alice@clyso.com"}` ✓
- Null expires test: `{"expires":null,"user":"alice@clyso.com"}` ✓
- SHA-256 hash via `sha2::Sha256::digest()`, hex-encoded ✓
- `max_token_ttl_seconds` clamping ✓

- `token_decode` validates expiry after decryption ✓
- Wrong-key rejection tested ✓
- 8 well-targeted tests ✓

**User DB operations (`db/users.rs`):**

- `create_or_update_user()` with `ON CONFLICT DO UPDATE` ✓
- `get_user()` returns `Option<UserRecord>` ✓
- `is_user_active()` treats missing user as inactive ✓

**Token DB operations (`db/tokens.rs`):**

- `insert_token()` with `last_insert_rowid()` return ✓
- `revoke_token()` with `revoked = 0` idempotency guard ✓
- `revoke_all_for_user()` with count return ✓

**AuthUser extractor (`auth/extractors.rs`):**

- Reads `Authorization: Bearer` header ✓
- Distinguishes PASETO vs API key by `cbsk_` prefix ✓
- API key path returns "not yet implemented" (correct for Commit 3) ✓
- Token decode → revocation check → active check → user load ✓
- Error response: `{"detail": "..."}` matching FastAPI convention ✓
- DB errors → 500, auth failures → 401 ✓

---

## Code Quality & Idiomatic Review

### ~~Issue 1 — FALSE POSITIVE (retracted)~~

~~Originally flagged as "dead `hex` module shadows `hex` crate." Verified: there
is no `hex` crate in any `Cargo.toml`. The manual `mod hex` in `paseto.rs` is
the sole hex implementation and is intentional — the module doc explicitly says
"avoids external `hex` crate dependency." This is the correct approach for two
trivial functions.~~

### Issue 2 — `BuildState::Display` uses serde_json round-trip (build.rs:57–63)

```rust
impl std::fmt::Display for BuildState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        write!(f, "{s}")
    }
}
```

This allocates a `serde_json::Value` and a `String` on every `Display` call just
to get the lowercase variant name. A simpler approach:

```rust
impl std::fmt::Display for BuildState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::Dispatched => write!(f, "dispatched"),
            Self::Started => write!(f, "started"),
            Self::Revoking => write!(f, "revoking"),
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
            Self::Revoked => write!(f, "revoked"),
        }
    }
}
```

Zero allocations, no serde dependency in the Display path, and the match arms
make the lowercase mapping explicit rather than relying on serde's `rename_all`.
This is a minor quality nit — the current code is correct.

### Issue 3 — `get_user` and `is_user_active` can be consolidated (users.rs)

`is_user_active` (lines 56–63) runs a separate SQL query to check the `active`
flag. But `get_user` (lines 42–53) already fetches `active` as part of
`UserRecord`. In the AuthUser extractor, both are called sequentially:

```rust
let active = db::users::is_user_active(&state.pool, &payload.user).await...;
// ...
let user = db::users::get_user(&state.pool, &payload.user).await...;
```

This executes two SQL queries for the same user. A single `get_user` call could
serve both purposes:

```rust
let user = db::users::get_user(&state.pool, &payload.user).await...?
    .ok_or_else(|| auth_error(StatusCode::UNAUTHORIZED, "user not found"))?;
if !user.active {
    return Err(auth_error(StatusCode::UNAUTHORIZED, "user account deactivated"));
}
```

This halves the DB round-trips in the auth hot path. The `is_user_active`
function may still be useful elsewhere (e.g., the last-admin guard in Commit 5),
so it doesn't need to be removed — just not used in the extractor.

### Issue 4 — `SeedConfig::Default` is manually implemented but could use `#[serde(default)]`

`SeedConfig` (config.rs:133–150) has a manual `Default` impl that returns `None`
/ empty `Vec`. Since both fields already have `Option` / `Vec` types,
`#[derive(Default)]` would produce identical behavior. The manual impl is not
wrong, just unnecessary boilerplate. Same applies to `LoggingConfig` — the
manual `Default` impl duplicates the `default_*` functions already used by
serde.

### Issue 5 — `TimeoutsConfig::Default` duplicates serde default functions

`TimeoutsConfig` has both `#[serde(default = "default_dispatch_ack_timeout")]`
on each field AND a manual `Default` impl that calls the same functions. The
`#[serde(default)]` at the struct level (already present on line 44 of
`ServerConfig`) means serde calls `TimeoutsConfig::default()` for missing
fields. The per-field `#[serde(default = "...")]` annotations are redundant when
the struct-level `#[serde(default)]` is also present on the parent. One or the
other suffices — having both is not wrong but adds maintenance surface.

### Issue 6 — No `.sqlx/` directory committed with Commit 3

Commit 3 introduces the first `sqlx::query()` calls (in `db/users.rs` and
`db/tokens.rs`). Per the plan and CLAUDE.md, any commit adding sqlx queries
should include the updated `.sqlx/` offline cache. The cache is absent.

This is not a correctness issue (the queries use `sqlx::query()` with string
SQL, not `sqlx::query!()` compile-time macros), so the build succeeds without
the cache. However, when compile-time checked queries are introduced in later
commits, the cache will be needed. The plan's bootstrap procedure should be
followed at the first commit that uses `sqlx::query!()` macros.

---

## Design Fidelity Summary

| Design requirement                                                    | Status | Commit |
| --------------------------------------------------------------------- | ------ | ------ |
| 7 correctness invariants in CLAUDE.md                                 | ✓      | 0      |
| BuildDescriptor preserves Python nesting                              | ✓      | 1      |
| `arm64` serde alias on Arch                                           | ✓      | 1      |
| `version_type` + `artifact_type` fields present                       | ✓      | 1      |
| `Welcome.grace_period_secs`                                           | ✓      | 1      |
| `BuildOutput.start_seq` (per-line seq)                                | ✓      | 1      |
| All 9 tables with correct schema                                      | ✓      | 2      |
| All 4 pragmas (WAL, FK, busy_timeout, synchronous)                    | ✓      | 2      |
| `max_connections = 4`                                                 | ✓      | 2      |
| `tower-sessions-sqlx-store` init with `.migrate()`                    | ✓      | 2      |
| `descriptor_version` + `trace_id` columns                             | ✓      | 2      |
| Config validation (domains, backoff ceiling)                          | ✓      | 2      |
| `--drain` CLI flag                                                    | ✓      | 2      |
| SIGTERM/SIGQUIT/Ctrl+C signal handling                                | ✓      | 2      |
| PASETO `CBSD_TOKEN_PAYLOAD_V1` frozen                                 | ✓      | 3      |
| SHA-256 of raw UTF-8 token string                                     | ✓      | 3      |
| `max_token_ttl_seconds` clamping                                      | ✓      | 3      |
| AuthUser: Bearer → prefix check → decode → revocation → active → load | ✓      | 3      |
| Error response `{"detail": "..."}`                                    | ✓      | 3      |
| `is_token_revoked` treats unknown as revoked                          | ✓      | 3      |

---

## Plan Progress

| Phase            | Plan Status        | Actual Status | Notes                                          |
| ---------------- | ------------------ | ------------- | ---------------------------------------------- |
| Phase 0 Commit 0 | Not started → Done | Done ✓        | Plan file updated                              |
| Phase 1 Commit 1 | Not started → Done | Done ✓        | Plan file updated. Missing `config.rs` (minor) |
| Phase 1 Commit 2 | Not started → Done | Done ✓        | Plan file updated. README status updated       |
| Phase 2 Commit 3 | Not started → Done | Done ✓        | Plan file updated                              |
