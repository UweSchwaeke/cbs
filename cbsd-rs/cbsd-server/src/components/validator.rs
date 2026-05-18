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

//! Centralised typed-descriptor validation (WCP D5 / G4).
//!
//! Every ingress path that accepts a build descriptor (REST `submit_build`,
//! periodic-task create/update, scheduler trigger) routes through
//! `validate_descriptor` so the rejection rules stay consistent. The
//! authoritative invariants today: the descriptor must list at least one
//! component, and every listed component must be known to the server's
//! discovered component registry.

use cbsd_proto::BuildDescriptor;

use crate::components::ComponentInfo;

/// Why a descriptor was rejected by [`validate_descriptor`].
#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    EmptyComponents,
    UnknownComponent(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyComponents => write!(f, "descriptor `components` array is empty"),
            Self::UnknownComponent(name) => write!(f, "unknown component: {name}"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate a typed `BuildDescriptor` against the server's known components.
///
/// Pure function: no I/O. The scheduler trigger uses this defensively at
/// fire time to disable tasks whose stored descriptor was rendered invalid
/// by a component being removed since task creation.
pub fn validate_descriptor(
    descriptor: &BuildDescriptor,
    known: &[ComponentInfo],
) -> Result<(), ValidationError> {
    if descriptor.components.is_empty() {
        return Err(ValidationError::EmptyComponents);
    }
    for comp in &descriptor.components {
        if !known.iter().any(|c| c.name == comp.name) {
            return Err(ValidationError::UnknownComponent(comp.name.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cbsd_proto::{
        Arch, BuildComponent, BuildDescriptor, BuildDestImage, BuildSignedOffBy, BuildTarget,
    };

    use super::*;

    fn known(names: &[&str]) -> Vec<ComponentInfo> {
        names
            .iter()
            .map(|n| ComponentInfo {
                name: (*n).to_string(),
                versions: Vec::new(),
            })
            .collect()
    }

    fn descriptor_with(components: Vec<BuildComponent>) -> BuildDescriptor {
        BuildDescriptor {
            version: "v".into(),
            channel: None,
            version_type: None,
            signed_off_by: BuildSignedOffBy {
                user: "u".into(),
                email: "u@e.com".into(),
            },
            dst_image: BuildDestImage {
                name: "img".into(),
                tag: "t".into(),
            },
            components,
            build: BuildTarget {
                distro: "fedora".into(),
                os_version: "42".into(),
                artifact_type: "rpm".into(),
                arch: Arch::X86_64,
            },
        }
    }

    fn comp(name: &str) -> BuildComponent {
        BuildComponent {
            name: name.into(),
            git_ref: "main".into(),
            repo: None,
        }
    }

    #[test]
    fn empty_components_array_is_rejected() {
        let d = descriptor_with(vec![]);
        assert_eq!(
            validate_descriptor(&d, &known(&["ceph"])),
            Err(ValidationError::EmptyComponents),
        );
    }

    #[test]
    fn unknown_component_name_is_rejected_with_name_echoed() {
        let d = descriptor_with(vec![comp("not-real")]);
        assert_eq!(
            validate_descriptor(&d, &known(&["ceph"])),
            Err(ValidationError::UnknownComponent("not-real".into())),
        );
    }

    #[test]
    fn descriptor_with_only_known_components_is_accepted() {
        let d = descriptor_with(vec![comp("ceph"), comp("dashboard")]);
        assert_eq!(
            validate_descriptor(&d, &known(&["ceph", "dashboard"])),
            Ok(()),
        );
    }

    #[test]
    fn first_unknown_component_short_circuits_check() {
        let d = descriptor_with(vec![comp("ceph"), comp("not-real"), comp("dashboard")]);
        assert_eq!(
            validate_descriptor(&d, &known(&["ceph", "dashboard"])),
            Err(ValidationError::UnknownComponent("not-real".into())),
        );
    }
}
