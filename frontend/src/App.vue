<script setup lang="ts">
import { ref, reactive, computed, watch, onMounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import FileBrowser from './components/FileBrowser.vue'
import LogPanel from './components/LogPanel.vue'
import FilterBar from './components/FilterBar.vue'
import TabBar from './components/TabBar.vue'
import BookmarkPanel from './components/BookmarkPanel.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import SelectionToolbar from './components/SelectionToolbar.vue'
import TokenDialog from './components/TokenDialog.vue'
import ToastContainer from './components/ToastContainer.vue'
import type { Settings } from './components/SettingsPanel.vue'
import { useTabs } from './composables/useTabs'
import { useLogLevels } from './composables/useLogLevels'
import { useAuth } from './composables/useAuth'
import { useRecentFiles } from './composables/useRecentFiles'
import { useCopyFeedback } from './composables/useClipboard'
import { useToast } from './composables/useToast'
import { useUpdateNotifier } from './composables/useUpdateNotifier'
import { filterEntries } from './utils/filter'
import { PanelLeft, Settings as SettingsIcon, Play, Pause, Share2, Check } from 'lucide-vue-next'

const { t } = useI18n()
const { info: toastInfo } = useToast()
const { shouldShowToast, markNotified, dismiss, hasUpdateBadge } = useUpdateNotifier()

const {
  tabs,
  activeTabPath,
  activeTab,
  maxLines,
  wsClient,
  openTab,
  setTailMode,
  reloadActiveTab,
  restoreTabs,
} = useTabs()

// v0.8: multi-instance — one LogPanel per tab, kept alive with v-show so each
// tab preserves its own scrollTop / measuredHeights / expandedLines / markedLine.
// logPanelRefs maps tab path → panel instance for bookmark scroll / tail calls.
const logPanelRefs = ref<Map<string, InstanceType<typeof LogPanel>>>(new Map())

function setPanelRef(path: string, el: any): void {
  if (el) {
    logPanelRefs.value.set(path, el as InstanceType<typeof LogPanel>)
  } else {
    logPanelRefs.value.delete(path)
  }
}

function getActivePanel(): InstanceType<typeof LogPanel> | undefined {
  return activeTabPath.value ? logPanelRefs.value.get(activeTabPath.value) : undefined
}

const filterBarRef = ref<InstanceType<typeof FilterBar> | null>(null)
const fileBrowserRef = ref<InstanceType<typeof FileBrowser> | null>(null)
const showSettings = ref(false)
// v0.9: latest update notice received via WS (for badge dismiss on open).
const pendingUpdate = ref<{ latestVersion: string; currentVersion: string; releaseUrl: string } | null>(null)
const sidebarCollapsed = ref(false)
const sidebarWidth = ref(300)
const refreshKey = ref(0)

const { token, showTokenDialog } = useAuth()
const { recordOpen } = useRecentFiles()

// v0.8: URL state restore (share link). Consumed once on load.
function restoreFromUrl(): void {
  const params = new URLSearchParams(location.search)
  const file = params.get('file')
  if (!file) return
  const kw = params.get('kw')?.split(',').filter(Boolean) ?? []
  const levels = params.get('levels')?.split(',').filter(Boolean) ?? []
  openTab(file)
  // Record recent-file open + reveal in tree, mirroring handleSelectFile
  // which we bypass by opening the tab directly from the URL.
  recordOpen(file)
  if (sidebarCollapsed.value) sidebarCollapsed.value = false
  fileBrowserRef.value?.ensureVisible(file)
  // URL filter applies for this session only (not persisted to localStorage).
  nextTick(() => {
    const tab = tabs.value.find((t) => t.path === file)
    if (tab) {
      if (kw.length) tab.filterKeywords = kw
      if (levels.length) tab.selectedLevels = levels
    }
  })
  // Clean the URL — share params are consumed, leave a clean root URL.
  // Sharing is always via the explicit Share button (buildShareUrl), never
  // by keeping the URL in sync with the active tab.
  history.replaceState({}, '', location.pathname)
}

// v0.8: build a share link URL from current tab state (always carries params).
function buildShareUrl(): string {
  const tab = activeTab.value
  const params = new URLSearchParams()
  if (tab) {
    params.set('file', tab.path)
    if (tab.filterKeywords.length) params.set('kw', tab.filterKeywords.join(','))
    if (tab.selectedLevels.length) params.set('levels', tab.selectedLevels.join(','))
  }
  return `${location.origin}${location.pathname}?${params}`
}

// v0.8: copy share link to clipboard.
const { copied: linkCopied, copy: copyShareLink } = useCopyFeedback()

async function handleShareLink(): Promise<void> {
  await copyShareLink(buildShareUrl())
}

const activeLineRange = computed(() => {
  const tab = activeTab.value
  if (!tab || tab.entries.length === 0) return { min: 0, max: 0 }
  return {
    min: tab.entries[0].lineNum,
    max: tab.entries[tab.entries.length - 1].lineNum,
  }
})

// When active tab changes (e.g. TabBar click), reveal file in tree.
// Do NOT auto-expand a collapsed sidebar — the user collapsed it intentionally.
watch(activeTabPath, (path) => {
  if (!path) return
  fileBrowserRef.value?.ensureVisible(path)
})

function handleSelectFile(path: string): void {
  openTab(path)
  recordOpen(path)
  // Expand sidebar and reveal file in tree
  if (sidebarCollapsed.value) sidebarCollapsed.value = false
  fileBrowserRef.value?.ensureVisible(path)
}

function handleBookmarkScroll(lineNum: number): void {
  getActivePanel()?.scrollToLine(lineNum)
}

watch(token, () => {
  refreshKey.value++
  loadLogLevels()
  wsClient.disconnect()
  wsClient.connect()
})

watch(activeTabPath, (p) => {
  document.title = p ? `Tailr - ${p}` : 'Tailr'
}, { immediate: true })

// Used by the statusbar counts. Shares the same filterEntries as LogPanel
// so counts never desync from the rendered rows.
const filteredEntries = computed(() => {
  const tab = activeTab.value
  if (!tab) return []
  return filterEntries(tab.entries, tab.selectedLevels, tab.filterKeywords, allLevels.value)
})

const matchCount = computed(() => {
  const tab = activeTab.value
  if (!tab || tab.filterKeywords.length === 0) return 0
  return filteredEntries.value.length
})

const SETTINGS_KEY = 'tailr-settings'

const defaultSettings: Settings = {
  fontSize: 14,
  fontFamily: 'JetBrains Mono',
  autoScroll: true,
  maxVisibleLines: 50000,
  darkTheme: true,
  displayMode: 'compact',
}

function loadSettings(): Settings {
  try {
    const saved = localStorage.getItem(SETTINGS_KEY)
    if (saved) {
      return { ...defaultSettings, ...JSON.parse(saved) }
    }
  } catch {}
  return { ...defaultSettings }
}

function saveSettings(s: Settings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(s))
  } catch {}
}

