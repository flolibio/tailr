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
}

export interface FileInfo {
  path: string
  size: number
  modified: string
  totalLines: number
}

export interface SearchResult {
  matches: LogEntry[]
  totalMatches: number
  query: string
  elapsedMs: number
}

const BASE = ''

async function request<T>(url: string): Promise<T> {
  const res = await fetch(`${BASE}${url}`)
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}: ${res.statusText}`)
  }
  return res.json() as Promise<T>
}

export async function listFiles(path?: string): Promise<FileEntry[]> {
  const qs = path ? `?path=${encodeURIComponent(path)}` : ''
  return request<FileEntry[]>(`/api/files${qs}`)
}

export async function getFileContent(
  path: string,
  offset: number,
  limit: number,
): Promise<FileContent> {
  return request<FileContent>(
    `/api/files/content?path=${encodeURIComponent(path)}&offset=${offset}&limit=${limit}`,
  )
}

export async function getFileTail(
  path: string,
  lines: number,
): Promise<LogEntry[]> {
  return request<LogEntry[]>(
    `/api/files/tail?path=${encodeURIComponent(path)}&lines=${lines}`,
  )
}

export async function getFileInfo(path: string): Promise<FileInfo> {
  return request<FileInfo>(
    `/api/files/info?path=${encodeURIComponent(path)}`,
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

export async function healthCheck(): Promise<{ status: string; uptime: number }> {
  return request<{ status: string; uptime: number }>('/api/health')
}
