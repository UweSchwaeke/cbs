# Design Review v2: cbscore Rust Port — Project Structure & Crate Layout

**Document reviewed:**
`cbsd-rs/docs/cbscore-rs/design/001-20260418T2045-cbscore-project-structure.md`

**Prior review:** `001-20260420T1132-design-cbscore-project-structure-v1.md`

---

## Summary

**Verdict: needs targeted revision before implementation.**

All five v1 findings were addressed in earlier revisions and remain closed. This
review focuses exclusively on the material changes introduced by the Open
Question resolutions in design 002. Three issues require editing this document
before M0 begins: the `## Python Coexistence` section directly contradicts the
no-cross- language-interchange position now established in design 002; the
`## Runner Container` section states Python and the venv are removed entirely,
which contradicts the OQ7 resolution (system `python3` stays); and the
`## Crate Dependencies` sketch for `cbscore` is missing four crates that are now
decided.

The `## Versioning` table is mostly fine with a minor wording issue.

---

## v1 Finding Verification

All five v1 findings are confirmed closed in the current document:

- **F1 (cbsd consumer import table):** Full import list present including
  `VersionDescriptor`, `ConfigError`, `logger.logger`.
- **F2 (`--cbscore-path` drop):** Documented explicitly with UX break notice, M1
  release notes callout, and clap error description.
- **F3 (`write_entrypoint` sync/async):** Snippet uses
  `std::io::Write::write_all` with comment; corrected.
- **F4 (`cbscore-types` pulls format crates):** `serde_json` and `serde_saphyr`
  removed from `cbscore-types` deps; `cbscore-types` carries only `serde`,
  `thiserror`, `tracing`, `chrono`.
- **F5 (`anyhow` in library):** `anyhow` removed from `cbscore` deps sketch; the
  comment "adapter glue" is gone.

---

## Findings

### F1 — `## Python Coexistence` contradicts the no-file-interchange position [BLOCKING]

**Section:** `## Python Coexistence`, item 1 (line ~443)

The section opens with:

> 1. **Wire-format parity (always).** Every config, descriptor, and output file
>    that cbscore produces must remain byte-compatible with the Python
>    implementation. Downstream Python consumers continue reading and writing
>    the same files regardless of which implementation produced them.

This is the exact opposite of what design 002 now says. Design 002
`§ Configuration & Secrets Subsystem` (resolved from OQ5/OQ6) states:

> Cross-implementation file interchange is **not** a requirement: a given
> deployment runs either Python cbscore or Rust cbscore, never both against the
> same on-disk files.

The same relaxed position was extended to release descriptors (OQ6),
`secrets.yaml` (OQ6 corollary noted in the session), and is encoded as
Correctness Invariant 1 in `cbsd-rs/docs/cbscore-rs/CLAUDE.md`, which reads:

> Round-trip wire-format stability… Cross-language byte-equality with pydantic
> output is **not** a requirement.

Design 001 item 1 of `## Python Coexistence` is now a false contract that will
mislead implementors who read this document first. It will also mislead anyone
writing the M1 release notes, who might impose byte-identical YAML output as an
acceptance gate.

**Why it matters:** An implementor reading only design 001 will add golden-file
tests comparing Rust output byte-for-byte against pydantic output. These tests
will fail on cosmetic YAML differences (key ordering, quoting) that are harmless
and intentional. More importantly, the position in item 1 was the basis for
keeping `schema_version` out of Python — but design 002 now requires
`schema_version` on every Rust-written file, which is explicitly not
byte-compatible with Python output. The two positions cannot coexist.

**Resolution:** Rewrite item 1 to match design 002's position. Suggested
replacement text:

> 1. **On-disk format stability (Rust side).** Config, secrets, descriptor, and
>    report files that cbscore-rs writes must round- trip stably on the Rust
>    side (write → load → equal) and remain stable across cbscore-rs versions.
>    Cross-language byte-equality with Python/pydantic output is **not** a
>    requirement: a given deployment runs either Python cbscore or Rust cbscore
>    at any one time, never both against the same on-disk files. Operators
>    migrating from Python to Rust re-tag or regenerate their files at cutover
>    (see design 002 § Configuration & Secrets and § Migration Strategy for the
>    exact migration recipe).

Also update the paragraph that precedes the list, which says "Three migration
shapes make up the approach" — item 1 is no longer a migration shape; it is an
on-disk-stability invariant. Renumber and rephrase accordingly.

---

### F2 — `## Runner Container` overstates Python removal [IMPORTANT]

**Section:** `## Runner Container`, bullet point starting "Binary mount instead
of source mount" (line ~488)

The section states (emphasis mine):

> This removes `uv`, Python, and the venv from the build image entirely (see
> design 002 § Runner Subsystem for the exact entrypoint script).

The OQ7 resolution in design 002 says:

> **Resolved: drop only the cbscore Python wheel (and its `uv` / venv
> installation), keep system `python3`.** Removing system `python3` from the
> builder image is impossible: Ceph's `do_cmake.sh` invokes Python during cmake
> configuration; many RPMs … require Python …

Design 001's sentence says "removes Python … from the build image entirely".
That is incorrect — system `python3` remains. An implementor reading this
sentence will attempt to produce a builder image without `python3` and discover
broken Ceph builds.

