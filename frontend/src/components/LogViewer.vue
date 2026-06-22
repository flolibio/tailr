<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import type { LogEntry } from '../services/api'

const { t } = useI18n()

const props = defineProps<{
  entries: LogEntry[]
  lineHeight?: number
  fontFamily?: string
  isTailMode?: boolean
  maxVisibleLines?: number
  highlightKeywords?: string[]
  levelColors?: Record<string, string>
}>()

const emit = defineEmits<{
  jumpToLine: [lineNum: number]
  stickToBottom: []
}>()

const lineHeight = computed(() => props.lineHeight ?? 26)
const fontFamily = computed(() => props.fontFamily ?? 'JetBrains Mono')
const containerRef = ref<HTMLDivElement | null>(null)
const scrollTop = ref(0)
const containerHeight = ref(600)
const showNewLogsButton = ref(false)
const copiedLine = ref<number | null>(null)
const userScrolledUp = ref(false)
const highlightedLine = ref<number | null>(null)
const markedLine = ref<number | null>(null)
let highlightTimer: ReturnType<typeof setTimeout> | null = null

// ── Measurement-based virtual scrolling ──
// Fixed-height math breaks the moment any row wraps or is expanded:
// totalHeight underestimates, scroll-spacer is too short, content beyond it
// becomes unreachable, and scrollHeight jumps on every re-render causing
// flicker. We measure actual rendered heights and use cumulative offsets.
const measuredHeights = ref<Map<number, number>>(new Map())
const heightsVersion = ref(0)
const rowRefs = new Map<number, HTMLElement>()

function getRowHeight(lineNum: number): number {
  return measuredHeights.value.get(lineNum) ?? lineHeight.value
}

let _sumsSig = ''
let _prefixSums: Float64Array = new Float64Array(0)
function getPrefixSums(): Float64Array {
  const sig = `${props.entries.length}|${heightsVersion.value}`
  if (sig === _sumsSig) return _prefixSums
  const sums = new Float64Array(props.entries.length + 1)
  sums[0] = 0
  for (let i = 0; i < props.entries.length; i++) {
    sums[i + 1] = sums[i] + getRowHeight(props.entries[i].lineNum)
  }
  _sumsSig = sig
  _prefixSums = sums
  return sums
}

const visibleRange = computed(() => {
  heightsVersion.value
  props.entries
  const sums = getPrefixSums()
  // Binary search: largest i with sums[i] <= scrollTop
  const target = scrollTop.value
  let lo = 0, hi = props.entries.length
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (sums[mid] <= target) lo = mid
    else hi = mid - 1
  }
  const startIndex = Math.max(0, lo - 5)
  // Walk forward until viewport is covered (plus buffer)
  let endIndex = startIndex
  let cum = sums[startIndex]
  const limit = scrollTop.value + containerHeight.value + lineHeight.value * 4
  while (endIndex < props.entries.length && cum < limit) {
    cum += getRowHeight(props.entries[endIndex].lineNum)
    endIndex++
  }
  return { startIndex, endIndex: Math.min(props.entries.length, endIndex + 5) }
})

const visibleEntries = computed(() => {
  return props.entries.slice(visibleRange.value.startIndex, visibleRange.value.endIndex)
})

const totalHeight = computed(() => {
  heightsVersion.value
  props.entries
  const sums = getPrefixSums()
  return sums[props.entries.length] || 0
})

const offsetY = computed(() => {
  heightsVersion.value
  props.entries
  const sums = getPrefixSums()
  return sums[visibleRange.value.startIndex] || 0
})

function setRowRef(lineNum: number, el: any): void {
  if (el) {
    rowRefs.set(lineNum, el as HTMLElement)
  } else {
    rowRefs.delete(lineNum)
  }
}

function measureVisibleRows(): void {
  let changed = false
  for (const [lineNum, el] of rowRefs.entries()) {
    const h = el.offsetHeight
    if (h > 0 && measuredHeights.value.get(lineNum) !== h) {
      measuredHeights.value.set(lineNum, h)
      changed = true
    }
  }
  if (changed) heightsVersion.value++
}

