//! Neutral error types for the domain core layer.
//!
//! Core exposes a fixed [`ErrorCode`] enum (the single source of truth for
//! which error codes exist) plus a [`Display`] impl giving each code a baseline
//! English description. This is a **domain fact** (like an HTTP status code's
//! reason phrase), not a UI string — presentation layers decide how to render
//! it (CLI prints directly, Web maps to HTTP status + i18n key, MCP maps to
//! JSON-RPC error object).
//!
//! Core functions return `Result<T, CoreError>` where [`CoreError`] carries the
//! code plus an optional dynamic detail string (for context that varies per
//! occurrence, like the underlying io error message). The detail is NOT a UI
//! string either — it's diagnostic context for logs and debugging.
//!
//! See `AGENTS.md` "Error handling (v0.12+)" for the full contract.

use std::fmt;

/// Domain-level error codes. The single source of truth for which error codes
/// tailr can produce.
///
/// **Naming**: SCREAMING_SNAKE_CASE. Stable identifiers that presentation layers
/// switch on (e.g. Web maps each to an HTTP status + i18n key).
///
/// **Stability (v1.0+)**: add-only. New codes may be added; existing codes are
/// never renamed, removed, or revalued. This preserves the public API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    // --- Authentication / authorization (HTTP 401 / 403) ---
    /// No authentication token provided, or the token is invalid/expired.
    /// Web: 401. The frontend shows the token input dialog on this code.
    Unauthorized,
    /// The authenticated principal lacks permission for this resource, or the
    /// CSRF header is missing on a mutating request. Web: 403.
    Forbidden,
    /// A non-empty auth token is required for this endpoint (e.g. binary
    /// upgrade is RCE-class and must never be reachable when auth is disabled).
    /// Web: 403. Distinct from `Unauthorized` (no token) and `Forbidden`
    /// (wrong token / no CSRF): this means auth is globally off and the
    /// endpoint refuses to proceed without it.
    TokenRequired,

    // --- Resource access (HTTP 404 / 400) ---
    /// The requested file or directory does not exist on disk.
    /// Web: 404.
    NotFound,
    /// The requested path resolves outside the configured log_dirs/log_files
    /// allowlist (path traversal attempt or simply not configured).
    /// Web: 403 (we don't confirm the path exists — same body as Forbidden).
    PathNotAllowed,

    // --- Rate limiting (HTTP 429) ---
    /// The client has exceeded the per-IP request rate limit. The frontend
    /// retries with exponential backoff on this code. Web: 429.
    RateLimited,

    // --- Request validation (HTTP 400) ---
    /// The request body or parameters failed validation/parsing.
    /// E.g. malformed JSON, invalid config TOML. Web: 400.
    BadRequest,

    // --- Upgrade-specific (HTTP 400 / 409 / 403) ---
    /// The current platform does not support automatic upgrade (e.g. macOS).
    /// The frontend shows a download link instead. Web: 400.
    UnsupportedPlatform,
    /// The running binary is not writable (can't atomically replace).
    /// Usually means tailr isn't installed in a user-writable location.
    /// Web: 403.
    PermissionDenied,
    /// An upgrade is already in progress (concurrent attempt rejected).
    /// Web: 409 Conflict.
    UpgradeInProgress,

    // --- Internal errors (HTTP 500) ---
    /// An unexpected internal failure (io error, serialization failure, etc.).
    /// The detail string carries diagnostic context for logs. Web: 500.
    Internal,
}

impl ErrorCode {
    /// The SCREAMING_SNAKE string identifier, suitable for serialization and
    /// machine-readable protocol fields. This is the stable wire format.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::Unauthorized => "UNAUTHORIZED",
            ErrorCode::Forbidden => "FORBIDDEN",
            ErrorCode::TokenRequired => "TOKEN_REQUIRED",
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::PathNotAllowed => "PATH_NOT_ALLOWED",
            ErrorCode::RateLimited => "RATE_LIMITED",
            ErrorCode::BadRequest => "BAD_REQUEST",
            ErrorCode::UnsupportedPlatform => "UNSUPPORTED_PLATFORM",
            ErrorCode::PermissionDenied => "PERMISSION_DENIED",
            ErrorCode::UpgradeInProgress => "UPGRADE_IN_PROGRESS",
            ErrorCode::Internal => "INTERNAL",
        }
    }
}

