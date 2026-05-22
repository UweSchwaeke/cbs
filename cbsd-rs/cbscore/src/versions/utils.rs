// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Pure version-string helpers — port of `cbscore/versions/utils.py`.
//!
//! Lives in [`cbscore`] (not [`cbscore_types`]) because the regex
//! pattern requires the `regex` crate, which design 001 §Cargo Sketch
//! deliberately excludes from `cbscore-types`.
//!
//! Closes the Phase 1 §Out of scope drift: the parse family is
//! authoritative here.

use std::collections::HashMap;
use std::sync::OnceLock;

use cbscore_types::errors::CbsError;
use cbscore_types::versions::{VersionError, VersionType};
use regex::{Captures, Regex};

fn captured(captures: &Captures<'_>, name: &str) -> Option<String> {
    captures.name(name).map(|m| m.as_str().to_owned())
}

/// Parsed structure for a `[prefix-]vM.m[.p][-suffix]` version
/// string.
///
/// Mirrors Python `cbscore.versions.utils.ParseVersionResult` —
/// `(prefix, major, minor, patch, suffix)` — as a named-field struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVersion {
    /// Optional prefix before the leading `v` (e.g. `ces`).
    pub prefix: Option<String>,
    /// Mandatory major version component.
    pub major: String,
    /// Optional minor version component.
    pub minor: Option<String>,
    /// Optional patch version component.
    pub patch: Option<String>,
    /// Optional suffix after the patch (e.g. `dev.1`).
    pub suffix: Option<String>,
}

/// Verbatim port of Python's `parse_version` regex
/// (`cbscore/versions/utils.py` line 45-55 verbose form).
fn version_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
            ^
            (?:(?P<prefix>\w+)-)?           # optional prefix
            v?                              # optional 'v'
            (?P<major>\d+)                  # mandatory major version
            (?:\.(?P<minor>\d+)             # optional minor version
                (?:\.(?P<patch>\d+)         # optional patch version
                (?:-(?P<suffix>[\w_.-]+))?  # optional suffix
                )?
            )?
            $
            ",
        )
        .expect("static regex compiles")
    })
}

/// Parse a version string into its components.
///
/// # Errors
///
/// Returns [`CbsError::MalformedVersion`] when the input doesn't match
/// the `[prefix-]vM.m[.p][-suffix]` regex.
///
/// # Examples
///
/// ```
/// use cbscore::versions::utils::{parse_version, ParsedVersion};
///
/// let v = parse_version("ces-v19.2.3-dev.1").unwrap();
/// assert_eq!(v.prefix.as_deref(), Some("ces"));
/// assert_eq!(v.major, "19");
/// assert_eq!(v.minor.as_deref(), Some("2"));
/// assert_eq!(v.patch.as_deref(), Some("3"));
/// assert_eq!(v.suffix.as_deref(), Some("dev.1"));
///
/// assert!(parse_version("not-a-version-at-all!").is_err());
/// ```
// `expect("regex enforces major presence")` cannot panic — the regex
// declares the `major` group as mandatory, so a successful `.captures()`
// match guarantees it is present. No `# Panics` doc warranted.
#[allow(clippy::missing_panics_doc)]
pub fn parse_version(s: &str) -> Result<ParsedVersion, CbsError> {
    let captures = version_regex()
        .captures(s)
        .ok_or_else(|| CbsError::MalformedVersion(s.to_owned()))?;
    Ok(ParsedVersion {
        prefix: captured(&captures, "prefix"),
        major: captures
            .name("major")
            .expect("regex enforces major presence")
            .as_str()
            .to_owned(),
        minor: captured(&captures, "minor"),
        patch: captured(&captures, "patch"),
        suffix: captured(&captures, "suffix"),
    })
}

