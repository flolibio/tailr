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
const settingsCollapsed = ref(false)
const selectedLevels = ref<string[]>([])

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

function selectFile(path: string): void {
  searchState.clear()
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

function toggleSettings(): void {
  settingsCollapsed.value = !settingsCollapsed.value
}

onMounted(() => {
  // wsClient.connect() is already called in useLogStream
  // Default is light theme
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
    } else {
      document.documentElement.dataset.theme = 'light'
    }
  }
  Object.assign(settings, s)
}
</script>

<template>
  <div class="app-layout">
    <aside class="sidebar-left">
      <FileBrowser :selected-file="currentFile" @select="selectFile" />
    </aside>

    <main class="main-area">
        <SearchPanel
          ref="searchPanelRef"
          :current-file="currentFile"
          :is-searching="searchState.isSearching.value"
          @search="handleSearch"
          @clear="searchState.clear"
          @jump-to-line="handleJumpToLine"
          @filter-levels="(l) => selectedLevels = l"
        />
      <div class="log-container" :style="{ fontSize: settings.fontSize + 'px' }">
        <div v-if="!currentFile" class="empty-state">
          <div class="empty-icon">📋</div>
          <div class="empty-text">Select a file from the sidebar to start viewing logs</div>
        </div>
        <div v-else-if="filteredEntries.length === 0" class="empty-state">
          <div class="empty-icon">⏳</div>
          <div class="empty-text">Waiting for log data...</div>
        </div>
        <LogViewer
          v-else
          ref="logViewerRef"
          :entries="filteredEntries"
          :line-height="Math.max(16, settings.fontSize + 6)"
          :is-tail-mode="isTailMode"
          :line-wrap="settings.lineWrap"
          :max-visible-lines="settings.maxVisibleLines"
          @scroll-up="handleScrollUp"
        />
      </div>
    </main>

    <aside class="sidebar-right" :class="{ collapsed: settingsCollapsed }">
      <SettingsPanel
        :settings="settings"
        :collapsed="settingsCollapsed"
        @update="handleSettingsUpdate"
        @toggle-collapse="toggleSettings"
      />
    </aside>
  </div>
</template>

<style scoped>
.log-container {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-muted);
}

.empty-icon {
  font-size: 48px;
  opacity: 0.5;
}

.empty-text {
  font-size: 14px;
}
</style>
