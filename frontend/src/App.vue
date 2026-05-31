<script setup lang="ts">
import { ref, reactive, computed, onMounted, onUnmounted } from 'vue'
import FileBrowser from './components/FileBrowser.vue'
import LogViewer from './components/LogViewer.vue'
import FilterBar from './components/FilterBar.vue'
import SettingsPanel from './components/SettingsPanel.vue'
import type { Settings } from './components/SettingsPanel.vue'
import { useLogStream } from './composables/useLogStream'

const {
  currentFile,
  entries,
  isTailMode,
  maxLines,
  wsClient,
  loadInitial,
} = useLogStream()

const logViewerRef = ref<InstanceType<typeof LogViewer> | null>(null)
const filterBarRef = ref<InstanceType<typeof FilterBar> | null>(null)
const selectedLevels = ref<string[]>([])
const filterKeywords = ref<string[]>([])
const settingsCollapsed = ref(true)
const sidebarCollapsed = ref(false)

const highlightKeywords = computed(() => filterKeywords.value)

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

const SETTINGS_KEY = 'logtailer-settings'

const defaultSettings: Settings = {
  fontSize: 14,
  autoScroll: true,
  lineWrap: false,
  maxVisibleLines: 50000,
  darkTheme: false,
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

const allLevels = ['ALERT', 'ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE'] as const

const levelDotColors: Record<string, string> = {
  ALERT: '#FF3B30',
  ERROR: '#FF453A',
  WARN: '#FF9F0A',
  INFO: '#30D158',
  DEBUG: '#64D2FF',
  TRACE: '#BF5AF2',
}

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

function handleScrollUp(): void {
  if (isTailMode.value) {
    isTailMode.value = false
    settings.autoScroll = false
  }
}

function handleAutoScrollChange(enabled: boolean): void {
  settings.autoScroll = enabled
  if (enabled) {
    isTailMode.value = true
    logViewerRef.value?.scrollToBottom()
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
  }
  Object.assign(settings, s)
  saveSettings(settings)
}
</script>

<template>
  <div class="app-shell" :class="{ 'settings-collapsed': settingsCollapsed, 'sidebar-collapsed': sidebarCollapsed }">
    <!-- Sidebar -->
    <aside class="sidebar">
      <FileBrowser
        v-show="!sidebarCollapsed"
        :selected-file="currentFile"
        @select="selectFile"
        @collapse="sidebarCollapsed = true"
      />
      <button v-if="sidebarCollapsed" class="sidebar-reopen" @click="sidebarCollapsed = false" title="Open files">
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
        @add-keyword="addKeyword"
        @remove-keyword="removeKeyword"
        @clear-all="clearAllKeywords"
      />
    </header>

    <!-- Filter bar (levels) -->
    <div class="filterbar">
      <div
        v-for="lv in allLevels"
        :key="lv"
        class="level-tag"
        :class="[lv.toLowerCase(), { off: selectedLevels.length > 0 && !selectedLevels.includes(lv) }]"
        @click="toggleLevel(lv)"
      >
        <span class="dot" :style="{ background: levelDotColors[lv] }"></span>
        {{ lv }}
      </div>
    </div>

    <!-- Log body -->
    <main class="log-body" :style="{ fontSize: settings.fontSize + 'px' }">
      <div v-if="!currentFile" class="empty-state">
        <div class="empty-text">Select a file to start viewing logs</div>
      </div>
      <div v-else-if="filteredEntries.length === 0" class="empty-state">
        <div class="empty-text">{{ filterKeywords.length ? 'No matching logs' : 'Waiting for log data...' }}</div>
      </div>
      <LogViewer
        v-else
        ref="logViewerRef"
        :entries="filteredEntries"
        :line-height="Math.max(18, settings.fontSize + 8)"
        :is-tail-mode="isTailMode"
        :line-wrap="settings.lineWrap"
        :max-visible-lines="settings.maxVisibleLines"
        :highlight-keywords="highlightKeywords"
        @scroll-up="handleScrollUp"
      />
    </main>

    <!-- Settings panel -->
    <aside class="settings-panel">
      <SettingsPanel
        v-if="!settingsCollapsed"
        :settings="settings"
        @update="handleSettingsUpdate"
        @collapse="settingsCollapsed = true"
      />
      <button v-else class="settings-reopen" @click="settingsCollapsed = false" title="Open settings">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
      </button>
    </aside>

    <!-- Status bar -->
    <div class="statusbar">
      <div class="status-chip">
        <div class="status-dot"></div>
        <span>{{ currentFile ? currentFile.split('/').pop() : 'No file' }}</span>
      </div>
      <span>{{ entries.length }} lines</span>
      <span v-if="filterKeywords.length" class="status-mode">🔴 Live · {{ matchCount }} matches · {{ filterKeywords.join(' + ') }}</span>
      <span v-else-if="isTailMode" class="status-mode">🔴 Live</span>
      <span v-if="filteredEntries.length < entries.length">{{ filteredEntries.length }} shown</span>
    </div>
  </div>
</template>