function getBadgeClass(level: string): string {
  if (!props.levelColors || !(level in props.levelColors)) return 'badge-unknown'
  const l = level.toLowerCase()
  if (l === 'alert') return 'badge-alert'
  if (l === 'error' || l === 'err') return 'badge-error'
  if (l === 'warn' || l === 'warning') return 'badge-warn'
  if (l === 'info') return 'badge-info'
  if (l === 'debug') return 'badge-debug'
  if (l === 'trace') return 'badge-trace'
  return 'badge-dynamic'
}

function getBadgeText(level: string): string {
  if (level === 'UNKNOWN') return 'UNK'
  return level
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
  } else {
    showNewLogsButton.value = false
    userScrolledUp.value = false
  }
}

function scrollToBottom(): void {
  if (!containerRef.value) return
  containerRef.value.scrollTop = containerRef.value.scrollHeight
  requestAnimationFrame(() => {
    if (containerRef.value) {
      containerRef.value.scrollTop = containerRef.value.scrollHeight
    }
  })
  showNewLogsButton.value = false
  userScrolledUp.value = false
}

function onNewLogsClick(): void {
  emit('stickToBottom')
  scrollToBottom()
}

function scrollToLine(lineNum: number): void {
  userScrolledUp.value = true
  const index = props.entries.findIndex((e) => e.lineNum === lineNum)
  if (index >= 0 && containerRef.value) {
    const sums = getPrefixSums()
    containerRef.value.scrollTop = sums[index] || 0
    if (highlightTimer) clearTimeout(highlightTimer)
    highlightedLine.value = lineNum
    highlightTimer = setTimeout(() => {
      highlightedLine.value = null
    }, 3000)
  }
}

async function copyLine(entry: LogEntry, event: MouseEvent): Promise<void> {
  event.stopPropagation()
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(entry.raw)
    } else {
      const textarea = document.createElement('textarea')
      textarea.value = entry.raw
      textarea.style.position = 'fixed'
      textarea.style.left = '-9999px'
      document.body.appendChild(textarea)
      textarea.select()
      document.execCommand('copy')
      document.body.removeChild(textarea)
    }
    copiedLine.value = entry.lineNum
    setTimeout(() => {
      if (copiedLine.value === entry.lineNum) copiedLine.value = null
    }, 1500)
  } catch {}
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

function highlightJson(json: string): string {
  return json.replace(
    /("(?:\\.|[^"\\])*")\s*:/g,
    '<span class="json-key">$1</span>:'
  ).replace(
    /:\s*("(?:\\.|[^"\\])*")/g,
    ': <span class="json-str">$1</span>'
  ).replace(
    /:\s*(\d+\.?\d*)/g,
    ': <span class="json-num">$1</span>'
  ).replace(
    /:\s*(true|false|null)/g,
    ': <span class="json-bool">$1</span>'
  )
}

const expandedLines = ref<Set<number>>(new Set())
const truncatedLines = ref<Set<number>>(new Set())
const msgRefs = ref<Map<number, HTMLSpanElement>>(new Map())

function setMsgRef(lineNum: number, el: any): void {
  if (el) {
    msgRefs.value.set(lineNum, el as HTMLSpanElement)
  }
}

function checkTruncation(): void {
  nextTick(() => {
    for (const [lineNum, el] of msgRefs.value.entries()) {
      if (expandedLines.value.has(lineNum)) {
        truncatedLines.value.delete(lineNum)
        continue
      }
      const textEl = el.querySelector('.truncate-check')
      if (textEl) {
        const isTruncated = textEl.scrollWidth > textEl.clientWidth + 2
        if (isTruncated) {
          truncatedLines.value.add(lineNum)
        } else {
          truncatedLines.value.delete(lineNum)
        }
      }
    }
  })
}

watch(() => props.entries.length, () => {
  checkTruncation()
  measureVisibleRows()
})

watch(visibleEntries, () => {
  checkTruncation()
  measureVisibleRows()
})

const lastToggledLine = ref<number | null>(null)

watch(expandedLines, () => {
  // Don't delete height immediately - let measureVisibleRows update it
  // This prevents the jump caused by falling back to default lineHeight
  lastToggledLine.value = null
  heightsVersion.value++
  nextTick(measureVisibleRows)
})

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

