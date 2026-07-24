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
//!
//! Process CPU: sysinfo's `refresh_processes_specifics(Some(&[pid]))` does NOT
//! compute per-process CPU% — `compute_cpu_usage` is only called for
//! `ProcessesToUpdate::All` (sysinfo 0.32 source, system.rs:280). So we read
//! `/proc/[pid]/stat` directly and diff utime+stime between samples, which is
//! exactly what `top` does. This works correctly on old kernels (CentOS 6 /
//! 2.6.32) where sysinfo's path silently returns ~0.

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

/// Clock ticks per second for `/proc/[pid]/stat` time fields. On all
/// Linux/x86_64 and Linux/aarch64 targets this is 100 (USER_HZ / CLK_TCK).
/// Hardcoding avoids a libc sysconf call; if a weird platform ever differs,
/// the CPU% will be proportionally off but won't crash.
#[cfg(target_os = "linux")]
const CLK_TCK: f32 = 100.0;

/// Runtime snapshot serialized to the frontend. All fields are instantaneous
/// values (not cumulative).
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    /// tailr process RSS memory (bytes). Physical RAM only, excludes shared libs.
    pub process_memory_bytes: u64,
    /// tailr process CPU usage (0.0-100.0 per core; >100 on multi-core).
    /// First sample after start returns 0 (needs two samples to diff).
    pub process_cpu_percent: f32,
    /// Total physical memory on the machine (bytes).
    pub system_total_memory_bytes: u64,
    /// Used memory (bytes). On Linux without MemAvailable (CentOS 6 etc),
    /// this is close to AnonPages — memory that cannot be reclaimed, excluding
    /// buffers/cached. This is a more meaningful "used" than `free`'s first
    /// line (which counts reclaimable cache as used).
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
    /// Previous `/proc/[pid]/stat` jiffies (utime+stime) + timestamp, for
    /// diffing process CPU% on Linux. None until the first real sample.
    /// Unused on macOS (sysinfo computes CPU% directly).
    #[cfg(target_os = "linux")]
    prev_proc_cpu: Option<(Instant, u64)>,
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
    /// CPU% requires two samples spaced in time, so the very first
    /// `/api/runtime` response after server start may report ~0% CPU — this is
    /// expected and documented. We seed `prev_proc_cpu` here so the second
    /// sample (the first real request) already has a baseline to diff against.
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

        // Seed process CPU baseline (Linux only): read /proc/[pid]/stat once
        // so the next sample has something to diff against.
        #[cfg(target_os = "linux")]
        let prev_proc_cpu = read_proc_cpu_jiffies(pid).map(|jiffies| (now, jiffies));

        // First snapshot: process CPU is 0 (no diff yet). The baseline seeded
        // above makes the *next* sample accurate.
        let cached = build_snapshot(&sys, &disks, pid, &log_dirs, 0.0);

        Self {
            inner: Mutex::new(Inner {
                sys: Arc::new(std::sync::Mutex::new(sys)),
                disks: Arc::new(std::sync::Mutex::new(disks)),
                last_sample_at: now,
                cached,
                #[cfg(target_os = "linux")]
                prev_proc_cpu,
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
        #[cfg(target_os = "linux")]
        let prev_proc_cpu = inner.prev_proc_cpu;

        let refresh_result = tokio::task::spawn_blocking(move || {
            let mut sys = sys.lock().unwrap();
            let mut disks = disks.lock().unwrap();
            // refresh_cpu_usage updates system/global CPU only.
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            // Refresh tailr's own process. On macOS, sysinfo's
            // refresh_processes_specifics DOES compute per-process CPU% (unlike
            // Linux where compute_cpu_usage only runs for ProcessesToUpdate::All).
            // So we enable with_cpu() on macOS to get it from sysinfo, and on
            // Linux we read /proc/[pid]/stat directly (see compute_proc_cpu).
            let pids = [pid];
            #[cfg(target_os = "macos")]
            let refresh_kind = sysinfo::ProcessRefreshKind::new().with_cpu().with_memory();
            #[cfg(target_os = "linux")]
            let refresh_kind = sysinfo::ProcessRefreshKind::new().with_memory();
            sys.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&pids),
                false,
                refresh_kind,
            );
            disks.refresh();

            // Process CPU%: on Linux, diff /proc/[pid]/stat jiffies (sysinfo's
            // value is stale — see module doc). On macOS, use sysinfo's value
            // (computed correctly above).
            #[cfg(target_os = "linux")]
            let proc_cpu = compute_proc_cpu(pid, prev_proc_cpu);
            #[cfg(not(target_os = "linux"))]
            let proc_cpu = sys.process(pid).map(|p| p.cpu_usage()).unwrap_or(0.0);

            build_snapshot(&sys, &disks, pid, &log_dirs, proc_cpu)
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

/// Read `/proc/[pid]/stat` and extract utime + stime (fields 14 + 15), the
/// CPU time spent in user + kernel mode measured in clock ticks.
///
/// Returns None if the file can't be read/parsed (process died, permission
/// denied). In that case process CPU% falls back to 0.
#[cfg(target_os = "linux")]
fn read_proc_cpu_jiffies(pid: Pid) -> Option<u64> {
    let stat_path = PathBuf::from("/proc").join(pid.as_u32().to_string()).join("stat");
    let stat_data = std::fs::read_to_string(&stat_path).ok()?;
    parse_proc_stat_jiffies(&stat_data)
}

/// Parse the utime+stime (jiffies) out of a `/proc/[pid]/stat` line.
///
/// Field layout (1-indexed): ... 14=utime 15=stime ...
/// The comm field (2) is parenthesized and may contain spaces or parens,
/// so we split on ')' first to skip past it safely.
#[cfg(target_os = "linux")]
fn parse_proc_stat_jiffies(stat: &str) -> Option<u64> {
    // Skip past the comm field: everything after the last ')'.
    let after_comm = stat.rsplit_once(')')?.1;
    let mut fields = after_comm.split_whitespace();
    // Fields after ')' start at field 3 (state). Skip to field 14 (utime):
    // that's 14 - 3 = 11 fields to skip.
    for _ in 0..11 {
        fields.next()?;
    }
    let utime: u64 = fields.next()?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some(utime + stime)
}

/// Compute process CPU% by diffing the current jiffies against the previous
/// sample. `prev` is (timestamp, jiffies) from the last refresh.
///
/// Formula: (delta_jiffies / CLK_TCK) / delta_seconds * 100
/// This matches what `top` reports: percentage of one CPU core. On a 4-core
/// machine, 100% = one core fully used, 400% = all cores.
#[cfg(target_os = "linux")]
fn compute_proc_cpu(pid: Pid, prev: Option<(Instant, u64)>) -> f32 {
    let (prev_time, prev_jiffies) = match prev {
        Some(v) => v,
        None => return 0.0, // no baseline yet (first real sample)
    };

    #[cfg(target_os = "linux")]
    {
        let curr_jiffies = match read_proc_cpu_jiffies(pid) {
            Some(j) => j,
            None => return 0.0,
        };
        let delta_secs = prev_time.elapsed().as_secs_f32();
        if delta_secs < 0.001 {
            return 0.0; // avoid division by near-zero
        }
        let delta_jiffies = curr_jiffies.saturating_sub(prev_jiffies) as f32;
        (delta_jiffies / CLK_TCK) / delta_secs * 100.0
    }

    #[cfg(not(target_os = "linux"))]
    {
        0.0 // macOS: /proc not available; sysinfo's value is used via build_snapshot
    }
}

/// Build a snapshot from the given (already-refreshed) sysinfo state.
/// Pure function — no locking, no I/O. `proc_cpu` is pre-computed by the
/// caller (via compute_proc_cpu) since it needs the previous sample's jiffies.
fn build_snapshot(
    sys: &System,
    disks: &Disks,
    pid: Pid,
    log_dirs: &[PathBuf],
    proc_cpu: f32,
) -> RuntimeSnapshot {
    let proc_mem = sys.process(pid).map(|p| p.memory()).unwrap_or(0);
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

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_proc_stat_handles_parens_in_comm() {
        // Real-world /proc/[pid]/stat (CentOS 6 tailr process). The comm
        // field has no spaces here, but the parser must handle cases where it
        // does (e.g. process named "my (test) app").
        let stat = "26782 (tailr) S 1 26781 26781 0 -1 4202816 3977 0 0 0 109 154 0 0 20 0 7 0 1004153259 26619904 1787";
        let jiffies = parse_proc_stat_jiffies(stat).unwrap();
        // utime=109 (field 14) + stime=154 (field 15) = 263
        assert_eq!(jiffies, 263);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_proc_stat_handles_spaces_in_comm() {
        // Process named "my test proc" — comm has spaces. The rsplit_once(')')
        // approach correctly skips the entire parenthesized field.
        // Fields after ')': 3=S 4=ppid 5=pgrp 6=sid 7=tty 8=tpgid 9=flags
        // 10=minflt 11=cminflt 12=majflt 13=cmajflt 14=utime 15=stime ...
        let stat = "12345 (my test proc) S 1 1 1 0 -1 0 0 0 0 0 0 50 60 0 0 20 0 1 0 0 0 0";
        let jiffies = parse_proc_stat_jiffies(stat).unwrap();
        assert_eq!(jiffies, 110); // utime=50 + stime=60
    }

    #[tokio::test]
    async fn sampler_returns_without_panic() {
        let sampler = RuntimeSampler::new(vec![]);
        let (snap, ws, uptime) = sampler.sample(1, 100).await;
        assert_eq!(ws, 1);
        assert_eq!(uptime, 100);
        assert!(snap.system_total_memory_bytes > 0, "system memory should be non-zero");
    }

    #[tokio::test]
    async fn sample_is_cached_within_ttl() {
        let sampler = RuntimeSampler::new(vec![]);
        let (s1, _, _) = sampler.sample(1, 100).await;
        let (s2, _, _) = sampler.sample(2, 200).await;
        assert_eq!(s1.process_memory_bytes, s2.process_memory_bytes);
        assert_eq!(s1.system_total_memory_bytes, s2.system_total_memory_bytes);
    }

    #[tokio::test]
    async fn concurrent_samples_dont_duplicate_refresh() {
        let sampler = Arc::new(RuntimeSampler::new(vec![]));
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
        let first_mem = results[0].0.process_memory_bytes;
        for r in &results {
            assert_eq!(r.0.process_memory_bytes, first_mem);
        }
    }
}
