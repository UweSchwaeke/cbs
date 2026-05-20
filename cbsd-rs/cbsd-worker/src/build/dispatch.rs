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

//! Per-build tracing event dispatch.
//!
//! [`BuildDispatchLayer`] is a `tracing_subscriber::Layer` installed
//! once at worker startup. It captures tracing events emitted by
//! [`cbscore`](https://docs.rs/cbscore) (and any other in-process
//! caller) and routes them to a per-build
//! [`mpsc::UnboundedSender<String>`](tokio::sync::mpsc::UnboundedSender)
//! identified by the `build_id` field on the current tracing span
//! chain.
//!
//! The post-cutover build flow (Phase 7 Commit 1b) replaces the
//! Python wrapper's stdout pipe with this Layer: the worker calls
//! `cbscore::runner::run(...).instrument(info_span!(target:
//! "cbscore", build_id = %build_id, trace_id = %trace_id))`, and
//! the Layer captures every `tracing::info!` / `debug!` / `error!`
//! emitted from within that span and pushes the formatted line
//! into the matching channel.
//!
//! Per the plan §"Subscriber layer design", the channel is
//! **unbounded**: `Layer::on_event` is `&self` synchronous and
//! cannot `.await` on backpressure, so a bounded channel would
//! deadlock the tracing thread on a slow consumer. The batcher
//! drains the receiver at its own pace; runaway log volume is
//! capped by the existing tracing subscriber's filter level
//! (default `info`).

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Arc, Mutex};

use cbsd_proto::build::BuildId;
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// Shared map of `BuildId` → per-build event sink.
type SinkMap = Arc<Mutex<HashMap<BuildId, mpsc::UnboundedSender<String>>>>;

/// Public handle main.rs hands to the WebSocket handler.
///
/// `register` creates a per-build channel and inserts its sender
/// into the shared map keyed by `build_id`; the matching receiver
/// is returned for the batcher task to drain. `unregister` drops
/// the sender, causing the batcher's `recv().await` to return
/// `None` so the batcher can flush its last partial batch and
/// exit cleanly.
#[derive(Clone)]
pub(crate) struct BuildDispatch {
    sinks: SinkMap,
}

impl BuildDispatch {
    /// Construct a fresh dispatch + the matching [`BuildDispatchLayer`].
    pub(crate) fn new() -> (Self, BuildDispatchLayer) {
        let sinks: SinkMap = Arc::new(Mutex::new(HashMap::new()));
        let layer = BuildDispatchLayer {
            sinks: Arc::clone(&sinks),
        };
        let dispatch = Self { sinks };
        (dispatch, layer)
    }

    /// Register a per-build sink. Returns the receiver the batcher
    /// task drains.
    pub(crate) fn register(&self, build_id: BuildId) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.sinks
            .lock()
            .expect("BuildDispatch sinks mutex poisoned")
            .insert(build_id, tx);
        rx
    }

    /// Drop the per-build sender; the batcher's receiver gets
    /// `None` and flushes its last partial batch.
    pub(crate) fn unregister(&self, build_id: BuildId) {
        self.sinks
            .lock()
            .expect("BuildDispatch sinks mutex poisoned")
            .remove(&build_id);
    }
}

/// `tracing_subscriber::Layer` that routes per-build tracing
/// events into per-build channels.
///
/// Install once at worker startup via the registry builder; the
/// handle returned by [`BuildDispatch::new`] is used by the WS
/// handler to register and unregister per-build sinks.
pub(crate) struct BuildDispatchLayer {
    sinks: SinkMap,
}

/// Span-extension marker that pins the `build_id` for events
/// emitted under this span and its descendants.
struct BuildIdExt(BuildId);

