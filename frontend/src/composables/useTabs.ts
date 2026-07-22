import { ref, shallowRef, computed, shallowReactive, type ComputedRef } from 'vue'
import { WSClient } from '../services/websocket'
import type { LogEntry } from '../services/api'
import { getFileTail, RateLimitError } from '../services/api'
import { useToast } from './useToast'
import i18n from '../locales'

const MAX_TABS = 10
const INITIAL_LINES = 300

/** Tab-level load error type. `null` = no error. */
export type TabLoadError = 'rateLimited' | 'generic' | null

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
  // file_tail returns estimated lineNums (tail_start); the precise count arrives
  // later via the WS Subscribed message. While set, entries[0..count) still carry
  // estimated lineNums and must be shifted by `delta` once the exact total lands.
  pendingCorrection: { count: number; estimatedTotal: number } | null
  // v0.8: lazy-load marker. Tabs restored from persistence start as lazy
  // (no entries, no WS subscription); loadInitial+subscribe fire on first switchTo.
  isLazy: boolean
  /** Non-null when the last loadInitial/reloadActiveTab failed. The LogPanel
   *  renders an error branch (with retry button) instead of an empty viewer.
   *  - 'rateLimited': server returned 429 (REST rate limit hit)
   *  - 'generic': any other failure (network, 5xx, etc.) */
  loadError: TabLoadError
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
    pendingCorrection: null,
    isLazy: false,
    loadError: null,
  })
}

const tabs = shallowRef<TabState[]>([])
const activeTabPath = ref<string | null>(null)
const maxLines = ref(50000)

const wsClient = new WSClient()
let wsInitialized = false

// ── Rate-limit toast dedup ──
// Multiple concurrent REST requests can all 429 at once (e.g. tab restore fires
// several getFileTail calls). Without dedup, each would pop its own toast and
// spam the user. We keep a single module-level toast id: the first 429 pops it,
// subsequent ones within the toast's lifetime are ignored. The toast is
// dismissed explicitly when any subsequent load succeeds.
let rateLimitToastId: string | null = null

/** Pop (or refresh) the global rate-limit toast. Called from loadInitial /
 *  reloadActiveTab catches on RateLimitError, and from App.vue on WS 1013.
 *  `retryAfter` is the server's hint in seconds (null = not provided, e.g. WS). */
function showRateLimitToast(retryAfter: number | null): void {
  const { warning, dismiss } = useToast()
  const t = i18n.global.t.bind(i18n.global)
  if (rateLimitToastId) dismiss(rateLimitToastId)
  const msg = retryAfter !== null
    ? t('errors.rateLimitedWithRetry', { seconds: retryAfter })
    : t('errors.rateLimited')
  rateLimitToastId = warning(msg, {
    title: t('errors.rateLimitTitle'),
    duration: 0, // persist until user acts or a load succeeds
    closeButton: true,
  })
}

/** Dismiss the rate-limit toast if present. Called on successful load to clear
 *  the "rate limited" notice once the user has recovered. */
function dismissRateLimitToast(): void {
  if (rateLimitToastId) {
    const { dismiss } = useToast()
    dismiss(rateLimitToastId)
    rateLimitToastId = null
  }
}

/** WS-specific rate-limit message (no Retry-After available from close frame). */
function showWsRateLimitToast(): void {
  const { warning, dismiss } = useToast()
  const t = i18n.global.t.bind(i18n.global)
  if (rateLimitToastId) dismiss(rateLimitToastId)
  rateLimitToastId = warning(t('errors.wsConnectionLimit'), {
    title: t('errors.rateLimitTitle'),
    duration: 0,
    closeButton: true,
  })
}

// ── v0.8: tab persistence (lazy-load) ──
// Only the tab list (paths + active) is persisted; entries are ephemeral.
// On reload, the active tab loads immediately; others load on first switchTo.
const OPEN_TABS_KEY = 'tailr-open-tabs'

interface OpenTabsState {
  paths: string[]
  activeTabPath: string | null
}

function persistOpenTabs(): void {
  try {
    const state: OpenTabsState = {
      paths: tabs.value.map((t) => t.path),
      activeTabPath: activeTabPath.value,
    }
    localStorage.setItem(OPEN_TABS_KEY, JSON.stringify(state))
  } catch { /* ignore quota / private mode */ }
}

function loadOpenTabs(): OpenTabsState | null {
  try {
    const raw = localStorage.getItem(OPEN_TABS_KEY)
    if (raw) return JSON.parse(raw)
  } catch {}
  return null
}

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
      // Cap pendingEntries so background tabs with high log volume don't grow
      // unbounded while the user is viewing another tab.
      tab.pendingEntries.push(...(newEntries as LogEntry[]))
      if (tab.pendingEntries.length > maxLines.value) {
        tab.pendingEntries.splice(0, tab.pendingEntries.length - maxLines.value)
      }
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

  // Subscribed carries the exact total line count (LineIndex::build), which
  // arrives after file_tail's estimate. Shift the initially-loaded entries'
  // lineNums into the precise coordinate system so bookmarks stay valid.
  wsClient.on('subscribed', (p: unknown, total: unknown) => {
    if (typeof p !== 'string' || typeof total !== 'number') return
    const tab = tabs.value.find((t) => t.path === p)
    if (!tab || !tab.pendingCorrection) return

    const delta = total - tab.pendingCorrection.estimatedTotal
    if (delta !== 0 && tab.entries.length > 0) {
      const count = Math.min(tab.pendingCorrection.count, tab.entries.length)
      tab.entries = tab.entries.map((e, i) =>
        i < count ? { ...e, lineNum: e.lineNum + delta } : e,
      )
    }
    tab.totalLines = total
    tab.pendingCorrection = null
  })
}

