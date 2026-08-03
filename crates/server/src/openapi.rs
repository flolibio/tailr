//! OpenAPI spec generation via utoipa.
//!
//! Only generates the spec JSON (served at `/api/docs/openapi.json`). No
//! swagger-ui is bundled — users paste the spec URL into editor.swagger.io to
//! render. This keeps the binary lean and avoids extra crates.
//!
//! The spec is built at compile time via the `#[derive(OpenApi)]` macro. Adding
//! a new endpoint: add a `#[utoipa::path]` annotation to the handler, then add
//! it to the `paths(...)` list below.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "tailr",
        version = "0.12.0",
        description = "Single-binary log tail and search server. WebSocket-based live tailing + REST API for file listing, runtime metrics, config, and self-upgrade.",
        license(name = "MIT"),
    ),
    paths(
        crate::api::list_files,
        crate::api::file_tail,
        crate::api::health,
        crate::api::runtime,
        crate::api::check_upgrade,
        crate::api::perform_upgrade,
        crate::api::get_log_levels,
        crate::api::save_log_levels,
    ),
    components(schemas(
        // Response data types
        crate::api::FileEntry,
        crate::api::FileListData,
        crate::api::FileTailData,
        crate::api::HealthData,
        crate::api::RuntimeData,
        // Error response types
        crate::error::ErrorBody,
        crate::error::ErrorDetail,
        // Types from core
        tailr_core::upgrade_engine::UpdateInfo,
        tailr_core::upgrade_engine::UpgradeResult,
        // Types from protocol
        tailr_protocol::LogEntry,
        tailr_protocol::LogLevelConfig,
        tailr_protocol::LevelDef,
    )),
    tags(
        (name = "files", description = "Log file listing and tailing"),
        (name = "system", description = "Health and runtime metrics"),
        (name = "upgrade", description = "Self-upgrade"),
        (name = "config", description = "Log level configuration"),
    ),
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the OpenAPI spec generates without panic and includes all paths.
    /// This catches: missing ToSchema on a referenced type, broken path
    /// annotations, and schema derivation errors — all compile-time issues that
    /// would surface as runtime panics when the spec endpoint is first hit.
    #[test]
    fn openapi_spec_includes_all_paths() {
        let spec = ApiDoc::openapi();
        let paths = &spec.paths.paths;
        // All annotated endpoints must appear in the spec.
        let expected = [
            "/api/files",
            "/api/file/tail",
            "/api/health",
            "/api/runtime",
            "/api/upgrade/check",
            "/api/upgrade",
            "/api/config/log-levels",
        ];
        for path in expected {
            assert!(
                paths.contains_key(path),
                "OpenAPI spec missing path: {path}"
            );
        }
    }

    /// Verify the spec includes the key component schemas (catches missing
    /// ToSchema derives on types referenced by handlers).
    #[test]
    fn openapi_spec_includes_component_schemas() {
        let spec = ApiDoc::openapi();
        let schemas = &spec.components.as_ref().unwrap().schemas;
        let expected = [
            "FileEntry",
            "FileListData",
            "FileTailData",
            "HealthData",
            "RuntimeData",
            "ErrorBody",
            "UpdateInfo",
            "UpgradeResult",
            "LogEntry",
            "LogLevelConfig",
        ];
        for name in expected {
            assert!(
                schemas.contains_key(name),
                "OpenAPI spec missing component schema: {name}"
            );
        }
    }
}
