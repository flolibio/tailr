<script setup lang="ts">
import { ref, reactive, computed, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import FileBrowser from './components/FileBrowser.vue'
import LogViewer from './components/LogViewer.vue'
import FilterBar from './components/FilterBar.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import SelectionToolbar from './components/SelectionToolbar.vue'
import type { Settings } from './components/SettingsPanel.vue'
import { useLogStream } from './composables/useLogStream'
import { useLogLevels } from './composables/useLogLevels'

const { t } = useI18n()

const {
  currentFile,
  entries,
  isTailMode,
  maxLines,
  totalLines,
  isLoading,
  wsClient,
  loadInitial,
  setTailMode,
} = useLogStream()

const logViewerRef = ref<InstanceType<typeof LogViewer> | null>(null)
const filterBarRef = ref<InstanceType<typeof FilterBar> | null>(null)
const selectedLevels = ref<string[]>([])
const filterKeywords = ref<string[]>([])
const showSettings = ref(false)
const sidebarCollapsed = ref(false)
const sidebarWidth = ref(220)

watch(currentFile, (f) => {
  document.title = f ? `Tailr - ${f}` : 'Tailr'
}, { immediate: true })

const highlightKeywords = computed(() => filterKeywords.value)

const highlightColors = [
  'rgba(255, 220, 0, 0.4)',
  'rgba(0, 200, 255, 0.3)',
  'rgba(255, 100, 255, 0.3)',
  'rgba(100, 255, 100, 0.3)',
  'rgba(255, 150, 0, 0.3)',
]

const filteredEntries = computed(() => {
  let result = entries.value

  if (selectedLevels.value.length > 0) {
    const levels = new Set(selectedLevels.value)
    result = result.filter((e) => levels.has(e.level))
  }

  if (filterKeywords.value.length > 0) {
    const kws = filterKeywords.value.map((k) => k.toLowerCase())
    result = result.filter((e) => {
      const lower = e.raw.toLowerCase()
      return kws.every((kw) => lower.includes(kw))
    })
  }

  return result
})

const matchCount = computed(() => {
  if (filterKeywords.value.length === 0) return 0
  return filteredEntries.value.length
})

const SETTINGS_KEY = 'tailr-settings'

const defaultSettings: Settings = {
  fontSize: 14,
  fontFamily: 'JetBrains Mono',
  autoScroll: true,
  maxVisibleLines: 50000,
  darkTheme: true,
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

// ── 动态日志级别 ──────────────────────────────────────────
const {
  config: logLevelConfig,
  levelNames,
  applyThemeColors,
  loadFromBackend: loadLogLevels,
} = useLogLevels()

// Dynamic level color mapping
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
  const idx = selectedLevels.value.indexOf(lv)
  if (idx >= 0) {
    selectedLevels.value.splice(idx, 1)
  } else {
    selectedLevels.value.push(lv)
  }
}

function selectFile(path: string): void {
  selectedLevels.value = []
  filterKeywords.value = []
  loadInitial(path)
}

function addKeyword(kw: string): void {
  if (!filterKeywords.value.includes(kw)) {
    filterKeywords.value = [...filterKeywords.value, kw]
  }
}

function removeKeyword(index: number): void {
  filterKeywords.value = filterKeywords.value.filter((_, i) => i !== index)
}

function clearAllKeywords(): void {
  filterKeywords.value = []
}

function handleStickToBottom(): void {
  setTailMode(true)
  settings.autoScroll = true
  logViewerRef.value?.scrollToBottom()
}

function toggleFollowTail(): void {
  if (isTailMode.value) {
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

  // 初始化动态日志级别颜色
  applyThemeColors(settings.darkTheme)
  loadLogLevels()

  document.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
      e.preventDefault()
      filterBarRef.value?.focus()
    }
  })
})

