<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import type { LogEntry } from '../services/api'

const props = defineProps<{
  entries: LogEntry[]
  lineHeight?: number
  isTailMode?: boolean
  lineWrap?: boolean
  maxVisibleLines?: number
  highlightKeywords?: string[]
}>()

const emit = defineEmits<{
  scrollUp: []
  jumpToLine: [lineNum: number]
}>()

const lineHeight = computed(() => props.lineHeight ?? 20)
const containerRef = ref<HTMLDivElement | null>(null)
const scrollTop = ref(0)
const containerHeight = ref(600)
const showNewLogsButton = ref(false)
const copiedLine = ref<number | null>(null)
const userScrolledUp = ref(false)
const highlightedLine = ref<number | null>(null)
let highlightTimer: ReturnType<typeof setTimeout> | null = null

const visibleRange = computed(() => {
  const start = Math.floor(scrollTop.value / lineHeight.value)
  const visibleCount = Math.ceil(containerHeight.value / lineHeight.value) + 2
  const startIndex = Math.max(0, start - 5)
  const endIndex = Math.min(props.entries.length, start + visibleCount + 5)
  return { startIndex, endIndex }
})

const visibleEntries = computed(() => {
  return props.entries.slice(visibleRange.value.startIndex, visibleRange.value.endIndex)
})

const totalHeight = computed(() => props.entries.length * lineHeight.value)

const offsetY = computed(() => visibleRange.value.startIndex * lineHeight.value)

function getLevelClass(level: string): string {
  const l = level.toLowerCase()
  if (l === 'error' || l === 'err') return 'lv-error'
  if (l === 'warn' || l === 'warning') return 'lv-warn'
  if (l === 'info') return 'lv-info'
  if (l === 'debug') return 'lv-debug'
  if (l === 'trace') return 'lv-trace'
  return 'lv-unknown'
}

function getBadgeClass(level: string): string {
  const l = level.toLowerCase()
  if (l === 'error' || l === 'err') return 'badge-error'
  if (l === 'warn' || l === 'warning') return 'badge-warn'
  if (l === 'info') return 'badge-info'
  if (l === 'debug') return 'badge-debug'
  if (l === 'trace') return 'badge-trace'
  return 'badge-unknown'
}

function formatTimestamp(ts: string | null | undefined): string {
  if (!ts) return ''
  try {
    const d = new Date(ts)
    const hh = String(d.getHours()).padStart(2, '0')
    const mm = String(d.getMinutes()).padStart(2, '0')
    const ss = String(d.getSeconds()).padStart(2, '0')
    const ms = String(d.getMilliseconds()).padStart(3, '0')
    return `${hh}:${mm}:${ss}.${ms}`
  } catch {
    return ts
  }
}

function onScroll(): void {
  if (!containerRef.value) return
  scrollTop.value = containerRef.value.scrollTop
  const distFromBottom =
    containerRef.value.scrollHeight - containerRef.value.scrollTop - containerRef.value.clientHeight
  if (distFromBottom > 100) {
    showNewLogsButton.value = true
    userScrolledUp.value = true
    emit('scrollUp')
  } else {
    showNewLogsButton.value = false
    userScrolledUp.value = false
  }
}

function scrollToBottom(): void {
  if (!containerRef.value) return
  containerRef.value.scrollTop = containerRef.value.scrollHeight
  showNewLogsButton.value = false
  userScrolledUp.value = false
}

function scrollToLine(lineNum: number): void {
  userScrolledUp.value = true
  const index = props.entries.findIndex((e) => e.lineNum === lineNum)
  if (index >= 0 && containerRef.value) {
    containerRef.value.scrollTop = index * lineHeight.value
    if (highlightTimer) clearTimeout(highlightTimer)
    highlightedLine.value = lineNum
    highlightTimer = setTimeout(() => {
      highlightedLine.value = null
    }, 3000)
  }
}

function copyLine(entry: LogEntry): void {
  navigator.clipboard.writeText(entry.raw).catch(() => {})
  copiedLine.value = entry.lineNum
  setTimeout(() => {
    if (copiedLine.value === entry.lineNum) copiedLine.value = null
  }, 1500)
}

function isJson(str: string): boolean {
  const trimmed = str.trim()
  return (trimmed.startsWith('{') && trimmed.endsWith('}')) || (trimmed.startsWith('[') && trimmed.endsWith(']'))
}

function formatJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2)
  } catch {
    return raw
  }
}

const expandedLines = ref<Set<number>>(new Set())

const HIGHLIGHT_COLORS = [
  'rgba(255, 220, 0, 0.4)',
  'rgba(0, 200, 255, 0.3)',
  'rgba(255, 100, 255, 0.3)',
  'rgba(100, 255, 100, 0.3)',
  'rgba(255, 150, 0, 0.3)',
]

function highlightText(text: string): string {
  const keywords = props.highlightKeywords
  if (!keywords || keywords.length === 0) return escapeHtml(text)

  const escaped = keywords.map((k) => k.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
  if (escaped.length === 0) return escapeHtml(text)

  const regex = new RegExp(`(${escaped.join('|')})`, 'gi')
  const parts = text.split(regex)

  return parts
    .map((part) => {
      const idx = keywords.findIndex((k) => k.toLowerCase() === part.toLowerCase())
      if (idx >= 0) {
        const color = HIGHLIGHT_COLORS[idx % HIGHLIGHT_COLORS.length]
        return `<mark style="background:${color};color:inherit;padding:0 1px;border-radius:2px">${escapeHtml(part)}</mark>`
      }
      return escapeHtml(part)
    })
    .join('')
}

function escapeHtml(str: string): string {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')
}

function toggleExpand(lineNum: number): void {
  if (expandedLines.value.has(lineNum)) {
    expandedLines.value.delete(lineNum)
  } else {
    expandedLines.value.add(lineNum)
  }
}

watch(
  () => props.entries.length,
  () => {
    if (props.isTailMode && !userScrolledUp.value) {
      nextTick(scrollToBottom)
    }
  },
)

watch(
  () => props.isTailMode,
  (val) => {
    if (val) {
      userScrolledUp.value = false
      nextTick(scrollToBottom)
    }
  },
)

let resizeObserver: ResizeObserver | null = null

onMounted(() => {
  if (containerRef.value) {
    containerHeight.value = containerRef.value.clientHeight
    resizeObserver = new ResizeObserver((entries) => {
      for (const entry of entries) {
        containerHeight.value = entry.contentRect.height
      }
    })
    resizeObserver.observe(containerRef.value)
  }
})

onUnmounted(() => {
  resizeObserver?.disconnect()
})

defineExpose({ scrollToBottom, scrollToLine })
</script>

<template>
  <div class="log-viewer-container">
    <div
      ref="containerRef"
      class="log-viewer"
      :style="{ '--line-height': lineHeight + 'px' }"
      @scroll="onScroll"
    >
      <div class="scroll-spacer" :style="{ height: totalHeight + 'px' }">
        <div :style="{ transform: `translateY(${offsetY}px)` }">
          <div
            v-for="(entry, idx) in visibleEntries"
            :key="entry.lineNum"
            class="log-row"
            :class="[
              getLevelClass(entry.level),
              { 'odd': (visibleRange.startIndex + idx) % 2 === 1, 'is-copied': copiedLine === entry.lineNum, 'wrap': lineWrap, 'expanded': expandedLines.has(entry.lineNum), 'is-highlighted': highlightedLine === entry.lineNum }
            ]"
            @click="copyLine(entry)"
          >
            <span class="col-ln">{{ entry.lineNum }}</span>
            <span class="col-ts">{{ formatTimestamp(entry.timestamp) }}</span>
            <span class="col-badge"><span class="badge" :class="getBadgeClass(entry.level)">{{ entry.level.toUpperCase() }}</span></span>
            <span class="col-msg">
              <template v-if="isJson(entry.raw)">
                <button class="json-toggle" @click.stop="toggleExpand(entry.lineNum)">
                  {{ expandedLines.has(entry.lineNum) ? '▾' : '▸' }}
                </button>
                <span v-if="!expandedLines.has(entry.lineNum)" class="json-preview" v-html="
                  highlightText(entry.raw.length > 200 ? entry.raw.slice(0, 200) + '…' : entry.raw)
                "></span>
                <pre v-else class="json-expanded" v-html="highlightText(formatJson(entry.raw))"></pre>
              </template>
              <template v-else>
                <span v-html="highlightText(entry.raw)"></span>
              </template>
            </span>
            <span v-if="copiedLine === entry.lineNum" class="copied-toast">Copied!</span>
          </div>
        </div>
      </div>
    </div>
    <button
      v-if="showNewLogsButton"
      class="new-logs-button"
      @click="scrollToBottom"
    >
      ↓ New logs
    </button>
  </div>
