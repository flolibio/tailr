<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import LogViewer from './LogViewer.vue'
import type { TabState } from '../composables/useTabs'
import type { LogEntry } from '../services/api'

const { t } = useI18n()

const props = defineProps<{
  tab: TabState
  allLevels: string[]
  lineHeight?: number
  maxVisibleLines?: number
  levelColors?: Record<string, string>
  displayMode?: 'compact' | 'cozy'
}>()

const emit = defineEmits<{
  stickToBottom: []
}>()

// Per-tab filtering — computed is scoped to this panel instance, so filtering
// one tab never re-renders another tab's LogViewer (state isolation).
const filteredEntries = computed(() => {
  let result: LogEntry[] = props.tab.entries

  const levels = props.tab.selectedLevels
  if (levels.length > 0 && levels.length < props.allLevels.length) {
    const levelSet = new Set(levels)
    result = result.filter((e) => levelSet.has(e.level))
  }

  const kws = props.tab.filterKeywords
  if (kws.length > 0) {
    const lowerKws = kws.map((k) => k.toLowerCase())
    result = result.filter((e) => {
      const lower = e.raw.toLowerCase()
      return lowerKws.every((kw) => lower.includes(kw))
    })
  }

  return result
})

// Expose the inner LogViewer instance so the parent can call
// scrollToLine (bookmarks) / scrollToBottom (tail) on the active panel.
let instanceRef: InstanceType<typeof LogViewer> | null = null
function setInstance(el: any): void {
  instanceRef = el as InstanceType<typeof LogViewer> | null
}

defineExpose({
  scrollToLine: (lineNum: number) => instanceRef?.scrollToLine(lineNum),
  scrollToBottom: () => instanceRef?.scrollToBottom(),
})
</script>

<template>
  <!-- Single stable root so v-show from parent reliably toggles this element
       regardless of which inner state (loading / empty / viewer) is active. -->
  <div class="log-panel">
    <!-- empty: no file selected (shouldn't happen in multi-instance, but guard anyway) -->
    <div v-if="!tab.path" class="empty-state">
      <div class="empty-text">{{ t('app.selectFile') }}</div>
    </div>
    <!-- loading: initial load or lazy tab not yet activated -->
    <div v-else-if="tab.isLoading || tab.isLazy" class="empty-state">
      <div class="loading-spinner"></div>
      <div class="empty-text">{{ t('app.loading') }}</div>
    </div>
    <!-- no data: loaded but entries empty (or all filtered out) -->
    <div v-else-if="filteredEntries.length === 0" class="empty-state">
      <div class="empty-text">{{ tab.filterKeywords.length ? t('app.noMatchingLogs') : t('app.waitingForData') }}</div>
    </div>
    <!-- log viewer -->
    <LogViewer
      v-else
      :ref="setInstance"
      :entries="filteredEntries"
      :file-path="tab.path"
      :line-height="lineHeight ?? 26"
      :is-tail-mode="tab.isTailMode"
      :max-visible-lines="maxVisibleLines"
      :highlight-keywords="tab.filterKeywords"
      :level-colors="levelColors"
      :display-mode="displayMode ?? 'compact'"
      @stick-to-bottom="emit('stickToBottom')"
    />
  </div>
</template>

<style scoped>
.log-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
</style>