onUnmounted(() => {
  wsClient.destroy()
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
  <div class="app-shell" :class="{ 'sidebar-collapsed': sidebarCollapsed }" :style="{ '--sidebar-current-width': sidebarWidth + 'px' }">
    <!-- Sidebar -->
    <aside class="sidebar">
      <FileBrowser
        v-show="!sidebarCollapsed"
        :selected-file="currentFile"
        :width="sidebarWidth"
        @select="selectFile"
        @collapse="sidebarCollapsed = true"
        @resize="sidebarWidth = $event"
      />
      <button v-if="sidebarCollapsed" class="sidebar-reopen" @click="sidebarCollapsed = false" :title="t('app.openFiles')">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="9 18 15 12 9 6"/>
        </svg>
      </button>
    </aside>

    <!-- Top bar (filter) -->
    <header class="topbar">
      <FilterBar
        ref="filterBarRef"
        :current-file="currentFile"
        :keywords="filterKeywords"
        :colors="highlightColors"
        @add-keyword="addKeyword"
        @remove-keyword="removeKeyword"
        @clear-all="clearAllKeywords"
      />
      <button class="settings-btn" @click="showSettings = true" :title="t('app.openSettings')">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
      </button>
    </header>

    <!-- Filter bar (levels) -->
    <div class="filterbar">
      <div
        v-for="lv in allLevels"
        :key="lv"
        class="level-tag dynamic-level"
        :class="{ off: selectedLevels.length > 0 && !selectedLevels.includes(lv) }"
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
    <main class="log-body" :style="{ fontSize: settings.fontSize + 'px', fontFamily: `'${settings.fontFamily}', var(--font-mono)` }">
      <div v-if="!currentFile" class="empty-state">
        <div class="empty-text">{{ t('app.selectFile') }}</div>
      </div>
      <div v-else-if="isLoading" class="empty-state">
        <div class="loading-spinner"></div>
        <div class="empty-text">{{ t('app.loading') }}</div>
      </div>
      <div v-else-if="filteredEntries.length === 0" class="empty-state">
        <div class="empty-text">{{ filterKeywords.length ? t('app.noMatchingLogs') : t('app.waitingForData') }}</div>
      </div>
      <LogViewer
        v-else
        ref="logViewerRef"
        :entries="filteredEntries"
        :line-height="26"
        :font-family="settings.fontFamily"
        :is-tail-mode="isTailMode"
        :max-visible-lines="settings.maxVisibleLines"
        :highlight-keywords="highlightKeywords"
        :level-colors="levelDotColors"
        @stick-to-bottom="handleStickToBottom"
      />
    </main>

    <!-- Settings dialog -->
    <SettingsDialog
      v-if="showSettings"
      :settings="settings"
      @update="handleSettingsUpdate"
      @close="showSettings = false"
    />

    <!-- Status bar -->
    <div class="statusbar">
      <div class="status-chip">
        <div class="status-dot"></div>
        <span>{{ currentFile ? currentFile.split('/').pop() : t('app.noFile') }}</span>
      </div>
      <span v-if="entries.length === totalLines">{{ entries.length }} {{ t('app.lines') }}</span>
      <span v-else>{{ entries.length }} / {{ totalLines }} {{ t('app.lines') }}</span>
      <span v-if="filteredEntries.length < entries.length">{{ filteredEntries.length }} {{ t('app.shown') }}</span>
      <span v-if="filterKeywords.length" class="status-filter-info">{{ matchCount }} {{ t('app.matches') }} · {{ filterKeywords.join(' + ') }}</span>
      <div class="status-spacer"></div>
      <button class="status-toggle" :class="{ active: isTailMode }" @click="toggleFollowTail" :title="isTailMode ? t('app.pauseTail') : t('app.startTail')">
        <svg v-if="isTailMode" width="10" height="10" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="4" width="4" height="16"/><rect x="14" y="4" width="4" height="16"/></svg>
        <svg v-else width="10" height="10" viewBox="0 0 24 24" fill="currentColor"><polygon points="5,3 19,12 5,21"/></svg>
        <span>{{ t('app.follow') }}</span>
      </button>
    </div>
    <SelectionToolbar @follow="addKeyword" />
  </div>
</template>
