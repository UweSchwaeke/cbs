# cbscore-rs Plan Review v21 — MN Closure Confirmation

## Scope

Confirmation review of commit `72852a8` ("cbsd-rs/docs: cbscore-rs — fix v20
MN-1+MN-2+MN-3 (C3 closure follow-up)"). The commit claims to close the three
minor findings v20 raised where the C3-1 and C3-4 closures had not been
propagated to all required locations. This pass verifies each of the three
closures independently and checks that no prior-review closures were disturbed.
No design or plan files are modified by this review.

This is the second confirmation layer in the pre-impl audit chain:

```
pre-impl audit → 4 C3 findings → closed in 6cc553f
  → v20 confirmation → 3 MN findings → closed in 72852a8
    → this v21 pass
```

Files verified:

- `cbsd-rs/docs/cbscore-rs/design/004-20260429T1319-configurable-version-descriptor-location.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-01-types.md`
- `cbsd-rs/docs/cbscore-rs/plans/002-20260508T1558-03-storage-and-secrets.md`

## Method

1. Read the full diff of `72852a8` against each of the three files.
2. For each MN finding, verify the exact text the v20 review named is gone and
   the replacement text is correct and internally consistent.
3. Cross-check each replacement against the authoritative source (design 002
   §Wire-Format Versioning for MN-1; design 002 §Secrets for MN-2 and MN-3).
4. Spot-check high-value prior closures named in the no-drift checklist to
   confirm they were not disturbed.
5. Run `prettier --check` on all three edited files.

---

## MN Closure Verification

### MN-1 — Design 004 §OQ6 kebab `schema-version` for Config wire values

**Finding (v20):** Three wire-value representations of the Config schema-version
marker in §OQ6 used snake-case (`schema_version`) — stale relative to the C3-1
rule pinned in design 002 that requires kebab (`schema-version`) for the Config
wire key. Line 174 additionally used `Config.schema_version` as a Rust field
reference (technically correct), while lines 177, 184, and 188 were wire-value
contexts that contradicted the rule. The §Migration end-note footer also used
snake form.

**Verification:**

`grep` for `schema_version` in
`design/004-20260429T1319-configurable-version-descriptor-location.md` returns
zero hits. Every former snake reference in §OQ6 has been replaced.

The replacement text at lines 174–189 (post-patch) is:

- Line 174: "The `Config`'s schema-version marker stays at 1" — correctly avoids
  the now-ambiguous `Config.schema_version` field reference and instead names
  the semantic concept.
- Line 177: "accumulates additions into `schema-version: 1` (kebab key —
  `Config` is a kebab-case struct per design 002 §Wire-Format Versioning)" —
  wire-key context, kebab, cross-reference explicit.
- Line 185: "Files written by cbscore-rs M1 carry `schema-version: 1` (kebab)" —
  wire-value context, kebab, parenthetical added.
- Line 189: "bumps the kebab `schema-version` to `2` per the standing rule" —
  wire-value context, kebab, qualifier added.
- §Migration footer (line 389): "no schema-version bump" — snake removed, kebab
  in place.

All five wire-value contexts now use kebab. The cross-reference to design 002
§Wire-Format Versioning is present at the first substantive change, giving a
reader who starts at design 004 the path to the authoritative rule. No new hedge
language introduced.

**Result: Closed. Correct.**

### MN-2 — Phase 1 Commit 3 §Files: four credential families, `StorageCreds` added

**Finding (v20):** Phase 1 Commit 3 §Files enumerated "the three credential
families" and listed only `GitCreds`, `SigningCreds`, and `RegistryCreds`.
`StorageCreds`, `StoragePlainCreds`, and `StorageVaultCreds` were absent — a
propagation gap from C3-4 that would have caused an implementer to omit the
storage family from `cbscore-types::utils::secrets` in Commit 3.

**Verification:**

`grep` for `three credential` in `plans/002-20260508T1558-01-types.md` returns
zero hits.