const settings = reactive<Settings>(loadSettings())

const {
  config: logLevelConfig,
  levelNames,
  applyThemeColors,
  loadFromBackend: loadLogLevels,
} = useLogLevels()

const levelDotColors = computed(() => {
  const colors: Record<string, string> = {}
  for (const level of logLevelConfig.value.levels) {
    const isDark = settings.darkTheme
    colors[level.name] = isDark ? level.colorDark : level.colorLight
  }
  return colors
})

const allLevels = computed(() => levelNames.value)

function toggleLevel(lv: string): void {
  const tab = activeTab.value
  if (!tab) return
  const idx = tab.selectedLevels.indexOf(lv)
  if (idx >= 0) {
    tab.selectedLevels = tab.selectedLevels.filter((_, i) => i !== idx)
  } else {
    tab.selectedLevels = [...tab.selectedLevels, lv]
  }
}

function addKeyword(kw: string): void {
  const tab = activeTab.value
  if (!tab || tab.filterKeywords.includes(kw)) return
  tab.filterKeywords = [...tab.filterKeywords, kw]
  saveSearchHistory(kw)
}

function saveSearchHistory(kw: string): void {
  try {
    const key = 'tailr-search-history'
    const history: string[] = JSON.parse(localStorage.getItem(key) || '[]')
    const normalized = kw.toLowerCase()
    const filtered = history.filter((h) => h.toLowerCase() !== normalized)
    filtered.unshift(kw)
    localStorage.setItem(key, JSON.stringify(filtered.slice(0, 20)))
  } catch { /* ignore */ }
}

function removeKeyword(index: number): void {
  const tab = activeTab.value
  if (!tab) return
  tab.filterKeywords = tab.filterKeywords.filter((_, i) => i !== index)
}

function editKeyword(index: number, newValue: string): void {
  const tab = activeTab.value
  if (!tab) return
  const updated = [...tab.filterKeywords]
  updated[index] = newValue
  tab.filterKeywords = updated
  saveSearchHistory(newValue)
}

function clearAllKeywords(): void {
  const tab = activeTab.value
  if (!tab) return
  tab.filterKeywords = []
}

function handleStickToBottom(): void {
  setTailMode(true)
  settings.autoScroll = true
  getActivePanel()?.scrollToBottom()
}

