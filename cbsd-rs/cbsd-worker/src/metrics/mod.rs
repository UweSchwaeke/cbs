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

//! Worker-side metrics collection (design 021).
//!
//! The worker exposes no HTTP endpoint: it samples host and application metrics
//! and *pushes* them over the existing WebSocket as [`cbsd_proto::ws::Metrics`]
//! when the server advertises `accepts_metrics`. The server stamps the `worker`
//! label and re-exposes them on its own `/metrics`.

pub mod app;
pub mod host;
pub mod sampler;
