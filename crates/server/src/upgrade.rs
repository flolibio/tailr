//! Self-upgrade support.
//!
//! Two-layer design so the CLI (`tailr upgrade`) and the Web UI (`POST /api/upgrade`)
//! share a single source of truth for all `self_update` configuration:
//!
//! - [`UpgradeEngine`] — pure upgrade logic (download + atomic binary replacement),
//!   no restart semantics. Reused by both entry points.
//! - [`UpgradeService`] — Web-specific wrapper: upgrade then delegate restart to the
//!   `tailr restart` subcommand. The CLI entry point does not go through this.

use std::sync::Arc;

use self_update::backends::github;
use self_update::update::ReleaseUpdate;
use serde::Serialize;

/// Result of a version check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    /// Whether the *current platform* supports automatic upgrade.
    /// `false` on macOS — Web UI shows a download link instead of an upgrade button.
    pub supported: bool,
    pub release_url: String,
}

/// Result of an upgrade.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeResult {
    pub status: String,
    pub message: String,
}

/// Pure upgrade engine: check + download + atomic binary replacement.
///
/// All `self_update` configuration lives here — the single place in the whole
/// project that configures `github::Update`. Both the CLI (`run_upgrade` in the
/// binary) and [`UpgradeService`] (Web) call into this, guaranteeing platform
/// judgment and updater config never drift between the two.
pub struct UpgradeEngine {
    current_version: String,
}

impl UpgradeEngine {
    pub fn new() -> Self {
        Self {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Whether the current platform supports automatic upgrade.
    ///
    /// Matches the judgment in the old `run_upgrade` (Linux x86_64/aarch64 only).
    /// Kept as the single source so CLI and Web cannot disagree.
    pub fn supported(&self) -> bool {
        std::env::consts::OS == "linux" && matches!(std::env::consts::ARCH, "x86_64" | "aarch64")
    }

    fn target(&self) -> Result<&'static str, String> {
        match std::env::consts::ARCH {
            "x86_64" => Ok("x86_64-linux-musl"),
            "aarch64" => Ok("aarch64-linux-musl"),
            arch => Err(format!("unsupported architecture: {arch}")),
        }
    }

    /// The single `self_update` configuration point. `.build()` returns
    /// `Result<Box<dyn ReleaseUpdate>>`; we map the error to `String` for callers.
    fn build_updater(&self) -> Result<Box<dyn ReleaseUpdate>, String> {
        github::Update::configure()
            .repo_owner("flolibio")
            .repo_name("tailr")
            .bin_name("tailr")
            .target(self.target()?)
            .current_version(&self.current_version)
            .build()
            .map_err(|e| e.to_string())
    }

    /// Check for a newer release on GitHub.
    ///
    /// Returns an [`UpdateInfo`] regardless of platform; callers gate on
    /// `supported` before offering to upgrade.
    pub fn check_update(&self) -> Result<UpdateInfo, String> {
        let latest = self
            .build_updater()?
            .get_latest_release()
            .map_err(|e| e.to_string())?;
        let latest_version = latest.version.clone();
        let has_update =
            self_update::version::bump_is_greater(&self.current_version, &latest_version)
                .unwrap_or(false);
        Ok(UpdateInfo {
            current_version: self.current_version.clone(),
            latest_version: latest_version.clone(),
            has_update,
            supported: self.supported(),
            release_url: format!(
                "https://github.com/flolibio/tailr/releases/tag/v{}",
                latest_version
            ),
        })
    }

    /// Perform the upgrade: permission check → download → atomic replace.
    ///
    /// Does **not** restart — the caller decides (CLI prints a hint; Web delegates
    /// to `tailr restart` via [`UpgradeService`]).
    pub fn perform_upgrade(&self) -> Result<String, String> {
        if !self.supported() {
            return Err("当前平台不支持自动升级，请手动下载".into());
        }
        self.check_write_permission()?;

        let status = github::Update::configure()
            .repo_owner("flolibio")
            .repo_name("tailr")
            .bin_name("tailr")
            .target(self.target()?)
            .current_version(&self.current_version)
            .no_confirm(true)
            .show_download_progress(false)
            .build()
            .map_err(|e| e.to_string())?
            .update()
            .map_err(|e| e.to_string())?;

        match status {
            self_update::Status::UpToDate(v) => Ok(format!("Already up to date (v{v})")),
            self_update::Status::Updated(v) => Ok(v),
        }
    }

    /// Probe whether the running binary is writable (cheap: write+remove a temp file
    /// beside it). Avoids downloading only to discover we can't replace.
    fn check_write_permission(&self) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let tmp = exe.with_extension("tmp.writecheck");
        if std::fs::write(&tmp, b"").is_err() {
            return Err("权限不足，请使用 sudo 运行或检查二进制可写性".into());
        }
        let _ = std::fs::remove_file(&tmp);
        Ok(())
    }
}

impl Default for UpgradeEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Web-specific upgrade wrapper: upgrade then delegate restart to the
/// `tailr restart` subcommand.
///
/// The CLI entry point (`run_upgrade`) does **not** use this — it only needs pure
/// upgrade and lets the user restart manually. Restart semantics live here so they
/// don't pollute the shared [`UpgradeEngine`].
///
/// Both methods offload the synchronous `self_update` (reqwest blocking) work to
/// `spawn_blocking`. reqwest's blocking client spins up its own tokio runtime on a
/// helper thread; dropping it from within an async context panics
/// ("Cannot drop a runtime in a context where blocking is not allowed").
/// `spawn_blocking` runs the call on the blocking pool, outside the async runtime.
pub struct UpgradeService {
    engine: Arc<UpgradeEngine>,
}

impl UpgradeService {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(UpgradeEngine::new()),
        }
    }

    pub async fn check_update(&self) -> Result<UpdateInfo, String> {
        let engine = self.engine.clone();
        tokio::task::spawn_blocking(move || engine.check_update())
            .await
            .map_err(|e| format!("upgrade check task failed: {e}"))?
    }

    /// Web upgrade: pure upgrade → spawn `tailr restart` after a 1s delay (lets the
    /// HTTP response flush first). Restart goes through `Commands::Restart`, which
    /// uses `stop_daemon` (graceful shutdown, PID cleanup) + re-exec — not a raw
    /// `exit(0)` that would skip cleanup.
    pub async fn perform_upgrade(&self) -> Result<UpgradeResult, String> {
        let engine = self.engine.clone();
        let version =
            tokio::task::spawn_blocking(move || engine.perform_upgrade())
                .await
                .map_err(|e| format!("upgrade task failed: {e}"))??;
        // Defer restart so the HTTP response is sent before the server shuts down.
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Err(e) = spawn_restart() {
                tracing::error!("failed to spawn restart after upgrade: {e}");
            }
        });
        Ok(UpgradeResult {
            status: "success".to_string(),
            message: format!("升级成功，服务即将重启 (v{version})"),
        })
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

/// Spawn `tailr restart` as a detached subprocess.
fn spawn_restart() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .arg("restart")
        .spawn()
        .map_err(|e| format!("failed to spawn restart: {e}"))?;
    Ok(())
}
