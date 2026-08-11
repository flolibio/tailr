//! Pure upgrade engine: check + download + atomic binary replacement.
//!
//! This is the domain core for tailr's self-upgrade capability. Both the CLI
//! (`run_upgrade` in the binary) and [`crate::UpgradeService`]-equivalent
//! wrappers in presentation layers call into this, guaranteeing platform
//! judgment and upgrade logic never drift between entry points.
//!
//! The engine is **synchronous** (per the core-layer rule "computation stays
//! sync"): `self_update`'s low-level `Download`/`Extract` use reqwest's
//! blocking client internally. Presentation layers wrap calls in
//! `spawn_blocking` to avoid stalling async workers.
//!
//! ## Why not `ReleaseUpdate::update()`?
//!
//! `self_update::ReleaseUpdate::update()` internally filters releases with
//! `bump_is_compatible`, which follows cargo's semver compatibility rules:
//! `0.x → 1.0` is treated as a breaking change and filtered out. That made
//! cross-major upgrades impossible — a v0.12 binary could never auto-upgrade
//! to v1.0+, always stalling at v0.12.4. We bypass that by using `self_update`'s
//! low-level components (`ReleaseList`, `Download`, `Extract`, `self_replace`)
//! directly, with our own `>` version comparison (`semver::Version`) that
//! allows any upward jump.

use self_update::backends::github;
use self_update::update::Release;
use semver::Version;
use serde::Serialize;

/// Result of a version check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct UpgradeResult {
    pub status: String,
    pub message: String,
}

/// GitHub repo coordinates — single source of truth for both check + upgrade.
const REPO_OWNER: &str = "flolibio";
const REPO_NAME: &str = "tailr";
const BIN_NAME: &str = "tailr";

/// Pure upgrade engine: check + download + atomic binary replacement.
///
/// Does **not** restart — the caller decides (CLI prints a hint; Web delegates
/// to `tailr restart`). This keeps restart semantics (a presentation-layer /
/// orchestration concern) out of the core engine.
pub struct UpgradeEngine {
    current_version: String,
}

impl UpgradeEngine {
    /// Construct with an explicit version string.
    ///
    /// Callers should pass **the binary's version** (from `env!("CARGO_PKG_VERSION")`
    /// in the binary/server crate), NOT this crate's version. `env!` expands to the
    /// version of the crate whose source the macro appears in — and `tailr-core` is
    /// versioned independently from the `tailr` binary. Hard-coding `env!` here
    /// silently made the upgrade engine compare against core's stale version
    /// (0.12.x) instead of the binary's (1.0.x); injecting the binary version at the
    /// call site removes that footgun permanently.
    pub fn with_version(version: impl Into<String>) -> Self {
        Self {
            current_version: version.into(),
        }
    }

    /// Whether the current platform supports automatic upgrade.
    ///
    /// Matches the judgment in the old `run_upgrade` (Linux x86_64/aarch64 only).
    /// Kept as the single source so CLI and Web cannot disagree.
    pub fn supported(&self) -> bool {
        std::env::consts::OS == "linux" && matches!(std::env::consts::ARCH, "x86_64" | "aarch64")
    }

