import { useAuth } from '../composables/useAuth'

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

export interface FileContent {
  entries: LogEntry[]
  totalLines: number
  offset: number
  limit: number
  hasMore: boolean
}

export interface FileInfo {
  path: string
  size: number
  modified: string
  totalLines: number
}

export interface SearchMatch {
  lineNumber: number
  content: string
  contextBefore: string[]
  contextAfter: string[]
}

export interface SearchResult {
  matches: SearchMatch[]
  totalMatches: number
  hasMore: boolean
}

export class AuthError extends Error {
  constructor() {
    super('Authentication required')
    this.name = 'AuthError'
  }
}

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

  const res = await fetch(`${BASE}${url}`, { headers })
  if (res.status === 401) {
    const { handleAuthError } = useAuth()
    handleAuthError()
    throw new AuthError()
  }
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}: ${res.statusText}`)
  }
  const json = await res.json()
  if (json.success === false) {
    throw new Error(json.error || 'Request failed')
  }
  // Backend wraps in { success, data }. Unwrap data.
  return (json.data ?? json) as T
}

export async function listFiles(path?: string, depth?: number): Promise<FileEntry[]> {
  const params = new URLSearchParams()
  if (path) params.set('path', path)
  if (depth && depth > 1) params.set('depth', String(depth))
  const qs = params.toString()
  const data = await request<{ entries: FileEntry[] }>(`/api/files${qs ? `?${qs}` : ''}`)
  return data.entries ?? []
}

export async function getFileContent(
  path: string,
  offset: number,
  limit: number,
): Promise<FileContent> {
  return request<FileContent>(
    `/api/file/content?path=${encodeURIComponent(path)}&offset=${offset}&limit=${limit}`,
  )
}

export async function getFileTail(
  path: string,
  lines: number,
): Promise<{ entries: LogEntry[]; totalLines: number }> {
  return request<{ entries: LogEntry[]; totalLines: number }>(
    `/api/file/tail?path=${encodeURIComponent(path)}&lines=${lines}`,
  )
}

export async function getFileInfo(path: string): Promise<FileInfo> {
  return request<FileInfo>(
    `/api/file/info?path=${encodeURIComponent(path)}`,
  )
}

export async function searchLogs(
  path: string,
  query: string,
  options?: { regex?: boolean; levels?: string[]; context?: number },
): Promise<SearchResult> {
  const params = new URLSearchParams({ path, q: query })
  if (options?.regex) params.set('regex', 'true')
  if (options?.levels?.length) params.set('levels', options.levels.join(','))
  if (options?.context !== undefined) params.set('context', String(options.context))
  return request<SearchResult>(`/api/search?${params.toString()}`)
}

export async function healthCheck(): Promise<{ status: string; version: string; uptimeSeconds: number }> {
  return request<{ status: string; version: string; uptimeSeconds: number }>('/api/health')
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
  if (!res.ok) throw new Error(`HTTP ${res.status}: ${res.statusText}`)
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

export async function checkUpgrade(): Promise<UpdateInfo> {
  return request<UpdateInfo>('/api/upgrade/check')
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
  if (res.status === 401) {
    const { handleAuthError } = useAuth()
    handleAuthError()
    throw new AuthError()
  }
  if (res.status === 403) {
    throw new Error('CSRF check failed')
  }
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}: ${res.statusText}`)
  }
  const json = await res.json()
  if (json.success === false) {
    throw new Error(json.error || 'Upgrade failed')
  }
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
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}: ${res.statusText}`)
  }
  const json = await res.json()
  if (json.success === false) {
    throw new Error(json.error || 'Save failed')
  }
  return (json.data ?? json) as LogLevelConfig
}
