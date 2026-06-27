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

//! Host resource sampling via `sysinfo` (design 021).
//!
//! The sampler is **process-global** behind a `std::sync::Mutex`: CPU usage is
//! a delta between two refreshes, so the underlying `System` must outlive any
//! single WebSocket connection or every reconnect would report a meaningless
//! first-sample CPU of zero. A reconnect spawns a fresh sampler *task*, but they
//! all read the one long-lived `System` here.
//!
//! Sampling is synchronous CPU/syscall work with no `.await`, so a blocking
//! `std::sync::Mutex` is correct (and avoids holding a tokio mutex across the
//! refresh). The caller runs it from the sampler task on the worker's runtime.

use std::sync::{Mutex, OnceLock};

use cbsd_proto::ws::{FilesystemUsage, HostMetrics};
use sysinfo::{Disks, System};

/// Process-global host sampler. Lazily initialised on first use.
static SAMPLER: OnceLock<HostSampler> = OnceLock::new();

/// Returns the process-global [`HostSampler`], creating it on first call.
pub fn global() -> &'static HostSampler {
    SAMPLER.get_or_init(HostSampler::new)
}

/// Long-lived host sampler. Holds the `sysinfo` state whose CPU delta must
/// persist across reconnects.
pub struct HostSampler {
    inner: Mutex<Inner>,
}

struct Inner {
    system: System,
    disks: Disks,
}

impl HostSampler {
    fn new() -> Self {
        // A freshly-created `System` has no prior CPU snapshot, so the first
        // `global_cpu_usage()` reads 0.0 until the second refresh — the
        // documented first-sample zeroing we accept rather than sleep for.
        let system = System::new();
        let disks = Disks::new_with_refreshed_list();
        Self {
            inner: Mutex::new(Inner { system, disks }),
        }
    }

    /// Take one host sample. CPU is the busy fraction since the previous call.
    pub fn sample(&self) -> HostMetrics {
        let mut inner = self.inner.lock().expect("host sampler mutex poisoned");
        let Inner { system, disks } = &mut *inner;

        system.refresh_cpu_usage();
        system.refresh_memory();
        disks.refresh(true);

        // `global_cpu_usage` is a percentage across all cores; normalise to a
        // 0.0–1.0 fraction for the `_ratio` metric name.
        let cpu_busy_ratio = (system.global_cpu_usage() as f64 / 100.0).clamp(0.0, 1.0);
        let load1 = System::load_average().one;

        let filesystems = disks
            .iter()
            .map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();
                FilesystemUsage {
                    mount: disk.mount_point().to_string_lossy().into_owned(),
                    total_bytes: total,
                    used_bytes: total.saturating_sub(available),
                }
            })
            .collect();

        // Cumulative since-monitoring byte counters, summed across devices. The
        // server republishes these as counters, so the wire value must be
        // monotonic — `total_*_bytes`, not the per-refresh `*_bytes` deltas.
        let (disk_read_bytes_total, disk_written_bytes_total) =
            disks.iter().fold((0u64, 0u64), |(r, w), disk| {
                let u = disk.usage();
                (
                    r.saturating_add(u.total_read_bytes),
                    w.saturating_add(u.total_written_bytes),
                )
            });

        HostMetrics {
            cpu_busy_ratio,
            load1,
            mem_total_bytes: system.total_memory(),
            mem_used_bytes: system.used_memory(),
            mem_available_bytes: system.available_memory(),
            swap_total_bytes: system.total_swap(),
            swap_used_bytes: system.used_swap(),
            filesystems,
            disk_read_bytes_total,
            disk_written_bytes_total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_sample_is_plausible() {
        let sampler = HostSampler::new();
        // First sample primes the CPU delta (ratio may be 0).
        let _ = sampler.sample();
        let snap = sampler.sample();

        // Memory totals must be populated on a real host.
        assert!(snap.mem_total_bytes > 0, "mem_total should be non-zero");
        assert!(
            snap.mem_used_bytes <= snap.mem_total_bytes,
            "used must not exceed total"
        );
        assert!(
            (0.0..=1.0).contains(&snap.cpu_busy_ratio),
            "cpu ratio out of range: {}",
            snap.cpu_busy_ratio
        );
    }
}
