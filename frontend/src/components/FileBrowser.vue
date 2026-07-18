<script lang="ts">
/// Shared tree node type used by FileBrowser and the recursive FileTreeNode.
/// Declared in a plain <script> block so it can be `export`ed (a <script setup>
/// block cannot export named types).
export interface TreeNode {
  name: string
  path: string
  isDir: boolean
  size?: number
  modified?: string
  children: TreeNode[]
  expanded: boolean
  loaded: boolean
  /** True while this directory's children are being fetched (inline spinner). */
  loading: boolean
}
</script>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { listFiles } from '../services/api'
import type { FileEntry } from '../services/api'
import { useHistoricalFilter } from '../composables/useHistoricalFilter'
import { useRecentFiles } from '../composables/useRecentFiles'
import { Search, ChevronDown, RefreshCw, Eye, EyeOff } from 'lucide-vue-next'
import FileTreeNode from './FileTreeNode.vue'

const { t } = useI18n()
const { showHistorical, isHistoricalFile, toggle: toggleHistorical } = useHistoricalFilter()
const { recentFiles, remove: removeRecent } = useRecentFiles()

const recentCollapsed = ref(recentFiles.value.length === 0)

function formatRelativeTime(ts: number): string {
  const diff = Date.now() - ts
  const min = Math.floor(diff / 60000)
  if (min < 1) return t('fileBrowser.justNow')
  if (min < 60) return `${min} ${t('fileBrowser.minAgo')}`
  const hr = Math.floor(min / 60)
  if (hr < 24) return `${hr} ${t('fileBrowser.hourAgo')}`
  const days = Math.floor(hr / 24)
  if (days === 1) return t('fileBrowser.yesterday')
  if (days < 7) return `${days} ${t('fileBrowser.daysAgo')}`
  return new Date(ts).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

function basename(path: string): string {
  const parts = path.split('/')
  return parts[parts.length - 1] || path
}

const emit = defineEmits<{
  select: [path: string]
  resize: [width: number]
}>()

const props = defineProps<{
  selectedFile: string | null
  width?: number
  refreshKey?: number
}>()

watch(() => props.refreshKey, (newVal, oldVal) => {
  if (newVal !== undefined && oldVal !== undefined && newVal !== oldVal) {
    refresh()
  }
})

const MIN_WIDTH = 180
const MAX_WIDTH = 400

const tree = ref<TreeNode[]>([])
const loading = ref(false)
const filterText = ref('')
const isDragging = ref(false)
const dragStartX = ref(0)
const dragStartWidth = ref(0)

/// Recursively drop historical (logrotate) files unless the toggle is on.
/// Must walk the whole depth so a rotated file nested several levels down is
/// still hidden — the old single-level filter missed those.
function applyHistoricalFilter(node: TreeNode): TreeNode | null {
  if (!showHistorical.value && !node.isDir && isHistoricalFile(node.name)) {
    return null
  }
  if (!node.isDir) return node
  const children = node.children
    .map(applyHistoricalFilter)
    .filter((n): n is TreeNode => n !== null)
  return { ...node, children }
}

/// Recursively filter the tree by search query. A dir survives if it matches
/// or any descendant matches; matching dirs are forced expanded.
function applySearchFilter(node: TreeNode, q: string): TreeNode | null {
  const selfMatch = node.name.toLowerCase().includes(q)
  if (!node.isDir) return selfMatch ? node : null
  const children = node.children
    .map((c) => applySearchFilter(c, q))
    .filter((n): n is TreeNode => n !== null)
  if (selfMatch || children.length > 0) {
    return { ...node, children, expanded: true }
  }
  return null
}

const filteredTree = computed(() => {
  const q = filterText.value.trim().toLowerCase()
  // Historical filter first (depth-aware), then search.
  let base = tree.value
  if (!showHistorical.value) {
    base = base.map(applyHistoricalFilter).filter((n): n is TreeNode => n !== null)
  }
  if (!q) return base
  return base
    .map((n) => applySearchFilter(n, q))
    .filter((n): n is TreeNode => n !== null)
})

const filteredRecentFiles = computed(() => {
  const q = filterText.value.trim().toLowerCase()
  if (!q) return recentFiles.value
  return recentFiles.value.filter((rf) =>
    basename(rf.path).toLowerCase().includes(q)
  )
})

function entriesToTree(entries: FileEntry[]): TreeNode[] {
  return entries
    .sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1
      return a.name.localeCompare(b.name)
    })
    .map((e) => entryToNode(e))
}