function appendToEntries(tab: TabState, newEntries: LogEntry[]): void {
  const arr = [...tab.entries, ...newEntries]
  if (arr.length > maxLines.value) {
    arr.splice(0, arr.length - maxLines.value)
  }
  tab.entries = arr
  // NOTE: do NOT overwrite totalLines here. It tracks the file's real line
  // count (set by file_tail / the WS Subscribed message); arr.length is only
  // the in-memory buffer length. See loadInitial/subscribed for the source of
  // truth. The statusbar deliberately renders entries.length / maxLines.
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
    // If the existing tab never loaded (entries empty + not lazy), its earlier
    // loadInitial failed (e.g. 401 before auth). Re-load so the content + WS
    // subscription are established once the token is valid. switchTo only
    // handles lazy tabs; a non-lazy empty tab would otherwise stay blank.
    if (!existing.isLazy && existing.entries.length === 0) {
      loadInitial(existing)
    }
    return
  }

  if (tabs.value.length >= MAX_TABS) {
    closeTab(tabs.value[tabs.value.length - 1].path)
  }

  const tab = createTab(path)
  tabs.value = [...tabs.value, tab]
  activeTabPath.value = path
  persistOpenTabs()
  loadInitial(tab)
}

async function loadInitial(tab: TabState): Promise<void> {
  tab.isLoading = true
  tab.loadError = null
  try {
    const data = await getFileTail(tab.path, INITIAL_LINES)
    // 校验 tab 在 await 期间未被关闭，避免幻影 WS 订阅
    if (!tabs.value.find((t) => t.path === tab.path)) return
    tab.entries = data.entries
    tab.totalLines = data.totalLines
    // file_tail's totalLines is estimated (tail_start). Record the estimate so
    // the Subscribed handler can shift these entries' lineNums to the exact
    // coordinate once LineIndex::build finishes on the backend.
    tab.pendingCorrection = { count: data.entries.length, estimatedTotal: data.totalLines }
    tab.isTailMode = true
    wsClient.subscribe(tab.path)
    dismissRateLimitToast()
  } catch (e) {
    if (e instanceof RateLimitError) {
      tab.loadError = 'rateLimited'
      showRateLimitToast(e.retryAfter)
    } else {
      console.error('Failed to load:', e)
      tab.loadError = 'generic'
    }
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
  persistOpenTabs()
}

function switchTo(path: string): void {
  const tab = tabs.value.find((t) => t.path === path)
  if (!tab) return
  activeTabPath.value = path
  tab.hasUnread = false
  // v0.8: lazy-loaded tabs load on first activation (restored from persistence).
  if (tab.isLazy) {
    tab.isLazy = false
    ensureWs()
    loadInitial(tab)
  } else {
    drainPending(tab)
  }
  persistOpenTabs()
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
  tab.loadError = null
  try {
    const count = Math.max(tab.entries.length, INITIAL_LINES)
    const data = await getFileTail(tab.path, count)
    tab.entries = data.entries
    tab.totalLines = data.totalLines
    // No WS resubscribe happens here, so no Subscribed message will arrive to
    // correct the estimate. Clear any stale pendingCorrection rather than leave
    // a shift pending that never resolves.
    tab.pendingCorrection = null
    dismissRateLimitToast()
  } catch (e) {
    if (e instanceof RateLimitError) {
      tab.loadError = 'rateLimited'
      showRateLimitToast(e.retryAfter)
    } else {
      console.error('Failed to reload after config change:', e)
      tab.loadError = 'generic'
    }
  }
}

// v0.8: restore the tab list from persistence on app startup.
// The active tab loads immediately; others start lazy (load on first switchTo).
function restoreTabs(): void {
  const state = loadOpenTabs()
  if (!state || state.paths.length === 0) return
  // Enforce MAX_TABS in case localStorage was hand-edited or grew stale.
  const paths = state.paths.slice(0, MAX_TABS)
  const active = state.activeTabPath && paths.includes(state.activeTabPath)
    ? state.activeTabPath
    : paths[0]
  tabs.value = paths.map((p) => {
    const tab = createTab(p)
    if (p !== active) tab.isLazy = true
    return tab
  })
  activeTabPath.value = active
  ensureWs()
  const activeTabObj = tabs.value.find((t) => t.path === active)
  if (activeTabObj) loadInitial(activeTabObj)
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
    restoreTabs,
    /** Called by App.vue when WSClient emits 'rateLimited' (close code 1013).
     *  Shows the WS-specific rate-limit toast (no Retry-After available). */
    notifyWsRateLimited: showWsRateLimitToast,
  }
}
