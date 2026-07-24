//! Runtime metrics sampler for `GET /api/runtime`.
//!
//! Wraps `sysinfo` with a TTL cache so repeated requests within the TTL window
//! return the same snapshot without re-sampling. The actual `refresh_*` calls
//! are blocking and run inside `spawn_blocking` to avoid stalling tokio workers
//! (same pattern as `LineIndex::build` in `ws.rs`).
//!
//! Sampling only happens on demand — there is no background thread. When nobody
//! is viewing the runtime panel, the cost is effectively zero (just the
//! `System`/`Disks` objects sitting in `AppState`).
//!
//! Concurrency: concurrent callers that find the cache stale serialize on a
//! `tokio::sync::Mutex`. The first caller refreshes via `spawn_blocking`;
//! followers wake up, re-check TTL (now fresh), and return the just-cached
//! snapshot — they do NOT each trigger a separate `refresh_*`. This avoids the
//! thundering-herd pattern where N simultaneous requests each pay the full
//! sysinfo cost.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde::Serialize;
use sysinfo::{Disks, Pid, System};
use tokio::sync::Mutex;

/// Cache TTL: a sample is reused for this long before triggering a fresh
/// `refresh`. Chosen as 5s to match the frontend polling interval — under
/// normal use each poll hits "just expired" and triggers one refresh, which is
/// predictable; rapid tab-switching within the window hits the cache and
/// doesn't thrash.
const SAMPLE_TTL: Duration = Duration::from_secs(5);

/// Runtime snapshot serialized to the frontend. All fields are instantaneous
/// values (not cumulative).
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    /// tailr process RSS memory (bytes). Physical RAM only, excludes shared libs.
    pub process_memory_bytes: u64,
    /// tailr process CPU usage (0.0-100.0 per core; >100 on multi-core).
    /// First sample after start returns 0 (sysinfo needs two samples to diff).
    pub process_cpu_percent: f32,
    /// Total physical memory on the machine (bytes).
    pub system_total_memory_bytes: u64,
    /// Used memory = total - available (bytes).
    pub system_used_memory_bytes: u64,
    /// Global CPU usage across all cores (0.0-100.0 × core count; 400 = 4 cores full).
    pub system_cpu_percent: f32,
    /// Total space on the disk hosting the first log_dir (bytes).
    pub disk_total_bytes: u64,
    /// Used space on that disk (bytes) = total - available.
    pub disk_used_bytes: u64,
}

/// Inner state guarded by a tokio Mutex. The mutex serializes concurrent
/// refresh attempts so only one `spawn_blocking` refresh runs at a time —
/// followers re-check TTL after the lock and return the fresh cache.
struct Inner {
    sys: Arc<std::sync::Mutex<System>>,
    disks: Arc<std::sync::Mutex<Disks>>,
    last_sample_at: Instant,
    cached: RuntimeSnapshot,
}

/// On-demand runtime sampler with TTL cache.
///
/// Held in `AppState` as `Arc<RuntimeSampler>`. Callers invoke
/// [`sample`][Self::sample] which checks the TTL and only re-runs `sysinfo`
/// refresh when the cache is stale.
pub struct RuntimeSampler {
    /// Guards the refresh + cache. `tokio::sync::Mutex` (not std) because we
    /// hold it across the `.await` on `spawn_blocking`. Concurrent callers that
    /// find the cache stale queue here; the first refreshes, the rest wake up
    /// to find a fresh cache and return immediately (no duplicate refresh).
    inner: Mutex<Inner>,
    /// tailr's own PID, captured once at construction.
    pid: Pid,
    /// Log dirs used to pick which disk to report (first log_dir's mount point).
    log_dirs: Vec<PathBuf>,
}

impl RuntimeSampler {
    /// Construct and do an initial full refresh to seed the CPU-usage baseline.
    ///
    /// CPU% requires two samples spaced by `MINIMUM_CPU_UPDATE_INTERVAL`, so
    /// the very first `/api/runtime` response after server start may report
    /// ~0% CPU — this is expected and documented. We refresh once here so the
    /// *second* sample (the first real request) already has a baseline to diff
    /// against.
    pub fn new(log_dirs: Vec<PathBuf>) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        // get_current_pid should never fail on supported platforms, but if it
        // does we log a warning rather than panic — build_snapshot will return
        // 0 for process metrics, which is honest (we couldn't measure them).
        let pid = sysinfo::get_current_pid().unwrap_or_else(|e| {
            tracing::warn!("sysinfo::get_current_pid() failed ({e}); process CPU/memory metrics will report 0");
            Pid::from_u32(0)
        });
        let disks = Disks::new_with_refreshed_list();
        let now = Instant::now();
        let cached = build_snapshot(&sys, &disks, pid, &log_dirs);

