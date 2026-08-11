//! Web-specific upgrade wrapper: upgrade then delegate restart to the
//! `tailr restart` subcommand.
//!
//! The pure upgrade logic (download + atomic binary replacement) lives in
//! `tailr_core::upgrade_engine` ([`UpgradeEngine`]). This module adds the
//! Web-specific concerns: a TTL cache for `check_update`, a concurrency lock
//! for `perform_upgrade`, and post-upgrade restart delegation.
//!
//! Both methods offload the synchronous `self_update` (reqwest blocking) work to
//! `spawn_blocking`. reqwest's blocking client spins up its own tokio runtime on a
//! helper thread; dropping it from within an async context panics
//! ("Cannot drop a runtime in a context where blocking is not allowed").
//! `spawn_blocking` runs the call on the blocking pool, outside the async runtime.

// Re-export the core types so existing references (`upgrade::UpgradeEngine`,
// `upgrade::UpdateInfo`, `upgrade::UpgradeResult`) keep working.
pub use tailr_core::upgrade_engine::{UpdateInfo, UpgradeEngine, UpgradeResult};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

/// Web-specific upgrade wrapper: upgrade then delegate restart to the
/// `tailr restart` subcommand.
///
/// The CLI entry point (`run_upgrade`) does **not** use this — it only needs pure
/// upgrade (via `UpgradeEngine` directly) and lets the user restart manually.
/// Restart semantics live here so they don't pollute the shared engine.
pub struct UpgradeService {
    engine: Arc<UpgradeEngine>,
    /// Cached result of the last GitHub check, with its fetch timestamp.
    /// Background polling refreshes this; `check_update` serves from cache when fresh.
    cache: Arc<RwLock<Option<(UpdateInfo, Instant)>>>,
    /// In-progress flag for `perform_upgrade`. Set to `true` when an upgrade
    /// starts, cleared when the detached upgrade task ends (success / failure /
    /// timeout / panic). Concurrent callers see `UPGRADE_IN_PROGRESS` while set.
    ///
    /// `AtomicBool` (not `Mutex`) so the "held" state can move into a detached
    /// `'static` task without lifetime issues — a `MutexGuard` borrows the lock
    /// and can't escape the method body, but an `Arc<AtomicBool>` is freely
    /// cloneable and `Send`.
    upgrade_in_progress: Arc<AtomicBool>,
}

/// Cache lifetime + poll interval. GitHub unauthenticated API allows 60 req/hour
/// per IP; one check per 6h is ~4/day — far under the limit, yet timely enough for
/// release cadence (days/weeks).
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Delay the first check after startup so it never blocks initial responsiveness.
const INITIAL_DELAY: Duration = Duration::from_secs(30);
/// Hard timeout for the upgrade (download + atomic replace). self_update uses
/// reqwest's blocking client with no built-in timeout; without this cap a stalled
/// GitHub download would occupy a blocking-pool thread (only 4 exist) forever,
/// hanging the upgrade flow and any future upgrade attempts. 5 minutes is ample
/// for a binary of tailr's size (~10 MB) on any reasonable connection.
const UPGRADE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