/// Convert a backend FileEntry to a TreeNode. Directories that already carry
/// children (from a recursive `?depth=N` listing) are marked loaded so the
/// pre-fetched subtree is available instantly on expand — but they start
/// collapsed. Preload is about speed, not flooding the view: 3 levels of data
/// are fetched, yet the user still expands directories on demand.
/// Empty-child directories stay unloaded for lazy expansion.
function entryToNode(e: FileEntry): TreeNode {
  const hasChildren = !!e.isDir && !!e.children && e.children.length > 0
  return {
    name: e.name,
    path: e.path,
    isDir: e.isDir,
    size: e.size,
    modified: e.modified,
    // Pre-fetched children → already loaded; otherwise lazy (load on expand).
    children: hasChildren ? e.children!.map(entryToNode) : [],
    // Collapsed by default — preload gives instant expand, not auto-expansion.
    expanded: false,
    loaded: hasChildren,
    loading: false,
  }
}

/**
 * Find the original TreeNode in `tree.value` by path.
 *
 * `filteredTree` creates shallow copies (`{ ...node }`) for rendering.
 * Those copies must NEVER be mutated — `toggleDir` / `loadChildren` would
 * write to the copy, bypassing the computed filter and leaking unfiltered
 * children into the view on the next re-render. Always look up the original.
 */
function findOriginalNode(path: string): TreeNode | null {
  function search(nodes: TreeNode[]): TreeNode | null {
    for (const n of nodes) {
      if (n.path === path) return n
      if (n.isDir) {
        const found = search(n.children)
        if (found) return found
      }
    }
    return null
  }
  return search(tree.value)
}

/// Default recursive depth for listings. The root load and each lazy dir
/// expansion fetch 3 levels so typical log trees are visible immediately,
/// while deeper nesting still expands on demand.
const LIST_DEPTH = 3

async function loadChildren(node: TreeNode): Promise<void> {
  if (node.loaded) return
  node.loading = true
  try {
    const entries = await listFiles(node.path, LIST_DEPTH)
    node.children = entriesToTree(entries)
    node.loaded = true
  } catch (e) {
    console.error('Failed to load directory:', e)
  } finally {
    node.loading = false
  }
}

async function toggleDir(node: TreeNode): Promise<void> {
  if (!node.isDir) return
  if (!node.loaded) {
    await loadChildren(node)
  }
  node.expanded = !node.expanded
}

/// Handle a directory toggle emitted from FileTreeNode. The emitted node may be
/// a shallow copy from `filteredTree`; always operate on the original in `tree`
/// so mutations (expand/loaded/children) persist and don't leak past the filter.
function onNodeToggle(node: TreeNode): void {
  if (!node.isDir) return
  const original = findOriginalNode(node.path)
  if (original) {
    toggleDir(original)
  }
}

function refresh(): void {
  loading.value = true
  listFiles(undefined, LIST_DEPTH)
    .then((entries) => {
      tree.value = entriesToTree(entries)
    })
    .catch((e) => {
      console.error('Failed to load files:', e)
    })
    .finally(() => {
      loading.value = false
    })
}

function onDragStart(event: MouseEvent): void {
  isDragging.value = true
  dragStartX.value = event.clientX
  dragStartWidth.value = props.width ?? 220
  document.addEventListener('mousemove', onDragMove)
  document.addEventListener('mouseup', onDragEnd)
  document.body.style.cursor = 'col-resize'
  document.body.style.userSelect = 'none'
}

function onDragMove(event: MouseEvent): void {
  if (!isDragging.value) return
  const delta = event.clientX - dragStartX.value
  const newWidth = Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, dragStartWidth.value + delta))
  emit('resize', newWidth)
}

function onDragEnd(): void {
  isDragging.value = false
  document.removeEventListener('mousemove', onDragMove)
  document.removeEventListener('mouseup', onDragEnd)
  document.body.style.cursor = ''
  document.body.style.userSelect = ''
}

