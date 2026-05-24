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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::ws;
use axum::extract::ws::Message;
use axum::http::{HeaderName, Request};
use axum::{Json, Router, routing::get};
use sqlx::SqlitePool;
use tokio::sync::{Mutex, mpsc, watch};
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;
use tower_http::trace::TraceLayer;
use tower_sessions::SessionManagerLayer;
use tower_sessions::service::SignedCookie;
use tower_sessions_sqlx_store::SqliteStore;
use tracing::Span;
use utoipa_axum::router::OpenApiRouter;

use crate::auth::oauth::OAuthState;
use crate::auth::token_cache::TokenCache;
use crate::components::ComponentInfo;
use crate::config::ServerConfig;
use crate::logs::writer::SharedLogWriter;
use crate::queue::SharedBuildQueue;
use crate::routes;

/// Per-worker channel sender for outbound WebSocket messages.
/// The WS handler loop reads from the receiver and forwards to the socket.
pub type WorkerSender = mpsc::UnboundedSender<Message>;

/// Map of connection_id -> WorkerSender for all connected workers.
pub type WorkerSenders = Arc<Mutex<HashMap<String, WorkerSender>>>;

/// Map of build_id -> watch::Sender for log file change notifications.
pub type LogWatchers = Arc<Mutex<HashMap<i64, watch::Sender<()>>>>;

/// Shared application state. Extended by subsequent commits.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<ServerConfig>,
    pub oauth: OAuthState,
    pub token_cache: Arc<Mutex<TokenCache>>,
    pub queue: SharedBuildQueue,
    pub components: Vec<ComponentInfo>,
    /// Per-worker outbound message channels.
    pub worker_senders: WorkerSenders,
    /// Build log file change watchers (notifies SSE/follow endpoints).
    pub log_watchers: LogWatchers,
    /// Build log writer state (seq-to-offset indices).
    pub log_writer: SharedLogWriter,
    /// Handle for the periodic re-dispatch sweep task.
    pub sweep_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Handle for the log GC background task.
    pub gc_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Notify handle for the periodic build scheduler (wakes on task changes).
    pub scheduler_notify: Arc<tokio::sync::Notify>,
    /// Handle for the periodic build scheduler task.
    pub scheduler_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

