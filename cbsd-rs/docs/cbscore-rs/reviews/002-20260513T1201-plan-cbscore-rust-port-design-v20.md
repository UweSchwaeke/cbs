# cbscore-rs Plan Review v20 — C3 Closure Confirmation

## Scope

Confirmation review of commit `6cc553f` ("cbsd-rs/docs: cbscore-rs — close
pre-impl audit C3 findings (C3-1+C3-2+C3-3+C3-4)"). The commit claims to close
all four Category 3 ("unclear / ambiguous") findings from the pre-implementation
design-corpus audit before Phase 1 implementation starts. This pass verifies
each closure independently against the Python source, the design corpus, and the
plan corpus. No design or plan files are modified by this review.

Files verified:

- `cbsd-rs/docs/cbscore-rs/design/002-20260418T2120-cbscore-rust-port-design.md`
- `cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-06-cbsbuild-cli.md`

Reference source: `cbscore/src/cbscore/utils/secrets/models.py`

## Method

1. `grep` for surviving hedge language on all five files.
2. Field-for-field comparison of each Rust enum tree against the Python source.
3. Read the §Write site schematic in design 004 for the C3-3 closure.
4. Trace C3-4 (`StorageCreds`) propagation through all plan files that enumerate
   secret families.
5. No-drift spot-checks on high-value closures from v15–v19 and seq-004 v1/v2.
6. `prettier --check` on all five files.

---

## C3 Closure Verification

### C3-1 — Schema-version casing rule (design 002, plan 002-01/03/06)

**Result: Closed in the intended files. One residual instance in design 004
§OQ6.**

**Hedge language:** `grep` for `decide at impl time`, `either is correct`,
`kebab.*OR.*snake`, `snake.*OR.*kebab`, and `both casings valid` across
`design/002-*` and `design/004-*` returns zero hits. The hedge is gone.

**Rule statement (design 002 §Wire-Format Versioning §Interaction):** The
two-case rule is clearly stated at lines 417–435 with no residual "single choice
applies to all formats" language. The pinned rule is:

- Kebab-case structs (`Config`, `Secrets`, `VaultConfig`, `CoreComponent`) →
  wire key `schema-version` (kebab), achieved via
  `#[serde(rename = "schema-version")]`.
- Snake-case descriptor structs (`VersionDescriptor`, `ReleaseDesc`,
  `ImageDescriptor`, `ReleaseComponent`, `ContainerDescriptor`,
  `BuildArtifactReport`) → wire key `schema_version` (snake), as-is.

**Schematic (design 002 §Implementation pattern):** `VersionedConfig` uses
`#[serde(tag = "schema-version")]`. The doc-comment on `Config::load` names
`ConfigError::MissingSchemaVersion` on a missing kebab key; `Config::store` says
"emits `schema-version: 1` as the first key (kebab on the `Config` side)". The
post-pattern note at lines 397–404 enumerates all types and assigns kebab or
snake correctly.

**§Interaction bullets:** `schema-version: 1` used for `secrets.yaml` (kebab —
correct, `Secrets` is kebab). `BuildArtifactReport.report_version` renamed to
`schema_version` (snake — correct, descriptor family).

**Migration Strategy (design 002 §M1 end-state paragraph):** Lines 1403–1415
correctly distinguish `schema-version: 1` for kebab formats and
`schema_version: 1` for descriptor formats. The rolling-upgrade prose (lines
1444–1451) uses `schema-version: 1` for `secrets.yaml` — correct.

**Plan corpus propagation:**

- Phase 1 Commit 5 §Files: `VersionedConfig` tagged on `schema-version` (kebab);
  fixture-corpus bullet specifies `schema-version: 1` for kebab YAML formats and
  `schema_version: 1` for snake descriptor formats. Clean.
- Phase 1 Commit 5 §Design constraints: pinned rule with per-format guidance.
  Clean.
- Phase 1 Commit 5 §Testable: negative tests specify kebab `schema-version` for
  Config YAML and snake `schema_version` for `VersionDescriptor` JSON. Clean.
- Phase 3 Commit 4 §Design constraints: `Config::store` writes kebab
  `schema-version: 1`. Clean.
- Phase 3 Commit 4 §Testable: tests assert kebab `schema-version: 1` as the
  first key; negative tests use kebab form. Clean.
- Phase 6 Commit 4 §Design constraints: `config init` writes `schema-version: 1`
  (kebab, Config is kebab-case). Clean.

**Residual instance in design 004 §OQ6 (lines 174–188):** Three uses of
`schema_version` (snake) when referring to the on-wire Config value. Exact
occurrences:

```
line 174: `Config.schema_version` stays at 1
line 177: accumulates additions into `schema_version: 1` until M1 1.0.0 ships
line 184: Files written by cbscore-rs M1 carry `schema_version: 1` as today
line 188: bumps to `schema_version: 2` per the standing rule
```

The first occurrence (`Config.schema_version` as a Rust struct-field reference)
is technically correct — the Rust field is `schema_version` (snake) in both
cases, the rename only applies on the wire. However lines 177, 184, and 188 show
the value as it appears in the YAML file — that is a wire-key context, and
Config YAML carries `schema-version: 1` (kebab) per the C3-1 rule. These are
wire-value representations, not Rust field references, and they contradict the
rule design 002 now pins. The OQ6 section was written before the C3-1
clarification and was not updated by `6cc553f`.

This is a **Minor Issue** (MN-1): the text in design 004 §OQ6 is stale relative
to C3-1. It does not affect Phase 1 implementation — the authoritative rule
lives in design 002 — but will confuse a reader who starts with design 004 and
sees snake-cased Config wire values there.

### C3-2 — SigningCreds + RegistryCreds full shapes (design 002 §Secrets)

**Result: Closed. Field-for-field verified against Python source.**

**Hedge language:** `grep` for `exact variant shapes mirror` and
`confirm field-for-field at implementation time` across `design/002-*` returns
zero hits.

**SigningCreds — field-for-field against `models.py`:**

| Python class               | Rust variant                      | `type` wire value | Fields                                                                                                                | Match   |
| -------------------------- | --------------------------------- | ----------------- | --------------------------------------------------------------------------------------------------------------------- | ------- |
| `GPGPlainSecret`           | `SigningPlainCreds::GpgArmorKey`  | `gpg-armor-key`   | `private_key` (rename `private-key`), `public_key` (rename `public-key`, optional), `passphrase` (optional), `email`  | Correct |
| `GPGVaultSingleSecret`     | `SigningVaultCreds::GpgSingleKey` | `gpg-single-key`  | `key` (from `VaultSecret`), `private_key` (rename), `public_key` (rename, optional), `passphrase` (optional), `email` | Correct |
| `GPGVaultPrivateKeySecret` | `SigningVaultCreds::GpgPvtKey`    | `gpg-pvt-key`     | `key`, `private_key` (rename), `passphrase` (optional), `email`                                                       | Correct |
| `GPGVaultPublicKeySecret`  | `SigningVaultCreds::GpgPubKey`    | `gpg-pub-key`     | `key`, `public_key` (rename `public-key`), `email`                                                                    | Correct |
| `VaultTransitSecret`       | `SigningVaultCreds::Transit`      | `transit`         | `key`, `mount`                                                                                                        | Correct |

Discriminator pattern: outer `#[serde(tag = "creds")]` on `SigningCreds`, inner
`#[serde(tag = "type")]` on `SigningPlainCreds` and `SigningVaultCreds`. Matches
the Python two-level discriminator (outer `creds`, inner `type`). Correct.

`optional` fields use
`#[serde(default, skip_serializing_if = "Option::is_none")]`. This matches the
Python `default=None` semantics (absent on write when None, accepted as absent
on read). Correct.

**RegistryCreds — field-for-field against `models.py`:**

| Python class          | Rust variant           | `creds` wire value | Fields                                                        | Match   |
| --------------------- | ---------------------- | ------------------ | ------------------------------------------------------------- | ------- |
| `RegistryPlainSecret` | `RegistryCreds::Plain` | `plain`            | `username`, `password`, `address`                             | Correct |
| `RegistryVaultSecret` | `RegistryCreds::Vault` | `vault`            | `key` (from `VaultSecret`), `username`, `password`, `address` | Correct |

Single-level discriminator `#[serde(tag = "creds")]`. Correct.

No renames needed for `username`, `password`, `address` — these are not aliased
in Python. `key` is the `VaultSecret.key: str` base field, present in the Python
`RegistryVaultSecret` because it inherits from `VaultSecret`. Correct.

### C3-3 — `write_descriptor` mkdir-p (design 004 §Write site)

**Result: Closed.**

**Hedge language:** `grep` for `either is correct` and `OR the call site` across
`design/004-*` returns zero hits.

**§Write site schematic:** The call site (design 004 lines 340–343) uses
`cbscore::versions::desc::write_descriptor(&desc, &path).await?` exclusively.
The `create_dir_all` lives **inside `write_descriptor`** per the explicit prose
at lines 345–349: "The call site does **not** repeat the `mkdir -p`; future
callers … inherit the same parent-create-on-write contract without having to
know it." The contract is stated as pinned public API.

**seq-004 plan Commit 3 consistency:** The seq-004 plan
(`004-20260513T0900-configurable-version-descriptor-location.md`) Commit 3 calls
`write_descriptor` from the `versions create` write site (lines 63–68) and
explicitly notes the helper carries the `mkdir -p` semantic matching
`Config::store`. No raw `desc.write` + separate `create_dir_all_async` appears.
Clean.

### C3-4 — StorageCreds added (design 002 §Secrets)

**Result: Closed in design 002. Plan corpus not fully propagated — see MN-2 and
MN-3.**

**§Secrets preamble:** Line 530 now reads "four distinct families — **git**,
**storage**, **signing**, and **registry**". Four named. Clean.

**§Storage secrets subsection:** Present at line 592, placed between §Git and
§Signing as specified. The `StorageCreds` / `StoragePlainCreds` /
`StorageVaultCreds` enum trees are present.

**Field-for-field against `models.py`:**

| Python class           | Rust variant            | `type` wire value | Fields                                                                                         | Match   |
| ---------------------- | ----------------------- | ----------------- | ---------------------------------------------------------------------------------------------- | ------- |
| `StoragePlainS3Secret` | `StoragePlainCreds::S3` | `s3`              | `access_id` (rename `access-id`), `secret_id` (rename `secret-id`)                             | Correct |
| `StorageVaultS3Secret` | `StorageVaultCreds::S3` | `s3`              | `key` (from `VaultSecret`), `access_id` (rename `access-id`), `secret_id` (rename `secret-id`) | Correct |

Discriminator pattern: outer `#[serde(tag = "creds")]` on `StorageCreds`, inner
`#[serde(tag = "type")]` on `StoragePlainCreds` and `StorageVaultCreds`.
Consistent with the two-level GitCreds pattern; leaves room for future backends
(`s3` is a variant of `StoragePlainCreds`, not the type itself). Correct.

**Plan-corpus propagation — Phase 1 Commit 3 (MN-2):** The plan file
`002-20260508T1558-01-types.md` at line 229 says "the three credential
families", and lines 229–236 enumerate git, signing, registry — `StorageCreds`
is absent. The `cbscore-types::utils::secrets` module spec does not mention
`StorageCreds` as a type to implement in Phase 1 Commit 3.

**Plan-corpus propagation — Phase 3 Commit 3 (MN-3):** The plan file
`002-20260508T1558-03-storage-and-secrets.md`:

- Line 40 reads "the three credential families" in the Depends-on description.
- Line 218 shows the `Secrets` struct as
  `Secrets { git: Vec<GitCreds>, signing: Vec<SigningCreds>, registry: Vec<RegistryCreds> }`
  — `StorageCreds` is absent from the Rust struct definition.
- Lines 218–220 name `GitCreds`, `SigningCreds`, `RegistryCreds` as the
  serde-derived leaf types coming from `cbscore-types` — `StorageCreds` not
  mentioned.
- Line 233 in the `resolve_vault_refs` spec walks `GitVaultCreds` /
  `RegistryCreds::Vault` — `StorageVaultCreds::S3` vault resolution is not
  mentioned.

The design 002 §Secrets preamble also states the Rust `Secrets` struct uses four
`HashMap<String, FamilyCreds>` fields, while the plan shows Vecs. The
Vec-vs-HashMap discrepancy predates this commit. The three-vs-four discrepancy
is attributable to C3-4 not being propagated into these two plan sections.

Note: the C3-4 brief states: "verify by reading the Phase 3 Commit 3 spec and
confirming it references the secrets families without enumerating them (so 'git,
signing, registry' never appears as an exhaustive list in the plan)." That
condition is not met: `git, signing, registry` appears as an exhaustive
enumeration in Phase 3 Commit 3 both in the struct definition (line 218) and in
the Depends-on section (line 40).

---

## No-Drift Check

**Phase 7 §Subscriber layer:** Present and intact at plan 002-07 lines 171–200.
Per-build `tracing_subscriber::Layer` with `on_event`, mpsc channel, batching
task, `future.instrument(span)` for multi-build correctness, and the mandatory
tokio mechanism note — all unchanged.

**Phase 7 config.rs field disposition:** Present and intact at plan 002-07 lines
115–143. `cbscore_wrapper_path` removed, `cbscore_config_path` retained,
`sigkill_escalation_timeout_secs` removed — exact dispositions with rationale.
Unchanged.

**Phase 4 `RunReport.build_report`:** Plan 002-04 line 195 defines
`RunReport { …, build_report: Option<serde_json::Value>, … }`. Phase 7 Commit 1
notes (line 202–214) the field source and the Phase 4 amendment. Both intact.

**seq-004 `resolve.rs` placement:** seq-004 plan Commit 2 places
`cbscore/src/versions/resolve.rs` under `cbscore/src/versions/` (line 64 of that
plan), consistent with the design 004 §Resolver sketch. Unchanged.

**Design 005 warn-and-skip:** Present in design 005 §Design Sketch and
§Migration table step 4. The target literal `"cbscore::builder::prepare"` is
pinned at design 005 line 503 and review v8 confirms the literal against
design 002. Unchanged.

**README total estimate:** Line 23: "~25–31 commits across 7 phases." Unchanged.

**Previous v15–v19 closures:** No regressions detected. The §Wire-Format
Versioning §Interaction rewrite does not contradict any earlier-closed finding.
The §Secrets additions extend an existing section without altering the git-creds
two-level pattern or the wire-break rationale.

---

## Findings

### MN-1 — design 004 §OQ6 uses snake `schema_version` for Config wire values

**File:** `design/004-20260429T1319-configurable-version-descriptor-location.md`
**Lines:** 174, 177, 184, 188

Three of the four occurrences present `schema_version` in a wire-key context
(the value as it appears in the YAML file for a `Config`, which is a kebab-case
struct). Per the C3-1 rule now pinned in design 002, that wire key is
`schema-version` (kebab). Line 174 (`Config.schema_version` as a Rust field
reference) is technically correct since the Rust field is always snake; lines
177, 184, and 188 are wire-value representations that contradict the C3-1 rule.

Design 004 §OQ6 was authored before the C3-1 clarification and was not updated
by `6cc553f`. An implementer reading only design 004 would see the wrong wire
key for Config YAML.

**Resolution:** Update the three wire-value references in design 004 §OQ6 to
`schema-version` (kebab). Line 174 (`Config.schema_version` as a Rust field
name) may be left as-is or clarified with a parenthetical noting the wire key
differs.

### MN-2 — Phase 1 Commit 3 (plan 002-01) does not include `StorageCreds`

**File:** `plans/002-20260508T1558-01-types.md` **Lines:** 229 ("the three
credential families"), 229–236 (enumeration of git, signing, registry)

Phase 1 Commit 3 is the commit that lands all serde-derived leaf types in
`cbscore-types::utils::secrets`. It names only three families. An implementer
following this plan exactly would not implement `StorageCreds`,
`StoragePlainCreds`, or `StorageVaultCreds` in that commit, leaving Phase 3
Commit 3 (which imports the types) without the fourth family's types. This is a
direct consequence of C3-4 not being propagated into the Phase 1 plan.

**Resolution:** Update Phase 1 Commit 3 §Files to:

- change "the three credential families" to "the four credential families";
- add `StorageCreds` to the mod.rs description alongside `GitCreds`,
  `SigningCreds`, `RegistryCreds`;
- note the two-level `creds` + `type` discriminator for `StorageCreds` mirroring
  `GitCreds`.

### MN-3 — Phase 3 Commit 3 (plan 002-03) `Secrets` struct and `resolve_vault_refs` omit `StorageCreds`

**File:** `plans/002-20260508T1558-03-storage-and-secrets.md` **Lines:** 40
("the three credential families"), 218 (struct def), 233 (`resolve_vault_refs`
walk list)

Three separate occurrences:

1. The Depends-on description (line 40) says "three credential families."
2. The `Secrets` struct literal (line 218) shows only `git`, `signing`,
   `registry` Vecs — the `storage: Vec<StorageCreds>` field is absent.
3. The `resolve_vault_refs` spec (line 233) walks `GitVaultCreds` /
   `RegistryCreds::Vault` but does not mention `StorageVaultCreds::S3`
   vault-reference resolution.

An implementer following this plan would produce a `Secrets` struct that omits
the storage family entirely, and a `resolve_vault_refs` that does not resolve S3
Vault references. Both are functional gaps.

**Resolution:**

- Change "three credential families" to "four credential families" at line 40.
- Add `storage: Vec<StorageCreds>` to the struct literal at line 218 and update
  the parenthetical naming `GitCreds`, `SigningCreds`, `RegistryCreds` to also
  name `StorageCreds`.
- Add `StorageVaultCreds::S3` to the `resolve_vault_refs` walk description.
- The `storage.rs` helper file is already listed (line 251) with the correct
  description ("S3 access key / secret key resolved from `RegistryCreds::Vault`
  references at runtime" — note: this says `RegistryCreds::Vault`, which appears
  to be a copy-paste error; it should say `StorageCreds::Vault` or
  `StorageVaultCreds::S3`). Fix that description while updating the section.

---

## Verdict

**Not yet approved. Three minor issues must be resolved before Phase 1
implementation starts.**

C3-1, C3-2, C3-3, and C3-4 are closed correctly in their primary targets (design
002 and design 004 §Write site). C3-2 and C3-3 are clean and complete. C3-1 has
one stale residual in design 004 §OQ6 (wire-key casing for Config, three
occurrences). C3-4 was not propagated into the plan corpus: Phase 1 Commit 3 and
Phase 3 Commit 3 both retain three-family language and omit `StorageCreds` from
struct definitions and `resolve_vault_refs`.

All three minor issues are in plan/design prose — no implementation has started,
no code is affected. Each fix is a targeted text edit of fewer than ten lines.
Address MN-1 through MN-3 and re-confirm. No new architectural concerns were
introduced by the substantial §Wire-Format Versioning rewrite; the no-drift
check is clean.

Revised verdict once MN-1/MN-2/MN-3 are fixed: **Approve — C3-1+C3-2+C3-3+C3-4
closed; design corpus + plan corpus ready for Phase 1 implementation start.**
