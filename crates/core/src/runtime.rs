//! Runtime metrics sampler for `GET /api/runtime`.
//!
//! Wraps `sysinfo` with a TTL cache so repeated requests within the TTL window
//! return the same snapshot without re-sampling. This module is **synchronous**
//! (per the core-layer rule "computation stays sync"): the presentation layer
//! (Web) wraps [`RuntimeSampler::sample_blocking`] in `spawn_blocking` to avoid
//! stalling tokio workers.
//!
//! Sampling only happens on demand — there is no background thread. When nobody
//! is viewing the runtime panel, the cost is effectively zero (just the
//! `System`/`Disks` objects sitting in the sampler).
//!
//! Concurrency: concurrent callers that find the cache stale serialize on a
//! `std::sync::Mutex` (the caller is expected to run this on the blocking pool).
//! The first caller refreshes; followers re-check TTL (now fresh) and return the
//! just-cached snapshot — they do NOT each trigger a separate `refresh_*`.
//!
//! Process CPU: sysinfo's `refresh_processes_specifics(Some(&[pid]))` does NOT
//! compute per-process CPU% — `compute_cpu_usage` is only called for
//! `ProcessesToUpdate::All` (sysinfo 0.32 source, system.rs:280). So we read
//! `/proc/[pid]/stat` directly and diff utime+stime between samples, which is
//! exactly what `top` does. This works correctly on old kernels (CentOS 6 /
//! 2.6.32) where sysinfo's path silently returns ~0.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use serde::Serialize;
use sysinfo::{Disks, Pid, System};

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

/// Inner state guarded by a Mutex. The mutex serializes concurrent refresh
/// attempts so only one refresh runs at a time — followers re-check TTL after
/// the lock and return the fresh cache.
struct Inner {
    sys: System,
    disks: Disks,
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
/// Held by the presentation layer (e.g. `AppState` as `Arc<RuntimeSampler>`).
/// Callers invoke [`sample_blocking`][Self::sample_blocking] which checks the
/// TTL and only re-runs `sysinfo` refresh when the cache is stale. The caller
/// is responsible for running this on a blocking thread (`spawn_blocking`).
pub struct RuntimeSampler {
    /// Guards the refresh + cache. Concurrent callers that find the cache
    /// stale queue here; the first refreshes, the rest wake up to find a fresh
    /// cache and return immediately (no duplicate refresh).
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
                sys,
                disks,
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
    /// **Synchronous** — the presentation layer must call this on a blocking
    /// thread (e.g. `tokio::task::spawn_blocking`). Concurrent callers that
    /// find the cache stale serialize on the inner mutex: the first triggers a
    /// refresh; followers re-check TTL (now fresh), and return the just-cached
    /// snapshot without triggering their own refresh.
    pub fn sample_blocking(
        &self,
        ws_connections: usize,
        uptime_seconds: u64,
    ) -> (RuntimeSnapshot, usize, u64) {
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            // Poisoned mutex means a refresh panicked mid-update. The cache
            // may be stale but is still structurally valid — return it rather
            // than poisoning the whole handler. (Matches the original async
            // behavior, which fell back to stale cache on JoinError.)
            Err(e) => {
                tracing::error!("runtime sampler mutex poisoned: {e}; returning stale cache");
                return (e.into_inner().cached.clone(), ws_connections, uptime_seconds);
            }
        };

        // Fast path: TTL not expired → return cached snapshot. Re-checked here
        // (after acquiring the lock) so a follower that queued behind a leader
        // returns the fresh cache instead of refreshing again.
        if inner.last_sample_at.elapsed() < SAMPLE_TTL {
            return (inner.cached.clone(), ws_connections, uptime_seconds);
        }

