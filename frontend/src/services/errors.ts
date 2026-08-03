/**
 * API error handling: error code constants, the ApiError class, and parsing.
 *
 * The backend returns errors as HTTP 4xx/5xx with body:
 *   { success: false, error: { code: "SCREAMING_SNAKE", message: "..." } }
 *
 * `code` is a stable, machine-readable identifier (add-only after v1.0).
 * `message` is the backend's baseline English; the frontend overrides it via
 * i18n (see getErrorMessage below).
 *
 * Callers should catch ApiError and branch on `e.code` for specific handling
 * (e.g. retry on RATE_LIMITED, show token dialog on UNAUTHORIZED).
 */

/** All known backend error codes. Mirror of tailr_core::error::ErrorCode. */
export const ErrorCode = {
  UNAUTHORIZED: 'UNAUTHORIZED',
  FORBIDDEN: 'FORBIDDEN',
  TOKEN_REQUIRED: 'TOKEN_REQUIRED',
  NOT_FOUND: 'NOT_FOUND',
  PATH_NOT_ALLOWED: 'PATH_NOT_ALLOWED',
  RATE_LIMITED: 'RATE_LIMITED',
  BAD_REQUEST: 'BAD_REQUEST',
  UNSUPPORTED_PLATFORM: 'UNSUPPORTED_PLATFORM',
  PERMISSION_DENIED: 'PERMISSION_DENIED',
  UPGRADE_IN_PROGRESS: 'UPGRADE_IN_PROGRESS',
  INTERNAL: 'INTERNAL',
} as const

export type ErrorCodeValue = typeof ErrorCode[keyof typeof ErrorCode]

/**
 * Error thrown when an API request fails with an HTTP 4xx/5xx status.
 * Carries the backend's error code (when parseable) for programmatic handling.
 */
export class ApiError extends Error {
  /** Backend error code (e.g. "NOT_FOUND"), or null if the body wasn't parseable. */
  readonly code: string | null
  /** HTTP status code (4xx/5xx). */
  readonly status: number

  constructor(status: number, code: string | null, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
  }

  /** True if this error matches the given error code constant. */
  is(code: ErrorCodeValue): boolean {
    return this.code === code
  }
}

/** Parse an error Response into an ApiError. Extracts code + message from the
 *  JSON body when available; falls back to HTTP status text. */
export async function parseApiError(res: Response): Promise<ApiError> {
  let code: string | null = null
  let message = `HTTP ${res.status}: ${res.statusText}`
  try {
    const json = await res.json()
    if (json?.error?.code) {
      code = json.error.code
    }
    if (json?.error?.message) {
      message = json.error.message
    } else if (json?.error) {
      // Legacy shape: { success:false, error:"string" } (pre-v0.12 fallback).
      message = typeof json.error === 'string' ? json.error : message
    }
  } catch {
    // Body wasn't JSON (e.g. empty 204, or HTML from a proxy). Keep the HTTP
    // status text as the message.
  }
  return new ApiError(res.status, code, message)
}

/**
 * Map an error code to an i18n key. Returns the key (e.g. "errors.notFound")
 * for known codes, or null for unknown codes (caller falls back to the raw
 * message). Used by components that want to show a localized error message
 * instead of the backend's baseline English.
 *
 * Usage:
 *   const key = errorCodeToI18nKey(e.code)
 *   const msg = key ? t(key) : e.message
 */
export function errorCodeToI18nKey(code: string | null): string | null {
  // Convert SCREAMING_SNAKE to camelCase and prefix with "errors.".
  // e.g. "NOT_FOUND" → "errors.notFound"
  const map: Record<string, string> = {
    [ErrorCode.UNAUTHORIZED]: 'errors.unauthorized',
    [ErrorCode.FORBIDDEN]: 'errors.forbidden',
    [ErrorCode.TOKEN_REQUIRED]: 'errors.tokenRequired',
    [ErrorCode.NOT_FOUND]: 'errors.notFound',
    [ErrorCode.PATH_NOT_ALLOWED]: 'errors.pathNotAllowed',
    [ErrorCode.RATE_LIMITED]: 'errors.rateLimited',
    [ErrorCode.BAD_REQUEST]: 'errors.badRequest',
    [ErrorCode.UNSUPPORTED_PLATFORM]: 'errors.unsupportedPlatform',
    [ErrorCode.PERMISSION_DENIED]: 'errors.permissionDenied',
    [ErrorCode.UPGRADE_IN_PROGRESS]: 'errors.upgradeInProgress',
    [ErrorCode.INTERNAL]: 'errors.internal',
  }
  return code ? (map[code] ?? null) : null
}