**Resolution:** Update the bullet to match OQ7 precisely:

> This removes `uv`, the cbscore Python wheel, and the cbscore venv from the
> build image. System `python3` is **not** removed — it remains as a transitive
> build dependency for Ceph and other components (Ceph's `do_cmake.sh`, RPM spec
> `%build` phases, and `python3-mgr-*` packages all require it). See design 002
> § Open Questions (OQ7) for the full rationale.

---

### F3 — `## Crate Dependencies` sketch for `cbscore` is missing four resolved deps [IMPORTANT]

**Section:** `## Crate Dependencies — cbscore` (Cargo.toml sketch)

The sketch predates the Open Question resolutions. Four crates are now decided
and must appear in the dependency list:

| Crate                               | Version                | Reason missing                                                                                                                                                                |
| ----------------------------------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `camino`                            | `1` (feature `serde1`) | OQ3 resolved `camino` at all API boundaries                                                                                                                                   |
| `camino-tempfile`                   | `1`                    | OQ3 — paired with `camino` for tempfile interop                                                                                                                               |
| `regex`                             | `1`                    | Required by `_sanitize_cmd` (§ Subprocess & Secret Redaction, the `--pass[phrase]` redact regex) and `parse_version` / `parse_component_refs` (§ Version Descriptors)         |
| `once_cell` / `std::sync::OnceLock` | —                      | The `OnceLock<Regex>` pattern in the `redact_inline` snippet in design 002 requires it (Rust 1.70+ `std::sync::OnceLock` is sufficient; no extra crate needed if MSRV is met) |

`dialoguer` is **not** yet a dep for `cbscore` or `cbscore-types` — it belongs
to `cbsbuild` and only after design 003 ships (post-M1). No change needed in
this sketch for `dialoguer`.

`which` is already listed (`which = "7"`) — no action needed.

In addition: the `cbscore-types` sketch uses `PathBuf` implicitly (via `serde`)
but no `camino` dep appears. Since `camino` is the declared path type at all API
boundaries (OQ3), `cbscore-types` must also carry
`camino = { version = "1", features = ["serde1"] }` so that `Utf8PathBuf`
derives `Serialize` / `Deserialize` in the type structs. The `Config`,
`PathsConfig`, and similar structs in design 002 still show `PathBuf` (see below
and F1 in the 002 v3 review) — reconcile both documents together.

**Resolution:** Add `camino`, `camino-tempfile`, and `regex` to the `cbscore`
Cargo sketch. Add `camino = { version = "1", features = ["serde1"] }` to
`cbscore-types`. Remove or replace any `PathBuf` references in the struct
sketches in design 001's body (none exist in 001's own body, but the preamble
sentence under `### cbscore-types` mentions "Config structs …
`#[serde(rename_all = "kebab-case")]`" — add a note that path fields use
`Utf8PathBuf`).

---

### F4 — `## Versioning` major-bump trigger wording is ambiguous [NICE-TO-HAVE]

**Section:** `## Versioning`, "When to bump which component" table, Major row
(line ~410)

The table reads:

> | Major | Wire-format change (config YAML, secrets YAML, descriptor JSON,
> on-disk layout); CLI UX break; or any change in a behaviour anchored by the
> Correctness Invariants in `cbsd-rs/docs/cbscore-rs/CLAUDE.md`.

"Wire-format change" as a major trigger is correct. The concern is that without
the no-cross-language caveat it might be read as "any change that breaks
Python's ability to read Rust-written files is a major bump". That
interpretation would wrongly treat adding `schema_version` as a major bump (it's
the initial v1 baseline) and wrongly demand that every new Rust format be
readable by pydantic.

This is a minor wording issue, not a conceptual error — but it could cause
confusion when the first minor-version additive field causes a `schema_version`
bump while leaving pydantic unaffected.

**Resolution:** Add a parenthetical clarifying the scope:

> | Major | Wire-format change to a Rust-side wire format (config YAML, secrets
> YAML, descriptor JSON, on-disk layout), meaning a `schema_version` bump in any
> format or an on-disk path/key rename; CLI UX break; or any change in a
> behaviour anchored by the Correctness Invariants. Note: cross-language
> byte-equality with Python/pydantic output is not the standard — see
> `§ Python Coexistence` (item 1).

---

## Cross-Document Notes

**001 / 002 consistency after OQ resolutions:** Once F1 and F2 above are fixed
in design 001, the two documents will agree on:

- No cross-language file interchange (F1 / design 002 § Configuration &
  Secrets).
- System `python3` stays in builder image (F2 / design 002 OQ7).
- `camino` at all API boundaries (F3 / design 002 OQ3).

**CLAUDE.md Correctness Invariant 1:** The CLAUDE.md file has already been
updated to the relaxed "Round-trip wire-format stability" wording. Design 001
`## Python Coexistence` is the last place in the document set that contradicts
this. No other CLAUDE.md / design 002 drift was found.

---

## Suggested Follow-ups

1. Rewrite `## Python Coexistence` item 1 per F1.
2. Update `## Runner Container` Python-removal sentence per F2.
3. Add `camino`, `camino-tempfile`, `regex` to `cbscore` Cargo sketch; add
   `camino` (serde1) to `cbscore-types` Cargo sketch (F3).
4. (Optional) Tighten the major-bump trigger wording per F4.
