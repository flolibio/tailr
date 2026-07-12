import { ref, shallowRef, computed, shallowReactive, type ComputedRef } from 'vue'
import { WSClient } from '../services/websocket'
import type { LogEntry } from '../services/api'
import { getFileTail } from '../services/api'

const MAX_TABS = 10
const INITIAL_LINES = 300

export interface TabState {
  path: string
  entries: LogEntry[]
  totalLines: number
  isTailMode: boolean
  pendingEntries: LogEntry[]
  isLoading: boolean
  filterKeywords: string[]
  selectedLevels: string[]
  hasUnread: boolean
}

function createTab(path: string): TabState {
  return shallowReactive<TabState>({
    path,
    entries: [],
    totalLines: 0,
    isTailMode: true,
    pendingEntries: [],
    isLoading: false,
    filterKeywords: [],
    selectedLevels: [],
    hasUnread: false,
  })
}

const tabs = shallowRef<TabState[]>([])
const activeTabPath = ref<string | null>(null)
const maxLines = ref(50000)

const wsClient = new WSClient()
let wsInitialized = false

function ensureWs(): void {
  if (wsInitialized) return
  wsInitialized = true
  wsClient.connect()

  wsClient.on('append', (p: unknown, newEntries: unknown) => {
    if (typeof p !== 'string' || !Array.isArray(newEntries)) return
    const tab = tabs.value.find((t) => t.path === p)
    if (!tab) return

    if (p === activeTabPath.value && tab.isTailMode) {
      appendToEntries(tab, newEntries as LogEntry[])
    } else {
      tab.pendingEntries.push(...(newEntries as LogEntry[]))
      tab.hasUnread = true
    }
  })

  wsClient.on('catchup', (p: unknown, catchupEntries: unknown) => {
    if (typeof p !== 'string' || !Array.isArray(catchupEntries)) return
    const tab = tabs.value.find((t) => t.path === p)
    if (!tab) return
    // 仅在 tail 模式下合并 catchup；用户在查看历史时不打断。
    // catchup 来自后端环形缓冲区，可能与已显示的历史（HTTP 初始加载 +
    // 实时 append）重叠。这里按 lineNum 去重合并，只补齐缺失的行，
    // 而不是用 catchup 整体覆盖——否则重连/切标签页会导致日志区被清空。
    if (tab.isTailMode) {
      mergeCatchup(tab, catchupEntries as LogEntry[])
    }
    tab.isLoading = false
  })

  wsClient.on('truncate', (p: unknown) => {
    if (typeof p !== 'string') return
    const tab = tabs.value.find((t) => t.path === p)
    if (!tab) return
    tab.entries = []
    tab.totalLines = 0
    tab.pendingEntries = []
  })

  wsClient.on('delete', (p: unknown) => {
    if (typeof p === 'string') closeTab(p)
  })
}

function appendToEntries(tab: TabState, newEntries: LogEntry[]): void {
  const arr = [...tab.entries, ...newEntries]
  if (arr.length > maxLines.value) {
    arr.splice(0, arr.length - maxLines.value)
  }
  tab.entries = arr
  tab.totalLines = arr.length
}

function mergeCatchup(tab: TabState, catchupEntries: LogEntry[]): void {
  if (catchupEntries.length === 0) return

  // catchup 来自后端环形缓冲区，lineNum 空间与 append 一致。
  // 只补齐比当前已显示最大 lineNum 更新的行，避免用缓冲区覆盖已有历史。
  const maxLineNum = tab.entries.length > 0 ? tab.entries[tab.entries.length - 1].lineNum : -1
  const newOnes = catchupEntries.filter((e) => e.lineNum > maxLineNum)
  if (newOnes.length === 0) return

  appendToEntries(tab, newOnes)
}

function drainPending(tab: TabState): void {
  if (tab.pendingEntries.length === 0) return
  if (tab.isTailMode) {
    appendToEntries(tab, tab.pendingEntries)
  }
  tab.pendingEntries = []
}

const activeTab: ComputedRef<TabState | null> = computed(
  () => tabs.value.find((t) => t.path === activeTabPath.value) ?? null,
)

function openTab(path: string): void {
  ensureWs()

  const existing = tabs.value.find((t) => t.path === path)
  if (existing) {
    switchTo(path)
    return
  }

  if (tabs.value.length >= MAX_TABS) {
    closeTab(tabs.value[tabs.value.length - 1].path)
  }

  const tab = createTab(path)
  tabs.value = [...tabs.value, tab]
  activeTabPath.value = path
  loadInitial(tab)
}

async function loadInitial(tab: TabState): Promise<void> {
  tab.isLoading = true
  try {
    const data = await getFileTail(tab.path, INITIAL_LINES)
    // 校验 tab 在 await 期间未被关闭，避免幻影 WS 订阅
    if (!tabs.value.find((t) => t.path === tab.path)) return
    tab.entries = data.entries
    tab.totalLines = data.totalLines
    tab.isTailMode = true
    wsClient.subscribe(tab.path)
  } catch (e) {
    console.error('Failed to load:', e)
  } finally {
    if (tabs.value.find((t) => t.path === tab.path)) {
      tab.isLoading = false
    }
  }
}

function closeTab(path: string): void {
  const idx = tabs.value.findIndex((t) => t.path === path)
  if (idx === -1) return

  wsClient.unsubscribe(path)
  const remaining = tabs.value.filter((t) => t.path !== path)
  tabs.value = remaining

  if (activeTabPath.value === path) {
    const nextIdx = Math.min(idx, remaining.length - 1)
    if (nextIdx >= 0) {
      switchTo(remaining[nextIdx].path)
    } else {
      activeTabPath.value = null
    }
  }
}

function switchTo(path: string): void {
  const tab = tabs.value.find((t) => t.path === path)
  if (!tab) return
  activeTabPath.value = path
  tab.hasUnread = false
  drainPending(tab)
}

function setTailMode(val: boolean): void {
  const tab = activeTab.value
  if (!tab) return
  tab.isTailMode = val
  if (val) {
    drainPending(tab)
  }
}

async function reloadActiveTab(): Promise<void> {
  const tab = activeTab.value
  if (!tab) return
  try {
    const count = Math.max(tab.entries.length, INITIAL_LINES)
    const data = await getFileTail(tab.path, count)
    tab.entries = data.entries
    tab.totalLines = data.totalLines
  } catch (e) {
    console.error('Failed to reload after config change:', e)
  }
}

export function useTabs() {
  return {
    tabs,
    activeTabPath,
    activeTab,
    maxLines,
    wsClient,
    openTab,
    closeTab,
    switchTo,
    setTailMode,
    reloadActiveTab,
  }
}