/// Header name used for request IDs (propagated to responses).
static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Build the axum router.
pub fn build_router(
    state: AppState,
    session_layer: SessionManagerLayer<SqliteStore, SignedCookie>,
) -> Router {
    let api = OpenApiRouter::new()
        .nest("/auth", routes::auth::router())
        .nest("/permissions", routes::permissions::router())
        .nest("/admin", routes::admin::router())
        .nest("/builds", routes::builds::router())
        .nest("/components", routes::components::router())
        .nest("/workers", routes::workers::router())
        .nest("/periodic", routes::periodic::router())
        .nest("/channels", routes::channels::router());

    // Split into plain Router + OpenApi spec; health and WS stay on Router.
    let (api, openapi_spec) = api.split_for_parts();
    let api = api
        .route("/health", get(health))
        .merge(crate::openapi::doc_routes(openapi_spec));

    // Request/response tracing: logs method, URI, status, and latency
    // for every HTTP request. The request ID is generated per-request
    // and included in both the tracing span and the response header.
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &Request<_>| {
            let request_id = request
                .headers()
                .get(&X_REQUEST_ID)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");
            // Per audit-rem D9: emit only the URI **path** in the
            // tracing span, never the full URI. Query strings can
            // carry bearer tokens (e.g. OAuth `code=`, legacy
            // `cli-token=`, retry tokens) that must not land in
            // access logs.
            tracing::info_span!(
                "request",
                method = %request.method(),
                path = %request.uri().path(),
                request_id = %request_id,
            )
        })
        .on_response(
            |response: &axum::http::Response<_>, latency: Duration, _span: &Span| {
                let status = response.status().as_u16();
                let latency_ms = latency.as_millis();
                if status >= 500 {
                    tracing::error!(status, latency_ms, "response");
                } else if status >= 400 {
                    tracing::warn!(status, latency_ms, "response");
                } else {
                    tracing::info!(status, latency_ms, "response");
                }
            },
        )
        .on_failure(
            |error: ServerErrorsFailureClass, latency: Duration, _span: &Span| {
                tracing::error!(latency_ms = latency.as_millis(), "request failed: {error}");
            },
        );

    // Redact Authorization header from debug-level logs to avoid
    // leaking tokens.
    let sensitive_headers_layer =
        SetSensitiveRequestHeadersLayer::new([axum::http::header::AUTHORIZATION]);

    // Per audit-rem D6: cap REST request bodies at 1 MiB. axum returns
    // 413 Payload Too Large when this is exceeded. The WS upgrade route
    // bypasses request-body reading, so this layer applies only to
    // regular REST endpoints.
    let body_limit_layer = RequestBodyLimitLayer::new(cbsd_common::limits::REQUEST_BODY_MAX_BYTES);

    // Layer ordering (outermost → innermost):
    //   1. SetRequestId — assigns x-request-id before tracing sees it
    //   2. Sensitive headers — marks Authorization as sensitive
    //   3. TraceLayer — logs request/response with the assigned ID
    //   4. PropagateRequestId — copies x-request-id to the response
    //   5. SessionManagerLayer — session handling for OAuth
    //   6. RequestBodyLimit — bounds REST payloads at 1 MiB
    Router::new()
        .nest(
            "/api",
            api.route("/ws/worker", get(ws::handler::ws_upgrade)),
        )
        .layer(body_limit_layer)
        .layer(session_layer)
        .layer(PropagateRequestIdLayer::new(X_REQUEST_ID.clone()))
        .layer(trace_layer)
        .layer(sensitive_headers_layer)
        .layer(SetRequestIdLayer::new(
            X_REQUEST_ID.clone(),
            MakeRequestUuid,
        ))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "version": crate::VERSION}))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::routes::test_support::{test_app_state, test_pool, test_session_layer};

    /// Per audit-rem D6: a request with `Content-Length` above
    /// [`cbsd_common::limits::REQUEST_BODY_MAX_BYTES`] must be rejected
    /// with `413 Payload Too Large` before reaching any handler.
    #[tokio::test]
    async fn rest_body_over_limit_returns_413() {
        let pool = test_pool().await;
        let state = test_app_state(pool.clone());
        let session_layer = test_session_layer(pool).await;
        let app = build_router(state, session_layer);

        let huge = cbsd_common::limits::REQUEST_BODY_MAX_BYTES + 1;
        let req = Request::builder()
            .method("POST")
            .uri("/api/builds")
            .header("content-length", huge.to_string())
            .body(Body::empty())
            .expect("build request");

        let response = app.oneshot(req).await.expect("router");
        assert_eq!(
            response.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "expected 413 for Content-Length > REQUEST_BODY_MAX_BYTES"
        );
    }

    /// Counterpart: a request with `Content-Length` at the limit must
    /// pass through the limit layer (it may then fail downstream — auth,
    /// JSON parse — but the limit layer must not be the rejecter).
    #[tokio::test]
    async fn rest_body_at_limit_passes_layer() {
        let pool = test_pool().await;
        let state = test_app_state(pool.clone());
        let session_layer = test_session_layer(pool).await;
        let app = build_router(state, session_layer);

        let at_limit = cbsd_common::limits::REQUEST_BODY_MAX_BYTES;
        let req = Request::builder()
            .method("POST")
            .uri("/api/builds")
            .header("content-length", at_limit.to_string())
            .body(Body::empty())
            .expect("build request");

        let response = app.oneshot(req).await.expect("router");
        assert_ne!(
            response.status(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "limit layer must not fire at exactly REQUEST_BODY_MAX_BYTES"
        );
    }
}