`grep` for `four credential` returns line 229: "the four credential families
(per design 002 §Secrets)" — count updated, cross-reference present.

The four-bullet list at lines 229–248 names:

1. `GitCreds` — two-level discriminator (`creds` outer, `type` inner), with
   `GitPlainCreds` / `GitVaultCreds`. Unchanged from v20.
2. `StorageCreds` (new bullet) — tagged on `creds`, with `StoragePlainCreds` and
   `StorageVaultCreds`, inner discriminator tagged on `type` (only `s3` today).
   The Vault variant's `key` field named. Cross-reference to design 002 §Storage
   secrets present.
3. `SigningCreds` — now expanded to name `SigningPlainCreds` (one variant:
   `gpg-armor-key`) and `SigningVaultCreds` (four variants). The prior version
   said only "tagged on `type` — deployed files already carry the field"; the
   new version gives implementer-actionable sub-variant names.
4. `RegistryCreds` — clarified: "single leaf per outer value, no inner type
   tag". Cross-reference to design 002 §Registry secrets added.

Note: `SigningCreds` and `RegistryCreds` received editorial expansion beyond the
strict MN-2 fix (more sub-variant detail). This is additive and consistent with
design 002 §Signing secrets and §Registry secrets. No contradiction introduced.

The two-level discriminator pattern for `StorageCreds` matches design 002
§Storage secrets (outer `creds`, inner `type` with `s3` as the single current
variant). Correct.

**Result: Closed. Correct.**

### MN-3 — Phase 3 Commit 3: `Secrets` struct, `resolve_vault_refs`, and `storage.rs` description

**Finding (v20):** Three separate gaps in plan
`002-20260508T1558-03-storage-and-secrets.md`:

1. §Depends on §Phase 1 bullet read "the three credential families."
2. `Secrets` struct literal showed only three `Vec` fields (`git`, `signing`,
   `registry`); `storage: Vec<StorageCreds>` was absent.
3. `resolve_vault_refs` walk description named only `GitVaultCreds` /
   `RegistryCreds::Vault`; `StorageVaultCreds` was absent.
4. `secrets/storage.rs` file description incorrectly attributed S3 credential
   resolution to `RegistryCreds::Vault` (copy-paste from the registry helper).

**Verification:**

_Sub-finding 1 — §Depends on §Phase 1 bullet:_

Line 40 now reads: "the four credential families — `GitCreds`, `StorageCreds`,
`SigningCreds`, `RegistryCreds`". All four named. The parenthetical names them
in the order they appear in the §Secrets preamble of design 002. Correct.

_Sub-finding 2 — `Secrets` struct literal:_

Line 219 now reads:
`Secrets { git: Vec<GitCreds>, storage: Vec<StorageCreds>, signing: Vec<SigningCreds>, registry: Vec<RegistryCreds> }`

Four fields. The parenthetical at line 220 says "four families, mirroring the
Python `Secrets` container per design 002 §Secrets." The leaf types named at
lines 221–222 include `StorageCreds`. Field order (git, storage, signing,
registry) matches design 002 §Secrets preamble. Correct.

_Sub-finding 3 — `resolve_vault_refs` walk description:_

Lines 237–238 now read: "walks each Vault-side entry across all four families
(`GitVaultCreds`, `StorageVaultCreds`, `SigningVaultCreds`,
`RegistryCreds::Vault`)". All four Vault-variant names present. `RegistryCreds`
correctly uses the single-level `::Vault` form (no inner type tag); the storage
and signing families use their dedicated `StorageVaultCreds` /
`SigningVaultCreds` names. Consistent with design 002 §Secrets and the type
shapes confirmed in v20 C3-2 and C3-4 verifications. Correct.

_Sub-finding 4 — `secrets/storage.rs` description:_

Lines 255–257 now read: "storage-credential resolution (S3 access-id / secret-id
resolved from `StorageVaultCreds::S3` references at runtime). Mirrors the role
of `git.rs` for the storage family." The stale `RegistryCreds::Vault` reference
is gone. The analogy to `git.rs` is useful and accurate. Correct.

