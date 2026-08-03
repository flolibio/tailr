//! Web-layer error type: maps core's neutral [`ErrorCode`] to HTTP responses.
//!
//! This is the transport-boundary translation point. Core returns
//! [`CoreError`] (a domain fact: code + baseline English). This module adds the
//! HTTP-specific concerns: status code mapping and the `{success:false,
//! error:{code, message}}` JSON body shape.
//!
//! Per the architecture rules: HTTP-ization is the Web layer's job. Core knows
//! nothing about `StatusCode` or `IntoResponse` — those live here.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use tailr_core::error::{CoreError, ErrorCode};

/// Map a core [`ErrorCode`] to its HTTP status code.
///
/// This is the single translation table from domain error codes to HTTP status.
/// Presentation-layer logic — changing it doesn't touch core.
pub(crate) fn status_for(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
        ErrorCode::Forbidden | ErrorCode::PathNotAllowed | ErrorCode::TokenRequired => {
            StatusCode::FORBIDDEN
        }
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::BadRequest | ErrorCode::UnsupportedPlatform => StatusCode::BAD_REQUEST,
        ErrorCode::PermissionDenied => StatusCode::FORBIDDEN,
        ErrorCode::UpgradeInProgress => StatusCode::CONFLICT,
        ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Error body serialized into the HTTP response. This is the frozen v1.0 shape:
/// `{success:false, error:{code, message}}` where `code` is SCREAMING_SNAKE.
///
/// `message` is the baseline English from core's `Display` — the frontend maps
/// `code` to an i18n key and shows a localized string; this English message is
/// the fallback (and what non-frontend clients see).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(utoipa::ToSchema)]
pub(crate) struct ErrorBody {
    success: bool,
    error: ErrorDetail,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(utoipa::ToSchema)]
pub(crate) struct ErrorDetail {
    /// SCREAMING_SNAKE machine-readable code (stable, add-only after v1.0).
    code: String,
    /// Baseline English message (the frontend overrides via i18n).
    message: String,
}

/// The Web-layer error type. Wraps a [`CoreError`] and implements
/// [`IntoResponse`] to produce the unified HTTP error response.
///
/// Handlers convert core errors into this via `.into()` or `?`, then return it.
/// Axum's `IntoResponse` machinery does the rest.
pub(crate) struct ApiError(CoreError);

impl ApiError {
    pub(crate) fn new(code: ErrorCode) -> Self {
        Self(CoreError::new(code))
    }

    pub(crate) fn from_core(e: CoreError) -> Self {
        Self(e)
    }
}

impl From<ErrorCode> for ApiError {
    fn from(code: ErrorCode) -> Self {
        Self::new(code)
    }
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        Self::from_core(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.0.code;
        let status = status_for(code);
        // Log the detail (if any) at the appropriate level for server-side
        // debugging. The detail is NOT sent to the client — only the code +
        // baseline message are.
        if let Some(detail) = &self.0.detail {
            match status.as_u16() {
                500..=599 => tracing::error!(code = code.as_str(), detail = %detail, "API error"),
                400..=499 => tracing::warn!(code = code.as_str(), detail = %detail, "API error"),
                _ => tracing::info!(code = code.as_str(), detail = %detail, "API error"),
            }
        }

        let body = ErrorBody {
            success: false,
            error: ErrorDetail {
                code: code.as_str().to_string(),
                message: code.to_string(),
            },
        };
        (status, Json(body)).into_response()
    }
}

/// Success response envelope: `{success:true, data:<T>}`.
///
/// Kept for the success path — errors now go through [`ApiError`] (HTTP 4xx/5xx
/// + error body) rather than HTTP 200 + `{success:false}`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(utoipa::ToSchema)]
pub(crate) struct ApiSuccess<T: Serialize> {
    success: bool,
    data: T,
}

impl<T: Serialize> ApiSuccess<T> {
    pub(crate) fn ok(data: T) -> Self {
        Self { success: true, data }
    }
}

impl<T: Serialize> IntoResponse for ApiSuccess<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_covers_all_codes() {
        // Every ErrorCode variant must map to a status (exhaustiveness check).
        let codes = [
            ErrorCode::Unauthorized,
            ErrorCode::Forbidden,
            ErrorCode::TokenRequired,
            ErrorCode::NotFound,
            ErrorCode::PathNotAllowed,
            ErrorCode::RateLimited,
            ErrorCode::BadRequest,
            ErrorCode::UnsupportedPlatform,
            ErrorCode::PermissionDenied,
            ErrorCode::UpgradeInProgress,
            ErrorCode::Internal,
        ];
        for code in codes {
            let status = status_for(code);
            assert!(
                status.is_client_error() || status.is_server_error(),
                "{:?} mapped to non-error status {}",
                code,
                status
            );
        }
    }

    #[test]
    fn unauthorized_maps_to_401() {
        assert_eq!(status_for(ErrorCode::Unauthorized), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn not_found_maps_to_404() {
        assert_eq!(status_for(ErrorCode::NotFound), StatusCode::NOT_FOUND);
    }

    #[test]
    fn rate_limited_maps_to_429() {
        assert_eq!(status_for(ErrorCode::RateLimited), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn upgrade_in_progress_maps_to_409() {
        assert_eq!(
            status_for(ErrorCode::UpgradeInProgress),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn path_not_allowed_maps_to_403() {
        // Path traversal returns 403 (we don't confirm the path exists).
        assert_eq!(status_for(ErrorCode::PathNotAllowed), StatusCode::FORBIDDEN);
    }

    #[test]
    fn api_error_from_code_roundtrip() {
        let e: ApiError = ErrorCode::NotFound.into();
        assert_eq!(e.0.code, ErrorCode::NotFound);
    }
}
