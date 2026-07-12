<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import type { LogEntry } from '../services/api'
import { useBookmarks } from '../composables/useBookmarks'
import { useCopyFeedbackId } from '../composables/useClipboard'
import { Bookmark, Maximize2, Minimize2, Check, Copy } from 'lucide-vue-next'

const { t } = useI18n()

const props = defineProps<{
  entries: LogEntry[]
  filePath?: string
  lineHeight?: number
  isTailMode?: boolean
  maxVisibleLines?: number
  highlightKeywords?: string[]
  levelColors?: Record<string, string>
  displayMode?: 'compact' | 'cozy'
}>()

const emit = defineEmits<{
  jumpToLine: [lineNum: number]
  stickToBottom: []
}>()

const lineHeight = computed(() => props.lineHeight ?? 26)
const displayMode = computed(() => props.displayMode ?? 'compact')
const { isBookmarked, add: addBookmark, remove: removeBookmark } = useBookmarks()
const containerRef = ref<HTMLDivElement | null>(null)
const scrollTop = ref(0)
const containerHeight = ref(600)
const showNewLogsButton = ref(false)
const { copiedId: copiedLine, copy: copyToText } = useCopyFeedbackId<number>()
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

function formatTimestamp(entry: LogEntry): string {
  const raw = entry.rawTimestamp

  // Cozy mode: long format YYYY-MM-DD HH:mm:ss.SSS
  if (displayMode.value === 'cozy') {
    if (raw && /^\d{4}-\d{2}-\d{2}/.test(raw)) {
      return raw
    }
    if (raw && /^\d{2}:\d{2}:\d{2}/.test(raw)) {
      const ts = entry.timestamp ?? raw
      try {
        const d = new Date(ts)
        if (!isNaN(d.getTime())) {
          const yyyy = d.getFullYear()
          const MM = String(d.getMonth() + 1).padStart(2, '0')
          const dd = String(d.getDate()).padStart(2, '0')
          return `${yyyy}-${MM}-${dd} ${raw}`
        }
      } catch {}
      return raw
    }
    const ts = entry.timestamp ?? raw
    if (!ts) return ''
    try {
      const d = new Date(ts)
      if (isNaN(d.getTime())) return ''
      return formatDateLong(d)
    } catch {
      return ''
    }
  }

  // Compact mode: short format HH:mm:ss.SSS
  if (raw && /^\d{2}:\d{2}:\d{2}/.test(raw)) {
    return raw
  }
  const ts = entry.timestamp ?? entry.rawTimestamp
  if (!ts) return ''
  try {
    const d = new Date(ts)
    if (isNaN(d.getTime())) return ''
    return formatDateShort(d)
  } catch {
    return ''
  }
}

function formatDateShort(d: Date): string {
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  const ss = String(d.getSeconds()).padStart(2, '0')
  const ms = String(d.getMilliseconds()).padStart(3, '0')
  return `${hh}:${mm}:${ss}.${ms}`
}