onUnmounted(() => {
  document.removeEventListener('mousemove', onDragMove)
  document.removeEventListener('mouseup', onDragEnd)
})

/** Ensure the given file path is visible in the tree: expand parent dirs as needed. */
async function ensureVisible(filePath: string): Promise<void> {
  await expandToPath(tree.value, filePath)
}

/** Recursively expand directories along the path to filePath. */
async function expandToPath(nodes: TreeNode[], filePath: string): Promise<void> {
  for (const node of nodes) {
    if (!node.isDir) continue
    // Check if target file lives inside this directory
    if (filePath.startsWith(node.path + '/')) {
      if (!node.loaded) await loadChildren(node)
      if (!node.expanded) node.expanded = true
      // Recurse into children to find deeper directories
      await expandToPath(node.children, filePath)
      return
    }
  }
}

defineExpose({ ensureVisible })

onMounted(() => {
  refresh()
})
</script>

<template>
  <div class="file-browser">
    <div class="sidebar-header">
      <div class="filter-wrap">
        <Search class="filter-icon" :size="14" :stroke-width="2" />
        <input
          v-model="filterText"
          type="text"
          :placeholder="t('fileBrowser.filterFiles')"
          class="filter-input"
        />
        <button v-if="filterText" class="filter-clear" @click="filterText = ''">✕</button>
      </div>
    </div>
    <div class="nav-scroll">
      <div class="quick-access">
        <div class="nav-section">
          <div class="section-header" @click="recentCollapsed = !recentCollapsed">
            <span class="section-title">{{ t('fileBrowser.recent') }}</span>
            <div class="section-chevron" :class="{ collapsed: recentCollapsed }">
              <ChevronDown :size="14" :stroke-width="2.5" />
            </div>
          </div>
          <div v-show="!recentCollapsed || (filterText && filteredRecentFiles.length > 0)" class="section-body">
            <div
              v-for="rf in filteredRecentFiles"
              :key="rf.path"
              class="nav-item"
              :title="rf.path"
              @click="emit('select', rf.path)"
            >
              <span class="nav-text">{{ basename(rf.path) }}</span>
              <span class="nav-time">{{ formatRelativeTime(rf.openedAt) }}</span>
              <button class="nav-remove" @click.stop="removeRecent(rf.path)">✕</button>
            </div>
          </div>
        </div>
      </div>

      <div class="nav-section files-section">
        <div class="section-header">
          <span class="section-title">{{ t('fileBrowser.files') }}</span>
          <div class="section-actions" @click.stop>
            <button
              class="section-icon-btn"
              @click="toggleHistorical"
              :title="showHistorical ? t('fileBrowser.hideHistory') : t('fileBrowser.showHistory')"
            >
              <Eye v-if="showHistorical" :size="17" :stroke-width="2" />
              <EyeOff v-else :size="17" :stroke-width="2" />
            </button>
            <button class="section-icon-btn" @click="refresh" :title="t('fileBrowser.refresh')">
              <RefreshCw :size="14" :stroke-width="2" />
            </button>
          </div>
        </div>
        <div class="files-body">
          <div class="file-list" v-if="filteredTree.length > 0">
            <FileTreeNode
              v-for="node in filteredTree"
              :key="node.path"
              :node="node"
              :selected-file="props.selectedFile"
              :level="0"
              @select="emit('select', $event)"
              @toggle="onNodeToggle"
            />
          </div>
          <div v-else-if="loading" class="file-empty">{{ t('fileBrowser.loading') }}</div>
          <div v-else-if="filterText" class="file-empty">{{ t('fileBrowser.noMatchingFiles') }}</div>
          <div v-else class="file-empty">{{ t('fileBrowser.noFilesFound') }}</div>
        </div>
      </div>
    </div>
    <div
      class="resize-handle"
      :class="{ active: isDragging }"
      @mousedown.prevent="onDragStart"
    ></div>
  </div>
</template>

<style scoped>
.file-browser {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.sidebar-header {
  height: var(--tabbar-h);
  padding: 0 8px;
  display: flex;
  align-items: center;
  gap: 6px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.filter-wrap {
  flex: 1;
  min-width: 0;
  position: relative;
  display: flex;
  align-items: center;
}

.filter-icon {
  position: absolute;
  left: 8px;
  color: var(--text-3);
  pointer-events: none;
}

.filter-input {
  width: 100%;
  height: 36px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm, 6px);
  background: var(--bg);
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--text);
  padding: 0 26px 0 28px;
  outline: none;
  transition: border-color .15s ease;
}