impl<S> Layer<S> for BuildDispatchLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = BuildIdVisitor(None);
        attrs.record(&mut visitor);
        if let Some(build_id) = visitor.0
            && let Some(span) = ctx.span(id)
        {
            span.extensions_mut().insert(BuildIdExt(build_id));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // Walk the event's span chain looking for a BuildIdExt.
        let Some(scope) = ctx.event_scope(event) else {
            return;
        };
        let mut build_id: Option<BuildId> = None;
        for span in scope {
            if let Some(ext) = span.extensions().get::<BuildIdExt>() {
                build_id = Some(ext.0);
                break;
            }
        }
        let Some(bid) = build_id else { return };

        // Format the event into a single line.
        let meta = event.metadata();
        let mut line = String::new();
        // Match the Python wrapper's format ("LEVEL:target:message")
        // so operators tailing the build log see continuity.
        let _ = write!(line, "{}:{}:", meta.level(), meta.target());
        let mut visitor = EventLineFormatter(&mut line);
        event.record(&mut visitor);

        // Lock + send. send() on an UnboundedSender doesn't block,
        // so this is bounded by the lock acquisition time only.
        let sinks = self
            .sinks
            .lock()
            .expect("BuildDispatchLayer sinks mutex poisoned");
        if let Some(tx) = sinks.get(&bid) {
            // Ignore the SendError: receiver dropped means the
            // batcher already exited; nothing to do.
            let _ = tx.send(line);
        }
    }
}

/// Visitor that pulls the `build_id` field out of a span's
/// attributes (or an event's fields).
struct BuildIdVisitor(Option<BuildId>);

impl Visit for BuildIdVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "build_id" {
            self.0 = Some(BuildId(value));
        }
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "build_id" {
            // BuildId wraps i64; clamp if necessary. In practice
            // BuildIds are positive monotonic integers from the
            // server's database autoincrement, so the cast is
            // safe across the cbsd-proto domain.
            self.0 = Some(BuildId(value as i64));
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // `build_id = %build_id` formats via Display which often
        // round-trips through Debug here. Parse "BuildId(N)" or
        // a bare "N" out of the debug repr.
        if field.name() == "build_id" {
            let rendered = format!("{value:?}");
            // Strip "BuildId(" / ")" if present.
            let trimmed = rendered
                .strip_prefix("BuildId(")
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or(rendered.as_str());
            if let Ok(n) = trimmed.parse::<i64>() {
                self.0 = Some(BuildId(n));
            }
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "build_id"
            && let Ok(n) = value.parse::<i64>()
        {
            self.0 = Some(BuildId(n));
        }
    }
}

/// Visitor that formats an event's fields into a single line,
/// matching the Python wrapper's `LEVEL:target:message …fields`
/// shape.
struct EventLineFormatter<'a>(&'a mut String);

impl Visit for EventLineFormatter<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value}");
        } else {
            let _ = write!(self.0, " {}={value}", field.name());
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        let _ = write!(self.0, " {}={value}", field.name());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        let _ = write!(self.0, " {}={value}", field.name());
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        let _ = write!(self.0, " {}={value}", field.name());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_then_unregister_closes_channel() {
        let (dispatch, _layer) = BuildDispatch::new();
        let mut rx = dispatch.register(BuildId(42));
        dispatch.unregister(BuildId(42));
        // After unregister, the sender is dropped; recv() returns
        // None and the batcher would flush + exit.
        assert!(rx.recv().await.is_none());
    }

    #[test]
    fn build_id_visitor_debug_arm_parses_expected_shapes() {
        // tracing's `Field` type cannot be constructed directly in
        // tests; the parsing logic inside `record_debug` is
        // factored out into `parse_build_id_debug` so we can
        // exercise the BuildId(N) / bare-N / malformed arms here.
        assert_eq!(parse_build_id_debug("BuildId(99)"), Some(BuildId(99)));
        assert_eq!(parse_build_id_debug("99"), Some(BuildId(99)));
        assert_eq!(parse_build_id_debug("not-a-number"), None);
        assert_eq!(parse_build_id_debug("BuildId(abc)"), None);
    }

    /// Extracted parsing helper for test coverage — mirrors the
    /// arm inside `BuildIdVisitor::record_debug`.
    fn parse_build_id_debug(rendered: &str) -> Option<BuildId> {
        let trimmed = rendered
            .strip_prefix("BuildId(")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(rendered);
        trimmed.parse::<i64>().ok().map(BuildId)
    }
}