impl fmt::Display for ErrorCode {
    /// Baseline English description — a domain fact (like an HTTP status code's
    /// reason phrase), NOT a UI string. Presentation layers may override this
    /// with their own localized/formatted copy.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ErrorCode::Unauthorized => "authentication required or token invalid",
            ErrorCode::Forbidden => "access denied",
            ErrorCode::TokenRequired => {
                "a non-empty authentication token is required for this operation"
            }
            ErrorCode::NotFound => "resource not found",
            ErrorCode::PathNotAllowed => "path is outside the configured log directories",
            ErrorCode::RateLimited => "rate limit exceeded",
            ErrorCode::BadRequest => "invalid request",
            ErrorCode::UnsupportedPlatform => {
                "automatic upgrade is not supported on this platform"
            }
            ErrorCode::PermissionDenied => "permission denied (binary not writable)",
            ErrorCode::UpgradeInProgress => "an upgrade is already in progress",
            ErrorCode::Internal => "internal server error",
        };
        f.write_str(msg)
    }
}

/// A core-layer error: an [`ErrorCode`] plus optional dynamic detail.
///
/// The detail string carries per-occurrence context (e.g. the underlying io
/// error message) for logs and debugging. It is NOT serialized to API clients
/// by default — presentation layers decide whether to expose it (Web doesn't;
/// it maps `code` → i18n key and keeps detail server-side).
///
/// Implements [`std::error::Error`] so it flows through `Result` chains and
/// `?` operators naturally.
#[derive(Debug)]
pub struct CoreError {
    /// The stable, machine-readable error code.
    pub code: ErrorCode,
    /// Optional dynamic detail (diagnostic context, not a UI string).
    /// `None` when the code alone fully describes the error.
    pub detail: Option<String>,
}

impl CoreError {
    /// Construct a bare error with no dynamic detail.
    pub fn new(code: ErrorCode) -> Self {
        Self { code, detail: None }
    }

    /// Construct an error with dynamic detail (e.g. the underlying io error msg).
    pub fn with_detail(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.detail {
            Some(d) => write!(f, "{}: {}", self.code, d),
            None => self.code.fmt(f),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<ErrorCode> for CoreError {
    fn from(code: ErrorCode) -> Self {
        Self::new(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_as_str_is_screaming_snake() {
        // All variants must have a stable SCREAMING_SNAKE wire identifier.
        assert_eq!(ErrorCode::Unauthorized.as_str(), "UNAUTHORIZED");
        assert_eq!(ErrorCode::NotFound.as_str(), "NOT_FOUND");
        assert_eq!(ErrorCode::PathNotAllowed.as_str(), "PATH_NOT_ALLOWED");
        assert_eq!(ErrorCode::UnsupportedPlatform.as_str(), "UNSUPPORTED_PLATFORM");
        assert_eq!(ErrorCode::UpgradeInProgress.as_str(), "UPGRADE_IN_PROGRESS");
    }

    #[test]
    fn code_display_is_english_baseline() {
        // Display gives a non-empty human-readable English phrase.
        assert!(!ErrorCode::NotFound.to_string().is_empty());
        assert!(ErrorCode::PermissionDenied.to_string().contains("permission"));
    }

    #[test]
    fn core_error_with_detail_formats_both() {
        let e = CoreError::with_detail(ErrorCode::Internal, "disk full");
        let s = e.to_string();
        assert!(s.contains("internal server error"));
        assert!(s.contains("disk full"));
    }

    #[test]
    fn core_error_without_detail_formats_code_only() {
        let e = CoreError::new(ErrorCode::NotFound);
        assert_eq!(e.to_string(), ErrorCode::NotFound.to_string());
    }

    #[test]
    fn error_code_into_core_error() {
        let e: CoreError = ErrorCode::Forbidden.into();
        assert_eq!(e.code, ErrorCode::Forbidden);
        assert!(e.detail.is_none());
    }

    #[test]
    fn core_error_is_std_error() {
        // Must implement std::error::Error so it flows through Result chains.
        fn assert_error<T: std::error::Error>() {}
        assert_error::<CoreError>();
    }
}