</template>

<style scoped>
.log-viewer-container {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.log-viewer {
  width: 100%;
  height: 100%;
  overflow-y: auto;
  overflow-x: auto;
  font-family: var(--font-mono);
  font-size: 12.5px;
  background: var(--bg);
}

.scroll-spacer {
  position: relative;
  width: 100%;
}

/* ── Log Row ── */
.log-row {
  display: flex;
  align-items: flex-start;
  height: var(--line-height);
  line-height: var(--line-height);
  padding: 0;
  cursor: pointer;
  white-space: nowrap;
  position: relative;
  border-bottom: 1px solid var(--border-2);
  transition: background .08s;
  background: var(--bg);
}

.log-row:last-child {
  border-bottom: none;
}

.log-row.odd {
  background: var(--bg-2);
}

.log-row:hover {
  background: var(--bg-3);
}

.log-row.wrap {
  white-space: pre-wrap;
  word-break: break-all;
  height: auto;
  min-height: var(--line-height);
}

.log-row.expanded {
  height: auto;
  align-items: flex-start;
  position: relative;
  z-index: 5;
  background: var(--bg);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
}

.log-row.is-copied {
  background: rgba(24, 95, 165, 0.1);
}

.log-row.is-highlighted {
  background: rgba(255, 220, 0, 0.2);
  box-shadow: inset 3px 0 0 #e6a800;
  transition: background 0.5s ease;
}

/* Level row backgrounds */
.log-row.lv-error {
  background: var(--c-error-bg);
}
.log-row.lv-error:hover {
  filter: brightness(0.95);
}
.log-row.lv-warn {
  background: var(--c-warn-bg);
}
.log-row.lv-warn:hover {
  filter: brightness(0.95);
}

/* ── Columns ── */
.col-ln {
  width: 36px;
  min-width: 36px;
  padding-left: 12px;
  padding-right: 8px;
  color: var(--log-ln);
  text-align: right;
  user-select: none;
  font-size: 11px;
}

.col-ts {
  width: 84px;
  min-width: 84px;
  padding-right: 10px;
  color: var(--log-ts);
  font-size: 11px;
  white-space: nowrap;
  flex-shrink: 0;
}

.col-badge {
  width: 48px;
  min-width: 48px;
  padding-right: 10px;
  flex-shrink: 0;
}

.badge {
  display: inline-block;
  padding: 0;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  font-family: var(--font-sans);
  background: transparent;
}

.badge-error { color: var(--c-error-text); }
.badge-warn  { color: var(--c-warn-text); }
.badge-info  { color: var(--c-info-text); }
.badge-debug { color: var(--c-debug-text); }
.badge-trace { color: var(--c-trace-text); }
.badge-unknown { color: var(--text-3); }

.col-msg {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.65;
  color: var(--log-msg);
}

.log-row.lv-error .col-msg {
  color: var(--c-error-text);
}

.log-row.lv-warn .col-msg {
  color: var(--c-warn-text);
}

/* ── JSON ── */
.json-toggle {
  background: none;
  border: none;
  color: var(--text-3);
  cursor: pointer;
  padding: 0 4px;
  font-size: 11px;
  line-height: var(--line-height);
  height: auto;
}

.json-toggle:hover {
  color: var(--text);
  background: none;
}

.json-preview {
  color: var(--text-2);
}

.json-expanded {
  display: block;
  white-space: pre-wrap;
  background: var(--bg-2);
  padding: 8px;
  margin: 2px 0;
  border-radius: 5px;
  max-height: 200px;
  overflow-y: auto;
  font-size: 12px;
  line-height: 1.4;
}

/* ── Toast ── */
.copied-toast {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%);
  background: var(--accent);
  color: var(--accent-light);
  padding: 2px 8px;
  border-radius: 5px;
  font-size: 11px;
  pointer-events: none;
}

/* ── New Logs Button ── */
.new-logs-button {
  position: absolute;
  bottom: 16px;
  left: 50%;
  transform: translateX(-50%);
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-light);
  padding: 6px 16px;
  border-radius: var(--radius);
  font-size: 13px;
  z-index: 10;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.new-logs-button:hover {
  opacity: 0.88;
}
</style>
