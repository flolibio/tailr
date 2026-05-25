<script setup lang="ts">
import { ref } from 'vue'
import type { SearchResult } from '../services/api'

const props = defineProps<{
  currentFile: string | null
  isSearching: boolean
}>()

const emit = defineEmits<{
  search: [query: string, options: SearchOptions]
  clear: []
  jumpToLine: [lineNum: number]
}>()

export interface SearchOptions {
  regex: boolean
  levels: string[]
  context: number
}

const query = ref('')
const isRegex = ref(false)
const contextLines = ref(3)
const selectedLevels = ref<string[]>([])
const results = ref<SearchResult | null>(null)

const allLevels = ['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE']

function toggleLevel(level: string): void {
  const idx = selectedLevels.value.indexOf(level)
  if (idx >= 0) {
    selectedLevels.value.splice(idx, 1)
  } else {
    selectedLevels.value.push(level)
  }
}

function doSearch(): void {
  if (!props.currentFile || !query.value.trim()) return
  emit('search', query.value, {
    regex: isRegex.value,
    levels: selectedLevels.value,
    context: contextLines.value,
  })
}

function doClear(): void {
  query.value = ''
  results.value = null
  emit('clear')
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter') doSearch()
}

function setResults(data: SearchResult): void {
  results.value = data
}

defineExpose({ setResults })
</script>

<template>
  <div class="search-panel">
    <div class="search-row">
      <div class="search-input-group">
        <input
          v-model="query"
          type="text"
          class="search-input"
          placeholder="Search logs..."
          @keydown="onKeydown"
          :disabled="!currentFile"
        />
        <label class="regex-toggle" title="Use regex">
          <input v-model="isRegex" type="checkbox" />
          <span>.*</span>
        </label>
      </div>
      <button class="primary" @click="doSearch" :disabled="!currentFile || !query.trim() || isSearching">
        {{ isSearching ? 'Searching...' : 'Search' }}
      </button>
      <button @click="doClear" :disabled="!query && !results">Clear</button>
    </div>
    <div class="search-filters">
      <div class="level-filters">
        <span class="filter-label">Levels:</span>
        <label
          v-for="level in allLevels"
          :key="level"
          class="level-chip"
          :class="['level-' + level.toLowerCase(), { active: selectedLevels.includes(level) }]"
        >
          <input
            type="checkbox"
            :checked="selectedLevels.includes(level)"
            @change="toggleLevel(level)"
          />
          {{ level }}
        </label>
      </div>
      <div class="context-control">
        <span class="filter-label">Context: {{ contextLines }}</span>
        <input v-model.number="contextLines" type="range" min="0" max="10" step="1" />
      </div>
    </div>
    <div v-if="results" class="search-results-info">
      <span>{{ results.totalMatches }} matches{{ results.hasMore ? ' (showing first 100)' : '' }}</span>
    </div>
    <div v-if="results && results.matches.length > 0" class="search-results">
      <div
        v-for="match in results.matches.slice(0, 100)"
        :key="match.lineNumber"
        class="search-result"
        @click="emit('jumpToLine', match.lineNumber)"
      >
        <span class="result-line-num">{{ match.lineNumber }}</span>
        <span class="result-text">{{ match.content }}</span>
      </div>
      <div v-if="results.matches.length > 100" class="results-truncated">
        Showing first 100 of {{ results.totalMatches }} matches
      </div>
    </div>
  </div>
</template>

<style scoped>
.search-panel {
  border-bottom: 1px solid var(--border-color);
  background: var(--bg-secondary);
  padding: 8px 12px;
}

.search-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.search-input-group {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 0;
}

.search-input {
  flex: 1;
  border-radius: 3px 0 0 3px;
  padding: 6px 10px;
  font-size: 13px;
}

.regex-toggle {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 8px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-color);
  border-left: none;
  border-radius: 0 3px 3px 0;
  cursor: pointer;
  font-family: var(--font-mono);
  font-size: 13px;
  color: var(--text-secondary);
  user-select: none;
}

.regex-toggle:has(input:checked) {
  color: var(--level-info);
  background: rgba(55, 148, 255, 0.1);
}

.regex-toggle input {
  display: none;
}

.search-filters {
  display: flex;
  gap: 16px;
  align-items: center;
  margin-top: 8px;
  flex-wrap: wrap;
}

.filter-label {
  font-size: 12px;
  color: var(--text-secondary);
  margin-right: 4px;
}

.level-filters {
  display: flex;
  align-items: center;
  gap: 4px;
}

.level-chip {
  display: flex;
  align-items: center;
  gap: 3px;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  opacity: 0.4;
  transition: opacity 0.15s;
  user-select: none;
}

.level-chip.active {
  opacity: 1;
}

.level-chip input {
  display: none;
}

.level-error { background: rgba(244, 71, 71, 0.15); color: var(--level-error); }
.level-warn { background: rgba(204, 167, 0, 0.15); color: var(--level-warn); }
.level-info { background: rgba(55, 148, 255, 0.15); color: var(--level-info); }
.level-debug { background: rgba(106, 153, 85, 0.15); color: var(--level-debug); }
.level-trace { background: rgba(128, 128, 128, 0.15); color: var(--level-trace); }

.context-control {
  display: flex;
  align-items: center;
  gap: 8px;
}

.context-control input[type="range"] {
  width: 80px;
}

.search-results-info {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}

.search-results {
  margin-top: 6px;
  max-height: 200px;
  overflow-y: auto;
  border: 1px solid var(--border-color);
  border-radius: 3px;
  background: var(--bg-primary);
}

.search-result {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 8px;
  font-family: var(--font-mono);
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
}

.search-result:hover {
  background: var(--bg-hover);
}

.result-line-num {
  color: var(--line-number);
  min-width: 50px;
  text-align: right;
}

.result-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.results-truncated {
  padding: 4px 8px;
  font-size: 12px;
  color: var(--text-muted);
  text-align: center;
  border-top: 1px solid var(--border-color);
}
</style>
