// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Image-descriptor types.
//!
//! Wire format is JSON with `snake_case` keys (no `rename_all`).

use serde::{Deserialize, Serialize};

/// Source / destination image reference pair.
///
/// # Examples
///
/// ```
/// use cbscore_types::images::desc::ImageLocations;
///
/// let l = ImageLocations {
///     src: "quay.io/build/ceph:19.2.3-dev1".into(),
///     dst: "quay.io/release/ceph:19.2.3".into(),
/// };
/// let json = serde_json::to_string(&l).unwrap();
/// let parsed: ImageLocations = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, l);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageLocations {
    /// Source image reference (where the builder pushed it).
    pub src: String,
    /// Destination image reference (where the sync step copies it to).
    pub dst: String,
}

/// Top-level image descriptor — paired list of releases the descriptor
/// covers and the image source/destination locations to sync.
///
/// # Examples
///
/// ```
/// use cbscore_types::images::desc::{ImageDescriptor, ImageLocations};
///
/// let d = ImageDescriptor {
///     releases: vec!["19.2.3".into()],
///     images: vec![ImageLocations {
///         src: "quay.io/build/ceph:19.2.3-dev1".into(),
///         dst: "quay.io/release/ceph:19.2.3".into(),
///     }],
/// };
/// let json = serde_json::to_string(&d).unwrap();
/// let parsed: ImageDescriptor = serde_json::from_str(&json).unwrap();
/// assert_eq!(parsed, d);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageDescriptor {
    /// Release-version strings this descriptor applies to.
    pub releases: Vec<String>,
    /// Source / destination image reference pairs to sync.
    pub images: Vec<ImageLocations>,
}