**Result: Closed. All four sub-findings addressed. Correct.**

---

## No-Drift Check

The following prior closures were verified against the current file state to
confirm no regression was introduced by the `72852a8` edits.

**Phase 1 Commit 4 §Design constraints — C3-1 type-specific casing rule:** Lines
297–303 of `plan-01-types.md` state the casing rule in full: snake-case
descriptor structs carry `schema_version` (snake); kebab-case structs (`Config`,
`Secrets`, `VaultConfig`, `CoreComponent`) carry `schema-version` (kebab) via
`#[serde(rename = "schema-version")]`. Intact.

**Phase 1 Commit 5 §Files / §Design constraints / §Testable — kebab vs snake
split:** Lines 323–357 of `plan-01-types.md` use `schema-version` for
`VersionedConfig` (kebab) and `schema_version` for the descriptor wrappers
(snake) throughout. Negative tests specify kebab form for Config YAML. Intact.

**Phase 3 Commit 4 §Design constraints + §Testable — `Config::store` writes
kebab `schema-version: 1`:** Lines 328 and 351–354 of
`plan-03-storage-and-secrets.md` use kebab throughout. Intact.

**Phase 6 Commit 4 — `config init` writes kebab `schema-version: 1`:** Line 323
of `plan-06-cbsbuild-cli.md` confirmed kebab. Intact.

**Design 002 §Wire-Format Versioning — type-specific rule:** Line 411 of
`design/002-*`: "use the wire key `schema-version` (kebab), matching the
`Secrets` is a kebab-case struct" (paraphrase). Full rule block at lines
397–435. Intact.

**Design 002 §Secrets preamble — "four distinct families":** Line 530:
"`cbscore/utils/secrets/` models four distinct families — **git**, **storage**,
**signing**, and **registry**." Intact.

**Design 002 §Storage secrets / §Signing secrets / §Registry secrets subsections
— full enum trees, no stubs:** §Storage at line 592 shows the full
`StorageCreds` / `StoragePlainCreds` / `StorageVaultCreds` enum tree (confirmed
in v20 C3-4). §Signing at line 636 and §Registry at line 706 present their full
trees (confirmed in v20 C3-2). All three intact; none replaced with stub
comments.

**Design 004 §Write site — `write_descriptor` helper carries mkdir-p:** Lines
344–353 of `design/004-*` use `write_descriptor` exclusively; `create_dir_all`
lives inside the helper per the explicit contract prose. Intact.

**`prettier --check` on all three edited files:** Pass. All files conform to the
repository prettier configuration.

**No new contradictions or hedge language introduced by the MN closures.**

---

## Findings

None.

---

## Verdict

**Approve — MN-1+MN-2+MN-3 closed; pre-impl audit fully resolved; design
corpus + plan corpus ready for Phase 1 implementation start.**

All three minor findings from v20 are correctly and completely closed in
`72852a8`. The snake-cased wire-value references in design 004 §OQ6 are replaced
with kebab-cased forms with cross-references to the authoritative design 002
rule. `StorageCreds`, `StoragePlainCreds`, and `StorageVaultCreds` are now
present in Phase 1 Commit 3 §Files with the correct two-level discriminator
description. The `Secrets` struct in Phase 3 Commit 3 has four `Vec` fields,
`resolve_vault_refs` walks all four families' Vault variants, and
`secrets/storage.rs` is correctly attributed to `StorageVaultCreds::S3`. No
prior-review closures were disturbed. Prettier passes.

The pre-impl audit chain is complete: C3-1, C3-2, C3-3, C3-4 closed in
`6cc553f`; MN-1, MN-2, MN-3 closed in `72852a8`; both confirmed clean by v20 and
v21 respectively. The design corpus (designs 001–005) and plan corpus (seq-002
phases 1–7, seq-004) are fully approved with zero open findings. Phase 1
implementation may begin.