.filter-input:focus {
  border-color: var(--accent, var(--border-2));
}

.filter-input::placeholder {
  color: var(--text-3);
}

.filter-clear {
  position: absolute;
  right: 4px;
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  padding: 0;
  border-radius: 4px;
}

.filter-clear:hover {
  background: var(--bg-3);
  color: var(--text);
}

.sidebar-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
}

/* ── Scroll container: quick-access card on top, Files tree fills the rest ── */
.nav-scroll {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  padding: 8px;
  gap: 8px;
}

.quick-access {
  flex-shrink: 0;
  max-height: 44%;
  overflow-y: auto;
  scrollbar-width: none;
}

.quick-access .nav-section {
  padding: 4px 0;
}

/* Files: the primary browse surface — no card, takes all remaining height */
.files-section {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 0;
}

.files-section .section-header {
  flex-shrink: 0;
  border-top: 1px solid var(--border);
}

.files-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.files-body .file-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  scrollbar-width: none;
}

.files-body .file-empty {
  flex: 1;
}

.nav-section {
  padding: 4px 0;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  cursor: pointer;
  user-select: none;
  transition: background .1s ease;
  height: 35px;
}

.section-chevron {
  width: 14px;
  height: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-3);
  flex-shrink: 0;
  margin-left: auto;
  transition: transform .15s ease;
}

.section-chevron.collapsed {
  transform: rotate(-90deg);
}

.section-title {
  font-size: 14px;
  color: var(--text-3);
}

.section-actions {
  margin-left: auto;
  display: flex;
  gap: 4px;
}

.section-icon-btn {
  width: 24px;
  height: 24px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm, 6px);
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: background .12s ease, color .12s ease, border-color .12s ease;
}

.section-icon-btn:hover {
  background: var(--bg-3);
  color: var(--text);
}

.section-icon-btn.active {
  background: var(--accent-light, var(--bg-2));
  color: var(--accent);
  border-color: var(--accent);
}

.section-body {
  max-height: 160px;
  overflow-y: auto;
  scrollbar-width: none;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius, 8px);
  cursor: pointer;
  transition: background .1s ease;
  user-select: none;
  position: relative;
  height: 40px;
}

.nav-item:hover {
  background: var(--bg-3);
}

.nav-item:hover .nav-remove {
  opacity: 1;
}

/* 互斥显示：默认 time 靠最右，hover 时让位给 remove 按钮 */
.nav-item:hover .nav-time {
  opacity: 0;
}

.nav-text {
  flex: 1;
  min-width: 0;
  font-size: 14px;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.nav-time {
  font-size: 12px;
  color: var(--text-2);
  flex-shrink: 0;
  transition: opacity .15s ease;
}

.nav-remove {
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 4px;
  opacity: 0;
  transition: opacity .15s ease, color .12s ease, background .12s ease;
  flex-shrink: 0;
  font-size: 12px;
  line-height: 1;
  /* 脱离文档流，浮在 nav-item 最右端；与 nav-time 互斥切换 */
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
}

.nav-remove:hover {
  background: var(--border-2);
  color: var(--text);
}

/* File rows / copy buttons / dir icons now live in FileTreeNode.vue (rendered
   recursively). Only the empty-state + container classes remain here. */

.file-empty {
  padding: 16px 12px;
  color: var(--text-3);
  font-size: 12px;
  text-align: center;
  flex: 1;
}

.resize-handle {
  position: absolute;
  top: 0;
  right: 0;
  width: 3px;
  height: 100%;
  cursor: col-resize;
  background: transparent;
  z-index: 10;
}

.resize-handle:hover,
.resize-handle.active {
  background: var(--accent);
  opacity: 0.5;
}

/* Hide scrollbar for all scrollable containers (Firefox + WebKit) */
.nav-scroll::-webkit-scrollbar,
.nav-quick-card::-webkit-scrollbar,
.quick-list::-webkit-scrollbar,
.bm-body::-webkit-scrollbar {
  display: none;
}
</style>