/// Derive the [`VersionType`] from a full version string by
/// inspecting its suffix.
///
/// - No suffix → [`VersionType::Release`].
/// - Suffix starts with `dev` → [`VersionType::Dev`].
/// - Suffix starts with `test` → [`VersionType::Test`].
/// - Suffix starts with `ci` → [`VersionType::Ci`].
/// - Anything else → [`VersionError::InvalidDescriptor`].
///
/// # Errors
///
/// Returns [`VersionError::InvalidDescriptor`] when the suffix doesn't
/// match a known type (or the version doesn't parse at all).
///
/// # Examples
///
/// ```
/// use cbscore::versions::utils::get_version_type;
/// use cbscore_types::versions::VersionType;
///
/// assert_eq!(get_version_type("ces-v19.2.3").unwrap(), VersionType::Release);
/// assert_eq!(get_version_type("ces-v19.2.3-dev.1").unwrap(), VersionType::Dev);
/// assert_eq!(get_version_type("ces-v19.2.3-test.1").unwrap(), VersionType::Test);
/// assert_eq!(get_version_type("ces-v19.2.3-ci.42").unwrap(), VersionType::Ci);
/// ```
pub fn get_version_type(name: &str) -> Result<VersionType, VersionError> {
    let parsed = parse_version(name).map_err(|_| VersionError::InvalidDescriptor {
        path: name.into(),
        message: format!("version '{name}' does not match the [prefix-]vM.m[.p][-suffix] regex"),
    })?;
    match parsed.suffix.as_deref() {
        None => Ok(VersionType::Release),
        Some(s) if s.starts_with("dev") => Ok(VersionType::Dev),
        Some(s) if s.starts_with("test") => Ok(VersionType::Test),
        Some(s) if s.starts_with("ci") => Ok(VersionType::Ci),
        Some(s) => Err(VersionError::InvalidDescriptor {
            path: name.into(),
            message: format!(
                "version '{name}' has unknown type suffix '{s}' \
                 (expected: dev*, test*, ci*, or none for release)"
            ),
        }),
    }
}

/// Return the `<major>.<minor>` prefix of `v` (CES/Ceph convention
/// where "major" denotes the first two components).
///
/// # Errors
///
/// Returns [`CbsError::MalformedVersion`] when `v` doesn't parse, or
/// when it parses but lacks a minor component.
///
/// # Examples
///
/// ```
/// use cbscore::versions::utils::get_major_version;
///
/// assert_eq!(get_major_version("ces-v19.2.3-dev.1").unwrap(), "19.2");
/// assert_eq!(get_major_version("ces-v19.2").unwrap(), "19.2");
/// ```
pub fn get_major_version(v: &str) -> Result<String, CbsError> {
    let parsed = parse_version(v)?;
    let minor = parsed
        .minor
        .ok_or_else(|| CbsError::MalformedVersion(v.to_owned()))?;
    Ok(format!("{}.{}", parsed.major, minor))
}

/// Return `<major>.<minor>.<patch>` of `v`, or `None` when the patch
/// component is missing.
///
/// # Errors
///
/// Returns [`CbsError::MalformedVersion`] when `v` doesn't parse.
///
/// # Examples
///
/// ```
/// use cbscore::versions::utils::get_minor_version;
///
/// assert_eq!(
///     get_minor_version("ces-v19.2.3-dev.1").unwrap().as_deref(),
///     Some("19.2.3"),
/// );
/// assert_eq!(get_minor_version("ces-v19.2").unwrap(), None);
/// ```
pub fn get_minor_version(v: &str) -> Result<Option<String>, CbsError> {
    let parsed = parse_version(v)?;
    let (Some(minor), Some(patch)) = (&parsed.minor, &parsed.patch) else {
        return Ok(None);
    };
    Ok(Some(format!("{}.{}.{}", parsed.major, minor, patch)))
}