function formatDateLong(d: Date): string {
  const yyyy = d.getFullYear()
  const MM = String(d.getMonth() + 1).padStart(2, '0')
  const dd = String(d.getDate()).padStart(2, '0')
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  const ss = String(d.getSeconds()).padStart(2, '0')
  const ms = String(d.getMilliseconds()).padStart(3, '0')
  return `${yyyy}-${MM}-${dd} ${hh}:${mm}:${ss}.${ms}`
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
  await copyToText(entry.raw, entry.lineNum)
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

watch(expandedLines, () => {
  heightsVersion.value++
  nextTick(measureVisibleRows)
})

/* Keyword highlight hues cycle through 5 CSS classes (kw-mark-1..5) defined
   globally in style.css, so chips and in-log <mark> share the same palette. */
const KW_CLASS_COUNT = 5

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
        const cls = `kw-mark kw-mark-${(idx % KW_CLASS_COUNT) + 1}`
        return `<mark class="${cls}">${escapeHtml(part)}</mark>`
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

function toggleBookmark(entry: LogEntry): void {
  if (!props.filePath) return
  if (isBookmarked(props.filePath, entry.lineNum)) {
    removeBookmark(props.filePath, entry.lineNum)
  } else {
    addBookmark(props.filePath, entry)
  }
}

function toggleExpand(lineNum: number): void {
  // Record scroll position and row position before toggle
  const container = containerRef.value
  const oldScrollTop = container?.scrollTop ?? 0
  
  // Find the row element and its position
  const rowEl = rowRefs.get(lineNum)
  const rowTop = rowEl?.offsetTop ?? 0
  const rowHeight = rowEl?.offsetHeight ?? 0
  
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
            :class="[
              'level-' + entry.level.toLowerCase(),
              'mode-' + displayMode,
              { 'is-copied': copiedLine === entry.lineNum, 'wrap': true, 'expanded': expandedLines.has(entry.lineNum), 'is-highlighted': highlightedLine === entry.lineNum, 'is-marked': markedLine === entry.lineNum, 'is-bookmarked': filePath ? isBookmarked(filePath, entry.lineNum) : false }
            ]"
            @click="toggleMark(entry.lineNum)"
          >
            <div class="row-meta">
              <span v-if="entry.rawTimestamp || entry.timestamp" class="col-ts" :style="levelColors && levelColors[entry.level] ? { color: levelColors[entry.level] } : undefined">{{ formatTimestamp(entry) }}</span>
              <span class="col-badge"><span class="badge" :class="getBadgeClass(entry.level)" :style="levelColors && levelColors[entry.level] ? { color: levelColors[entry.level] } : undefined">{{ getBadgeText(entry.level) }}</span></span>
            </div>
            <div class="row-content">
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
                <span class="action-btn" @click="toggleBookmark(entry)" :title="t('logViewer.bookmark')">
                  <Bookmark :size="16" :stroke-width="2" fill="none" :color="filePath && isBookmarked(filePath, entry.lineNum) ? '#FF453A' : 'currentColor'" />
                </span>
                <span v-if="isJson(entry.raw)" class="action-btn" @click.stop="toggleExpand(entry.lineNum)" :title="expandedLines.has(entry.lineNum) ? t('logViewer.collapse') : t('logViewer.expandJson')">
                  <Minimize2 v-if="expandedLines.has(entry.lineNum)" :size="16" :stroke-width="2" />
                  <Maximize2 v-else :size="16" :stroke-width="2" />
                </span>
                <span class="action-btn" @click="copyLine(entry, $event)" :title="t('logViewer.copy')">
                  <span v-if="copiedLine === entry.lineNum" class="copy-icon copied">
                    <Check :size="16" :stroke-width="3" />
                  </span>
                  <span v-else class="copy-icon">
                    <Copy :size="16" :stroke-width="2" />
                  </span>
                </span>
              </span>
            </div>
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
  flex-direction: column;
  align-items: stretch;
  padding: 0 10px;
  white-space: nowrap;
  position: relative;
  transition: background .4s ease;
  background: var(--bg);
  border-radius: 5px;
  margin: 5px 0;
  border: 1px solid var(--border);
}

.log-row:hover {
  background: var(--bg-3);
}

/* ── Row state colors: same-hue transparency layering ──
   Every state derives from the row's level root color (--c-*-text), so an
   ERROR row reads red whether resting, hovered, marked, or bookmarked.
   Opacity encodes intensity: hover 10%, marked 14%, bookmarked 8%.
   Compound selectors (.is-X:hover.level-Y) win over plain :hover so state
   feedback survives hovering. Order matters: later = higher priority. */
.log-row.level-alert:hover   { background: color-mix(in srgb, var(--c-alert-text)  10%, var(--bg)); }
.log-row.level-error:hover   { background: color-mix(in srgb, var(--c-error-text)  10%, var(--bg)); }
.log-row.level-warn:hover    { background: color-mix(in srgb, var(--c-warn-text)   10%, var(--bg)); }
.log-row.level-info:hover    { background: color-mix(in srgb, var(--c-info-text)   10%, var(--bg)); }
.log-row.level-debug:hover   { background: color-mix(in srgb, var(--c-debug-text)  10%, var(--bg)); }
.log-row.level-trace:hover   { background: color-mix(in srgb, var(--c-trace-text)  10%, var(--bg)); }
.log-row.level-unknown:hover { background: var(--bg-3); }