function toggleMark(lineNum: number): void {
  markedLine.value = markedLine.value === lineNum ? null : lineNum
}

function toggleExpand(lineNum: number): void {
  // Record scroll position and row position before toggle
  const container = containerRef.value
  const oldScrollTop = container?.scrollTop ?? 0
  
  // Find the row element and its position
  const rowEl = rowRefs.get(lineNum)
  const rowTop = rowEl?.offsetTop ?? 0
  const rowHeight = rowEl?.offsetHeight ?? 0
  
  lastToggledLine.value = lineNum
  const next = new Set(expandedLines.value)
  if (next.has(lineNum)) next.delete(lineNum)
  else next.add(lineNum)
  expandedLines.value = next
  
  // After DOM update, adjust scroll to keep row position stable
  nextTick(() => {
    if (!container) return
    const newRowEl = rowRefs.get(lineNum)
    if (!newRowEl) return
    
    const newRowHeight = newRowEl.offsetHeight
    const heightDiff = newRowHeight - rowHeight
    
    // If the toggled row is above the current viewport, adjust scroll
    if (rowTop < oldScrollTop) {
      container.scrollTop = oldScrollTop + heightDiff
    }
  })
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

watch(heightsVersion, () => {
  if (props.isTailMode && !userScrolledUp.value) {
    nextTick(scrollToBottom)
  }
})

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
  if (props.isTailMode) {
    nextTick(scrollToBottom)
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
            :ref="(el) => setRowRef(entry.lineNum, el)"
            class="log-row"
            :style="{ fontFamily: `'${fontFamily}', var(--font-mono)` }"
            :class="[
              'level-' + entry.level.toLowerCase(),
              { 'is-copied': copiedLine === entry.lineNum, 'wrap': true, 'expanded': expandedLines.has(entry.lineNum), 'is-highlighted': highlightedLine === entry.lineNum, 'is-marked': markedLine === entry.lineNum }
            ]"
            @click="toggleMark(entry.lineNum)"
          >
            <span v-if="entry.timestamp" class="col-ts">{{ formatTimestamp(entry.timestamp) }}</span>
            <span class="col-badge"><span class="badge" :class="getBadgeClass(entry.level)" :style="levelColors && levelColors[entry.level] ? { color: levelColors[entry.level] } : undefined">{{ getBadgeText(entry.level) }}</span></span>
            <span class="col-msg" :ref="(el) => setMsgRef(entry.lineNum, el)">
              <template v-if="isJson(entry.raw)">
                <span v-if="!expandedLines.has(entry.lineNum)" class="truncate-check" v-html="
                  highlightText(entry.raw)
                "></span>
                <pre v-else class="json-expanded" v-html="highlightJson(formatJson(entry.raw))"></pre>
              </template>
              <template v-else>
                <span class="truncate-check" v-html="highlightText(entry.raw)"></span>
              </template>
            </span>
            <span class="col-actions">
              <span v-if="isJson(entry.raw)" class="action-btn" @click.stop="toggleExpand(entry.lineNum)" :title="expandedLines.has(entry.lineNum) ? t('logViewer.collapse') : t('logViewer.expandJson')">
                <svg v-if="expandedLines.has(entry.lineNum)" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 14 10 14 10 20"/><polyline points="20 10 14 10 14 4"/><line x1="14" y1="10" x2="21" y2="3"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
                <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
              </span>
              <span class="action-btn" @click="copyLine(entry, $event)" :title="t('logViewer.copy')">
                <span v-if="copiedLine === entry.lineNum" class="copy-icon copied">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                </span>
                <span v-else class="copy-icon">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                </span>
              </span>
            </span>
          </div>
        </div>
      </div>
    </div>
    <button
      v-if="showNewLogsButton"
      class="new-logs-button"
      @click="onNewLogsClick"
    >
      {{ t('logViewer.newLogs') }}
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
  background: var(--bg);
  padding: 10px;
}

.scroll-spacer {
  position: relative;
  width: 100%;
}

/* ── Log Row ── */
.log-row {
  display: flex;
  align-items: center;
  height: 26px;
  line-height: 26px;
  padding: 0 10px;
  white-space: nowrap;
  position: relative;
  transition: background .08s;
  background: var(--bg);
}