function toggleFollowTail(): void {
  const tab = activeTab.value
  if (!tab) return
  if (tab.isTailMode) {
    setTailMode(false)
    settings.autoScroll = false
  } else {
    setTailMode(true)
    settings.autoScroll = true
    getActivePanel()?.scrollToBottom()
  }
}

function handleAutoScrollChange(enabled: boolean): void {
  settings.autoScroll = enabled
  if (enabled) {
    setTailMode(true)
    getActivePanel()?.scrollToBottom()
  } else {
    setTailMode(false)
  }
}

onMounted(() => {
  if (settings.darkTheme) {
    document.documentElement.classList.add('dark')
    delete document.documentElement.dataset.theme
  } else {
    document.documentElement.classList.remove('dark')
    document.documentElement.dataset.theme = 'light'
  }

  applyThemeColors(settings.darkTheme)
  loadLogLevels()

  document.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
      e.preventDefault()
      filterBarRef.value?.focus()
    }
  })

  // v0.8: restore the tab list from persistence first (active loads, rest lazy),
  // then apply URL share-link override if present. Share links are consumed once
  // on load; subsequent user actions (switch tab, filter) do NOT pollute the URL
  // — sharing is always via the explicit Share button (buildShareUrl).
  restoreTabs()
  restoreFromUrl()

  // v0.9: listen for server-pushed update notifications. On first sight of a new
  // version, show a toast (deduped via localStorage) and light the Settings badge.
  wsClient.on(
    'updateAvailable',
    (latest: unknown, current: unknown, releaseUrl: unknown) => {
      if (typeof latest !== 'string') return
      if (shouldShowToast(latest)) {
        toastInfo(t('settings.newVersionAvailable'), {
          title: t('settings.updateToastTitle', { version: latest }),
          action: {
            label: t('settings.view'),
            onClick: () => { showSettings.value = true },
          },
          duration: 8000,
          closeButton: true,
        })
        markNotified(latest)
      } else {
        // Already notified for this version — just ensure the badge is lit.
        markNotified(latest)
      }
      // Stash for the dismiss-on-open logic below.
      pendingUpdate.value = {
        latestVersion: latest,
        currentVersion: typeof current === 'string' ? current : '',
        releaseUrl: typeof releaseUrl === 'string' ? releaseUrl : '',
      }
    },
  )

  // Dev-only: expose toast API on window for manual visual testing without
  // triggering the full upgrade-detection pipeline. No-op in production builds.
  if (import.meta.env.DEV) {
    const { info, success, warning, error } = useToast()
    Object.assign(window, {
      __tailr: {
        ...(window as any).__tailr,
        toast: { info, success, warning, error },
        showUpdateToast: (v: string) =>
          info(`新版本 v${v} 可用`, {
            title: '更新提醒',
            action: { label: '查看', onClick: () => { showSettings.value = true } },
            duration: 8000,
            closeButton: true,
          }),
      },
    })
  }
})

function openSettings(): void {
  showSettings.value = true
  // Dismiss the badge once the user opens settings (they've seen the update notice).
  if (pendingUpdate.value) {
    dismiss(pendingUpdate.value.latestVersion)
  }
}

function handleSettingsUpdate(s: Settings): void {
  if (s.autoScroll !== settings.autoScroll) {
    handleAutoScrollChange(s.autoScroll)
  }
  maxLines.value = s.maxVisibleLines
  if (s.darkTheme !== settings.darkTheme) {
    if (s.darkTheme) {
      delete document.documentElement.dataset.theme
      document.documentElement.classList.add('dark')
    } else {
      document.documentElement.dataset.theme = 'light'
      document.documentElement.classList.remove('dark')
    }
    applyThemeColors(s.darkTheme)
  }
  Object.assign(settings, s)
  saveSettings(settings)
}
</script>

