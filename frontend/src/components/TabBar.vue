<script setup lang="ts">
import { ref, nextTick, watch, onMounted, onBeforeUnmount } from 'vue'
import { ChevronLeft, ChevronRight } from 'lucide-vue-next'
import { useTabs } from '../composables/useTabs'

const { tabs, activeTabPath, switchTo, closeTab } = useTabs()

const scroller = ref<HTMLElement | null>(null)
const canScrollLeft = ref(false)
const canScrollRight = ref(false)

function basename(path: string): string {
  const parts = path.split('/')
  return parts[parts.length - 1] || path
}

function handleClose(path: string, event: MouseEvent): void {
  event.stopPropagation()
  closeTab(path)
}

function handleMiddleClick(path: string, event: MouseEvent): void {
  if (event.button === 1) {
    event.preventDefault()
    closeTab(path)
  }
}

/** Recompute whether each arrow should be shown. Called on scroll, resize,
 *  and whenever the tab list / active tab changes. */
function updateScrollState(): void {
  const el = scroller.value
  if (!el) {
    canScrollLeft.value = false
    canScrollRight.value = false
    return
  }
  // 1px tolerance: fractional-pixel layouts on hi-DPI can leave a sub-pixel
  // sliver that flips the arrow on/off jitteringly.
  canScrollLeft.value = el.scrollLeft > 1
  canScrollRight.value = el.scrollLeft + el.clientWidth < el.scrollWidth - 1
}

function scrollBy(direction: 1 | -1): void {
  const el = scroller.value
  if (!el) return
  // Step ~80% of the visible width — enough to move a meaningful chunk of
  // tabs into view without overshooting past fully-visible tabs.
  el.scrollBy({ left: direction * el.clientWidth * 0.8, behavior: 'smooth' })
}

function onScroll(): void {
  updateScrollState()
}

/** Bring the active tab fully into view. Called after tab open/switch so the
 *  active tab is never left scrolled out of sight. */
function scrollActiveIntoView(): void {
  const el = scroller.value
  if (!el) return
  const active = el.querySelector<HTMLElement>('.tab.active')
  if (!active) return
  const margin = 8
  const left = active.offsetLeft
  const right = left + active.offsetWidth
  if (left < el.scrollLeft) {
    el.scrollTo({ left: Math.max(0, left - margin), behavior: 'smooth' })
  } else if (right > el.scrollLeft + el.clientWidth) {
    el.scrollTo({ left: right - el.clientWidth + margin, behavior: 'smooth' })
  }
}

watch(
  () => [tabs.value.length, activeTabPath.value],
  () => {
    // Layout must settle before we can measure offsets / scrollWidth.
    nextTick(() => {
      updateScrollState()
      scrollActiveIntoView()
    })
  },
)

let resizeObserver: ResizeObserver | null = null
onMounted(() => {
  updateScrollState()
  if (scroller.value) {
    resizeObserver = new ResizeObserver(() => updateScrollState())
    resizeObserver.observe(scroller.value)
  }
})
onBeforeUnmount(() => {
  resizeObserver?.disconnect()
})
</script>

<template>
  <div class="tabbar-wrap">
    <button
      v-show="canScrollLeft"
      class="tabbar-arrow tabbar-arrow--left"
      type="button"
      aria-label="Scroll tabs left"
      @click="scrollBy(-1)"
    >
      <ChevronLeft :size="16" :stroke-width="2" />
    </button>
    <div ref="scroller" class="tabbar" @scroll.passive="onScroll">
      <div
        v-for="tab in tabs"
        :key="tab.path"
        class="tab"
        :class="{ active: tab.path === activeTabPath }"
        :title="tab.path"
        @click="switchTo(tab.path)"
        @mouseup="handleMiddleClick(tab.path, $event)"
      >
        <span class="tab-dot" :class="{ 'is-unread': tab.hasUnread && tab.path !== activeTabPath }"></span>
        <span class="tab-name">{{ basename(tab.path) }}</span>
        <button class="tab-close" @click="handleClose(tab.path, $event)">✕</button>
      </div>
    </div>
    <button
      v-show="canScrollRight"
      class="tabbar-arrow tabbar-arrow--right"
      type="button"
      aria-label="Scroll tabs right"
      @click="scrollBy(1)"
    >
      <ChevronRight :size="16" :stroke-width="2" />
    </button>
  </div>
</template>

<style scoped>
.tabbar-wrap {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: flex-end;
  height: var(--tabbar-h);
}

