import { ref, shallowRef } from 'vue'
import { WSClient } from '../services/websocket'
import type { LogEntry } from '../services/api'
import { getFileTail, getFileContent } from '../services/api'

export function useLogStream() {
  const currentFile = ref<string | null>(null)
  const entries = shallowRef<LogEntry[]>([])
  const isTailMode = ref(true)
  const totalLines = ref(0)
  const isLoading = ref(false)
  const isLoadingOlder = ref(false)
  const hasMoreHistory = ref(true)
  const maxLines = ref(50000)
  const pendingEntries = ref<LogEntry[]>([])

  const wsClient = new WSClient()
  wsClient.connect()

  let unsubscribeAppend: (() => void) | null = null
  let unsubscribeCatchup: (() => void) | null = null
  let unsubscribeTruncate: (() => void) | null = null
  let unsubscribeDelete: (() => void) | null = null

  function cleanupWsListeners(): void {
    unsubscribeAppend?.()
    unsubscribeCatchup?.()
    unsubscribeTruncate?.()
    unsubscribeDelete?.()
    unsubscribeAppend = null
    unsubscribeCatchup = null
    unsubscribeTruncate = null
    unsubscribeDelete = null
  }

  function appendEntries(newEntries: LogEntry[]): void {
    if (!isTailMode.value) {
      pendingEntries.value = [...pendingEntries.value, ...newEntries]
      return
    }
    const arr = entries.value
    arr.push(...newEntries)
    if (arr.length > maxLines.value) {
      arr.splice(0, arr.length - maxLines.value)
    }
    totalLines.value = arr.length
    entries.value = [...arr]
  }

  function drainPending(): void {
    if (pendingEntries.value.length === 0) return
    const arr = entries.value
    arr.push(...pendingEntries.value)
    if (arr.length > maxLines.value) {
      arr.splice(0, arr.length - maxLines.value)
    }
    totalLines.value = arr.length
    entries.value = [...arr]
    pendingEntries.value = []
  }

  function selectFile(path: string): void {
    if (currentFile.value) {
      wsClient.unsubscribe(currentFile.value)
    }
    cleanupWsListeners()
    currentFile.value = path
    entries.value = []
    totalLines.value = 0
    isTailMode.value = true
    isLoading.value = true
    hasMoreHistory.value = true

    unsubscribeAppend = wsClient.on('append', (p: unknown, newEntries: unknown) => {
      if (p === path && Array.isArray(newEntries)) {
        appendEntries(newEntries as LogEntry[])
      }
    })

    unsubscribeCatchup = wsClient.on('catchup', (p: unknown, catchupEntries: unknown) => {
      if (p === path && Array.isArray(catchupEntries)) {
        entries.value = catchupEntries as LogEntry[]
        totalLines.value = entries.value.length
        isLoading.value = false
      }
    })

    unsubscribeTruncate = wsClient.on('truncate', (p: unknown) => {
      if (p === path) {
        entries.value = []
        totalLines.value = 0
        hasMoreHistory.value = true
      }
    })

    unsubscribeDelete = wsClient.on('delete', (p: unknown) => {
      if (p === path) {
        entries.value = []
        totalLines.value = 0
        currentFile.value = null
      }
    })

    wsClient.subscribe(path)
  }

  async function loadInitial(path: string, lines: number = 100): Promise<void> {
    isLoading.value = true
    cleanupWsListeners()

    if (currentFile.value) {
      wsClient.unsubscribe(currentFile.value)
    }

    // Register WS listeners for real-time updates
    unsubscribeAppend = wsClient.on('append', (p: unknown, newEntries: unknown) => {
      if (p === path && Array.isArray(newEntries)) {
        appendEntries(newEntries as LogEntry[])
      }
    })

    unsubscribeTruncate = wsClient.on('truncate', (p: unknown) => {
      if (p === path) {
        entries.value = []
        totalLines.value = 0
        hasMoreHistory.value = true
      }
    })

    unsubscribeDelete = wsClient.on('delete', (p: unknown) => {
      if (p === path) {
        entries.value = []
        totalLines.value = 0
        currentFile.value = null
      }
    })

    try {
      const data = await getFileTail(path, lines)
      entries.value = data.entries
      totalLines.value = data.totalLines
      currentFile.value = path
      isTailMode.value = true
      hasMoreHistory.value = data.entries.length > 0 && data.entries[0].lineNum > 0
      wsClient.subscribe(path)
    } catch (e) {
      console.error('Failed to load initial data:', e)
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Load older entries prepended before the current earliest entry.
   * Returns the number of new entries loaded (used for scroll position adjustment).
   */
  async function loadOlder(limit: number = 200): Promise<number> {
    if (!currentFile.value || isLoadingOlder.value || !hasMoreHistory.value) return 0

    const firstLine = entries.value[0]?.lineNum
    if (firstLine === undefined || firstLine === 0) {
      hasMoreHistory.value = false
      return 0
    }

    isLoadingOlder.value = true
    try {
      const offset = firstLine > limit ? firstLine - limit : 0
      const data = await getFileContent(currentFile.value, offset, limit)

      if (data.entries.length === 0) {
        hasMoreHistory.value = false
        return 0
      }

      // Prepend older entries (API returns entries starting from `offset`)
      const merged = new Map<number, LogEntry>()
      for (const e of data.entries) merged.set(e.lineNum, e)
      for (const e of entries.value) merged.set(e.lineNum, e)
      entries.value = Array.from(merged.values()).sort((a, b) => a.lineNum - b.lineNum)
      totalLines.value = data.totalLines

      // If we got to line 0 or got fewer than requested, no more history
      if (offset === 0 || data.entries.length < limit) {
        hasMoreHistory.value = false
      }

      return data.entries.length
    } catch (e) {
      console.error('Failed to load older entries:', e)
      return 0
    } finally {
      isLoadingOlder.value = false
    }
  }

  function toggleTailMode(): void {
    isTailMode.value = !isTailMode.value
    if (isTailMode.value) {
      drainPending()
    }
  }

  function setTailMode(val: boolean): void {
    isTailMode.value = val
    if (val) {
      drainPending()
    }
  }

  return {
    currentFile,
    entries,
    isTailMode,
    totalLines,
    isLoading,
    isLoadingOlder,
    hasMoreHistory,
    maxLines,
    pendingEntries,
    wsClient,
    selectFile,
    loadInitial,
    loadOlder,
    toggleTailMode,
    setTailMode,
  }
}
