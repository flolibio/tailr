//! Pure upgrade engine: check + download + atomic binary replacement.
//!
//! This is the domain core for tailr's self-upgrade capability. All
//! `self_update` configuration lives here — the single place in the whole
//! project that configures `github::Update`. Both the CLI (`run_upgrade` in the
//! binary) and [`crate::UpgradeService`]-equivalent wrappers in presentation
//! layers call into this, guaranteeing platform judgment and updater config
//! never drift between entry points.
//!
//! The engine is **synchronous** (per the core-layer rule "computation stays
//! sync"): `self_update` uses reqwest's blocking client internally. Presentation
//! layers wrap calls in `spawn_blocking` to avoid stalling async workers.

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
/// Does **not** restart — the caller decides (CLI prints a hint; Web delegates
/// to `tailr restart`). This keeps restart semantics (a presentation-layer /
/// orchestration concern) out of the core engine.
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
    /// to `tailr restart` via the presentation-layer service wrapper).
    pub fn perform_upgrade(&self) -> Result<String, String> {
        if !self.supported() {
            return Err("UNSUPPORTED_PLATFORM".into());
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
            return Err("PERMISSION_DENIED".into());
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
