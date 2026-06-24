// Copyright (C) 2026  Clyso
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

//! Pack a component directory into a gzip-compressed tar archive.

use std::io;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};

/// Pack one or more component directories into a single gzip-compressed tar
/// archive.
///
/// Each component `name` is read from `components_dir/<name>` and stored under
/// its own `<name>/` top-level prefix, so the worker's unpack root holds one
/// subdirectory per component — exactly the layout cbscore's `load_components`
/// enumerates. This is what lets a multi-component build reach the worker: the
/// descriptor lists every component, and every component's files ride in the
/// one tarball.
///
/// Returns `(tar_gz_bytes, sha256_hex)` where `sha256_hex` is the hex-encoded
/// SHA-256 digest of the final gzip bytes (over the combined archive).
pub fn pack_components(
    components_dir: &Path,
    component_names: &[&str],
) -> Result<(Vec<u8>, String), io::Error> {
    let buf = Vec::new();
    let encoder = GzEncoder::new(buf, Compression::fast());
    let mut archive = tar::Builder::new(encoder);

    // Append each component directory under its own name prefix. The tar
    // `Builder` defaults to `HeaderMode::Complete`, preserving file modes so
    // executable component scripts stay executable on the worker.
    for name in component_names {
        archive.append_dir_all(name, components_dir.join(name))?;
    }

    // Finish the tar archive and then the gzip stream.
    let encoder = archive.into_inner()?;
    let gz_bytes = encoder.finish()?;

    // Compute SHA-256 over the final gzip bytes.
    let mut hasher = Sha256::new();
    hasher.update(&gz_bytes);
    let hash = hasher.finalize();
    let hex = hex_encode(&hash);

    Ok((gz_bytes, hex))
}

/// Encode a byte slice as lowercase hex.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Gz-decode `gz_bytes` and return the set of top-level path segments of
    /// every archive entry (e.g. `"ceph"` for an entry `ceph/scripts/x.sh`).
    fn top_level_entries(gz_bytes: &[u8]) -> BTreeSet<String> {
        let decoder = flate2::read::GzDecoder::new(gz_bytes);
        let mut archive = tar::Archive::new(decoder);
        let mut tops = BTreeSet::new();
        for entry in archive.entries().unwrap() {
            let entry = entry.unwrap();
            let path = entry.path().unwrap().into_owned();
            if let Some(first) = path.components().next() {
                tops.insert(first.as_os_str().to_string_lossy().into_owned());
            }
        }
        tops
    }

    #[test]
    fn pack_and_verify_sha256() {
        let tmp = tempfile::TempDir::new().unwrap();
        let comp = tmp.path().join("test-component");
        std::fs::create_dir_all(comp.join("sub")).unwrap();
        std::fs::write(comp.join("file.txt"), b"hello world").unwrap();
        std::fs::write(comp.join("sub/nested.txt"), b"nested content").unwrap();

        let (gz_bytes, sha256_hex) = pack_components(tmp.path(), &["test-component"]).unwrap();

        // Verify the bytes are non-empty gzip (magic bytes 1f 8b)
        assert!(gz_bytes.len() > 20);
        assert_eq!(gz_bytes[0], 0x1f);
        assert_eq!(gz_bytes[1], 0x8b);

        // Verify SHA-256 is 64 hex chars
        assert_eq!(sha256_hex.len(), 64);

        // Re-hash and verify consistency
        let mut hasher = sha2::Sha256::new();
        hasher.update(&gz_bytes);
        let hash = hasher.finalize();
        assert_eq!(hex_encode(&hash), sha256_hex);
    }

    /// The fix: a multi-component build must ship every referenced component's
    /// directory in the one tarball, each under its own `<name>/` prefix. This
    /// pins the packing boundary directly — before the fix only the first
    /// component made it in.
    #[test]
    fn packs_all_components_under_their_name_prefix() {
        let tmp = tempfile::TempDir::new().unwrap();
        for name in ["alpha", "beta"] {
            let dir = tmp.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("cbs.component.yaml"), b"name: x\n").unwrap();
        }

        let (gz_bytes, _sha) = pack_components(tmp.path(), &["alpha", "beta"]).unwrap();

        let tops = top_level_entries(&gz_bytes);
        assert!(tops.contains("alpha"), "missing alpha/ in {tops:?}");
        assert!(tops.contains("beta"), "missing beta/ in {tops:?}");

        // Both components' descriptor files must be present in the archive.
        let decoder = flate2::read::GzDecoder::new(&gz_bytes[..]);
        let mut archive = tar::Archive::new(decoder);
        let mut found = BTreeSet::new();
        for entry in archive.entries().unwrap() {
            let entry = entry.unwrap();
            let path = entry.path().unwrap().into_owned();
            if path.file_name().and_then(|n| n.to_str()) == Some("cbs.component.yaml") {
                found.insert(path.to_string_lossy().into_owned());
            }
        }
        assert!(
            found.iter().any(|p| p.starts_with("alpha/"))
                && found.iter().any(|p| p.starts_with("beta/")),
            "expected both components' cbs.component.yaml, got {found:?}"
        );
    }

    /// A missing component source directory must surface as an `io::Error`
    /// (the dispatch path turns this into a terminal build FAILURE, see
    /// `try_dispatch_pack_failure_fails_build_end_to_end`).
    #[test]
    fn missing_component_dir_errors() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = pack_components(tmp.path(), &["does-not-exist"]);
        assert!(result.is_err(), "expected error for missing component dir");
    }
}
