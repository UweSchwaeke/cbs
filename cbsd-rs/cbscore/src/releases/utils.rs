// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Small helpers for the release publish path: S3 key-layout
//! derivation and descriptor → manifest projection helpers.

use camino::Utf8Path;

/// Derive the S3 key for the release-descriptor JSON object:
/// `<loc>/<version>.json`.
///
/// Matches Python `cbscore.releases.s3._release_desc_key` per
/// design 002 §S3 operations line 1156–1158.
///
/// # Examples
///
/// ```
/// use cbscore::releases::utils::release_desc_key;
///
/// assert_eq!(
///     release_desc_key("ceph/dev", "19.2.3"),
///     "ceph/dev/19.2.3.json",
/// );
/// ```
#[must_use]
pub fn release_desc_key(loc: &str, version: &str) -> String {
    format!("{}/{}.json", loc.trim_end_matches('/'), version)
}

/// Derive the S3 key prefix the per-component RPM uploads land
/// under: `<loc>/<version>/`. The trailing slash is included so
/// callers can concat the basename without adding another `/`.
///
/// # Examples
///
/// ```
/// use cbscore::releases::utils::release_rpm_prefix;
///
/// assert_eq!(
///     release_rpm_prefix("ceph/dev", "19.2.3"),
///     "ceph/dev/19.2.3/",
/// );
/// ```
#[must_use]
pub fn release_rpm_prefix(loc: &str, version: &str) -> String {
    format!("{}/{}/", loc.trim_end_matches('/'), version)
}

/// Derive the full S3 key for a single RPM artefact: prefix +
/// basename.
///
/// # Examples
///
/// ```
/// use camino::Utf8Path;
/// use cbscore::releases::utils::rpm_key;
///
/// assert_eq!(
///     rpm_key("ceph/dev", "19.2.3", Utf8Path::new("/tmp/ceph-1.0.x86_64.rpm")),
///     "ceph/dev/19.2.3/ceph-1.0.x86_64.rpm",
/// );
/// ```
#[must_use]
pub fn rpm_key(loc: &str, version: &str, rpm_path: &Utf8Path) -> String {
    let basename = rpm_path.file_name().unwrap_or("");
    format!("{}{basename}", release_rpm_prefix(loc, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_desc_key_no_trailing_slash_in_loc() {
        assert_eq!(release_desc_key("a/b", "v"), "a/b/v.json");
    }

    #[test]
    fn release_desc_key_strips_trailing_slash_in_loc() {
        assert_eq!(release_desc_key("a/b/", "v"), "a/b/v.json");
    }

    #[test]
    fn release_rpm_prefix_trailing_slash() {
        let p = release_rpm_prefix("a/b", "v");
        assert!(p.ends_with('/'));
        assert_eq!(p, "a/b/v/");
    }

    #[test]
    fn rpm_key_appends_basename() {
        let k = rpm_key("a/b", "v", Utf8Path::new("/x/y/z.rpm"));
        assert_eq!(k, "a/b/v/z.rpm");
    }
}