    /// The `self_update` target triple suffix used in release asset names.
    ///
    /// Release archives are named `tailr-<target>.tar.gz` where `<target>` is
    /// `x86_64-linux-musl` or `aarch64-linux-musl` (the `-unknown` part of the
    /// Rust target triple is stripped in the release workflow).
    fn target(&self) -> Result<&'static str, String> {
        match std::env::consts::ARCH {
            "x86_64" => Ok("x86_64-linux-musl"),
            "aarch64" => Ok("aarch64-linux-musl"),
            arch => Err(format!("unsupported architecture: {arch}")),
        }
    }

    /// Fetch all releases from GitHub that have an asset for our target platform.
    ///
    /// Uses `ReleaseList` (low-level) instead of `ReleaseUpdate::update()` to
    /// avoid the latter's `bump_is_compatible` filter, which blocks cross-major
    /// upgrades (0.x → 1.0).
    fn fetch_releases(&self, target: &str) -> Result<Vec<Release>, String> {
        github::ReleaseList::configure()
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .with_target(target)
            .build()
            .map_err(|e| e.to_string())?
            .fetch()
            .map_err(|e| e.to_string())
    }

    /// Select the newest release strictly greater than `current_version`.
    ///
    /// Comparison is plain semver `>` (via `semver::Version`), NOT
    /// `bump_is_compatible` — so `0.12.4 → 1.0.0` is allowed. Pre-release
    /// versions (rc, beta) are excluded from auto-upgrade candidates.
    fn newest_above_current<'a>(
        &self,
        releases: &'a [Release],
        current: &Version,
    ) -> Option<&'a Release> {
        releases.iter().filter(|r| {
            // Skip releases whose version fails to parse — malformed tag.
            Version::parse(&r.version).is_ok_and(|v| v > *current && v.pre.is_empty())
        }).max_by(|a, b| {
            // max_by on parsed versions so "0.12.4" < "1.0.0" compares correctly
            // (string comparison would wrongly put "0.12.4" > "1.0.0").
            let va = Version::parse(&a.version).unwrap();
            let vb = Version::parse(&b.version).unwrap();
            va.cmp(&vb)
        })
    }

    /// Check for a newer release on GitHub.
    ///
    /// Returns an [`UpdateInfo`] regardless of platform; callers gate on
    /// `supported` before offering to upgrade.
    pub fn check_update(&self) -> Result<UpdateInfo, String> {
        let target = self.target()?;
        let releases = self.fetch_releases(target)?;
        let current = Version::parse(&self.current_version).map_err(|e| e.to_string())?;

        let latest = self.newest_above_current(&releases, &current);
        let (latest_version, has_update) = match latest {
            Some(r) => (r.version.clone(), true),
            None => (self.current_version.clone(), false),
        };

        Ok(UpdateInfo {
            current_version: self.current_version.clone(),
            latest_version: latest_version.clone(),
            has_update,
            supported: self.supported(),
            release_url: format!(
                "https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/tag/v{latest_version}"
            ),
        })
    }

    /// Perform the upgrade: permission check → download → atomic replace.
    ///
    /// Does **not** restart — the caller decides (CLI prints a hint; Web delegates
    /// to `tailr restart` via the presentation-layer service wrapper).
    ///
    /// Returns the new version string on success, or an error code string
    /// (UNSUPPORTED_PLATFORM, PERMISSION_DENIED, ALREADY_UP_TO_DATE, etc.).
    pub fn perform_upgrade(&self) -> Result<String, String> {
        if !self.supported() {
            return Err("UNSUPPORTED_PLATFORM".into());
        }
        self.check_write_permission()?;

        let target = self.target()?;
        let releases = self.fetch_releases(target)?;
        let current = Version::parse(&self.current_version).map_err(|e| e.to_string())?;

        let release = self
            .newest_above_current(&releases, &current)
            .ok_or_else(|| "ALREADY_UP_TO_DATE".to_string())?;

        let asset = release
            .asset_for(target, None)
            .ok_or_else(|| "NO_ASSET_FOR_TARGET".to_string())?;

        tracing::info!(
            current = %self.current_version,
            target = %release.version,
            asset = %asset.name,
            "downloading upgrade"
        );

        // Download the archive to a temp directory. TempDir auto-cleans on drop.
        let tmp_dir = tempfile::TempDir::new().map_err(|e| format!("tempdir failed: {e}"))?;
        let archive_path = tmp_dir.path().join(&asset.name);
        let mut archive_file =
            std::fs::File::create(&archive_path).map_err(|e| format!("create archive file: {e}"))?;

        // GitHub release asset URLs are API endpoints (api.github.com/.../assets/XXX)
        // that return JSON metadata unless `Accept: application/octet-stream` is sent.
        // Without this header, the downloaded "archive" is actually JSON → tar extraction
        // fails with "Could not find the required path in the archive". This mirrors what
        // self_update::ReleaseUpdate::update() does internally (api_headers + Accept).
        let mut download = self_update::Download::from_url(&asset.download_url);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            "application/octet-stream"
                .parse()
                .map_err(|e| format!("invalid accept header: {e}"))?,
        );
        headers.insert(
            reqwest::header::USER_AGENT,
            concat!("tailr-upgrade/", env!("CARGO_PKG_VERSION"))
                .parse()
                .map_err(|e| format!("invalid user-agent header: {e}"))?,
        );
        download.set_headers(headers);
        download
            .download_to(&mut archive_file)
            .map_err(|e| format!("download failed: {e}"))?;

        tracing::info!(archive = %archive_path.display(), "extracting binary");
        // The release archive contains the binary at the root (no subdirectory):
        // `tar czf ... tailr` in the release workflow. Extract it into tmp_dir.
        self_update::Extract::from_source(&archive_path)
            .extract_file(tmp_dir.path(), BIN_NAME)
            .map_err(|e| format!("extract failed: {e}"))?;

        let new_exe = tmp_dir.path().join(BIN_NAME);
        tracing::info!(new_exe = %new_exe.display(), "replacing running binary");
        self_replace::self_replace(&new_exe).map_err(|e| format!("self_replace failed: {e}"))?;

        tracing::info!(version = %release.version, "upgrade complete");
        Ok(release.version.clone())
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
        // Fallback: uses core's own crate version. Real call sites should prefer
        // `with_version(env!("CARGO_PKG_VERSION"))` from the binary crate so the
        // engine tracks the binary's version, not core's.
        Self::with_version(env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_above_current_allows_cross_major() {
        // Simulate releases from GitHub: 0.12.4, 1.0.0, 1.0.2, 1.0.3
        let mk = |v: &str| Release {
            name: format!("v{v}"),
            version: v.to_string(),
            date: String::new(),
            body: None,
            assets: vec![],
        };
        let releases = vec![mk("0.12.4"), mk("1.0.0"), mk("1.0.2"), mk("1.0.3")];

        let engine = UpgradeEngine::with_version("0.12.4");
        let current = Version::parse("0.12.4").unwrap();
        let best = engine.newest_above_current(&releases, &current).unwrap();
        // Must pick 1.0.3 (the highest), not stall at 0.12.x.
        assert_eq!(best.version, "1.0.3");
    }

    #[test]
    fn newest_above_current_returns_none_when_up_to_date() {
        let mk = |v: &str| Release {
            name: format!("v{v}"),
            version: v.to_string(),
            date: String::new(),
            body: None,
            assets: vec![],
        };
        let releases = vec![mk("1.0.0"), mk("1.0.2")];

        let engine = UpgradeEngine::with_version("1.0.2");
        let current = Version::parse("1.0.2").unwrap();
        assert!(engine.newest_above_current(&releases, &current).is_none());
    }

    #[test]
    fn newest_above_current_skips_prereleases() {
        let mk = |v: &str| Release {
            name: format!("v{v}"),
            version: v.to_string(),
            date: String::new(),
            body: None,
            assets: vec![],
        };
        // A 1.0.0-rc1 exists but should be skipped for auto-upgrade.
        let releases = vec![mk("1.0.0-rc1")];

        let engine = UpgradeEngine::with_version("0.12.4");
        let current = Version::parse("0.12.4").unwrap();
        assert!(engine.newest_above_current(&releases, &current).is_none());
    }

    #[test]
    fn newest_above_current_picks_highest_not_first() {
        // GitHub returns releases newest-first, but our logic must not rely on
        // order — it picks the semver-highest, which may differ from list order.
        let mk = |v: &str| Release {
            name: format!("v{v}"),
            version: v.to_string(),
            date: String::new(),
            body: None,
            assets: vec![],
        };
        let releases = vec![mk("1.0.3"), mk("2.0.0"), mk("1.5.0")];

        let engine = UpgradeEngine::with_version("1.0.0");
        let current = Version::parse("1.0.0").unwrap();
        let best = engine.newest_above_current(&releases, &current).unwrap();
        assert_eq!(best.version, "2.0.0");
    }
}
