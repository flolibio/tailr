<script setup lang="ts">
import { ref, reactive, computed, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import FileBrowser from './components/FileBrowser.vue'
import LogViewer from './components/LogViewer.vue'
import FilterBar from './components/FilterBar.vue'
import TabBar from './components/TabBar.vue'
import BookmarkPanel from './components/BookmarkPanel.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import SelectionToolbar from './components/SelectionToolbar.vue'
import TokenDialog from './components/TokenDialog.vue'
import type { Settings } from './components/SettingsPanel.vue'
import { useTabs } from './composables/useTabs'
import { useLogLevels } from './composables/useLogLevels'
import { useAuth } from './composables/useAuth'
import { useRecentFiles } from './composables/useRecentFiles'
import { PanelLeft, Settings as SettingsIcon, Play, Pause } from 'lucide-vue-next'

const { t } = useI18n()

const {
  tabs,
  activeTabPath,
  activeTab,
  maxLines,
  wsClient,
  openTab,
  setTailMode,
  reloadActiveTab,
} = useTabs()

const logViewerRef = ref<InstanceType<typeof LogViewer> | null>(null)
const filterBarRef = ref<InstanceType<typeof FilterBar> | null>(null)
const fileBrowserRef = ref<InstanceType<typeof FileBrowser> | null>(null)
const showSettings = ref(false)
const sidebarCollapsed = ref(false)
const sidebarWidth = ref(300)
const refreshKey = ref(0)

const { token, showTokenDialog } = useAuth()
const { recordOpen } = useRecentFiles()

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
  logViewerRef.value?.scrollToLine(lineNum)
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

const highlightKeywords = computed(() => activeTab.value?.filterKeywords ?? [])

const filteredEntries = computed(() => {
  const tab = activeTab.value
  if (!tab) return []

  let result = tab.entries

  const levels = tab.selectedLevels
  if (levels.length > 0 && levels.length < allLevels.value.length) {
    const levelSet = new Set(levels)
    result = result.filter((e) => levelSet.has(e.level))
  }

  const kws = tab.filterKeywords
  if (kws.length > 0) {
    const lowerKws = kws.map((k) => k.toLowerCase())
    result = result.filter((e) => {
      const lower = e.raw.toLowerCase()
      return lowerKws.every((kw) => lower.includes(kw))
    })
  }

  return result
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
  logViewerRef.value?.scrollToBottom()
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
    logViewerRef.value?.scrollToBottom()
  }
}

function handleAutoScrollChange(enabled: boolean): void {
  settings.autoScroll = enabled
  if (enabled) {
    setTailMode(true)
    logViewerRef.value?.scrollToBottom()
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
})

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
      <button class="settings-btn" @click="showSettings = true" :title="t('app.openSettings')">
        <SettingsIcon :size="16" :stroke-width="2" />
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

    <!-- Log body -->
    <main class="log-body" :style="{ fontSize: settings.fontSize + 'px', fontFamily: settings.fontFamily === 'monospace' ? 'monospace' : `'${settings.fontFamily}'` }">
      <div v-if="!activeTabPath" class="empty-state">
        <div class="empty-text">{{ t('app.selectFile') }}</div>
      </div>
      <div v-else-if="activeTab?.isLoading" class="empty-state">
        <div class="loading-spinner"></div>
        <div class="empty-text">{{ t('app.loading') }}</div>
      </div>
      <div v-else-if="filteredEntries.length === 0" class="empty-state">
        <div class="empty-text">{{ (activeTab?.filterKeywords.length ?? 0) ? t('app.noMatchingLogs') : t('app.waitingForData') }}</div>
      </div>
      <LogViewer
        v-else
        ref="logViewerRef"
        :key="activeTabPath ?? ''"
        :entries="filteredEntries"
        :file-path="activeTabPath ?? undefined"
        :line-height="26"
        :is-tail-mode="activeTab?.isTailMode ?? true"
        :max-visible-lines="settings.maxVisibleLines"
        :highlight-keywords="highlightKeywords"
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
  </div>
</template>
