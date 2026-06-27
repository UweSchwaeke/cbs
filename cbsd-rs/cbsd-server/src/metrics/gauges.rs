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

//! Server-owned gauges, set by a periodic refresh task.
//!
//! These gauges read live state the server already holds — the in-memory queue
//! and the sqlx pool — so they require no instrumentation at the transition
//! sites. The refresh task doubles as startup resync (the first tick sets
//! correct values) and is load-bearing: a GAUGE `idle_timeout` would prune
//! these series between scrapes if they were not re-set on a cadence inside the
//! timeout (design 022).

use std::collections::HashMap;
use std::time::Duration;

use cbsd_proto::{Arch, Priority};
use metrics::gauge;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::SqlitePool;

use crate::app::AppState;
use crate::queue::BuildQueue;

/// All architectures, for zero-baselining the enumerable label space.
const ARCHES: [Arch; 2] = [Arch::X86_64, Arch::Aarch64];

/// All priorities, for zero-baselining the enumerable label space.
const PRIORITIES: [Priority; 3] = [Priority::High, Priority::Normal, Priority::Low];

/// Label string for a priority (matches the lowercase serde form).
fn priority_label(p: Priority) -> &'static str {
    match p {
        Priority::High => "high",
        Priority::Normal => "normal",
        Priority::Low => "low",
    }
}

/// Set every server-owned gauge from the current queue and pool state.
///
/// The enumerable label spaces (priority×arch, arch, worker state×arch, pool
/// state) are zero-baselined each call so an emptied series reports `0` rather
/// than leaving a stale value or a gap, and stays refreshed so it is never
/// idle-pruned. Emits to whichever recorder is active (global in production,
/// thread-local under test).
pub fn emit_server_gauges(queue: &BuildQueue, pool: &SqlitePool) {
    // Queue depth by (priority, arch).
    let mut queued: HashMap<(&'static str, String), u64> = HashMap::new();
    for p in PRIORITIES {
        for a in ARCHES {
            queued.insert((priority_label(p), a.to_string()), 0);
        }
    }
    for (p, a, n) in queue.queued_by_priority_arch() {
        queued.insert((priority_label(p), a.to_string()), n);
    }
    for ((priority, arch), n) in &queued {
        gauge!("cbsd_builds_queued", "priority" => *priority, "arch" => arch.clone())
            .set(*n as f64);
    }

    // Active builds by arch.
    let mut active: HashMap<String, u64> = ARCHES.iter().map(|a| (a.to_string(), 0)).collect();
    for ab in queue.active.values() {
        *active
            .entry(ab.descriptor.build.arch.to_string())
            .or_insert(0) += 1;
    }
    for (arch, n) in &active {
        gauge!("cbsd_builds_active", "arch" => arch.clone()).set(*n as f64);
    }

    // Connected workers by (state, arch). Invariant: `Connected`/`Disconnected`
    // always carry an arch (so `ws.arch()` is `Some`), while `Stopping`/`Dead`
    // never do (`None` → `unknown`). The baseline below mirrors exactly that, so
    // every emitted key is pre-seeded; a future `WorkerState` variant must keep
    // this alignment or it would emit an un-baselined series.
    let mut workers: HashMap<(&'static str, String), u64> = HashMap::new();
    for st in ["connected", "disconnected"] {
        for a in ARCHES {
            workers.insert((st, a.to_string()), 0);
        }
    }
    for st in ["stopping", "dead"] {
        workers.insert((st, "unknown".to_string()), 0);
    }
    for ws in queue.workers.values() {
        let arch = ws
            .arch()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        *workers.entry((ws.state_name(), arch)).or_insert(0) += 1;
    }
    for ((state, arch), n) in &workers {
        gauge!("cbsd_workers_connected", "state" => *state, "arch" => arch.clone()).set(*n as f64);
    }

    // SQLite pool saturation. sqlx exposes no direct acquired count, so derive
    // it as size - idle. The 4-connection cap is a known deadlock risk
    // (CLAUDE.md invariant #2); making saturation observable is the point.
    let size = u64::from(pool.size());
    let idle = pool.num_idle() as u64;
    let acquired = size.saturating_sub(idle);
    gauge!("cbsd_db_pool_connections", "state" => "acquired").set(acquired as f64);
    gauge!("cbsd_db_pool_connections", "state" => "idle").set(idle as f64);
}

/// Periodic refresh loop: every `refresh` seconds, re-set the server-owned
/// gauges and run recorder upkeep (which performs idle pruning of stale worker
/// gauges independent of scrape liveness — design 022 B1). Runs until aborted.
pub async fn run_gauge_refresh(state: AppState, handle: PrometheusHandle, refresh: Duration) {
    let mut ticker = tokio::time::interval(refresh);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        {
            let queue = state.queue.lock().await;
            emit_server_gauges(&queue, &state.pool);
        }
        handle.run_upkeep();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::test_support::test_pool;
    use crate::ws::liveness::WorkerState;
    use metrics_exporter_prometheus::PrometheusBuilder;

    /// Emit gauges under a thread-local recorder and return the rendered
    /// exposition, so tests need not install a global recorder.
    fn render_after_emit(queue: &BuildQueue, pool: &SqlitePool) -> String {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::with_local_recorder(&recorder, || emit_server_gauges(queue, pool));
        handle.render()
    }

    #[tokio::test]
    async fn empty_queue_renders_zero_baselined_gauges() {
        let pool = test_pool().await;
        let queue = BuildQueue::new();

        let out = render_after_emit(&queue, &pool);

        // Zero-baselined series are present even with nothing queued.
        assert!(
            out.contains("cbsd_builds_queued"),
            "queue-depth gauge missing:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_builds_queued{priority="high",arch="x86_64"} 0"#),
            "expected zero-baselined high/x86_64 series:\n{out}"
        );
        // Pool gauge reflects the live pool (min_connections=1 ⇒ size ≥ 1).
        assert!(
            out.contains(r#"cbsd_db_pool_connections{state="idle"}"#),
            "pool idle gauge missing:\n{out}"
        );
        assert!(
            out.contains(r#"cbsd_db_pool_connections{state="acquired"}"#),
            "pool acquired gauge missing:\n{out}"
        );
    }

    #[tokio::test]
    async fn seeded_worker_reflected_in_gauge() {
        let pool = test_pool().await;
        let mut queue = BuildQueue::new();
        queue.register_worker(
            "conn-1".to_string(),
            WorkerState::Connected {
                registered_worker_id: "w-1".to_string(),
                worker_name: "worker-1".to_string(),
                arch: Arch::X86_64,
                cores_total: 8,
                ram_total_mb: 16_384,
                version: None,
            },
        );

        let out = render_after_emit(&queue, &pool);

        assert!(
            out.contains(r#"cbsd_workers_connected{state="connected",arch="x86_64"} 1"#),
            "expected one connected x86_64 worker:\n{out}"
        );
        // A state with no workers is still zero-baselined.
        assert!(
            out.contains(r#"cbsd_workers_connected{state="connected",arch="aarch64"} 0"#),
            "expected zero-baselined connected/aarch64 series:\n{out}"
        );
    }
}
