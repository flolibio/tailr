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
  if (l === 'error' || l === 'err') return 'level-error'
  if (l === 'warn' || l === 'warning') return 'level-warn'
  if (l === 'info') return 'level-info'
  if (l === 'debug') return 'level-debug'
  if (l === 'trace') return 'level-trace'
  return ''
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
  // First, disable tail mode so we can scroll freely
  userScrolledUp.value = true
  const index = props.entries.findIndex((e) => e.lineNum === lineNum)
  if (index >= 0 && containerRef.value) {
    containerRef.value.scrollTop = index * lineHeight.value
    // Highlight the line
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

  // Build a combined regex for all keywords
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
            v-for="entry in visibleEntries"
            :key="entry.lineNum"
            class="log-line"
            :class="[
              getLevelClass(entry.level),
              { 'is-copied': copiedLine === entry.lineNum, 'wrap': lineWrap, 'expanded': expandedLines.has(entry.lineNum), 'is-highlighted': highlightedLine === entry.lineNum }
            ]"
            @click="copyLine(entry)"
          >
            <span class="line-number">{{ entry.lineNum }}</span>
            <span class="line-level">{{ entry.level }}</span>
            <span v-if="entry.timestamp" class="line-timestamp">{{ entry.timestamp }}</span>
            <span class="line-content">
              <template v-if="isJson(entry.raw)">
                <button class="json-toggle" @click.stop="toggleExpand(entry.lineNum)">
                  {{ expandedLines.has(entry.lineNum) ? '▼' : '▶' }}
                </button>
                <span v-if="!expandedLines.has(entry.lineNum)" class="json-preview" v-html="
                  highlightText(entry.raw.length > 200 ? entry.raw.slice(0, 200) + '...' : entry.raw)
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
  font-size: 13px;
  background: var(--bg-primary);
}

.scroll-spacer {
  position: relative;
  width: 100%;
}

.log-line {
  display: flex;
  align-items: flex-start;
  height: var(--line-height);
  line-height: var(--line-height);
  padding: 0 8px;
  cursor: pointer;
  white-space: nowrap;
  position: relative;
}

.log-line.wrap {
  white-space: pre-wrap;
  word-break: break-all;
  height: auto;
  min-height: var(--line-height);
}

.log-line.expanded {
  height: auto;
  align-items: flex-start;
  position: relative;
  z-index: 5;
  background: var(--bg-primary);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
}

.log-line:hover {
  background: var(--bg-hover);
}

.log-line.is-copied {
  background: rgba(38, 79, 120, 0.3);
}

.log-line.is-highlighted {
  background: rgba(255, 220, 0, 0.25);
  box-shadow: inset 3px 0 0 #e6a800;
  transition: background 0.5s ease;
}

.line-number {
  width: 60px;
  min-width: 60px;
  color: var(--line-number);
  text-align: right;
  padding-right: 12px;
  user-select: none;
  font-size: 12px;
}

.line-level {
  width: 50px;
  min-width: 50px;
  font-weight: 600;
  text-transform: uppercase;
  font-size: 11px;
  padding-right: 8px;
}

.line-timestamp {
  color: var(--text-secondary);
  padding-right: 12px;
  font-size: 12px;
  flex-shrink: 0;
}

.line-content {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.json-toggle {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 0 4px;
  font-size: 10px;
  line-height: var(--line-height);
}

.json-toggle:hover {
  color: var(--text-primary);
  background: none;
}

.json-preview {
  color: var(--text-secondary);
}

.json-expanded {
  display: block;
  white-space: pre-wrap;
  background: var(--bg-tertiary);
  padding: 8px;
  margin: 2px 0;
  border-radius: 3px;
  max-height: 200px;
  overflow-y: auto;
  font-size: 12px;
  line-height: 1.4;
}

.copied-toast {
  position: absolute;
  right: 12px;
  top: 50%;
  transform: translateY(-50%);
  background: #0e639c;
  color: #fff;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 11px;
  pointer-events: none;
}

.new-logs-button {
  position: absolute;
  bottom: 16px;
  left: 50%;
  transform: translateX(-50%);
  background: #0e639c;
  border-color: #0e639c;
  color: #fff;
  padding: 6px 16px;
  border-radius: 4px;
  font-size: 13px;
  z-index: 10;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
}

.new-logs-button:hover {
  background: #1177bb;
}

.log-line.level-error .line-level {
  color: var(--level-error);
}

.log-line.level-warn .line-level {
  color: var(--level-warn);
}

.log-line.level-info .line-level {
  color: var(--level-info);
}

.log-line.level-debug .line-level {
  color: var(--level-debug);
}

.log-line.level-trace .line-level {
  color: var(--level-trace);
}
</style>
