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

//! HTTP RED (Rate / Errors / Duration) metrics.
//!
//! An axum middleware that labels by the **matched route pattern** (e.g.
//! `/api/builds/{id}`), not the raw URI path — so high-cardinality path
//! segments like build ids do not explode the label space. Unmatched requests
//! (404s, scans) collapse to a single `unmatched` route for the same reason.

use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use metrics::{counter, histogram};

/// Emit one HTTP request observation. Split out so it is unit-testable without
/// driving a full async router (the macros emit to whichever recorder is
/// active).
pub fn record_http(route: &str, method: &str, status: u16, secs: f64) {
    counter!(
        "cbsd_http_requests_total",
        "route" => route.to_string(),
        "method" => method.to_string(),
        "status" => status.to_string(),
    )
    .increment(1);
    histogram!(
        "cbsd_http_request_duration_seconds",
        "route" => route.to_string(),
        "method" => method.to_string(),
    )
    .record(secs);
}

/// Axum middleware recording the RED metrics. Added via `Router::layer`, so the
/// `MatchedPath` extension is already populated by routing when this runs.
pub async fn track_metrics(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned());
    let method = req.method().as_str().to_owned();

    let response = next.run(req).await;

    record_http(
        &route,
        &method,
        response.status().as_u16(),
        start.elapsed().as_secs_f64(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics_exporter_prometheus::PrometheusBuilder;

    #[test]
    fn record_http_emits_counter_and_duration_by_route() {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::with_local_recorder(&recorder, || {
            record_http("/api/builds/{id}", "GET", 200, 0.012);
        });
        let out = handle.render();

        assert!(
            out.contains(
                r#"cbsd_http_requests_total{route="/api/builds/{id}",method="GET",status="200"} 1"#
            ),
            "request counter missing/incorrect:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_http_request_duration_seconds_count{route="/api/builds/{id}",method="GET"} 1"#),
            "duration histogram missing:\n{out}"
        );
    }

    /// Drive a request through the layer to prove the label is the matched
    /// route **pattern** (`/items/{id}`), not the concrete path (`/items/42`).
    /// Uses a process-global recorder — this is the only test that installs one
    /// (the async router cannot carry a thread-local recorder across `.await`).
    #[tokio::test]
    async fn middleware_labels_by_matched_route_pattern() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::routing::get;
        use axum::{Router, middleware};
        use tower::ServiceExt;

        let handle = PrometheusBuilder::new()
            .install_recorder()
            .expect("install global recorder (only this test installs one)");

        let app = Router::new()
            .route("/items/{id}", get(|| async { "ok" }))
            .layer(middleware::from_fn(track_metrics));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/items/42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let out = handle.render();
        assert!(
            out.contains(r#"route="/items/{id}""#),
            "expected matched-route-pattern label, not the concrete path:\n{out}"
        );
        assert!(
            !out.contains(r#"route="/items/42""#),
            "concrete path must not appear as a label:\n{out}"
        );
    }
}