/// Canonicalise `v` back to `[<prefix>-]v<major>.<minor>[.<patch>][-<suffix>]`.
///
/// # Errors
///
/// Returns [`CbsError::MalformedVersion`] when `v` doesn't parse, or
/// when it parses but lacks a minor component.
///
/// # Examples
///
/// ```
/// use cbscore::versions::utils::normalize_version;
///
/// assert_eq!(
///     normalize_version("ces-v19.2.3-dev.1").unwrap(),
///     "ces-v19.2.3-dev.1",
/// );
/// assert_eq!(normalize_version("19.2").unwrap(), "v19.2");
/// ```
pub fn normalize_version(v: &str) -> Result<String, CbsError> {
    let parsed = parse_version(v)?;
    let minor = parsed
        .minor
        .as_ref()
        .ok_or_else(|| CbsError::MalformedVersion(v.to_owned()))?;
    let mut out = String::new();
    if let Some(p) = &parsed.prefix {
        out.push_str(p);
        out.push('-');
    }
    out.push('v');
    out.push_str(&parsed.major);
    out.push('.');
    out.push_str(minor);
    if let Some(p) = &parsed.patch {
        out.push('.');
        out.push_str(p);
    }
    if let Some(s) = &parsed.suffix {
        out.push('-');
        out.push_str(s);
    }
    Ok(out)
}

/// Parse `COMPONENT@REF` entries into a name → ref map.
///
/// The component pattern is `^([\w_-]+)@([\d\w_./-]+)$` per design 002
/// line 700.
///
/// # Errors
///
/// Returns [`VersionError::InvalidDescriptor`] on any entry that does
/// not match the pattern.
///
/// # Examples
///
/// ```
/// use cbscore::versions::utils::parse_component_refs;
///
/// let refs = parse_component_refs(&[
///     "ceph@master".to_owned(),
///     "el9@v1.0".to_owned(),
/// ]).unwrap();
/// assert_eq!(refs.get("ceph").map(String::as_str), Some("master"));
/// assert_eq!(refs.get("el9").map(String::as_str), Some("v1.0"));
/// ```
// `expect("static regex")` cannot panic at runtime — the pattern is a
// fixed literal and is compile-tested by the doctest above. No `# Panics`
// doc warranted.
#[allow(clippy::missing_panics_doc)]
pub fn parse_component_refs(
    components: &[String],
) -> Result<HashMap<String, String>, VersionError> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^([\w_-]+)@([\d\w_./-]+)$").expect("static regex"));

    let mut out = HashMap::new();
    for c in components {
        let captures = re
            .captures(c)
            .ok_or_else(|| VersionError::InvalidDescriptor {
                path: "<component-refs>".into(),
                message: format!("malformed component name/version pair '{c}'"),
            })?;
        out.insert(captures[1].to_owned(), captures[2].to_owned());
    }
    Ok(out)
}