        Self {
            inner: Mutex::new(Inner {
                sys: Arc::new(std::sync::Mutex::new(sys)),
                disks: Arc::new(std::sync::Mutex::new(disks)),
                last_sample_at: now,
                cached,
            }),
            pid,
            log_dirs,
        }
    }

    /// Return a snapshot, refreshing from sysinfo only if the cache is older
    /// than `SAMPLE_TTL`. `ws_connections` and `uptime_seconds` are passed in
    /// by the caller (they live on `AppState` and are cheap to read directly,
    /// so they bypass the cache and are always fresh).
    ///
    /// Concurrent callers that find the cache stale serialize on the inner
    /// mutex: the first triggers a `spawn_blocking` refresh; followers wake up,
    /// re-check TTL (now fresh), and return the just-cached snapshot without
    /// triggering their own refresh.
    pub async fn sample(
        &self,
        ws_connections: usize,
        uptime_seconds: u64,
    ) -> (RuntimeSnapshot, usize, u64) {
        // Acquire the mutex. This is the serialization point for concurrent
        // refreshes. Contention is rare (5s TTL, single panel), but when it
        // happens followers just wait for the leader's refresh to finish.
        let mut inner = self.inner.lock().await;

        // Fast path: TTL not expired → return cached snapshot. Re-checked here
        // (after acquiring the lock) so a follower that queued behind a leader
        // returns the fresh cache instead of refreshing again.
        if inner.last_sample_at.elapsed() < SAMPLE_TTL {
            return (inner.cached.clone(), ws_connections, uptime_seconds);
        }

        // Slow path: cache stale — refresh. Clone the Arcs out of the guard
        // so spawn_blocking owns them without borrowing `inner` (which we need
        // to mutably access after .await to update the cache).
        let sys = inner.sys.clone();
        let disks = inner.disks.clone();
        let pid = self.pid;
        let log_dirs = self.log_dirs.clone();

        let refresh_result = tokio::task::spawn_blocking(move || {
            let mut sys = sys.lock().unwrap();
            let mut disks = disks.lock().unwrap();
            // refresh_cpu_usage only updates system/global CPU, NOT per-process
            // CPU. Process::cpu_usage() stays stale unless we explicitly refresh
            // our own PID with cpu enabled. This is a common sysinfo pitfall.
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            // Refresh only tailr's own process (not the whole process list —
            // that would be O(n) over all processes every poll). with_cpu()
            // + with_memory() gives us exactly what build_snapshot reads.
            let pids = [pid];
            sys.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&pids),
                false,
                sysinfo::ProcessRefreshKind::new().with_cpu().with_memory(),
            );
            disks.refresh();
            build_snapshot(&sys, &disks, pid, &log_dirs)
        })
        .await;

        // If the refresh task panicked (e.g. sysinfo edge case), fall back to
        // the stale cached snapshot rather than panicking the handler. Log so
        // the failure is visible; the caller still gets a usable (if stale)
        // response instead of a 500.
        match refresh_result {
            Ok(new_snap) => {
                inner.cached = new_snap.clone();
                inner.last_sample_at = Instant::now();
                (new_snap, ws_connections, uptime_seconds)
            }
            Err(e) => {
                tracing::error!("runtime sample task failed: {e}; returning stale cache");
                (inner.cached.clone(), ws_connections, uptime_seconds)
            }
        }
    }
}

