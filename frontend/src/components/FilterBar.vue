<script setup lang="ts">
import { ref } from 'vue'

const props = defineProps<{
  currentFile: string | null
  keywords: string[]
  colors?: string[]
}>()

const emit = defineEmits<{
  addKeyword: [keyword: string]
  removeKeyword: [index: number]
  clearAll: []
}>()

const input = ref('')
const inputRef = ref<HTMLInputElement | null>(null)

function onKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter') {
    e.preventDefault()
    const kw = input.value.trim()
    if (kw && !props.keywords.includes(kw)) {
      emit('addKeyword', kw)
      input.value = ''
    }
  } else if (e.key === 'Escape') {
    if (input.value) {
      input.value = ''
    } else {
      emit('clearAll')
    }
  }
}

function doClearAll(): void {
  input.value = ''
  emit('clearAll')
}

function focus(): void {
  inputRef.value?.focus()
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
          <span class="chip-text">{{ kw }}</span>
          <button class="chip-remove" @click="emit('removeKeyword', i)">✕</button>
        </span>
        <input
          ref="inputRef"
          v-model="input"
          type="text"
          class="filter-input"
          :placeholder="keywords.length ? 'Add keyword…' : 'Filter logs (Enter to add)…'"
          @keydown="onKeydown"
          :disabled="!currentFile"
        />
      </div>
      <button v-if="keywords.length || input" class="filter-clear" @click="doClearAll" title="Clear all">
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
}

.filter-content {
  flex: 1;
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
  font-family: var(--font-mono);
  color: var(--chip-text);
  white-space: nowrap;
  flex-shrink: 0;
  transition: background .15s;
}

.chip-text {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
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
</style>
