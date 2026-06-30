<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  currentFile: string | null
  keywords: string[]
  colors?: string[]
}>()

const emit = defineEmits<{
  addKeyword: [keyword: string]
  removeKeyword: [index: number]
  editKeyword: [index: number, newValue: string]
  clearAll: []
}>()

const input = ref('')
const inputRef = ref<HTMLInputElement | null>(null)
const showSuggestions = ref(false)
const suggestionsRef = ref<HTMLDivElement | null>(null)

const editingIndex = ref<number | null>(null)
const editingValue = ref('')
const editInputRef = ref<HTMLInputElement | null>(null)

const HISTORY_KEY = 'tailr-search-history'
const MAX_HISTORY = 20

const searchHistory = ref<string[]>([])

function loadHistory(): void {
  try {
    const saved = localStorage.getItem(HISTORY_KEY)
    if (saved) {
      searchHistory.value = JSON.parse(saved)
    }
  } catch { /* ignore */ }
}

function saveHistory(): void {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(searchHistory.value))
  } catch { /* ignore */ }
}

function addToHistory(kw: string): void {
  const normalized = kw.toLowerCase()
  searchHistory.value = searchHistory.value.filter((h) => h.toLowerCase() !== normalized)
  searchHistory.value.unshift(kw)
  if (searchHistory.value.length > MAX_HISTORY) {
    searchHistory.value = searchHistory.value.slice(0, MAX_HISTORY)
  }
  saveHistory()
}

const suggestions = computed(() => {
  if (!input.value.trim()) return []
  const lower = input.value.toLowerCase()
  return searchHistory.value.filter(
    (h) => h.toLowerCase().includes(lower) && !props.keywords.includes(h),
  ).slice(0, 8)
})

function onInput(): void {
  showSuggestions.value = input.value.trim().length > 0 && suggestions.value.length > 0
}

function selectSuggestion(kw: string): void {
  input.value = ''
  showSuggestions.value = false
  emit('addKeyword', kw)
}

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter') {
    e.preventDefault()
    showSuggestions.value = false
    const kw = input.value.trim()
    if (kw && !props.keywords.includes(kw)) {
      emit('addKeyword', kw)
      addToHistory(kw)
      input.value = ''
    }
  } else if (e.key === 'Escape') {
    if (showSuggestions.value) {
      showSuggestions.value = false
    } else if (input.value) {
      input.value = ''
    } else {
      emit('clearAll')
    }
  } else if (
    e.key === 'Backspace' &&
    input.value === '' &&
    props.keywords.length > 0
  ) {
    // Backspace on empty input: revert last chip back into the input field
    // for editing (open gate, no char deleted on this first press).
    e.preventDefault()
    const lastIdx = props.keywords.length - 1
    input.value = props.keywords[lastIdx]
    emit('removeKeyword', lastIdx)
    nextTick(() => {
      const el = inputRef.value
      if (el) {
        const len = input.value.length
        el.setSelectionRange(len, len)
      }
    })
  } else if (e.key === 'Tab' && suggestions.value.length > 0) {
    e.preventDefault()
    selectSuggestion(suggestions.value[0])
  }
}

function doClearAll(): void {
  input.value = ''
  showSuggestions.value = false
  emit('clearAll')
}

function focus(): void {
  inputRef.value?.focus()
}

function onClickOutside(e: MouseEvent): void {
  const target = e.target as Node
  if (
    suggestionsRef.value &&
    !suggestionsRef.value.contains(target) &&
    inputRef.value &&
    !inputRef.value.contains(target)
  ) {
    showSuggestions.value = false
  }
}

onMounted(() => {
  loadHistory()
  document.addEventListener('mousedown', onClickOutside)
})

onUnmounted(() => {
  document.removeEventListener('mousedown', onClickOutside)
})

function startEdit(index: number): void {
  editingIndex.value = index
  editingValue.value = props.keywords[index]
  nextTick(() => {
    const el = editInputRef.value
    if (el) {
      el.focus()
      el.select()
    }
  })
}

function confirmEdit(): void {
  if (editingIndex.value === null) return
  const val = editingValue.value.trim()
  const idx = editingIndex.value
  editingIndex.value = null
  editingValue.value = ''
  if (val && val !== props.keywords[idx]) {
    emit('editKeyword', idx, val)
  }
}

function cancelEdit(): void {
  editingIndex.value = null
  editingValue.value = ''
}

function onEditKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter') {
    e.preventDefault()
    confirmEdit()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    cancelEdit()
  }
}

function onEditBlur(): void {
  confirmEdit()
}

defineExpose({ focus })
</script>

<template>
  <div class="filter-bar">
    <div class="filter-wrap">
      <span class="filter-icon">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
        </svg>
      </span>
      <div class="filter-content">
        <span v-for="(kw, i) in keywords" :key="kw" class="chip" :style="colors ? { background: colors[i % colors.length], color: 'inherit' } : {}">
          <template v-if="editingIndex === i">
            <input
              ref="editInputRef"
              v-model="editingValue"
              type="text"
              class="chip-edit-input"
              @keydown="onEditKeydown"
              @blur="onEditBlur"
              @mousedown.stop
            />
          </template>
          <template v-else>
            <span class="chip-text" @dblclick="startEdit(i)">{{ kw }}</span>
          </template>
          <button class="chip-remove" @click="emit('removeKeyword', i)">✕</button>
        </span>
        <input
          ref="inputRef"
          v-model="input"
          type="text"
          class="filter-input"
          :placeholder="keywords.length ? t('filter.addKeyword') : t('filter.filterLogs')"
          @keydown="onKeydown"
          @input="onInput"
          @focus="onInput"
          :disabled="!currentFile"
        />
      </div>
      <!-- Suggestions dropdown — positioned relative to filter-wrap -->
      <div v-if="showSuggestions" ref="suggestionsRef" class="suggestions-dropdown">
        <div
          v-for="s in suggestions"
          :key="s"
          class="suggestion-item"
          @mousedown.prevent="selectSuggestion(s)"
        >
          <span class="suggestion-icon">↻</span>
          <span class="suggestion-text">{{ s }}</span>
        </div>
      </div>
      <button v-if="keywords.length || input" class="filter-clear" @click="doClearAll" :title="t('filter.clearAll')">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
          <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.filter-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  position: relative;
}

.filter-wrap {
  flex: 1;
  position: relative;
  display: flex;
  align-items: center;
}

.filter-icon {
  position: absolute;
  left: 10px;
  color: var(--text-3);
  pointer-events: none;
  display: flex;
  align-items: center;
  z-index: 1;
}

.filter-content {
  flex: 1;
  position: relative;
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  height: 36px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-2);
  padding: 0 30px 0 34px;
  transition: border-color .15s, background .15s;
  overflow-x: auto;
}

.filter-content:focus-within {
  border-color: var(--border-2);
  background: var(--bg);
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 6px 2px 8px;
  background: var(--chip-bg);
  border: 1px solid var(--border);
  border-radius: 4px;
  font-size: 12px;
  color: var(--chip-text);
  white-space: nowrap;
  flex-shrink: 0;
  transition: background .15s;
}

.chip-text {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: default;
}

.chip-edit-input {
  width: 80px;
  min-width: 40px;
  max-width: 200px;
  height: 18px;
  border: none;
  background: var(--bg);
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
  outline: none;
  padding: 0 2px;
  border-radius: 2px;
}

.chip-remove {
  width: 16px;
  height: 16px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  padding: 0;
  border-radius: 3px;
  flex-shrink: 0;
}

.chip-remove:hover {
  background: var(--bg-3);
  color: var(--text);
}

.filter-input {
  flex: 1;
  min-width: 80px;
  height: 100%;
  border: none;
  background: transparent;
  font-family: var(--font-mono);
  font-size: 12.5px;
  color: var(--text);
  outline: none;
}

.filter-input::placeholder {
  color: var(--text-3);
}

.filter-clear {
  position: absolute;
  right: 6px;
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 4px;
  transition: color .12s, background .12s;
}

.filter-clear:hover {
  background: var(--bg-3);
  color: var(--text);
}

/* ── Suggestions dropdown ── */
.suggestions-dropdown {
  position: absolute;
  top: 100%;
  left: 0;
  right: 30px;
  margin-top: 4px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12);
  z-index: 100;
  max-height: 240px;
  overflow-y: auto;
}

.suggestion-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  cursor: pointer;
  font-size: 12px;
  font-family: var(--font-mono);
  color: var(--text);
  transition: background .1s;
}

.suggestion-item:hover {
  background: var(--bg-3);
}

.suggestion-icon {
  color: var(--text-3);
  font-size: 12px;
  flex-shrink: 0;
}

.suggestion-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