.tabbar {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: flex-end;
  overflow-x: auto;
  scrollbar-width: none;
  height: 100%;
  /* Accommodate the active tab's outward "ear" pseudo-elements (±--radius).
     Without this, the first/last tab's outer ear is clipped by overflow-x. */
  padding: 0 var(--radius);
}

.tabbar::-webkit-scrollbar {
  display: none;
}

.tabbar-arrow {
  flex-shrink: 0;
  /* Match the tab's geometry (height 40px + flex-end) so the arrow's icon
     center aligns with the tab content center, rather than centering against
     the full --tabbar-h strip (which left the arrow ~5px higher than the
     tab text). Bottom inset mirrors the tab's 5px content padding. */
  align-self: flex-end;
  width: 22px;
  height: 40px;
  padding: 0 0 5px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  transition: color .12s;
}

.tabbar-arrow:hover {
  color: var(--text);
}

/* Vertical separators between the arrows and the tab strip. Left arrow gets
   a right border (sits between arrow and tabs); right arrow gets both, since
   it also divides the tabs from the trailing globalbar controls. */
.tabbar-arrow--left {
  border-right: 1px solid var(--border);
}

.tabbar-arrow--right {
  border-left: 1px solid var(--border);
  border-right: 1px solid var(--border);
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
  font-size: 13px;
  color: var(--text-3);
  cursor: pointer;
  transition: color .12s, background .12s;
  user-select: none;
  white-space: nowrap;
  position: relative;
  /* anchor to bottom of the bar so the active tab grows upward to meet content */
  align-self: flex-end;
  height: 40px;
  /* Bottom padding matches the hover-pill's bottom inset (5px), so content
     centers in the pill region (top 0 → bottom 5) rather than the full height.
     Result: content sits in the pill's vertical center; on the active tab the
     fill spans the full height, so content reads as nudged slightly up. */
  padding: 0 12px 5px;
  flex-shrink: 0;
  border-radius: var(--radius) var(--radius) 0 0;
  background: transparent;
}

.tab:hover:not(.active) {
  color: var(--text-2);
}

/* Hover highlight is an inset rounded "pill" (Chrome-style), not a full-bleed
   fill: left/right/bottom margins keep the bar's recessed color showing so the
   highlight reads as a floating rectangle. The top is flush (0) so the pill's top
   edge aligns with the active tab's top. Scoped to non-active tabs via
   :not(.active) so it never collides with the active tab's ear pseudo-elements. */
.tab:not(.active)::before {
  content: '';
  position: absolute;
  top: 0;
  right: 5px;
  bottom: 5px;
  left: 5px;
  border-radius: var(--radius);
  background: transparent;
  z-index: 0;
  pointer-events: none;
  transition: background .12s;
}

.tab:not(.active):hover::before {
  background: var(--bg-3);
}

/* Tab content sits above the hover-highlight layer */
.tab > * {
  position: relative;
  z-index: 1;
}

.tab.active {
  color: var(--text);
  background: var(--bg);
  z-index: 2;
}

/* ★ Chrome-style "ears": radial-gradient circles sit just outside each bottom
   corner of the active tab. Only the inner quarter is visible (the rest is
   clipped by overflow), so it looks like the white tab splays outward at the
   base. Color comes from --bg, so it tracks theme automatically. */
.tab.active::before,
.tab.active::after {
  content: '';
  position: absolute;
  bottom: 0;
  width: var(--radius);
  height: var(--radius);
  pointer-events: none;
}

.tab.active::before {
  left: calc(-1 * var(--radius));
  background: radial-gradient(circle at top left, transparent calc(var(--radius) - 1px), var(--bg) calc(var(--radius) - 1px));
}

.tab.active::after {
  right: calc(-1 * var(--radius));
  background: radial-gradient(circle at top right, transparent calc(var(--radius) - 1px), var(--bg) calc(var(--radius) - 1px));
}

.tab-close {
  width: 16px;
  height: 16px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 3px;
  opacity: 0.4;
  transition: opacity .12s, background .12s, color .12s;
  font-size: 14px;
  line-height: 1;
}

.tab:hover .tab-close {
  opacity: 1;
}

.tab.active .tab-close {
  opacity: 0.7;
}

.tab-close:hover {
  background: var(--c-error-bg, var(--bg-3));
  color: var(--c-error-text, var(--text));
}

/* The dot always reserves its slot (no v-if) so unread state toggling doesn't
   shift the tab width. Only opacity changes when there's actually unread content. */
.tab-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--c-unread, #5B9DFF);
  flex-shrink: 0;
  opacity: 0;
}

.tab-dot.is-unread {
  opacity: 1;
}

.tab-name {
  font-weight: 500;
}
</style>
