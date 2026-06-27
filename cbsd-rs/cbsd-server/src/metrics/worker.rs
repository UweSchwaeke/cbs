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

//! Ingestion of worker-pushed host/app metrics (design 022).
//!
//! A worker that the server told `accepts_metrics` sends periodic
//! [`cbsd_proto::ws::WorkerMessage::Metrics`] snapshots. There is no cache: the
//! snapshot is written straight into the `metrics` facade here, with the
//! `worker` label stamped **server-side** from the connection's stable
//! `registered_worker_id` (F8 — the worker never sends its own label, so it
//! cannot forge another's series and a reconnect on a fresh `connection_id`
//! keeps updating one continuous series).
//!
//! Point-in-time values are gauges (`.set`), so a silent worker's gauges idle-
//! expire after `stale_after` and disappear. Cumulative since-start values are
//! counters republished with `.absolute(v)`: `rate()` absorbs the reset when a
//! worker restarts, which is independently visible as a
//! `cbsd_worker_uptime_seconds` regression. Counters are never idle-expired, so
//! a decommissioned worker leaves a benign flat series rather than vanishing.

use cbsd_proto::ws::{AppMetrics, HostMetrics};
use metrics::{counter, gauge};

/// Write one worker metrics snapshot into the facade under the server-stamped
/// `worker` label. Pure emission with no IO, so it is exercised directly under
/// a local recorder in tests; the async WS handler just calls it.
pub fn record_worker_metrics(worker: &str, uptime_secs: u64, host: &HostMetrics, app: &AppMetrics) {
    // Uptime is a gauge in its own right and the reset signal for the counters.
    gauge!("cbsd_worker_uptime_seconds", "worker" => worker.to_string()).set(uptime_secs as f64);

    // Host point-in-time gauges.
    gauge!("cbsd_worker_host_cpu_busy_ratio", "worker" => worker.to_string())
        .set(host.cpu_busy_ratio);
    gauge!("cbsd_worker_host_load1", "worker" => worker.to_string()).set(host.load1);
    gauge!("cbsd_worker_host_mem_total_bytes", "worker" => worker.to_string())
        .set(host.mem_total_bytes as f64);
    gauge!("cbsd_worker_host_mem_used_bytes", "worker" => worker.to_string())
        .set(host.mem_used_bytes as f64);
    gauge!("cbsd_worker_host_mem_available_bytes", "worker" => worker.to_string())
        .set(host.mem_available_bytes as f64);
    gauge!("cbsd_worker_host_swap_total_bytes", "worker" => worker.to_string())
        .set(host.swap_total_bytes as f64);
    gauge!("cbsd_worker_host_swap_used_bytes", "worker" => worker.to_string())
        .set(host.swap_used_bytes as f64);

    // Per-filesystem gauges carry an extra `mount` label. The worker bounds the
    // set to the few mounts it cares about, keeping cardinality small.
    for fs in &host.filesystems {
        gauge!(
            "cbsd_worker_host_fs_total_bytes",
            "worker" => worker.to_string(),
            "mount" => fs.mount.clone(),
        )
        .set(fs.total_bytes as f64);
        gauge!(
            "cbsd_worker_host_fs_used_bytes",
            "worker" => worker.to_string(),
            "mount" => fs.mount.clone(),
        )
        .set(fs.used_bytes as f64);
    }

    // Cumulative host disk IO — counters set to their absolute since-boot value.
    counter!("cbsd_worker_host_disk_read_bytes_total", "worker" => worker.to_string())
        .absolute(host.disk_read_bytes_total);
    counter!("cbsd_worker_host_disk_written_bytes_total", "worker" => worker.to_string())
        .absolute(host.disk_written_bytes_total);

    // Application gauges: ccache (when available) and spool occupancy.
    if let Some(ccache) = &app.ccache {
        gauge!("cbsd_worker_ccache_size_bytes", "worker" => worker.to_string())
            .set(ccache.size_bytes as f64);
        gauge!("cbsd_worker_ccache_max_bytes", "worker" => worker.to_string())
            .set(ccache.max_bytes as f64);
        gauge!("cbsd_worker_ccache_hit_ratio", "worker" => worker.to_string())
            .set(ccache.hit_ratio);
    }
    gauge!("cbsd_worker_spool_bytes", "worker" => worker.to_string()).set(app.spool_bytes as f64);

    // Cumulative application counters. Subprocess exits share the `result`
    // vocabulary with `cbsd_build_results_total` for consistent querying.
    counter!(
        "cbsd_worker_subprocess_exits_total",
        "worker" => worker.to_string(),
        "result" => "success",
    )
    .absolute(app.subprocess_exits.success);
    counter!(
        "cbsd_worker_subprocess_exits_total",
        "worker" => worker.to_string(),
        "result" => "failure",
    )
    .absolute(app.subprocess_exits.failure);
    counter!(
        "cbsd_worker_subprocess_exits_total",
        "worker" => worker.to_string(),
        "result" => "revoked",
    )
    .absolute(app.subprocess_exits.revoked);
    counter!("cbsd_worker_metrics_push_drops_total", "worker" => worker.to_string())
        .absolute(app.push_drops_total);
    // SIGKILL escalations are a worker-side event (the design originally placed
    // this server-side, but only the worker observes it); keep the design's
    // metric name and add the `worker` label like the other pushed counters.
    counter!("cbsd_sigkill_escalations_total", "worker" => worker.to_string())
        .absolute(app.sigkill_escalations_total);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbsd_proto::ws::{CcacheMetrics, FilesystemUsage, SubprocessExitCounts};
    use metrics_exporter_prometheus::PrometheusBuilder;

    fn sample_host() -> HostMetrics {
        HostMetrics {
            cpu_busy_ratio: 0.5,
            load1: 2.0,
            mem_total_bytes: 1000,
            mem_used_bytes: 400,
            mem_available_bytes: 600,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            filesystems: vec![FilesystemUsage {
                mount: "/cbs/spool".to_string(),
                total_bytes: 5000,
                used_bytes: 1200,
            }],
            disk_read_bytes_total: 111,
            disk_written_bytes_total: 222,
        }
    }

    fn sample_app() -> AppMetrics {
        AppMetrics {
            ccache: Some(CcacheMetrics {
                size_bytes: 800,
                max_bytes: 2000,
                hit_ratio: 0.9,
            }),
            subprocess_exits: SubprocessExitCounts {
                success: 7,
                failure: 3,
                revoked: 1,
            },
            spool_bytes: 64,
            push_drops_total: 2,
            sigkill_escalations_total: 5,
        }
    }

    fn render_after<F: FnOnce()>(f: F) -> String {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::with_local_recorder(&recorder, f);
        handle.render()
    }

    #[test]
    fn snapshot_emits_gauges_and_counters_under_worker_label() {
        let out = render_after(|| {
            record_worker_metrics("w-7", 3600, &sample_host(), &sample_app());
        });

        assert!(
            out.contains(r#"cbsd_worker_uptime_seconds{worker="w-7"} 3600"#),
            "uptime gauge missing:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_worker_host_mem_used_bytes{worker="w-7"} 400"#),
            "mem gauge missing:\n{out}"
        );
        // Filesystem gauge carries the extra `mount` label.
        assert!(
            out.contains(r#"cbsd_worker_host_fs_used_bytes{worker="w-7",mount="/cbs/spool"} 1200"#),
            "fs gauge missing/incorrect:\n{out}"
        );
        // Cumulative counters set to their absolute value.
        assert!(
            out.contains(r#"cbsd_worker_host_disk_written_bytes_total{worker="w-7"} 222"#),
            "disk counter missing:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_worker_subprocess_exits_total{worker="w-7",result="failure"} 3"#),
            "subprocess failure counter missing:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_worker_ccache_hit_ratio{worker="w-7"} 0.9"#),
            "ccache gauge missing:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_sigkill_escalations_total{worker="w-7"} 5"#),
            "sigkill-escalation counter missing:\n{out}"
        );
    }

    #[test]
    fn absent_ccache_emits_no_ccache_series() {
        let mut app = sample_app();
        app.ccache = None;
        let out = render_after(|| {
            record_worker_metrics("w-8", 10, &sample_host(), &app);
        });
        assert!(
            !out.contains("cbsd_worker_ccache_"),
            "ccache series must be absent when the worker reports no ccache:\n{out}"
        );
        // Other series are still emitted.
        assert!(out.contains(r#"cbsd_worker_spool_bytes{worker="w-8"} 64"#));
    }

    #[test]
    fn reconnect_same_worker_label_continues_one_series() {
        // Two pushes under the same `worker` label (a reconnect reuses the
        // stable registered_worker_id) update one series, not two.
        let out = render_after(|| {
            let mut app = sample_app();
            record_worker_metrics("w-9", 100, &sample_host(), &app);
            app.spool_bytes = 999;
            record_worker_metrics("w-9", 200, &sample_host(), &app);
        });
        assert!(
            out.contains(r#"cbsd_worker_spool_bytes{worker="w-9"} 999"#),
            "second push should overwrite the gauge for the same worker:\n{out}"
        );
        // Exactly one uptime series line for w-9 (the latest value).
        let uptime_lines = out
            .lines()
            .filter(|l| l.starts_with("cbsd_worker_uptime_seconds{worker=\"w-9\"}"))
            .count();
        assert_eq!(uptime_lines, 1, "expected a single series for w-9:\n{out}");
    }

    /// B1 (design 022): a silent worker's gauges idle-expire on upkeep while a
    /// server-owned gauge re-set within the window survives. Uses the same
    /// GAUGE-only idle-timeout mask as the production `install()`, so the test
    /// exercises the real pruning contract — pushed host gauges disappear once a
    /// worker stops reporting, but server counters/gauges never vanish from
    /// under it. Pruning is render/upkeep-driven, not scrape-driven.
    #[test]
    fn idle_worker_gauges_prune_while_server_gauge_persists() {
        use std::time::Duration;

        use metrics_util::MetricKindMask;

        let recorder = PrometheusBuilder::new()
            .idle_timeout(MetricKindMask::GAUGE, Some(Duration::from_millis(80)))
            .build_recorder();
        let handle = recorder.handle();

        // First pass: ingest a worker snapshot and set a server-owned gauge.
        metrics::with_local_recorder(&recorder, || {
            record_worker_metrics("w-gone", 5, &sample_host(), &sample_app());
            metrics::gauge!("cbsd_builds_active", "arch" => "x86_64").set(3.0);
        });
        // Seed the recency tracker's baseline timestamps; idle pruning compares
        // against the last render, so the first observation can never be idle.
        let _ = handle.render();

        // Age the worker gauges past the idle window, then re-touch ONLY the
        // server gauge so it stays fresh (its generation advances).
        std::thread::sleep(Duration::from_millis(220));
        metrics::with_local_recorder(&recorder, || {
            metrics::gauge!("cbsd_builds_active", "arch" => "x86_64").set(4.0);
        });

        // Upkeep + render drive idle pruning independent of any scrape.
        handle.run_upkeep();
        let out = handle.render();

        assert!(
            !out.contains(r#"cbsd_worker_uptime_seconds{worker="w-gone"}"#),
            "stale worker gauge series must be pruned after the idle window:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_builds_active{arch="x86_64"} 4"#),
            "a server-owned gauge re-set within the window must persist:\n{out}"
        );
    }
}
