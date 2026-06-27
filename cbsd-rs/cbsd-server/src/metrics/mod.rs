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

//! Prometheus metrics exporter.
//!
//! All metrics — server-owned and (later) pushed-worker — are recorded through
//! a single `metrics` facade + `metrics-exporter-prometheus` recorder. The
//! `/metrics` endpoint is just [`PrometheusHandle::render`]; there is no second
//! exposition path and no hand-rolled cache (design 022).
//!
//! [`install`] builds the recorder with a GAUGE-only `idle_timeout` so a silent
//! worker's host gauges disappear while server-owned counters are never pruned.
//! Each later commit adds its own `set_buckets_for_metric` call here when it
//! first emits a histogram, so no bucket constant sits unused.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use metrics_exporter_prometheus::{BuildError, Matcher, PrometheusBuilder, PrometheusHandle};
use metrics_util::MetricKindMask;

use crate::app::AppState;

pub mod builds;
pub mod gauges;
pub mod lifecycle;

/// Prometheus exposition content type (text format v0.0.4).
const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4";

/// Histogram buckets (seconds) for `cbsd_build_duration_seconds`. Sized for the
/// observed range — minutes to ~1-2h depending on ccache warmth — with headroom
/// to 3h and an implicit `+Inf` catch-all (design 022).
const BUILD_DURATION_BUCKETS: [f64; 12] = [
    30.0, 60.0, 120.0, 240.0, 480.0, 900.0, 1800.0, 2700.0, 3600.0, 5400.0, 7200.0, 10800.0,
];

/// Buckets (seconds) for `cbsd_build_queue_wait_seconds` — sub-second to an hour.
const QUEUE_WAIT_BUCKETS: [f64; 9] = [1.0, 5.0, 15.0, 30.0, 60.0, 300.0, 900.0, 1800.0, 3600.0];

/// Buckets (seconds) for `cbsd_dispatch_latency_seconds` — the in-dispatch
/// tarball pack + send, expected sub-second to tens of seconds.
const DISPATCH_LATENCY_BUCKETS: [f64; 8] = [0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0];

/// Buckets (seconds) for `cbsd_periodic_schedule_lag_seconds` — how late a cron
/// fire ran versus its intended time.
const SCHEDULE_LAG_BUCKETS: [f64; 6] = [1.0, 5.0, 15.0, 60.0, 300.0, 900.0];

/// Render handle for the installed Prometheus recorder. Cloneable and cheap;
/// the only state `/metrics` and the gauge-refresh task need.
#[derive(Clone)]
pub struct MetricsState {
    pub handle: PrometheusHandle,
}

/// Install the global Prometheus recorder.
///
/// `stale_after` is the GAUGE idle-timeout: a gauge series not updated within
/// this window is pruned on the next render/upkeep. Counters and histograms are
/// never idle-expired (GAUGE-only mask), so cumulative server-owned data
/// survives a decommissioned worker as a benign flat series.
///
/// Must be called once, before any metric is emitted.
pub fn install(stale_after: Duration) -> Result<PrometheusHandle, BuildError> {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("cbsd_build_duration_seconds".to_string()),
            &BUILD_DURATION_BUCKETS,
        )?
        .set_buckets_for_metric(
            Matcher::Full("cbsd_build_queue_wait_seconds".to_string()),
            &QUEUE_WAIT_BUCKETS,
        )?
        .set_buckets_for_metric(
            Matcher::Full("cbsd_dispatch_latency_seconds".to_string()),
            &DISPATCH_LATENCY_BUCKETS,
        )?
        .set_buckets_for_metric(
            Matcher::Full("cbsd_periodic_schedule_lag_seconds".to_string()),
            &SCHEDULE_LAG_BUCKETS,
        )?
        .idle_timeout(MetricKindMask::GAUGE, Some(stale_after))
        .install_recorder()
}

/// `GET /metrics` — renders the current Prometheus exposition. Returns 404 when
/// metrics are disabled (no recorder installed).
pub async fn metrics_handler(State(state): State<AppState>) -> Response {
    match &state.metrics {
        Some(m) => ([(CONTENT_TYPE, PROMETHEUS_CONTENT_TYPE)], m.handle.render()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
