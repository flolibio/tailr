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
    /// Serializes concurrent upgrade attempts. Held for the duration of
    /// `perform_upgrade` (download + replace) so two simultaneous callers can't
    /// race on the atomic binary replacement. `try_lock` returns busy immediately.
    upgrade_lock: tokio::sync::Mutex<()>,
}

/// Cache lifetime + poll interval. GitHub unauthenticated API allows 60 req/hour
/// per IP; one check per 6h is ~4/day — far under the limit, yet timely enough for
/// release cadence (days/weeks).
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Delay the first check after startup so it never blocks initial responsiveness.
const INITIAL_DELAY: Duration = Duration::from_secs(30);

impl UpgradeService {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(UpgradeEngine::new()),
            cache: Arc::new(RwLock::new(None)),
            upgrade_lock: tokio::sync::Mutex::new(()),
        }
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

    /// Web upgrade: pure upgrade → spawn `tailr restart` after a 1s delay (lets the
    /// HTTP response flush first). Restart goes through `Commands::Restart`, which
    /// uses `stop_daemon` (graceful shutdown, PID cleanup) + re-exec — not a raw
    /// `exit(0)` that would skip cleanup.
    pub async fn perform_upgrade(&self) -> Result<UpgradeResult, String> {
        // Reject concurrent upgrade attempts immediately — two simultaneous
        // binary replacements would race on the atomic rename.
        let _guard = self
            .upgrade_lock
            .try_lock()
            .map_err(|_| "UPGRADE_IN_PROGRESS".to_string())?;

        tracing::info!("upgrade started");
        let engine = self.engine.clone();
        let version =
            tokio::task::spawn_blocking(move || engine.perform_upgrade())
                .await
                .map_err(|e| format!("upgrade task failed: {e}"))??;
        tracing::info!(version = %version, "binary replaced successfully, scheduling restart");
        // Invalidate the update cache: it holds the pre-upgrade result
        // (hasUpdate=true for the version we just installed). Without this, any
        // check between now and restart serves a stale "update available".
        *self.cache.write().await = None;
        tracing::info!("update cache invalidated after upgrade");
        // Defer restart so the HTTP response is sent before the server shuts down.
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            tracing::info!("spawning restart subprocess");
            if let Err(e) = spawn_restart() {
                tracing::error!("failed to spawn restart after upgrade: {e}");
            } else {
                tracing::info!("restart subprocess spawned successfully");
            }
        });
        Ok(UpgradeResult {
            status: "success".to_string(),
            message: format!("UPGRADE_SUCCESS:{version}"),
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