/* marked (click-select): 14% tint + left color bar, per level */
.log-row.is-marked.level-alert   { background: color-mix(in srgb, var(--c-alert-text)  14%, var(--bg)); }
.log-row.is-marked.level-error   { background: color-mix(in srgb, var(--c-error-text)  14%, var(--bg)); }
.log-row.is-marked.level-warn    { background: color-mix(in srgb, var(--c-warn-text)   14%, var(--bg)); }
.log-row.is-marked.level-info    { background: color-mix(in srgb, var(--c-info-text)   14%, var(--bg)); }
.log-row.is-marked.level-debug   { background: color-mix(in srgb, var(--c-debug-text)  14%, var(--bg)); }
.log-row.is-marked.level-trace   { background: color-mix(in srgb, var(--c-trace-text)  14%, var(--bg)); }
.log-row.is-marked.level-unknown { background: var(--bg-3); }

/* bookmarked: 8% tint + ★ icon (added in template), per level */
.log-row.is-bookmarked.level-alert   { background: color-mix(in srgb, var(--c-alert-text)  8%, var(--bg)); }
.log-row.is-bookmarked.level-error   { background: color-mix(in srgb, var(--c-error-text)  8%, var(--bg)); }
.log-row.is-bookmarked.level-warn    { background: color-mix(in srgb, var(--c-warn-text)   8%, var(--bg)); }
.log-row.is-bookmarked.level-info    { background: color-mix(in srgb, var(--c-info-text)   8%, var(--bg)); }
.log-row.is-bookmarked.level-debug   { background: color-mix(in srgb, var(--c-debug-text)  8%, var(--bg)); }
.log-row.is-bookmarked.level-trace   { background: color-mix(in srgb, var(--c-trace-text)  8%, var(--bg)); }
.log-row.is-bookmarked.level-unknown { background: color-mix(in srgb, var(--text-3) 8%, var(--bg)); }

/* Stacked hover on state rows: deepen so feedback isn't lost (16% / 18%) */
.log-row.is-bookmarked:hover.level-alert   { background: color-mix(in srgb, var(--c-alert-text)  16%, var(--bg)); }
.log-row.is-bookmarked:hover.level-error   { background: color-mix(in srgb, var(--c-error-text)  16%, var(--bg)); }
.log-row.is-bookmarked:hover.level-warn    { background: color-mix(in srgb, var(--c-warn-text)   16%, var(--bg)); }
.log-row.is-bookmarked:hover.level-info    { background: color-mix(in srgb, var(--c-info-text)   16%, var(--bg)); }
.log-row.is-bookmarked:hover.level-debug   { background: color-mix(in srgb, var(--c-debug-text)  16%, var(--bg)); }
.log-row.is-bookmarked:hover.level-trace   { background: color-mix(in srgb, var(--c-trace-text)  16%, var(--bg)); }
.log-row.is-bookmarked:hover.level-unknown { background: color-mix(in srgb, var(--text-3) 16%, var(--bg)); }

.log-row.is-marked:hover.level-alert   { background: color-mix(in srgb, var(--c-alert-text)  18%, var(--bg)); }
.log-row.is-marked:hover.level-error   { background: color-mix(in srgb, var(--c-error-text)  18%, var(--bg)); }
.log-row.is-marked:hover.level-warn    { background: color-mix(in srgb, var(--c-warn-text)   18%, var(--bg)); }
.log-row.is-marked:hover.level-info    { background: color-mix(in srgb, var(--c-info-text)   18%, var(--bg)); }
.log-row.is-marked:hover.level-debug   { background: color-mix(in srgb, var(--c-debug-text)  18%, var(--bg)); }
.log-row.is-marked:hover.level-trace   { background: color-mix(in srgb, var(--c-trace-text)  18%, var(--bg)); }
.log-row.is-marked:hover.level-unknown { background: var(--bg-3); }

