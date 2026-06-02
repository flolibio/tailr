export interface LogEntry {
  lineNum: number
  raw: string
  level: string
  timestamp?: string
  fields?: Record<string, unknown>
}

export interface FileEntry {
  name: string
  path: string
  size: number
  modified: string
  isDir: boolean
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

const BASE = ''

async function request<T>(url: string): Promise<T> {
  const res = await fetch(`${BASE}${url}`)
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

export async function listFiles(path?: string): Promise<FileEntry[]> {
  const qs = path ? `?path=${encodeURIComponent(path)}` : ''
  const data = await request<{ entries: FileEntry[] }>(`/api/files${qs}`)
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

export async function healthCheck(): Promise<{ status: string; uptimeSeconds: number }> {
  return request<{ status: string; uptimeSeconds: number }>('/api/health')
}