impl UpgradeService {
    pub fn new() -> Self {
        Self {
            // Inject the *server crate's* version (= the binary's, both bumped
            // together at release). Using `UpgradeEngine::default()` would pick up
            // `tailr-core`'s version, which is versioned independently and lags
            // behind the binary — that silently broke version comparison (core was
            // stuck at 0.12.x while the binary shipped 1.0.x).
            engine: Arc::new(UpgradeEngine::with_version(env!("CARGO_PKG_VERSION"))),
            cache: Arc::new(RwLock::new(None)),
            upgrade_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether an upgrade is currently in progress (download/replace/restart).
    /// Exposed via `/api/health` so the frontend can show a spinner and disable
    /// the upgrade button — survives page refresh (the flag lives in the process,
    /// not the browser).
    pub fn is_upgrade_in_progress(&self) -> bool {
        self.upgrade_in_progress.load(Ordering::SeqCst)
    }

    /// Serve from cache if fresh; otherwise fetch from GitHub (spawn_blocking).
    /// `force` bypasses the cache for an explicit user-triggered refresh.
    pub async fn check_update(&self, force: bool) -> Result<UpdateInfo, String> {
        if !force {
            let cache = self.cache.read().await;
            if let Some((info, fetched_at)) = cache.as_ref() {
                if fetched_at.elapsed() < CHECK_INTERVAL {
                    return Ok(info.clone());
                }
            }
        }
        let engine = self.engine.clone();
        let info = tokio::task::spawn_blocking(move || engine.check_update())
            .await
            .map_err(|e| format!("upgrade check task failed: {e}"))??;
        *self.cache.write().await = Some((info.clone(), Instant::now()));
        Ok(info)
    }

    /// Web upgrade: trigger a fully detached background task that does
    /// download → atomic replace → restart scheduling. The HTTP handler returns
    /// immediately after triggering; the upgrade runs to completion **even if the
    /// client disconnects mid-upgrade**.
    ///
    /// Why detached: the previous design awaited the download in the request
    /// future. If the browser tab closed mid-upgrade, axum dropped the future,
    /// the post-replace `spawn_restart()` never ran, and the process was left
    /// with a replaced on-disk binary but no restart — locking out further
    /// upgrade/restart attempts. Detaching decouples the upgrade lifecycle from
    /// the connection lifecycle.
    ///
    /// Restart goes through `Commands::Restart`, which uses `stop_daemon`
    /// (graceful shutdown, PID cleanup) + re-exec — not a raw `exit(0)`.
    pub async fn perform_upgrade(&self) -> Result<UpgradeResult, String> {
        // Atomically claim the upgrade slot. `compare_exchange` ensures only one
        // caller wins; everyone else gets UPGRADE_IN_PROGRESS immediately.
        if self
            .upgrade_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("UPGRADE_IN_PROGRESS".to_string());
        }

        tracing::info!("upgrade started (detached task will continue after response)");
        let engine = self.engine.clone();
        let cache = self.cache.clone();
        let flag = self.upgrade_in_progress.clone();

        tokio::spawn(async move {
            // `UpgradeGuard` clears `flag` on drop — runs on every exit path
            // (success, error, timeout, panic-unwind via the spawn task dying).
            let _guard = UpgradeGuard(flag);

            // Download + atomic replace, with a hard timeout. self_update uses
            // reqwest's blocking client; without a timeout a stalled GitHub API
            // call would occupy a blocking-pool thread (only 4 exist) forever.
            let upgrade_result =
                match tokio::time::timeout(UPGRADE_TIMEOUT, tokio::task::spawn_blocking(
                    move || engine.perform_upgrade(),
                ))
                .await
                {
                    Ok(Ok(Ok(version))) => {
                        tracing::info!(
                            version = %version,
                            "binary replaced successfully, scheduling restart"
                        );
                        Ok(version)
                    }
                    Ok(Ok(Err(e))) => {
                        tracing::error!("upgrade failed: {e}");
                        Err(e)
                    }
                    Ok(Err(join_err)) => {
                        let e = format!("upgrade task panicked/join failed: {join_err}");
                        tracing::error!("{e}");
                        Err(e)
                    }
                    Err(_) => {
                        let e = format!(
                            "upgrade timed out after {}s",
                            UPGRADE_TIMEOUT.as_secs()
                        );
                        tracing::error!("{e}");
                        Err(e)
                    }
                };

            if let Ok(_version) = upgrade_result {
                // Invalidate the update cache: it holds the pre-upgrade result
                // (hasUpdate=true for the version we just installed). Without
                // this, any check between now and restart serves a stale
                // "update available".
                *cache.write().await = None;
                tracing::info!("update cache invalidated after upgrade");
                // Defer restart so the HTTP response (already sent) is flushed
                // and any in-flight requests complete. Detached, so even if the
                // original client is gone this still fires.
                tokio::spawn(async {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    tracing::info!("spawning restart subprocess");
                    if let Err(e) = spawn_restart() {
                        tracing::error!("failed to spawn restart after upgrade: {e}");
                    } else {
                        tracing::info!("restart subprocess spawned successfully");
                    }
                });
            }
            // On failure/timeout the binary is unchanged — no restart, no cache
            // invalidation. `_guard` drops here, clearing the in-progress flag.
        });

        Ok(UpgradeResult {
            status: "started".to_string(),
            message: "upgrade started; restart will follow automatically".to_string(),
        })
    }

    /// Spawn the background update-check loop. Checks GitHub every 6h; on detecting
    /// a *new* version (transition from none/old → newer), broadcasts
    /// `UpdateAvailable` to all WS clients. Network errors are logged and swallowed
    /// — a failed check never disturbs the user.
    pub fn start_background_check(self: &Arc<Self>, state: Arc<crate::AppState>) {
        let service = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(INITIAL_DELAY).await;
            let mut last_seen_version: Option<String> = None;
            loop {
                match service.check_update(false).await {
                    Ok(info) => {
                        if info.has_update {
                            // Only broadcast when the latest version changed since
                            // our last check (avoids re-notifying on every poll).
                            if last_seen_version.as_deref() != Some(&info.latest_version)
                            {
                                tracing::info!(
                                    latest = %info.latest_version,
                                    "new version detected, broadcasting UpdateAvailable"
                                );
                                crate::ws::broadcast(
                                    &state,
                                    tailr_protocol::WSMessage::UpdateAvailable {
                                        latest_version: info.latest_version.clone(),
                                        current_version: info.current_version.clone(),
                                        release_url: info.release_url.clone(),
                                    },
                                )
                                .await;
                            }
                            last_seen_version = Some(info.latest_version);
                        } else {
                            last_seen_version = None;
                        }
                    }
                    Err(e) => {
                        // Silent failure: update-check is best-effort. Never surface
                        // network errors to the user as toasts.
                        tracing::warn!("background update check failed: {e}");
                    }
                }
                tokio::time::sleep(CHECK_INTERVAL).await;
            }
        });
    }
}

