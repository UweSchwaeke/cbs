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

//! Queue, dispatch, scheduler, and liveness lifecycle metrics.
//!
//! Thin, type-safe emit helpers so the call sites scattered across the dispatch
//! loop, the ack/revoke timers, the reconnect path, and the scheduler stay
//! one-liners and the label vocabulary lives in one place.

use cbsd_proto::{Arch, Priority};
use metrics::{counter, histogram};

/// Label string for a priority (matches the lowercase serde form).
fn priority_label(p: Priority) -> &'static str {
    match p {
        Priority::High => "high",
        Priority::Normal => "normal",
        Priority::Low => "low",
    }
}

/// Time a build spent queued before it was dispatched (seconds).
pub fn record_queue_wait(priority: Priority, arch: Arch, wait_secs: f64) {
    histogram!(
        "cbsd_build_queue_wait_seconds",
        "priority" => priority_label(priority),
        "arch" => arch.to_string(),
    )
    .record(wait_secs);
}

/// In-`try_dispatch` work to hand a build to a worker — tarball pack + send
/// (seconds).
pub fn record_dispatch_latency(arch: Arch, secs: f64) {
    histogram!("cbsd_dispatch_latency_seconds", "arch" => arch.to_string()).record(secs);
}

/// A build was re-queued for another dispatch attempt. `reason` is one of a
/// small fixed set (`ack_timeout`, `rejected`, `reconnect_stale`,
/// `worker_dead`) — keep it a `&'static str` so the label stays bounded.
pub fn record_requeue(reason: &'static str) {
    counter!("cbsd_build_requeues_total", "reason" => reason).increment(1);
}

/// A worker failed to acknowledge a dispatch within the timeout.
pub fn record_dispatch_ack_timeout() {
    counter!("cbsd_dispatch_ack_timeouts_total").increment(1);
}

/// A worker failed to acknowledge a revoke within the timeout (the server
/// then resolved the build unilaterally).
pub fn record_revoke_ack_timeout() {
    counter!("cbsd_revoke_ack_timeouts_total").increment(1);
}

/// A worker reconnected under a new connection while a prior connection for the
/// same `registered_worker_id` was still tracked (the bounded, stable id — see
/// `metrics::builds`).
pub fn record_worker_reconnect(worker: &str) {
    counter!("cbsd_worker_reconnects_total", "worker" => worker.to_string()).increment(1);
}

/// A periodic task fired. `result` is `success` or `failure`.
pub fn record_periodic_fire(result: &'static str) {
    counter!("cbsd_periodic_fires_total", "result" => result).increment(1);
}

/// How late a periodic task fired versus its intended cron time (seconds,
/// clamped at 0 — early fires are not meaningful).
pub fn record_periodic_schedule_lag(secs: f64) {
    histogram!("cbsd_periodic_schedule_lag_seconds").record(secs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics_exporter_prometheus::PrometheusBuilder;

    fn render_after<F: FnOnce()>(f: F) -> String {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::with_local_recorder(&recorder, f);
        handle.render()
    }

    #[test]
    fn queue_wait_carries_priority_and_arch() {
        let out = render_after(|| record_queue_wait(Priority::High, Arch::X86_64, 12.0));
        assert!(
            out.contains(r#"cbsd_build_queue_wait_seconds_sum{priority="high",arch="x86_64"} 12"#),
            "queue-wait sample missing:\n{out}"
        );
    }

    #[test]
    fn requeue_reason_is_labelled() {
        let out = render_after(|| {
            record_requeue("ack_timeout");
            record_requeue("worker_dead");
        });
        assert!(
            out.contains(r#"cbsd_build_requeues_total{reason="ack_timeout"} 1"#),
            "{out}"
        );
        assert!(
            out.contains(r#"cbsd_build_requeues_total{reason="worker_dead"} 1"#),
            "{out}"
        );
    }

    #[test]
    fn periodic_fire_and_lag_recorded() {
        let out = render_after(|| {
            record_periodic_fire("success");
            record_periodic_schedule_lag(3.5);
        });
        assert!(
            out.contains(r#"cbsd_periodic_fires_total{result="success"} 1"#),
            "{out}"
        );
        assert!(out.contains("cbsd_periodic_schedule_lag_seconds"), "{out}");
    }
}
