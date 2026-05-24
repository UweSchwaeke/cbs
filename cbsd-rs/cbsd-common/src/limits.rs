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

//! Workspace-wide request and message size limits (audit-rem D6).
//!
//! These ceilings are applied on both the server-accept and the
//! worker-connect paths so neither side has to trust the other to
//! enforce them. Keeping them in `cbsd-common` prevents drift between
//! the server and the worker.

/// Maximum REST request body size (1 MiB). axum's
/// `tower_http::limit::RequestBodyLimitLayer` returns 413 Payload Too
/// Large when this is exceeded. Build descriptors are small JSON
/// documents; nothing legitimate is anywhere near this limit today.
pub const REQUEST_BODY_MAX_BYTES: usize = 1024 * 1024;

/// Maximum size of a single WebSocket message (8 MiB). Component
/// tarballs travel as binary frames; ~4× the worker's default
/// component-decompression cap accommodates substantial future growth
/// while still bounding per-message memory.
pub const WS_MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

/// Maximum size of a single WebSocket frame (1 MiB). Frames smaller
/// than `WS_MAX_MESSAGE_BYTES` permit larger messages via continuation
/// frames but cap a single frame's memory footprint.
pub const WS_MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Static guarantee: the per-frame ceiling must not exceed the
/// per-message ceiling. A frame larger than a message is unreachable
/// in practice and would indicate a config bug.
const _: () = assert!(WS_MAX_FRAME_BYTES <= WS_MAX_MESSAGE_BYTES);

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the numeric values so the audit-rem D6 design contract is
    /// enforced as code — changing them requires explicitly updating
    /// this test, which makes any drift loud.
    #[test]
    fn limits_match_design() {
        assert_eq!(REQUEST_BODY_MAX_BYTES, 1024 * 1024);
        assert_eq!(WS_MAX_MESSAGE_BYTES, 8 * 1024 * 1024);
        assert_eq!(WS_MAX_FRAME_BYTES, 1024 * 1024);
    }
}
