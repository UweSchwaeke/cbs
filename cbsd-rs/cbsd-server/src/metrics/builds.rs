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

//! Build-outcome metrics, emitted from the terminal-state choke point in
//! `db::builds::set_build_finished` so every finish path (worker-reported,
//! integrity reject, dead-worker resolution, revoke timeout, drain) is counted
//! exactly once.

use metrics::{counter, histogram};

/// Record a finished build: increment the result counter and, when the build
/// actually ran, observe its wall-clock duration.
///
/// The `worker` label is the stable `registered_worker_id` (reused across
/// reconnects), so its cardinality is bounded by the number of distinct workers
/// ever seen — a small, slowly-growing set for a build farm, which the design
/// accepts (proposal 002 / design 022). It is not a per-connection identity.
///
/// F6 duration guard: a build with no `started_at` (e.g. revoked or failed
/// before it ever started — `started_at` was cleared by
/// `rollback_dispatch_to_queued`) is counted in the result counter but
/// contributes no duration sample. `finished_at < started_at` is likewise
/// rejected as nonsensical rather than recorded as a negative duration.
pub fn record_build_finished(
    result: &str,
    arch: &str,
    worker: &str,
    periodic: bool,
    started_at: Option<i64>,
    finished_at: Option<i64>,
) {
    counter!(
        "cbsd_build_results_total",
        "result" => result.to_string(),
        "arch" => arch.to_string(),
        "periodic" => if periodic { "true" } else { "false" },
        "worker" => worker.to_string(),
    )
    .increment(1);

    if let (Some(started), Some(finished)) = (started_at, finished_at)
        && finished >= started
    {
        histogram!(
            "cbsd_build_duration_seconds",
            "result" => result.to_string(),
            "arch" => arch.to_string(),
            "worker" => worker.to_string(),
        )
        .record((finished - started) as f64);
    }
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
    fn finished_build_records_result_and_duration() {
        let out = render_after(|| {
            record_build_finished("success", "x86_64", "w-1", false, Some(100), Some(460));
        });

        assert!(
            out.contains(
                r#"cbsd_build_results_total{result="success",arch="x86_64",periodic="false",worker="w-1"} 1"#
            ),
            "result counter missing/incorrect:\n{out}"
        );
        // 360s duration falls in the 480s bucket but above 240s.
        assert!(
            out.contains("cbsd_build_duration_seconds"),
            "duration histogram missing:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_build_duration_seconds_sum{result="success",arch="x86_64",worker="w-1"} 360"#),
            "duration sum should be 360s:\n{out}"
        );
    }

    #[test]
    fn revoked_before_start_records_result_but_no_duration() {
        // F6: no started_at ⇒ counted, but no duration sample.
        let out = render_after(|| {
            record_build_finished("revoked", "aarch64", "w-2", false, None, Some(500));
        });

        assert!(
            out.contains(r#"cbsd_build_results_total{result="revoked",arch="aarch64",periodic="false",worker="w-2"} 1"#),
            "result counter missing:\n{out}"
        );
        assert!(
            !out.contains("cbsd_build_duration_seconds"),
            "no duration histogram should be emitted for a build with no start:\n{out}"
        );
    }

    #[test]
    fn periodic_label_set_for_scheduled_builds() {
        let out = render_after(|| {
            record_build_finished("failure", "x86_64", "w-3", true, Some(10), Some(70));
        });
        assert!(
            out.contains(r#"periodic="true""#),
            "expected periodic=true label:\n{out}"
        );
    }
}