impl Default for UpgradeService {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that clears the `upgrade_in_progress` flag when dropped.
///
/// Moved into the detached upgrade task so the flag is released on every exit
/// path: success, returned `Err`, timeout, and task cancellation (the runtime
/// dropping the task future). Without this, a panic or cancellation would leave
/// the flag stuck at `true`, permanently locking out future upgrades.
struct UpgradeGuard(Arc<AtomicBool>);

impl Drop for UpgradeGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Wrap an [`UpgradeService`] for `AppState`. Centralizes the `Arc` so handlers
/// don't repeat it.
pub fn shared_service() -> Arc<UpgradeService> {
    Arc::new(UpgradeService::new())
}

/// Spawn `tailr restart` as a detached subprocess, with a fallback.
///
/// Primary path: the exe persisted in `tailr.cmd` at server startup (read via
/// [`tailr_core::daemon::read_restart_cmd`]). This is the reliable source —
/// `tailr.cmd` is written once at boot, before any binary replacement, so it
/// holds the clean on-disk path.
///
/// `current_exe()` is used only as a fallback. Right after `self_replace`
/// overwrites the running binary, Linux marks `/proc/self/exe` as
/// `"/path/to/exe (deleted)"` (the running process's original file is gone).
/// `current_exe()` returns that `(deleted)`-suffixed string verbatim, which
/// can't be spawned — so we prefer `tailr.cmd` and strip any `(deleted)` marker
/// from `current_exe()` before trying it. Both paths spawn detached (setsid).
fn spawn_restart() -> Result<(), String> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    // Primary: persisted cmd (clean path recorded at startup).
    if let Some((exe, _args)) = tailr_core::daemon::read_restart_cmd() {
        if !candidates.contains(&exe) {
            candidates.push(exe);
        }
    }

    // Fallback: current_exe(), with the "(deleted)" marker Linux appends after
    // the binary is replaced. The kernel suffixes /proc/self/exe with " (deleted)"
    // when the original file has been overwritten; that string isn't a real path.
    if let Ok(exe) = std::env::current_exe() {
        let cleaned = strip_deleted_marker(&exe);
        if !candidates.contains(&cleaned) {
            candidates.push(cleaned);
        }
    }

    let mut last_err = "no restart exe candidate resolved".to_string();
    for exe in &candidates {
        tracing::info!(
            exe = %exe.display(),
            exists = exe.exists(),
            "spawn_restart: trying candidate"
        );
        match build_restart_command(exe) {
            Ok(mut c) => match c.spawn() {
                Ok(_) => {
                    tracing::info!(exe = %exe.display(), "restart subprocess spawned");
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(
                        exe = %exe.display(),
                        exists = exe.exists(),
                        error = %e,
                        "spawn_restart: candidate failed, trying next"
                    );
                    last_err = format!("failed to spawn restart (exe={}, exists={}): {e}", exe.display(), exe.exists());
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "spawn_restart: could not build command");
                last_err = e;
            }
        }
    }
    Err(last_err)
}

/// Strip the " (deleted)" suffix Linux appends to `/proc/self/exe` when the
/// running binary has been replaced on disk. The result is the real on-disk
/// path of the new binary. If the path doesn't carry the marker, return as-is.
fn strip_deleted_marker(path: &std::path::Path) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_suffix(" (deleted)") {
        std::path::PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Build the `tailr restart` command for a given exe, detached (setsid, null stdio).
fn build_restart_command(
    exe: &std::path::Path,
) -> Result<std::process::Command, String> {
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("restart");
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        extern "C" {
            fn setsid() -> i32;
        }
        unsafe {
            cmd.pre_exec(|| {
                setsid();
                Ok(())
            });
        }
    }
    Ok(cmd)
}
