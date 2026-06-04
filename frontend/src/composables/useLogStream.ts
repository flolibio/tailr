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

  async function loadInitial(path: string, lines: number = 200): Promise<void> {
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
      wsClient.subscribe(path)
    } catch (e) {
      console.error('Failed to load initial data:', e)
    } finally {
      isLoading.value = false
    }
  }

  async function loadMore(offset: number, limit: number): Promise<void> {
    if (!currentFile.value) return
    isLoading.value = true
    try {
      const data = await getFileContent(currentFile.value, offset, limit)
      const merged = new Map<number, LogEntry>()
      for (const e of entries.value) merged.set(e.lineNum, e)
      for (const e of data.entries) merged.set(e.lineNum, e)
      entries.value = Array.from(merged.values()).sort((a, b) => a.lineNum - b.lineNum)
      totalLines.value = data.totalLines
    } catch (e) {
      console.error('Failed to load more:', e)
    } finally {
      isLoading.value = false
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
    maxLines,
    pendingEntries,
    wsClient,
    selectFile,
    loadInitial,
    loadMore,
    toggleTailMode,
    setTailMode,
  }
}