<template>
  <div class="app-shell" :class="{ 'sidebar-collapsed': sidebarCollapsed, 'no-tabs': tabs.length === 0 }" :style="{ '--sidebar-current-width': sidebarWidth + 'px' }">
    <!-- Sidebar -->
    <aside class="sidebar">
      <FileBrowser
        ref="fileBrowserRef"
        v-show="!sidebarCollapsed"
        :selected-file="activeTabPath"
        :width="sidebarWidth"
        :refresh-key="refreshKey"
        @select="handleSelectFile"
        @resize="sidebarWidth = $event"
      />
      <BookmarkPanel
        v-show="!sidebarCollapsed"
        :file-path="activeTabPath"
        :level-colors="levelDotColors"
        :valid-range="activeLineRange"
        @scroll-to="handleBookmarkScroll"
      />
    </aside>

    <!-- Global bar (app-level: sidebar toggle + tabs + settings) -->
    <header class="globalbar">
      <button class="icon-btn" @click="sidebarCollapsed = !sidebarCollapsed" :title="sidebarCollapsed ? t('app.openFiles') : t('fileBrowser.collapse')">
        <PanelLeft :size="14" :stroke-width="2" />
      </button>
      <TabBar class="globalbar-tabs" />
      <button
        class="icon-btn share-btn"
        :class="{ copied: linkCopied }"
        :disabled="!activeTabPath"
        @click="handleShareLink"
        :title="linkCopied ? t('app.copied') : t('app.shareLink')"
      >
        <Check v-if="linkCopied" :size="16" :stroke-width="2.5" />
        <Share2 v-else :size="16" :stroke-width="2" />
      </button>
      <button class="settings-btn" @click="openSettings" :title="t('app.openSettings')">
        <SettingsIcon :size="16" :stroke-width="2" />
        <span v-if="hasUpdateBadge" class="settings-badge"></span>
      </button>
    </header>

    <!-- File toolbar (per-file: keyword filter + levels) -->
    <div class="filterbar" v-show="tabs.length > 0">
      <FilterBar
        ref="filterBarRef"
        :current-file="activeTabPath"
        :keywords="activeTab?.filterKeywords ?? []"
        @add-keyword="addKeyword"
        @remove-keyword="removeKeyword"
        @edit-keyword="editKeyword"
        @clear-all="clearAllKeywords"
      />
      <div class="filter-sep"></div>
      <div
        v-for="lv in allLevels"
        :key="lv"
        class="level-tag dynamic-level"
        :class="{ off: (activeTab?.selectedLevels.length ?? 0) > 0 && !(activeTab?.selectedLevels.includes(lv) ?? false) }"
        :style="{
          color: levelDotColors[lv],
          background: levelDotColors[lv] + '18',
          borderColor: levelDotColors[lv] + '40',
        }"
        @click="toggleLevel(lv)"
      >
        <span class="dot" :style="{ background: levelDotColors[lv] }"></span>
        {{ lv }}
      </div>
    </div>

    <!-- Log body: one LogPanel per tab, kept alive with v-show so switching
         tabs preserves scroll position, measuredHeights, expandedLines, etc. -->
    <main class="log-body" :style="{ fontSize: settings.fontSize + 'px', fontFamily: settings.fontFamily === 'monospace' ? 'monospace' : `'${settings.fontFamily}'` }">
      <div v-if="tabs.length === 0" class="empty-state">
        <div class="empty-text">{{ t('app.selectFile') }}</div>
      </div>
      <LogPanel
        v-for="tab in tabs"
        :key="tab.path"
        v-show="tab.path === activeTabPath"
        :ref="(el) => setPanelRef(tab.path, el)"
        :tab="tab"
        :all-levels="allLevels"
        :line-height="26"
        :max-visible-lines="settings.maxVisibleLines"
        :level-colors="levelDotColors"
        :display-mode="settings.displayMode"
        @stick-to-bottom="handleStickToBottom"
      />
    </main>

    <!-- Settings dialog -->
    <SettingsDialog
      v-if="showSettings"
      :settings="settings"
      @update="handleSettingsUpdate"
      @log-levels-saved="reloadActiveTab"
      @close="showSettings = false"
    />

    <!-- Token dialog -->
    <TokenDialog v-if="showTokenDialog" />

    <!-- Status bar -->
    <div class="statusbar">
      <div class="status-chip">
        <div class="status-dot"></div>
        <span>{{ activeTabPath ? activeTabPath.split('/').pop() : t('app.noFile') }}</span>
      </div>
      <template v-if="activeTab">
        <span>{{ activeTab.entries.length }} / {{ maxLines }} {{ t('app.lines') }}</span>
        <span v-if="filteredEntries.length < activeTab.entries.length">{{ filteredEntries.length }} {{ t('app.shown') }}</span>
        <span v-if="activeTab.filterKeywords.length" class="status-filter-info">{{ matchCount }} {{ t('app.matches') }} · {{ activeTab.filterKeywords.join(' + ') }}</span>
      </template>
      <div class="status-spacer"></div>
      <button class="status-toggle" :class="{ active: activeTab?.isTailMode ?? false }" @click="toggleFollowTail" :title="(activeTab?.isTailMode ?? false) ? t('app.pauseTail') : t('app.startTail')">
        <Pause v-if="activeTab?.isTailMode ?? false" :size="10" fill="currentColor" />
        <Play v-else :size="10" fill="currentColor" />
        <span>{{ t('app.follow') }}</span>
      </button>
    </div>
    <SelectionToolbar @follow="addKeyword" />
    <ToastContainer />
  </div>
</template>