.log-row:hover {
  background: var(--bg-3);
}

.log-row.level-alert:hover { background: var(--c-alert-bg); }
.log-row.level-error:hover { background: var(--c-error-bg); }
.log-row.level-warn:hover { background: var(--c-warn-bg); }
.log-row.level-info:hover { background: var(--c-info-bg); }
.log-row.level-debug:hover { background: var(--c-debug-bg); }
.log-row.level-trace:hover { background: var(--c-trace-bg); }
.log-row.level-unknown:hover { background: var(--bg-3); }

.log-row:hover .col-actions {
  opacity: 1;
}

.log-row.wrap {
  white-space: pre-wrap;
  word-break: break-all;
  height: auto;
  min-height: 26px;
}

.log-row.wrap .truncate-check {
  white-space: pre-wrap;
  word-break: break-all;
}

.log-row.expanded {
  height: auto;
  align-items: flex-start;
  position: relative;
  z-index: 5;
  background: var(--bg);
}

.log-row.is-copied {
  background: rgba(24, 95, 165, 0.1);
}

.log-row.is-highlighted {
  background: rgba(255, 220, 0, 0.2);
  box-shadow: inset 3px 0 0 #e6a800;
  transition: background 0.5s ease;
}

.log-row.is-marked {
  background: rgba(100, 210, 255, 0.15);
  box-shadow: inset 3px 0 0 #64d2ff;
}

/* ── Columns ── */
.col-ts {
  width: 84px;
  min-width: 84px;
  padding-right: 10px;
  color: var(--log-ts);
  font-size: 11px;
  white-space: nowrap;
  flex-shrink: 0;
  align-self: flex-start;
}

.col-badge {
  min-width: 48px;
  padding-right: 14px;
  flex-shrink: 0;
  align-self: flex-start;
}

.badge {
  display: inline-block;
  padding: 0;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.04em;
  font-family: var(--font-mono);
  background: transparent;
  white-space: nowrap;
  text-align: left;
}

.badge-alert { color: #FF3B30; }
.badge-error { color: #FF453A; }
.badge-warn  { color: #FF9F0A; }
.badge-info  { color: #64D2FF; }
.badge-debug { color: #30D158; }
.badge-trace { color: #BF5AF2; }
.badge-unknown { color: #666666; }
.badge-dynamic { /* color from inline style */ }

.col-msg {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.8;
  color: var(--text);
  display: flex;
  align-items: center;
  gap: 4px;
}

.col-actions {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  opacity: 0;
  transition: opacity 0.15s;
  flex-shrink: 0;
  margin-left: 8px;
  align-self: flex-start;
  margin-top: 5px;
}

.action-btn {
  cursor: pointer;
  width: 20px;
  height: 20px;
  color: var(--text-3);
  border-radius: 4px;
  justify-content: center;
  align-items: center;
  transition: background .1s, color .1s;
  display: flex;
}

.action-btn:hover {
  background: var(--bg-2);
  color: var(--text);
}

.copy-icon {
  display: flex;
  align-items: center;
  justify-content: center;
}

.copy-icon.copied {
  color: var(--accent);
}

/* ── JSON ── */
.truncate-check {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.json-expanded {
  display: block;
  flex: 1;
  min-width: 0;
  white-space: pre-wrap;
  background: var(--bg-2);
  padding: 12px;
  margin: 2px 0;
  border-radius: 6px;
  max-height: 400px;
  overflow-y: auto;
  font-size: 13px;
  line-height: 1.5;
  border: 1px solid var(--border);
}

.json-expanded :deep(.json-key) {
  color: #a626a4;
}

.json-expanded :deep(.json-str) {
  color: #50a14f;
}

.json-expanded :deep(.json-num) {
  color: #986801;
}

.json-expanded :deep(.json-bool) {
  color: #e45649;
}

/* Dark theme JSON colors */
:root.dark .json-expanded :deep(.json-key) {
  color: #c678dd;
}

:root.dark .json-expanded :deep(.json-str) {
  color: #98c379;
}

:root.dark .json-expanded :deep(.json-num) {
  color: #d19a66;
}

:root.dark .json-expanded :deep(.json-bool) {
  color: #e06c75;
}

/* ── Long Line ── */
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
