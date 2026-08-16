import { useAuth } from '../composables/useAuth'
import { ApiError, parseApiError } from './errors'

export interface LogEntry {
  lineNum: number
  raw: string
  level: string
  timestamp?: string
  rawTimestamp?: string
  fields?: Record<string, unknown>
}

export interface FileEntry {
  name: string
  path: string
  size: number
  modified: string
  isDir: boolean
  /** Nested children when listed with ?depth=N (absent/empty = flat). */
  children?: FileEntry[]
}

export class AuthError extends Error {
  constructor() {
    super('Authentication required')
    this.name = 'AuthError'
  }
}

/** Thrown when the server returns 429 (rate limited). Carries the
 *  Retry-After hint in seconds when the server provides one (may be null). */
export class RateLimitError extends Error {
  readonly retryAfter: number | null
  constructor(retryAfter: number | null) {
    super('Rate limited')
    this.name = 'RateLimitError'
    this.retryAfter = retryAfter
  }
}

/** Parse Retry-After header (seconds) from a 429 response. Returns null if
 *  the header is missing or unparseable. */
function parseRetryAfter(res: Response): number | null {
  const raw = res.headers.get('Retry-After')
  if (!raw) return null
  const n = parseInt(raw, 10)
  return Number.isNaN(n) ? null : n
}

/** Check a fetch Response for 429 and throw RateLimitError if so.
 *  Used by both request() and the direct-fetch functions that bypass it. */
function checkRateLimit(res: Response): void {
  if (res.status === 429) {
    throw new RateLimitError(parseRetryAfter(res))
  }
}

// Re-export ApiError + parsing helpers so callers can import from api.ts
// (the historical import path) without touching errors.ts directly.
export { ApiError, parseApiError }

const BASE = ''

function getToken(): string {
  return localStorage.getItem('tailr-token') || ''
}

async function request<T>(url: string): Promise<T> {
  const token = getToken()
  const headers: HeadersInit = {}
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  // 429 auto-retry with exponential backoff. Transient 429s (a stray request
  // hitting a near-empty bucket) are retried transparently instead of surfacing
  // as a visible error. The server's burst capacity (rps × 10) is sized so
  // normal use — including rapid page reloads — almost never exhausts it; this
  // backoff handles the rare edge case. We honor Retry-After when present
  // (capped); otherwise 1s → 2s → 4s. After MAX_RETRIES, RateLimitError is
  // thrown so the existing error UI (retry button + dedup toast) takes over.
  const MAX_RETRIES = 3
  const BACKOFF_MS = [1000, 2000, 4000]
  let attempt = 0
  for (;;) {
    const res = await fetch(`${BASE}${url}`, { headers })
    // ── Branch order is critical ──
    // 1. 401 → AuthError (triggers token dialog). Must come before 429: a
    //    request that's both unauthenticated and rate-limited should prompt
    //    for token, not silently retry.
    if (res.status === 401) {
      const { handleAuthError } = useAuth()
      handleAuthError()
      throw new AuthError()
    }
    // 2. 429 → retry with backoff, then RateLimitError. Must come before the
    //    generic !res.ok check so retries happen transparently.
    if (res.status === 429 && attempt < MAX_RETRIES) {
      const retryAfter = parseRetryAfter(res) // seconds, or null
      // Prefer server hint (capped at 4s); otherwise the backoff table.
      const base = retryAfter !== null
        ? Math.min(retryAfter, 4) * 1000
        : BACKOFF_MS[attempt]
      // Add 0-25% jitter to de-synchronize concurrent retries. Without this,
      // a tab-restore burst (12 parallel requests) all 429 at once and would
      // retry in lockstep — re-hitting the still-depleted bucket together.
      const jitter = base * 0.25 * Math.random()
      await new Promise((r) => setTimeout(r, base + jitter))
      attempt++
      continue
    }
    checkRateLimit(res) // throws RateLimitError when retries exhausted
    // 3. Generic error (any other 4xx/5xx) → ApiError with parsed code.
    if (!res.ok) {
      throw await parseApiError(res)
    }
    // Success: backend wraps in { success, data }. Unwrap data.
    const json = await res.json()
    return (json.data ?? json) as T
  }
}

export async function listFiles(path?: string, depth?: number): Promise<FileEntry[]> {
  const params = new URLSearchParams()
  if (path) params.set('path', path)
  if (depth && depth > 1) params.set('depth', String(depth))
  const qs = params.toString()
  const data = await request<{ entries: FileEntry[] }>(`/api/files${qs ? `?${qs}` : ''}`)
  return data.entries ?? []
}