/// Build a snapshot from the given (already-refreshed) sysinfo state.
/// Pure function — no locking, no I/O. Kept separate so it can be unit-tested
/// with synthetic `System`/`Disks` if needed.
fn build_snapshot(
    sys: &System,
    disks: &Disks,
    pid: Pid,
    log_dirs: &[PathBuf],
) -> RuntimeSnapshot {
    let proc_mem = sys.process(pid).map(|p| p.memory()).unwrap_or(0);
    let proc_cpu = sys.process(pid).map(|p| p.cpu_usage()).unwrap_or(0.0);
    let total_mem = sys.total_memory();
    let used_mem = sys.used_memory();
    let sys_cpu = sys.global_cpu_usage();

    let (disk_total, disk_used) = find_relevant_disk(disks, log_dirs)
        .map(|d| (d.total_space(), d.total_space() - d.available_space()))
        .unwrap_or((0, 0));

    RuntimeSnapshot {
        process_memory_bytes: proc_mem,
        process_cpu_percent: proc_cpu,
        system_total_memory_bytes: total_mem,
        system_used_memory_bytes: used_mem,
        system_cpu_percent: sys_cpu,
        disk_total_bytes: disk_total,
        disk_used_bytes: disk_used,
    }
}

/// Pick the disk whose mount point contains the first configured log_dir.
/// Falls back to the largest disk if no match (typical when log_dirs is empty
/// at construction, e.g. in tests).
///
/// Rationale (see design doc §3.4): typical single-disk deployment hits the
/// match; multi-disk is a minority case deferred to a future dashboard. We
/// don't aggregate because summing "used" across disks is semantically
/// meaningless.
fn find_relevant_disk<'a>(disks: &'a Disks, log_dirs: &[PathBuf]) -> Option<&'a sysinfo::Disk> {
    let target = log_dirs
        .first()
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    disks
        .list()
        .iter()
        .find(|d| target.starts_with(d.mount_point()))
        // fallback: largest disk (usually the data disk)
        .or_else(|| disks.list().iter().max_by_key(|d| d.total_space()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_default_is_zero() {
        let s = RuntimeSnapshot::default();
        assert_eq!(s.process_memory_bytes, 0);
        assert_eq!(s.process_cpu_percent, 0.0);
        assert_eq!(s.system_total_memory_bytes, 0);
    }

    #[tokio::test]
    async fn sampler_returns_without_panic() {
        // Construction triggers a real refresh; on any supported platform this
        // must not panic and must seed a non-empty System.
        let sampler = RuntimeSampler::new(vec![]);
        let (snap, ws, uptime) = sampler.sample(1, 100).await;
        // ws_connections / uptime are caller-supplied, always pass through.
        assert_eq!(ws, 1);
        assert_eq!(uptime, 100);
        // total memory should be > 0 on any real machine.
        assert!(snap.system_total_memory_bytes > 0, "system memory should be non-zero");
    }

    #[tokio::test]
    async fn sample_is_cached_within_ttl() {
        let sampler = RuntimeSampler::new(vec![]);
        // First sample (within TTL of construction) should return cached value
        // without triggering a refresh — verified by the fact that two rapid
        // calls return identical snapshots.
        let (s1, _, _) = sampler.sample(1, 100).await;
        let (s2, _, _) = sampler.sample(2, 200).await;
        // ws_connections / uptime differ (caller-supplied, bypass cache), but
        // the sysinfo-derived fields must be identical (cache hit).
        assert_eq!(s1.process_memory_bytes, s2.process_memory_bytes);
        assert_eq!(s1.system_total_memory_bytes, s2.system_total_memory_bytes);
    }

    #[tokio::test]
    async fn concurrent_samples_dont_duplicate_refresh() {
        // Fire multiple concurrent sample() calls simultaneously. With the
        // mutex serialization, only the first refreshes; the rest queue and
        // return the fresh cache. We verify by checking all return the same
        // sysinfo-derived values (they shared one refresh).
        let sampler = Arc::new(RuntimeSampler::new(vec![]));
        // Wait for TTL to expire so the first call actually refreshes.
        tokio::time::sleep(SAMPLE_TTL + Duration::from_millis(50)).await;

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let s = sampler.clone();
                tokio::spawn(async move { s.sample(i, i as u64 * 10).await })
            })
            .collect();
        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        // All sysinfo-derived fields must be identical across the 5 concurrent
        // calls — proving they shared one refresh, not 5 separate ones.
        let first_mem = results[0].0.process_memory_bytes;
        for r in &results {
            assert_eq!(r.0.process_memory_bytes, first_mem);
        }
    }
}