/// Validate a `desc.version` candidate string.
///
/// Accepts:
///
/// - A `UUIDv7` string (RFC 9562 canonical hyphenated form,
///   case-insensitive — [`uuid::Uuid::parse_str`] handles both lower
///   and upper hex). The resolver in [`super::resolve::resolve_version`]
///   mints lowercase `UUIDv7s` on the no-VERSION path; operators may
///   also type one explicitly.
/// - Any string matching the Python regex `[prefix-]vM.m.p[-suffix]`
///   with both minor AND patch components present. Mirrors Python's
///   `cbscore/versions/create.py:_validate_version` — regex match
///   plus `minor is not None and patch is not None`.
///
/// Rejects: bare `19`, `19.2`, `foobar`, `UUIDv4` strings, and anything
/// else that fails both checks. Each reject surfaces as
/// [`CbsError::MalformedVersion`] carrying the offending input
/// verbatim — operator-actionable, matches the Python side's
/// `MalformedVersionError` shape.
///
/// `cbsbuild versions create` (seq-005 Commit 3) calls this
/// unconditionally on the resolved VERSION; `UUIDv7` passes by the
/// carve-out, operator-supplied strings get the regex check.
///
/// # Errors
///
/// Returns [`CbsError::MalformedVersion`] when the input fails both
/// the `UUIDv7` detection and the regex+minor+patch validation.
///
/// # Examples
///
/// ```
/// use cbscore::versions::utils::validate_version;
///
/// // Accepts a Python-shaped VERSION string.
/// assert!(validate_version("19.2.3").is_ok());
/// // Accepts a UUIDv7 string.
/// assert!(validate_version(&uuid::Uuid::now_v7().to_string()).is_ok());
/// // Rejects a malformed string.
/// assert!(validate_version("19").is_err());
/// ```
pub fn validate_version(v: &str) -> Result<(), CbsError> {
    // UUIDv7 fast path: lets resolver-generated v7s (and explicit
    // operator-typed ones) pass without running the regex.
    if let Ok(uuid) = uuid::Uuid::parse_str(v)
        && uuid.get_version() == Some(uuid::Version::SortRand)
    {
        return Ok(());
    }
    // Python parity: regex match + both minor and patch present.
    let parsed = parse_version(v)?;
    if parsed.minor.is_some() && parsed.patch.is_some() {
        Ok(())
    } else {
        Err(CbsError::MalformedVersion(v.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_full() {
        let v = parse_version("ces-v99.99.1-asd-qwe").unwrap();
        assert_eq!(v.prefix.as_deref(), Some("ces"));
        assert_eq!(v.major, "99");
        assert_eq!(v.minor.as_deref(), Some("99"));
        assert_eq!(v.patch.as_deref(), Some("1"));
        assert_eq!(v.suffix.as_deref(), Some("asd-qwe"));
    }

    #[test]
    fn parse_version_no_suffix() {
        let v = parse_version("ces-v99.99.1").unwrap();
        assert_eq!(v.suffix, None);
    }

    #[test]
    fn parse_version_no_prefix() {
        let v = parse_version("v19.2.3").unwrap();
        assert_eq!(v.prefix, None);
        assert_eq!(v.major, "19");
    }

    #[test]
    fn parse_version_major_only() {
        let v = parse_version("99").unwrap();
        assert_eq!(v.major, "99");
        assert_eq!(v.minor, None);
        assert_eq!(v.patch, None);
    }

    #[test]
    fn parse_version_rejects_garbage() {
        assert!(parse_version("not-a-version-at-all!").is_err());
    }

    #[test]
    fn parse_version_rejects_uuidv7() {
        // UUIDv7 from design 005 must NOT match the version regex.
        assert!(
            parse_version("0193e1a8-7c2e-7000-8000-0000000000ab").is_err(),
            "UUIDv7 should fail parse_version per design 005",
        );
    }

    #[test]
    fn get_version_type_release() {
        assert_eq!(
            get_version_type("ces-v19.2.3").unwrap(),
            VersionType::Release,
        );
    }

    #[test]
    fn get_version_type_dev() {
        assert_eq!(
            get_version_type("ces-v19.2.3-dev.1").unwrap(),
            VersionType::Dev,
        );
    }

    #[test]
    fn get_version_type_test() {
        assert_eq!(
            get_version_type("ces-v19.2.3-test.1").unwrap(),
            VersionType::Test,
        );
    }

    #[test]
    fn get_version_type_ci() {
        assert_eq!(
            get_version_type("ces-v19.2.3-ci.42").unwrap(),
            VersionType::Ci,
        );
    }

    #[test]
    fn get_version_type_unknown_suffix() {
        assert!(get_version_type("ces-v19.2.3-asd-qwe").is_err());
    }

    #[test]
    fn get_major_version_works() {
        assert_eq!(get_major_version("ces-v19.2.3-dev.1").unwrap(), "19.2");
        assert_eq!(get_major_version("ces-v19.2").unwrap(), "19.2");
    }

    #[test]
    fn get_major_version_rejects_no_minor() {
        // Major-only — no minor component — fails per Python.
        assert!(get_major_version("ces-v19").is_err());
    }

    #[test]
    fn get_minor_version_works() {
        assert_eq!(
            get_minor_version("ces-v19.2.3-dev.1").unwrap().as_deref(),
            Some("19.2.3"),
        );
    }

    #[test]
    fn get_minor_version_no_patch_is_none() {
        assert_eq!(get_minor_version("ces-v19.2").unwrap(), None);
    }

    #[test]
    fn normalize_round_trips() {
        let inputs = ["ces-v19.2.3-dev.1", "v19.2.3", "v19.2", "ces-v19.2.3-dev.1"];
        for v in inputs {
            let normalized = normalize_version(v).unwrap();
            assert_eq!(normalize_version(&normalized).unwrap(), normalized);
        }
    }

    #[test]
    fn normalize_synthesises_prefix() {
        // No prefix on input → no prefix on output, but the `v` is
        // re-added.
        assert_eq!(normalize_version("19.2").unwrap(), "v19.2");
    }

    #[test]
    fn parse_component_refs_works() {
        let refs = parse_component_refs(&[
            "ceph@master".to_owned(),
            "el9@v1.0".to_owned(),
            "cbs-build@abc123def".to_owned(),
        ])
        .unwrap();
        assert_eq!(refs.len(), 3);
        assert_eq!(refs["ceph"], "master");
        assert_eq!(refs["el9"], "v1.0");
        assert_eq!(refs["cbs-build"], "abc123def");
    }

    #[test]
    fn parse_component_refs_rejects_malformed() {
        assert!(parse_component_refs(&["ceph-without-ref".to_owned()]).is_err());
        assert!(parse_component_refs(&["@ref-without-name".to_owned()]).is_err());
    }

    // ----------------------------------------------------------------
    // validate_version (seq-005)
    // ----------------------------------------------------------------

    /// Full Python-shape VERSION strings pass — the supplied-VERSION
    /// path's regex-matched accept set.
    #[test]
    fn validate_version_accepts_python_shape() {
        assert!(validate_version("19.2.3").is_ok());
        assert!(validate_version("v19.2.3").is_ok());
        assert!(validate_version("ces-v19.2.3-dev.1").is_ok());
        assert!(validate_version("19.2.3-rc1").is_ok());
    }

    /// `UUIDv7` strings pass — the seq-005 carve-out for both resolver-
    /// generated and operator-typed `UUIDv7s`.
    #[test]
    fn validate_version_accepts_uuidv7() {
        let lowercase = uuid::Uuid::now_v7().to_string();
        assert!(validate_version(&lowercase).is_ok());
        // Uuid::parse_str is case-insensitive — uppercase passes too.
        let uppercase = lowercase.to_uppercase();
        assert!(validate_version(&uppercase).is_ok());
    }

    /// Missing minor/patch fails — Python parity for `_validate_version`'s
    /// `minor is not None and patch is not None` check.
    #[test]
    fn validate_version_rejects_missing_minor_or_patch() {
        for input in ["19", "19.2", "v19", "v19.2", "ces-v19", "ces-v19.2"] {
            let err = validate_version(input).expect_err(input);
            assert!(matches!(err, CbsError::MalformedVersion(ref s) if s == input));
        }
    }

    /// Garbage that doesn't even pass the regex.
    #[test]
    fn validate_version_rejects_regex_misses() {
        for input in ["foobar", "", "v", "1.2.x", "this is not a version"] {
            assert!(
                validate_version(input).is_err(),
                "expected reject: {input:?}"
            );
        }
    }

    /// `UUIDv4` falls through to the regex path and gets rejected
    /// there (the regex doesn't match the v4 shape, no carve-out
    /// applies). Pins the seq-005 design choice that only `UUIDv7`
    /// is accepted, not "any UUID".
    #[test]
    fn validate_version_rejects_uuidv4() {
        let v4 = uuid::Uuid::new_v4().to_string();
        let err = validate_version(&v4).expect_err("must reject v4");
        assert!(matches!(err, CbsError::MalformedVersion(_)));
    }
}
