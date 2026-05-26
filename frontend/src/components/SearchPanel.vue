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
const results = ref<SearchResult | null>(null)
const showResults = ref(false)

function doSearch(): void {
  if (!props.currentFile || !query.value.trim()) return
  showResults.value = true
  emit('search', query.value, {
    regex: isRegex.value,
    levels: [],
    context: contextLines.value,
  })
}

function doClear(): void {
  query.value = ''
  results.value = null
  showResults.value = false
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
  <div class="search-inline">
    <div class="search-wrap">
      <span class="search-icon">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
        </svg>
      </span>
      <input
        v-model="query"
        type="text"
        class="search-input"
        placeholder="Search logs…"
        @keydown="onKeydown"
        :disabled="!currentFile"
      />
      <button
        class="regex-badge"
        :class="{ active: isRegex }"
        @click="isRegex = !isRegex"
        title="Use regex"
      >.*</button>
    </div>
    <button class="btn-search" @click="doSearch" :disabled="!currentFile || !query.trim() || isSearching">
      {{ isSearching ? '…' : 'Search' }}
    </button>
    <button class="btn-clear" @click="doClear" :disabled="!query && !results">Clear</button>

    <!-- Results dropdown -->
    <div v-if="showResults && results" class="results-dropdown">
      <div class="results-header">
        <span>{{ results.totalMatches }} matches{{ results.hasMore ? ' (showing first 100)' : '' }}</span>
        <button class="results-close" @click="showResults = false">✕</button>
      </div>
      <div v-if="results.matches.length > 0" class="results-list">
        <div
          v-for="match in results.matches.slice(0, 100)"
          :key="match.lineNumber"
          class="result-group"
        >
          <div
            v-for="(line, i) in match.contextBefore"
            :key="'b' + i"
            class="result-line context"
            @click="emit('jumpToLine', match.lineNumber - match.contextBefore.length + i)"
          >
            <span class="result-ln">{{ match.lineNumber - match.contextBefore.length + i }}</span>
            <span class="result-text">{{ line }}</span>
          </div>
          <div
            class="result-line match"
            @click="emit('jumpToLine', match.lineNumber)"
          >
            <span class="result-ln">{{ match.lineNumber }}</span>
            <span class="result-text">{{ match.content }}</span>
          </div>
          <div
            v-for="(line, i) in match.contextAfter"
            :key="'a' + i"
            class="result-line context"
            @click="emit('jumpToLine', match.lineNumber + 1 + i)"
          >
            <span class="result-ln">{{ match.lineNumber + 1 + i }}</span>
            <span class="result-text">{{ line }}</span>
          </div>
        </div>
        <div v-if="results.matches.length > 100" class="results-truncated">
          Showing first 100 of {{ results.totalMatches }} matches
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.search-inline {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  position: relative;
}

.search-wrap {
  flex: 1;
  position: relative;
  display: flex;
  align-items: center;
}

.search-icon {
  position: absolute;
  left: 10px;
  color: var(--text-3);
  pointer-events: none;
  display: flex;
  align-items: center;
}

.search-input {
  width: 100%;
  height: 36px;
  padding: 0 42px 0 34px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-2);
  font-family: var(--font-mono);
  font-size: 12.5px;
  color: var(--text);
  outline: none;
  transition: border-color .15s, background .15s;
}

.search-input:focus {
  border-color: var(--border-2);
  background: var(--bg);
}

.regex-badge {
  position: absolute;
  right: 8px;
  font-size: 10px;
  font-weight: 700;
  font-family: var(--font-mono);
  color: var(--text-3);
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  border: 1px solid var(--border);
  background: transparent;
  transition: all .12s;
  user-select: none;
  line-height: 1.4;
  height: auto;
}

.regex-badge:hover,
.regex-badge.active {
  border-color: var(--border-2);
  color: var(--text);
  background: var(--bg-2);
}

.btn-search {
  height: 36px;
  padding: 0 16px;
  background: var(--accent);
  color: var(--accent-light);
  border: none;
  border-radius: var(--radius);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: opacity .12s;
  flex-shrink: 0;
}

.btn-search:hover {
  opacity: 0.88;
}

.btn-clear {
  height: 36px;
  padding: 0 14px;
  background: transparent;
  color: var(--text-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  font-size: 13px;
  cursor: pointer;
  transition: background .12s;
  flex-shrink: 0;
}

.btn-clear:hover {
  background: var(--bg-2);
}

/* Results dropdown */
.results-dropdown {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  margin-top: 4px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: 0 4px 16px rgba(0,0,0,0.12);
  z-index: 100;
  max-height: 320px;
  overflow-y: auto;
}

.results-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  font-size: 12px;
  color: var(--text-2);
  border-bottom: 1px solid var(--border);
  position: sticky;
  top: 0;
  background: var(--bg);
  z-index: 1;
}

.results-close {
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  padding: 0;
  border-radius: 4px;
}

.results-close:hover {
  background: var(--bg-2);
  color: var(--text);
}

.result-group {
  border-bottom: 1px solid var(--border);
}

.result-group:last-child {
  border-bottom: none;
}

.result-line {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 12px;
  font-family: var(--font-mono);
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  height: auto;
  border: none;
}

.result-line:hover {
  background: var(--bg-2);
}

.result-line.match {
  background: rgba(24, 95, 165, 0.06);
  font-weight: 600;
}

.result-line.context {
  opacity: 0.5;
}

.result-ln {
  color: var(--text-3);
  min-width: 50px;
  text-align: right;
  flex-shrink: 0;
}

.result-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.results-truncated {
  padding: 6px 12px;
  font-size: 12px;
  color: var(--text-3);
  text-align: center;
  border-top: 1px solid var(--border);
}
</style>
