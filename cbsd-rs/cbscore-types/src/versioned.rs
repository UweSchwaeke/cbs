// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026  Clyso

//! Internal helpers for the `Versioned*` wrappers per design 002
//! §Wire-Format Versioning.
//!
//! Every wire format the Rust port reads or writes carries a top-level
//! integer marker — `schema-version` (kebab YAML) or `schema_version`
//! (snake JSON descriptors). This module factors the shared dispatch
//! into one place; per-format wrappers
//! (e.g. [`crate::config::VersionedConfig`]) lift a `serde_value::Value`
//! through [`extract_schema_version`] and then re-deserialize the
//! payload as the typed inner struct.

use serde::ser::SerializeMap;
use serde_value::Value;

/// Reasons [`extract_schema_version`] can fail.
#[derive(Debug, PartialEq, Eq)]
pub enum ExtractError {
    /// Top-level wire value was not a YAML mapping / JSON object.
    NotMap,
    /// Map exists but the marker key is absent.
    Missing,
    /// Marker key is present but not a non-negative integer.
    NotInteger,
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMap => f.write_str("expected a map / object at the top level"),
            Self::Missing => f.write_str("schema-version marker missing"),
            Self::NotInteger => f.write_str("schema-version must be a non-negative integer"),
        }
    }
}

/// Extract the `schema-version` / `schema_version` u64 marker from a
/// pre-parsed serde value.
pub fn extract_schema_version(value: &Value, tag_key: &str) -> Result<u64, ExtractError> {
    let Value::Map(map) = value else {
        return Err(ExtractError::NotMap);
    };
    let key = Value::String(tag_key.to_string());
    match map.get(&key) {
        None => Err(ExtractError::Missing),
        Some(Value::U64(n)) => Ok(*n),
        Some(Value::I64(n)) if *n >= 0 => Ok((*n).cast_unsigned()),
        Some(_) => Err(ExtractError::NotInteger),
    }
}

/// Serialize the marker integer first under `tag_key`, then the
/// flattened inner-struct fields.
pub fn serialize_versioned<S, T>(
    serializer: S,
    tag_key: &'static str,
    version: u64,
    inner: &T,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: serde::Serialize,
{
    let inner_value = serde_value::to_value(inner)
        .map_err(|e| serde::ser::Error::custom(format!("internal serde_value: {e}")))?;
    let Value::Map(map) = inner_value else {
        return Err(serde::ser::Error::custom(
            "inner value must serialize to a map",
        ));
    };
    let mut s = serializer.serialize_map(Some(map.len() + 1))?;
    s.serialize_entry(tag_key, &version)?;
    for (k, v) in &map {
        s.serialize_entry(k, v)?;
    }
    s.end()
}
