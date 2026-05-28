<script setup lang="ts">
import { ref, reactive, computed, onMounted, onUnmounted } from 'vue'
import FileBrowser from './components/FileBrowser.vue'
import LogViewer from './components/LogViewer.vue'
import SearchPanel from './components/SearchPanel.vue'
import SettingsPanel from './components/SettingsPanel.vue'
import type { Settings } from './components/SettingsPanel.vue'
import type { SearchOptions } from './components/SearchPanel.vue'
import { useLogStream } from './composables/useLogStream'
import { useSearch } from './composables/useSearch'
import { searchLogs } from './services/api'

const {
  currentFile,
  entries,
  isTailMode,
  maxLines,
  wsClient,
  loadInitial,
} = useLogStream()

const searchState = useSearch()

const logViewerRef = ref<InstanceType<typeof LogViewer> | null>(null)
const searchPanelRef = ref<InstanceType<typeof SearchPanel> | null>(null)
const selectedLevels = ref<string[]>([])
const highlightInput = ref('')
const settingsCollapsed = ref(true)
const sidebarCollapsed = ref(false)

const highlightKeywords = computed(() => {
  const raw = highlightInput.value.trim()
  if (!raw) return []
  return raw.split(',').map((s) => s.trim()).filter(Boolean)
})

const filteredEntries = computed(() => {
  if (selectedLevels.value.length === 0) return entries.value
  const levels = new Set(selectedLevels.value)
  return entries.value.filter((e) => levels.has(e.level))
})

const settings = reactive<Settings>({
  fontSize: 14,
  autoScroll: true,
  lineWrap: false,
  maxVisibleLines: 50000,
  darkTheme: false,
})

const allLevels = ['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE'] as const

const levelDotColors: Record<string, string> = {
  ERROR: '#E24B4A',
  WARN: '#BA7517',
  INFO: '#378ADD',
  DEBUG: '#639922',
  TRACE: '#888780',
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
  searchState.clear()
  selectedLevels.value = []
  highlightInput.value = ''
  loadInitial(path)
}

async function handleSearch(query: string, options: SearchOptions): Promise<void> {
  if (!currentFile.value) return
  try {
    const data = await searchLogs(currentFile.value, query, {
      regex: options.regex,
      levels: options.levels.length > 0 ? options.levels : undefined,
      context: options.context,
    })
    searchPanelRef.value?.setResults(data)
  } catch (e) {
    console.error('Search failed:', e)
  }
}

function handleJumpToLine(lineNum: number): void {
  logViewerRef.value?.scrollToLine(lineNum)
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
  document.documentElement.dataset.theme = 'light'
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
}
</script>

<template>
  <div class="app-shell" :class="{ 'settings-collapsed': settingsCollapsed, 'sidebar-collapsed': sidebarCollapsed }">
    <!-- Sidebar -->
    <aside class="sidebar">
      <FileBrowser
        v-if="!sidebarCollapsed"
        :selected-file="currentFile"
        @select="selectFile"
        @collapse="sidebarCollapsed = true"
      />
      <button v-else class="sidebar-reopen" @click="sidebarCollapsed = false" title="Open files">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="9 18 15 12 9 6"/>
        </svg>
      </button>
    </aside>

    <!-- Top bar (search) -->
    <header class="topbar">
      <SearchPanel
        ref="searchPanelRef"
        :current-file="currentFile"
        :is-searching="searchState.isSearching.value"
        @search="handleSearch"
        @clear="searchState.clear"
        @jump-to-line="handleJumpToLine"
      />
    </header>

    <!-- Filter bar (levels + highlight) -->
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
      <div class="filter-sep"></div>
      <div class="highlight-wrap">
        <span class="highlight-label">Highlight</span>
        <input
          v-model="highlightInput"
          type="text"
          class="highlight-input"
          placeholder="keywords (comma-separated)"
        />
        <button v-if="highlightInput" class="highlight-clear" @click="highlightInput = ''">✕</button>
      </div>
    </div>

    <!-- Log body -->
    <main class="log-body" :style="{ fontSize: settings.fontSize + 'px' }">
      <div v-if="!currentFile" class="empty-state">
        <div class="empty-text">Select a file to start viewing logs</div>
      </div>
      <div v-else-if="filteredEntries.length === 0" class="empty-state">
        <div class="empty-text">Waiting for log data...</div>
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
      <span v-if="filteredEntries.length < entries.length">{{ filteredEntries.length }} shown</span>
    </div>
  </div>
</template>