        // Slow path: cache stale — refresh inline (caller is on blocking thread).
        // refresh_cpu_usage updates system/global CPU only.
        inner.sys.refresh_cpu_usage();
        inner.sys.refresh_memory();
        // Refresh tailr's own process with both cpu + memory. We always
        // enable with_cpu() so sysinfo's cpu_usage() is available as a
        // fallback (see proc_cpu logic below). On macOS this is the primary
        // source; on Linux it's the fallback when /proc/[pid]/stat fails.
        let pids = [self.pid];
        inner.sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&pids),
            false,
            sysinfo::ProcessRefreshKind::new().with_cpu().with_memory(),
        );
        inner.disks.refresh();

        // Process CPU%:
        //   Linux:  primary = /proc/[pid]/stat diff; fallback = sysinfo
        //   macOS:  primary = sysinfo (computed correctly by refresh above)
        //
        // The sysinfo fallback on Linux currently returns ~0 for single-PID
        // refresh (compute_cpu_usage only runs for ProcessesToUpdate::All),
        // but the structure is kept uniform so a future sysinfo fix makes
        // the fallback work automatically without code changes.
        let sysinfo_cpu = inner
            .sys
            .process(self.pid)
            .map(|p| p.cpu_usage())
            .unwrap_or(0.0);

        #[cfg(target_os = "linux")]
        let proc_cpu = {
            let prev = inner.prev_proc_cpu;
            let computed = compute_proc_cpu(self.pid, prev);
            let cpu = computed.unwrap_or(sysinfo_cpu);
            // Update baseline for next sample.
            inner.prev_proc_cpu =
                read_proc_cpu_jiffies(self.pid).map(|jiffies| (Instant::now(), jiffies));
            cpu
        };
        #[cfg(not(target_os = "linux"))]
        let proc_cpu = sysinfo_cpu;

        let new_snap = build_snapshot(&inner.sys, &inner.disks, self.pid, &self.log_dirs, proc_cpu);
        inner.cached = new_snap.clone();
        inner.last_sample_at = Instant::now();
        (new_snap, ws_connections, uptime_seconds)
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
///
/// Returns None when the computation can't be done (no baseline yet, /proc
/// unreadable, elapsed too small). The caller falls back to sysinfo's value
/// in that case — keeping a uniform "primary → fallback" structure across
/// platforms.
#[cfg(target_os = "linux")]
fn compute_proc_cpu(pid: Pid, prev: Option<(Instant, u64)>) -> Option<f32> {
    let (prev_time, prev_jiffies) = prev?;
    let curr_jiffies = read_proc_cpu_jiffies(pid)?;
    let delta_secs = prev_time.elapsed().as_secs_f32();
    if delta_secs < 0.001 {
        return None; // avoid division by near-zero
    }
    let delta_jiffies = curr_jiffies.saturating_sub(prev_jiffies) as f32;
    Some((delta_jiffies / CLK_TCK) / delta_secs * 100.0)
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
        let stat = "12345 (my test proc) S 1 1 1 0 -1 0 0 0 0 0 50 60 0 0 20 0 1 0 0 0 0";
        let jiffies = parse_proc_stat_jiffies(stat).unwrap();
        assert_eq!(jiffies, 110); // utime=50 + stime=60
    }

    #[test]
    fn sampler_returns_without_panic() {
        let sampler = RuntimeSampler::new(vec![]);
        let (snap, ws, uptime) = sampler.sample_blocking(1, 100);
        assert_eq!(ws, 1);
        assert_eq!(uptime, 100);
        assert!(snap.system_total_memory_bytes > 0, "system memory should be non-zero");
    }

    #[test]
    fn sample_is_cached_within_ttl() {
        let sampler = RuntimeSampler::new(vec![]);
        let (s1, _, _) = sampler.sample_blocking(1, 100);
        let (s2, _, _) = sampler.sample_blocking(2, 200);
        assert_eq!(s1.process_memory_bytes, s2.process_memory_bytes);
        assert_eq!(s1.system_total_memory_bytes, s2.system_total_memory_bytes);
    }

    #[test]
    fn concurrent_samples_dont_duplicate_refresh() {
        use std::sync::Arc;
        use std::thread;
        let sampler = Arc::new(RuntimeSampler::new(vec![]));
        thread::sleep(SAMPLE_TTL + Duration::from_millis(50));

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let s = sampler.clone();
                thread::spawn(move || s.sample_blocking(i, i as u64 * 10))
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect();
        let first_mem = results[0].0.process_memory_bytes;
        for r in &results {
            assert_eq!(r.0.process_memory_bytes, first_mem);
        }
    }
}