export async function getFileTail(
  path: string,
  lines: number,
): Promise<{ entries: LogEntry[]; totalLines: number }> {
  return request<{ entries: LogEntry[]; totalLines: number }>(
    `/api/file/tail?path=${encodeURIComponent(path)}&lines=${lines}`,
  )
}

export interface HealthData {
  status: string
  version: string
  uptimeSeconds: number
  upgradeInProgress: boolean
  /** True when the /mcp endpoint is mounted ([mcp] enabled, default true). */
  mcpEnabled?: boolean
}

export async function healthCheck(): Promise<HealthData> {
  return request<HealthData>('/api/health')
}

// ── 运行时指标 API ─────────────────────────────────────────

export interface RuntimeData {
  processMemoryBytes: number
  processCpuPercent: number
  systemTotalMemoryBytes: number
  systemUsedMemoryBytes: number
  systemCpuPercent: number
  diskTotalBytes: number
  diskUsedBytes: number
  wsConnections: number
  uptimeSeconds: number
}

/** Fetch a runtime resource snapshot (CPU / memory / disk / WS / uptime).
 *  TTL-cached server-side (5s); safe to poll at 5s intervals. */
export async function fetchRuntime(): Promise<RuntimeData> {
  return request<RuntimeData>('/api/runtime')
}

/// Verify a candidate token WITHOUT persisting it. Used by the token dialog to
/// validate before saving: sends the token to /api/health and returns true only
/// on 200. A 401 returns false; other errors throw.
export async function verifyToken(candidate: string): Promise<boolean> {
  const headers: HeadersInit = {}
  if (candidate) {
    headers['Authorization'] = `Bearer ${candidate}`
  }
  const res = await fetch(`${BASE}/api/health`, { headers })
  if (res.status === 401) return false
  checkRateLimit(res)
  if (!res.ok) throw await parseApiError(res)
  return true
}

// ── 升级 API ──────────────────────────────────────────────

export interface UpdateInfo {
  currentVersion: string
  latestVersion: string
  hasUpdate: boolean
  /** Whether the current platform supports automatic upgrade (Linux x86_64/aarch64). */
  supported: boolean
  releaseUrl: string
}

export interface UpgradeResult {
  status: string
  message: string
}

export async function checkUpgrade(force = false): Promise<UpdateInfo> {
  const qs = force ? '?force=true' : ''
  return request<UpdateInfo>(`/api/upgrade/check${qs}`)
}

export async function performUpgrade(): Promise<UpgradeResult> {
  const token = getToken()
  const headers: HeadersInit = {
    'Content-Type': 'application/json',
    'X-Requested-With': 'XMLHttpRequest',
  }
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  const res = await fetch(`${BASE}/api/upgrade`, {
    method: 'POST',
    headers,
  })
  // ── Branch order (same rationale as request()) ──
  if (res.status === 401) {
    const { handleAuthError } = useAuth()
    handleAuthError()
    throw new AuthError()
  }
  checkRateLimit(res) // throws RateLimitError on 429
  if (!res.ok) {
    // 403 (CSRF missing / token-required), 400 (unsupported platform),
    // 409 (upgrade in progress), 500 (internal) — all carry {error:{code}}.
    throw await parseApiError(res)
  }
  const json = await res.json()
  return (json.data ?? json) as UpgradeResult
}

// ── 日志级别配置 API ──────────────────────────────────────

export interface LevelDef {
  name: string
  keywords: string[]
  colorLight: string
  colorDark: string
}

export interface LogLevelConfig {
  preset: string
  levels: LevelDef[]
}

export async function getLogLevelConfig(): Promise<LogLevelConfig> {
  return request<LogLevelConfig>('/api/config/log-levels')
}

export async function saveLogLevelConfig(config: LogLevelConfig): Promise<LogLevelConfig> {
  const token = getToken()
  const headers: HeadersInit = {
    'Content-Type': 'application/json',
    'X-Requested-With': 'XMLHttpRequest',
  }
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  const res = await fetch(`${BASE}/api/config/log-levels`, {
    method: 'POST',
    headers,
    body: JSON.stringify(config),
  })
  if (res.status === 401) {
    const { handleAuthError } = useAuth()
    handleAuthError()
    throw new AuthError()
  }
  checkRateLimit(res)
  if (!res.ok) {
    throw await parseApiError(res)
  }
  const json = await res.json()
  return (json.data ?? json) as LogLevelConfig
}