.log-row:hover .col-actions {
  opacity: 1;
}

.log-row.wrap {
  white-space: pre-wrap;
  word-break: break-all;
  height: auto;
  min-height: calc(var(--line-height, 26px) * 2);
}

.log-row.wrap .truncate-check {
  white-space: pre-wrap;
  word-break: break-all;
}

.log-row.expanded {
  height: auto;
  align-items: stretch;
  position: relative;
  z-index: 5;
  background: var(--bg);
}

/* Copied feedback: accent-derived (not level), transient confirmation */
.log-row.is-copied {
  background: color-mix(in srgb, var(--accent) 10%, var(--bg));
}

/* Jump-highlight on bookmark navigation: accent pulse (was hardcoded yellow) */
.log-row.is-highlighted {
  background: color-mix(in srgb, var(--accent) 14%, var(--bg));
  animation: highlight-flash 0.5s ease-out;
}

@keyframes highlight-flash {
  from { background: color-mix(in srgb, var(--accent) 28%, var(--bg)); }
}

/* ── Row Sections ── */
.row-meta {
  display: flex;
  align-items: center;
  height: var(--line-height, 26px);
  line-height: var(--line-height, 26px);
  flex-shrink: 0;
}

.row-content {
  display: flex;
  align-items: flex-start;
  min-height: var(--line-height, 26px);
  line-height: var(--line-height, 26px);
}

/* ── Columns ── */
.col-ts {
  width: auto;
  min-width: 84px;
  padding-right: 10px;
  color: var(--log-ts);
  font-size: 12px;
  white-space: nowrap;
  flex-shrink: 0;
}

/* Cozy 模式：actions 浮在 row-meta 行右侧（与时间戳/level 同行），
   row-content 消息在第二行不重叠；actions 仅 hover 可见。*/
.log-row.mode-cozy .col-actions {
  flex-direction: row;
  position: absolute;
  top: 0;
  right: 10px;
  height: var(--line-height, 26px);
  align-items: center;
  margin-left: 0;
}

.col-badge {
  min-width: 48px;
  padding-right: 14px;
  flex-shrink: 0;
}

.badge {
  display: inline-block;
  padding: 0;
  font-size: 12px;
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
  line-height: 2;
  color: var(--text);
  display: flex;
  align-items: center;
  gap: 4px;
}

.col-actions {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  opacity: 0;
  transition: opacity 0.15s;
  flex-shrink: 0;
  margin-left: 8px;
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
  font-size: 14px;
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

/* ── Compact Mode (single-line, original layout) ── */
.log-row.mode-compact {
  flex-direction: row;
  align-items: center;
  height: var(--line-height, 26px);
  line-height: var(--line-height, 26px);
  white-space: nowrap;
}

.log-row.mode-compact.wrap {
  white-space: pre-wrap;
  word-break: break-all;
  height: auto;
  min-height: var(--line-height, 26px);
  align-items: flex-start;
}

.log-row.mode-compact.expanded {
  align-items: flex-start;
}

.log-row.mode-compact .row-meta,
.log-row.mode-compact .row-content {
  display: contents;
}

/* In compact mode the actions column reverts to vertical (original look) */
.log-row.mode-compact .col-actions {
  flex-direction: row;
  align-self: center;
  margin-top: 2px;
}

/* ── New Logs Button ── */
.new-logs-button {
  position: absolute;
  bottom: 16px;
  left: 50%;
  transform: translateX(-50%);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border-color: var(--accent);
  color: var(--accent);
  padding: 6px 16px;
  border-radius: var(--radius);
  font-size: 14px;
  z-index: 10;
  box-shadow: var(--shadow-md);
  backdrop-filter: blur(8px);
  transition: background .12s ease;
}

.new-logs-button:hover {
  background: color-mix(in srgb, var(--accent) 20%, transparent);
}
</style>
