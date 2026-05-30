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

pub mod arch;
pub mod build;
pub mod ws;

pub use arch::Arch;
pub use build::{
    BuildComponent, BuildDescriptor, BuildDestImage, BuildId, BuildSignedOffBy, BuildState,
    BuildTarget, Priority, VersionType,
};

/// Worker token payload — the base64url-encoded JSON blob returned by the
/// server at worker registration. Contains everything the worker needs to
/// connect. Serialized by the server, deserialized by the worker.
///
/// Not annotated with `ToSchema` — this is an internal registration payload,
/// not part of the documented REST API surface.
///
/// SECURITY: this is a transport-only DTO that, by design, carries a plaintext
/// `api_key`, so its fields stay plain `String` (the design's "separate the
/// wire DTO from the in-memory secret holder" option). The long-lived in-memory
/// holder is the worker's `ResolvedWorkerConfig.api_key: SecretString`. `Debug`
/// is hand-written to redact `api_key`; do not add a `Display` or any log path
/// that emits the raw key.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkerToken {
    pub worker_id: String,
    pub worker_name: String,
    pub api_key: String,
    pub arch: String,
}

impl std::fmt::Debug for WorkerToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerToken")
            .field("worker_id", &self.worker_id)
            .field("worker_name", &self.worker_name)
            .field("api_key", &"<redacted>")
            .field("arch", &self.arch)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_token_debug_redacts_api_key() {
        let token = WorkerToken {
            worker_id: "w-1".to_string(),
            worker_name: "builder".to_string(),
            api_key: "cbsk_super_secret_value".to_string(),
            arch: "x86_64".to_string(),
        };
        let rendered = format!("{token:?}");
        assert!(
            !rendered.contains("cbsk_super_secret_value"),
            "api_key must not appear in WorkerToken Debug output: {rendered}"
        );
        assert!(
            rendered.contains("<redacted>"),
            "api_key should render as <redacted>: {rendered}"
        );
    }
}
