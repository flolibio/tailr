<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search, X, History } from 'lucide-vue-next'

const { t } = useI18n()

const props = defineProps<{
  currentFile: string | null
  keywords: string[]
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
const selectedSuggestionIndex = ref(-1)

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

// Escape user text for safe insertion via v-html (suggestion match highlight).
function escapeHtml(str: string): string {
  return str.replace(/[&<>"']/g, (m) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[m]!))
}

// Wrap the matched substring of `word` in <mark> so the user sees where their
// query matches inside each suggestion (learned from chip-search-demo-v2).
function highlightMatch(word: string, query: string): string {
  const idx = word.toLowerCase().indexOf(query.toLowerCase())
  if (idx === -1) return escapeHtml(word)
  return (
    escapeHtml(word.slice(0, idx)) +
    '<mark>' + escapeHtml(word.slice(idx, idx + query.length)) + '</mark>' +
    escapeHtml(word.slice(idx + query.length))
  )
}

function onInput(): void {
  // Default-highlight the first item so Enter can commit it immediately
  // (matches the demo's activeIndex = 0 on open).
  selectedSuggestionIndex.value = suggestions.value.length > 0 ? 0 : -1
  showSuggestions.value = input.value.trim().length > 0 && suggestions.value.length > 0
}

function selectSuggestion(kw: string): void {
  input.value = ''
  showSuggestions.value = false
  selectedSuggestionIndex.value = -1
  emit('addKeyword', kw)
}

// Add the current input as a keyword (shared by Enter / Space / paste-split).
// Returns true if a keyword was actually added.
function commitInput(): boolean {
  const kw = input.value.trim()
  if (!kw || props.keywords.includes(kw)) {
    input.value = ''
    return false
  }
  emit('addKeyword', kw)
  addToHistory(kw)
  input.value = ''
  return true
}

function onKeydown(e: KeyboardEvent): void {
  // Keyboard navigation within the suggestions dropdown.
  if (showSuggestions.value && suggestions.value.length > 0) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedSuggestionIndex.value =
        (selectedSuggestionIndex.value + 1) % suggestions.value.length
      scrollSelectedIntoView()
      return
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      const n = suggestions.value.length
      selectedSuggestionIndex.value =
        (selectedSuggestionIndex.value - 1 + n) % n
      scrollSelectedIntoView()
      return
    }
    // Enter / Tab with a highlighted suggestion: commit that suggestion.
    if ((e.key === 'Enter' || e.key === 'Tab') && selectedSuggestionIndex.value >= 0) {
      e.preventDefault()
      selectSuggestion(suggestions.value[selectedSuggestionIndex.value])
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      showSuggestions.value = false
      selectedSuggestionIndex.value = -1
      return
    }
  }

  // Enter OR Space adds the keyword (Space only commits when there is text to
  // commit; an empty input + Space falls through to default so the user can
  // still type a space if they really want one mid-token).
  if (e.key === 'Enter' || (e.key === ' ' && input.value.trim())) {
    e.preventDefault()
    showSuggestions.value = false
    selectedSuggestionIndex.value = -1
    commitInput()
  } else if (e.key === 'Escape') {
    if (showSuggestions.value) {
      showSuggestions.value = false
      selectedSuggestionIndex.value = -1
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
    // Tab with no highlight: pick the first suggestion.
    e.preventDefault()
    selectSuggestion(suggestions.value[0])
  }
}

// Scroll the highlighted suggestion item into view inside the scrollable list.
function scrollSelectedIntoView(): void {
  nextTick(() => {
    const list = suggestionsRef.value
    if (!list) return
    const el = list.children[selectedSuggestionIndex.value] as HTMLElement | undefined
    el?.scrollIntoView({ block: 'nearest' })
  })
}

// Paste handling: if the pasted text contains commas or whitespace, split it
// into multiple keywords (matching the demo's UX). A single bare token with no
// separator is left to normal input/Enter handling.
function onPaste(e: ClipboardEvent): void {
  const text = e.clipboardData?.getData('text') ?? ''
  if (!/[,\s]/.test(text)) return
  e.preventDefault()
  const tokens = text.split(/[,\s]+/).map((s) => s.trim()).filter(Boolean)
  for (const tok of tokens) {
    if (!props.keywords.includes(tok)) {
      emit('addKeyword', tok)
      addToHistory(tok)
    }
  }
  input.value = ''
  showSuggestions.value = false
}

function doClearAll(): void {
  input.value = ''
  showSuggestions.value = false
  emit('clearAll')
}

function focus(): void {
  inputRef.value?.focus()
}

// ── Overflow handling: hide chips from the front when they don't fit ──
//
// Approach (learned from docs/feat/chip-search-demo-v2.html): let the browser
// measure, don't estimate a budget in JS. A hidden probe element renders ALL
// chips so its scrollWidth is the true natural width of the full chip row.
// We then binary-search the smallest `hiddenCount` (chips folded into +N from
// the front) such that the remaining chips + badge fit inside .filter-content.
//
// Front-folding keeps the badge pinned left and the input pinned right, so the
// two never compete for space — the failure mode of the earlier budget-based
// attempts.

const hiddenCount = ref(0)
const showOverflowPopover = ref(false)
const filterContentRef = ref<HTMLElement | null>(null)
const chipProbeRef = ref<HTMLElement | null>(null)
const overflowBadgeRef = ref<HTMLElement | null>(null)
const overflowPopoverRef = ref<HTMLElement | null>(null)

const overflowCount = computed(() => hiddenCount.value)
// Visible chips = the tail of the array (newest, nearest the input).
const visibleKeywords = computed(() => props.keywords.slice(hiddenCount.value))
// Hidden chips = the head of the array (oldest, folded into +N).
const hiddenKeywords = computed(() => props.keywords.slice(0, hiddenCount.value))

// Padding inside .filter-content (left icon area + right clear area). Kept in
// sync with the CSS (padding: 0 30px 0 34px) and the flex gap.
const PADDING_LEFT = 34
const PADDING_RIGHT = 30
const GAP = 4
// Input keeps a min-width so the field stays usable even when chips fill the row.
const INPUT_MIN = 40
// Badge width estimate used before the badge has rendered; corrected after.
const BADGE_W_FALLBACK = 36

let resizeRaf = 0

function recomputeVisible(): void {
  const content = filterContentRef.value
  const probe = chipProbeRef.value
  if (!content || !probe) return

  // Width available for (badge? + chips + input), excluding horizontal padding.
  const availAll = content.clientWidth - PADDING_LEFT - PADDING_RIGHT
  if (availAll <= 0) return

  // No chips → nothing to hide.
  if (props.keywords.length === 0) {
    if (hiddenCount.value !== 0) hiddenCount.value = 0
    return
  }

  // Collect each chip's natural width from the probe (gap-separated).
  const chipEls = Array.from(probe.querySelectorAll<HTMLElement>('.chip'))
  const widths = chipEls.map((el) => el.offsetWidth)
  const totalAll = widths.reduce((s, w) => s + w, 0) + GAP * Math.max(0, widths.length - 1)

  // Everything fits with room for the input → no folding.
  if (totalAll + GAP + INPUT_MIN <= availAll) {
    if (hiddenCount.value !== 0) hiddenCount.value = 0
    return
  }

  // Badge width: prefer measured (after render), else fallback.
  const badgeW = overflowBadgeRef.value?.offsetWidth ?? BADGE_W_FALLBACK

  // Binary-search the smallest hiddenCount so that
  //   badge + (remaining chips) + gap + input-min  ≤  availAll
  let lo = 1
  let hi = props.keywords.length
  let best = props.keywords.length
  while (lo <= hi) {
    const mid = (lo + hi) >> 1
    const remaining = widths.slice(mid)
    const sumRem = remaining.reduce((s, w) => s + w, 0) + GAP * Math.max(0, remaining.length - 1)
    // Layout when badge is showing: badge, gap, [remaining chips with gaps], gap, input.
    const used = badgeW + GAP + sumRem + GAP + INPUT_MIN
    if (used <= availAll) {
      best = mid
      hi = mid - 1
    } else {
      lo = mid + 1
    }
  }

  if (hiddenCount.value !== best) hiddenCount.value = best
}

function scheduleRecompute(): void {
  if (resizeRaf) cancelAnimationFrame(resizeRaf)
  resizeRaf = requestAnimationFrame(() => {
    resizeRaf = 0
    recomputeVisible()
  })
}

let resizeObserver: ResizeObserver | null = null

watch(
  () => props.keywords,
  () => {
    // Chips may have grown/shrunk; close popover if nothing is hidden anymore.
    nextTick(() => {
      recomputeVisible()
      if (hiddenCount.value === 0) showOverflowPopover.value = false
    })
  },
)

// Keep popover open only while overflow exists.
watch(overflowCount, (n) => {
  if (n === 0) showOverflowPopover.value = false
})

function onClickOutside(e: MouseEvent): void {
  const target = e.target as Node
  // Close suggestions dropdown
  if (
    suggestionsRef.value &&
    !suggestionsRef.value.contains(target) &&
    inputRef.value &&
    !inputRef.value.contains(target)
  ) {
    showSuggestions.value = false
  }
  // Close overflow popover when clicking outside the badge/popover
  const badge = overflowBadgeRef.value
  const popover = overflowPopoverRef.value
  if (
    showOverflowPopover.value &&
    badge && !badge.contains(target) &&
    (!popover || !popover.contains(target))
  ) {
    showOverflowPopover.value = false
  }
}

onMounted(() => {
  loadHistory()
  document.addEventListener('mousedown', onClickOutside)

  nextTick(() => {
    recomputeVisible()
    if (filterContentRef.value && typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(() => scheduleRecompute())
      resizeObserver.observe(filterContentRef.value)
    }
  })
})

onUnmounted(() => {
  document.removeEventListener('mousedown', onClickOutside)
  if (resizeRaf) cancelAnimationFrame(resizeRaf)
  resizeObserver?.disconnect()
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
        <Search :size="14" :stroke-width="2" />
      </span>
      <div ref="filterContentRef" class="filter-content">
        <!-- "+N" overflow badge: front-pinned so it never fights the input -->
        <button
          v-if="overflowCount > 0"
          ref="overflowBadgeRef"
          class="chip-overflow-badge"
          :class="{ active: showOverflowPopover }"
          @click="showOverflowPopover = !showOverflowPopover"
        >+{{ overflowCount }}</button>

        <!-- Visible chips (tail of the array). Template index i is relative to
             the visible slice; emit using the original array index (i + hiddenCount).
             The kw-N color class also uses the original index so chip colors stay
             aligned with the in-log <mark> highlights. -->
        <span
          v-for="(kw, i) in visibleKeywords"
          :key="kw"
          :title="kw"
          class="chip kw-chip"
          :class="'kw-' + (((i + hiddenCount) % 5) + 1)"
        >
          <template v-if="editingIndex === i + hiddenCount">
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
            <span class="chip-text" @dblclick="startEdit(i + hiddenCount)">{{ kw }}</span>
          </template>
          <button class="chip-remove" @click="emit('removeKeyword', i + hiddenCount)">✕</button>
        </span>

        <input
          ref="inputRef"
          v-model="input"
          type="text"
          class="filter-input"
          :placeholder="keywords.length ? '' : t('filter.filterLogs')"
          @keydown="onKeydown"
          @input="onInput"
          @focus="onInput"
          @paste="onPaste"
          :disabled="!currentFile"
        />

        <!-- Hidden measuring probe: renders ALL chips at their natural width so
             scrollWidth/offsetWidth reflect the true layout. Never visible. -->
        <div ref="chipProbeRef" class="chip-measure-probe" aria-hidden="true">
          <span
            v-for="(kw, i) in keywords"
            :key="kw"
            class="chip kw-chip"
            :class="'kw-' + ((i % 5) + 1)"
          >
            <span class="chip-text">{{ kw }}</span>
            <button class="chip-remove" tabindex="-1">✕</button>
          </span>
        </div>
      </div>
      <!-- Suggestions dropdown — positioned relative to filter-wrap -->
      <div v-if="showSuggestions" ref="suggestionsRef" class="suggestions-dropdown">
        <div
          v-for="(s, i) in suggestions"
          :key="s"
          class="suggestion-item"
          :class="{ active: i === selectedSuggestionIndex }"
          @mousedown.prevent="selectSuggestion(s)"
          @mouseenter="selectedSuggestionIndex = i"
        >
          <History class="suggestion-icon" :size="13" :stroke-width="2" />
          <span class="suggestion-text" v-html="highlightMatch(s, input.trim())"></span>
        </div>
      </div>
      <button v-if="keywords.length || input" class="filter-clear" @click="doClearAll" :title="t('filter.clearAll')">
        <X :size="14" :stroke-width="2.5" />
      </button>

      <!-- Overflow popover — lists chips folded into +N -->
      <div
        v-if="showOverflowPopover && overflowCount > 0"
        ref="overflowPopoverRef"
        class="overflow-popover"
      >
        <span
          v-for="(kw, i) in hiddenKeywords"
          :key="kw"
          :title="kw"
          class="chip kw-chip"
          :class="'kw-' + ((i % 5) + 1)"
        >
          <span class="chip-text">{{ kw }}</span>
          <button class="chip-remove" @click="emit('removeKeyword', i)">✕</button>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.filter-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  /* Component root is a flex child of the grid item; without min-width:0 it
     defaults to min-width:auto (content size) and pushes the whole layout
     wider than the viewport. Must allow shrinking down the chain. */
  min-width: 0;
  position: relative;
}

.filter-wrap {
  flex: 1;
  position: relative;
  display: flex;
  align-items: center;
  /* min-width:0 is required here too — without it this flex item defaults to
     min-width:auto (content size) and the chips push it (and the whole row)
     wider than the viewport. Must be 0 down the whole flex chain. */
  min-width: 0;
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
  overflow: hidden;
}

.filter-content:focus-within {
  border-color: var(--border-2);
  background: var(--bg);
}

/* "+N" overflow badge. A flex sibling of the chips and input so it never
   overlaps them; front-pinned via document order (renders before the chips).
   Explicit height matches .chip's rendered height so they align on the row;
   appearance:none neutralizes the <button> default look. Solid dark fill +
   light text, like the demo's .more-badge. */
.chip-overflow-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  align-self: center;
  appearance: none;
  -webkit-appearance: none;
  min-width: 26px;
  height: 26px;
  padding: 0 8px;
  border: none;
  border-radius: 20px;
  background: var(--text);
  color: var(--bg);
  font-size: 12px;
  font-weight: 500;
  line-height: 1;
  cursor: pointer;
  user-select: none;
  transition: background .12s;
}
.chip-overflow-badge:hover,
.chip-overflow-badge.active {
  background: var(--text-3);
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 6px 4px 10px;
  background: var(--chip-bg);
  border: 1px solid var(--border);
  border-radius: 9999px;
  font-size: 12px;
  line-height: 1;
  color: var(--chip-text);
  white-space: nowrap;
  flex-shrink: 0;
  transition: background .15s;
  font-weight: 600;
  animation: chip-pop .15s ease;
}

@keyframes chip-pop {
  from { transform: scale(.7); opacity: 0; }
  to   { transform: scale(1); opacity: 1; }
}

/* Keyword chips: each chip gets a kw-N class that sets --kw-cur, then the
   shared .kw-chip rule derives bg/text/border from that hue. Shares the same
   --kw-* palette as in-log <mark> highlights (defined in style.css). */
.chip.kw-chip {
  background: hsl(var(--kw-cur) / 14%);
  color: hsl(var(--kw-cur) / 90%);
  border-color: hsl(var(--kw-cur) / 30%);
}
.chip.kw-chip:hover {
  background: hsl(var(--kw-cur) / 22%);
  color: hsl(var(--kw-cur));
}
:global(:root.dark) .chip.kw-chip {
  background: hsl(var(--kw-cur) / 16%);
  color: hsl(var(--kw-cur) / 85%);
}
:global(:root.dark) .chip.kw-chip:hover {
  background: hsl(var(--kw-cur) / 26%);
}
.chip.kw-1 { --kw-cur: var(--kw-1); }
.chip.kw-2 { --kw-cur: var(--kw-2); }
.chip.kw-3 { --kw-cur: var(--kw-3); }
.chip.kw-4 { --kw-cur: var(--kw-4); }
.chip.kw-5 { --kw-cur: var(--kw-5); }

.chip-text {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: default;
  line-height: 1.2;
}

.chip-edit-input {
  width: 80px;
  min-width: 40px;
  max-width: 200px;
  height: 18px;
  border: none;
  background: var(--bg);
  font-family: var(--font-mono);
  font-size: 14px;
  color: var(--text);
  outline: none;
  padding: 0 2px;
  border-radius: 2px;
}

/* Circular, prominent remove button. On hover it inverts to the chip's hue,
   making the click target obvious (learned from chip-search-demo-v2). */
.chip-remove {
  width: 16px;
  height: 16px;
  border: none;
  background: transparent;
  color: hsl(var(--kw-cur) / 70%);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  padding: 0;
  border-radius: 50%;
  flex-shrink: 0;
  transition: background .12s, color .12s;
}

.chip-remove:hover {
  background: hsl(var(--kw-cur));
  color: #fff;
}

.filter-input {
  flex: 1;
  min-width: 40px;
  height: 100%;
  border: none;
  background: transparent;
  font-family: var(--font-mono);
  font-size: 14px;
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

/* Hidden measuring probe: out of flow, invisible, but offsetWidth/scrollWidth
   still report the chips' true natural widths. Used by recomputeVisible(). */
.chip-measure-probe {
  position: absolute;
  visibility: hidden;
  pointer-events: none;
  top: 0;
  left: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
}

/* Overflow popover: lists chips folded into +N. Anchored to .filter-wrap
   (its offset parent), left:0/right:0 make it span the search box width —
   matching the demo where the popup aligns to the search box edges. */
.overflow-popover {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 50;
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 8px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
}

/* ── Suggestions dropdown ── */
.suggestions-dropdown {
  position: absolute;
  top: 100%;
  left: 0;
  right: 0;
  margin-top: 4px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12);
  z-index: 100;
  max-height: 240px;
  overflow-y: auto;
  padding: 4px;
}

.suggestion-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 12px;
  font-family: var(--font-mono);
  color: var(--text);
  transition: background .1s;
}

.suggestion-item:hover {
  background: var(--bg-3);
}

.suggestion-item.active {
  background: var(--bg-3);
}

/* Highlight the matched substring inside a suggestion item. */
.suggestion-text :deep(mark) {
  background: hsl(var(--kw-1) / 18%);
  color: inherit;
  font-weight: 600;
  border-radius: 2px;
  padding: 0 1px;
}

.suggestion-icon {
  color: var(--text-3);
  flex-shrink: 0;
}

.suggestion-item.active .suggestion-icon {
  color: var(--text);
}

.suggestion-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
