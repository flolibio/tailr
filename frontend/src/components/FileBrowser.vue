<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { listFiles } from '../services/api'
import type { FileEntry } from '../services/api'
import { useHistoricalFilter } from '../composables/useHistoricalFilter'
import { useFavoriteFiles } from '../composables/useFavoriteFiles'
import { useRecentFiles } from '../composables/useRecentFiles'

const { t } = useI18n()
const { showHistorical, isHistoricalFile, toggle: toggleHistorical } = useHistoricalFilter()
const { favoriteFiles, isFavorite, toggle: toggleFavorite } = useFavoriteFiles()
const { recentFiles, remove: removeRecent } = useRecentFiles()

const favCollapsed = ref(favoriteFiles.value.length === 0)
const recentCollapsed = ref(recentFiles.value.length === 0)
const filesCollapsed = ref(false)

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

interface TreeNode {
  name: string
  path: string
  isDir: boolean
  size?: number
  modified?: string
  children: TreeNode[]
  expanded: boolean
  loaded: boolean
}

const tree = ref<TreeNode[]>([])
const loading = ref(false)
const filterText = ref('')
const copiedPath = ref<string | null>(null)
const isDragging = ref(false)
const dragStartX = ref(0)
const dragStartWidth = ref(0)

const filteredTree = computed(() => {
  const q = filterText.value.trim().toLowerCase()
  const hideHist = !showHistorical.value

  // Apply historical file filter (hide logrotate files unless toggle is on)
  const baseTree = hideHist
    ? tree.value
        .map((node) => {
          if (node.isDir) {
            return {
              ...node,
              children: node.children.filter((c) => c.isDir || !isHistoricalFile(c.name)),
            }
          }
          if (isHistoricalFile(node.name)) {
            return null
          }
          return node
        })
        .filter(Boolean) as TreeNode[]
    : tree.value

  if (!q) return baseTree
  return baseTree
    .map((node) => {
      if (node.isDir) {
        const filteredChildren = node.children.filter((c) =>
          c.name.toLowerCase().includes(q)
        )
        if (filteredChildren.length > 0 || node.name.toLowerCase().includes(q)) {
          return { ...node, children: filteredChildren, expanded: true }
        }
        return null
      }
      return node.name.toLowerCase().includes(q) ? node : null
    })
    .filter(Boolean) as TreeNode[]
})

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}

function entriesToTree(entries: FileEntry[]): TreeNode[] {
  return entries
    .sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1
      return a.name.localeCompare(b.name)
    })
    .map((e) => ({
      name: e.name,
      path: e.path,
      isDir: e.isDir,
      size: e.size,
      modified: e.modified,
      children: [],
      expanded: false,
      loaded: false,
    }))
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

async function loadChildren(node: TreeNode): Promise<void> {
  if (node.loaded) return
  loading.value = true
  try {
    const entries = await listFiles(node.path)
    node.children = entriesToTree(entries)
    node.loaded = true
  } catch (e) {
    console.error('Failed to load directory:', e)
  } finally {
    loading.value = false
  }
}

async function toggleDir(node: TreeNode): Promise<void> {
  if (!node.isDir) return
  if (!node.loaded) {
    await loadChildren(node)
  }
  node.expanded = !node.expanded
}

function selectFile(node: TreeNode): void {
  if (node.isDir) {
    // Always operate on the original tree node — the `node` received from the
    // template may be a shallow copy produced by `filteredTree`, and mutating
    // it would bypass the historical-file filter (children set on the copy are
    // unfiltered, and they leak into view on the next re-render).
    const original = findOriginalNode(node.path)
    if (original) {
      toggleDir(original)
    }
  } else {
    emit('select', node.path)
  }
}

function refresh(): void {
  loading.value = true
  listFiles()
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

async function copyPath(path: string, event: MouseEvent): Promise<void> {
  event.stopPropagation()
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(path)
    } else {
      const textarea = document.createElement('textarea')
      textarea.value = path
      textarea.style.position = 'fixed'
      textarea.style.left = '-9999px'
      document.body.appendChild(textarea)
      textarea.select()
      document.execCommand('copy')
      document.body.removeChild(textarea)
    }
    copiedPath.value = path
    setTimeout(() => {
      if (copiedPath.value === path) copiedPath.value = null
    }, 1500)
  } catch {}
}

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
        <svg class="filter-icon" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
        </svg>
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
        <!-- TODO: temporarily hidden favorites section — re-enable when favorites feature is finalized -->
        <!-- <div class="nav-section">
          <div class="section-header" @click="favCollapsed = !favCollapsed">
            <div class="section-chevron" :class="{ collapsed: favCollapsed }">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
            </div>
            <span class="section-title">{{ t('fileBrowser.favorites') }}</span>
          </div>
          <div v-show="!favCollapsed" class="section-body">
            <div
              v-for="fav in favoriteFiles"
              :key="fav.path"
              class="nav-item"
              :title="fav.path"
              @click="emit('select', fav.path)"
            >
              <span class="nav-text">{{ basename(fav.path) }}</span>
              <button class="nav-remove" @click.stop="toggleFavorite(fav.path)">✕</button>
            </div>
          </div>
        </div>
        -->

        <div class="nav-section">
          <div class="section-header" @click="recentCollapsed = !recentCollapsed">
            <div class="section-chevron" :class="{ collapsed: recentCollapsed }">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
            </div>
            <span class="section-title">{{ t('fileBrowser.recent') }}</span>
          </div>
          <div v-show="!recentCollapsed" class="section-body">
            <div
              v-for="rf in recentFiles"
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
        <div class="section-header" @click="filesCollapsed = !filesCollapsed">
          <div class="section-chevron" :class="{ collapsed: filesCollapsed }">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
          </div>
          <span class="section-title">{{ t('fileBrowser.files') }}</span>
          <div class="section-actions" @click.stop>
            <button
              class="section-icon-btn"
              :class="{ active: showHistorical }"
              @click="toggleHistorical"
              :title="showHistorical ? t('fileBrowser.hideHistory') : t('fileBrowser.showHistory')"
            >
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
              </svg>
            </button>
            <button class="section-icon-btn" @click="refresh" :title="t('fileBrowser.refresh')">
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M1 4v6h6"/><path d="M23 20v-6h-6"/>
                <path d="M20.49 9A9 9 0 0 0 5.64 5.64L1 10m22 4-4.64 4.36A9 9 0 0 1 3.51 15"/>
              </svg>
            </button>
          </div>
        </div>
        <div v-show="!filesCollapsed" class="files-body">
          <div class="file-list" v-if="filteredTree.length > 0">
        <template v-for="node in filteredTree" :key="node.path">
          <div
            class="file-item"
            :class="{
              'is-dir': node.isDir,
              'is-selected': !node.isDir && props.selectedFile === node.path,
            }"
            @click="selectFile(node)"
          >
            <div v-if="!node.isDir" class="file-icon">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
            </div>
            <div v-else class="file-dir-icon">
              <svg v-if="node.expanded" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><path d="M2 10h20"/></svg>
              <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
            </div>
            <div class="file-meta">
              <div class="file-name">{{ node.name }}</div>
              <span v-if="!node.isDir && node.size != null" class="file-size">{{ formatSize(node.size) }}</span>
            </div>
            <!-- TODO: temporarily hidden favorite button — re-enable when favorites feature is finalized -->
            <!-- <button v-if="!node.isDir" class="star-btn" :class="{ favorited: isFavorite(node.path) }" @click.stop="toggleFavorite(node.path)" :title="t('fileBrowser.toggleFavorite')">
              <svg v-if="isFavorite(node.path)" width="16" height="16" viewBox="0 0 24 24" fill="currentColor" stroke="none"><polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26"/></svg>
              <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26"/></svg>
            </button> -->
            <button class="copy-path-btn" @click="copyPath(node.path, $event)" :title="t('fileBrowser.copyPath')">
              <svg v-if="copiedPath === node.path" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
              <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
            </button>
          </div>
          <template v-if="node.isDir && node.expanded">
            <div v-if="node.children.length === 0" class="file-empty-child">{{ t('fileBrowser.emptyDir') }}</div>
            <div
              v-for="child in node.children"
              :key="child.path"
              class="file-item child"
              :class="{
                'is-dir': child.isDir,
                'is-selected': !child.isDir && props.selectedFile === child.path,
              }"
              @click="selectFile(child)"
            >
              <div v-if="!child.isDir" class="file-icon">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
              </div>
              <div v-else class="file-dir-icon">
                <svg v-if="child.expanded" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><path d="M2 10h20"/></svg>
                <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
              </div>
              <div class="file-meta">
                <div class="file-name">{{ child.name }}</div>
                <span v-if="!child.isDir && child.size != null" class="file-size">{{ formatSize(child.size) }}</span>
              </div>
              <!-- TODO: temporarily hidden favorite button — re-enable when favorites feature is finalized -->
              <!-- <button v-if="!child.isDir" class="star-btn" :class="{ favorited: isFavorite(child.path) }" @click.stop="toggleFavorite(child.path)" :title="t('fileBrowser.toggleFavorite')">
                <svg v-if="isFavorite(child.path)" width="16" height="16" viewBox="0 0 24 24" fill="currentColor" stroke="none"><polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26"/></svg>
                <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26"/></svg>
              </button> -->
              <button class="copy-path-btn" @click="copyPath(child.path, $event)" :title="t('fileBrowser.copyPath')">
                <svg v-if="copiedPath === child.path" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
              </button>
            </div>
          </template>
        </template>
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
  height: var(--topbar-h);
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
  height: 34px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm, 6px);
  background: var(--bg-2);
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--text);
  padding: 0 26px 0 28px;
  outline: none;
  transition: border-color .15s ease, background .15s ease;
}

.filter-input:focus {
  border-color: var(--accent, var(--border-2));
  background: var(--bg);
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

/* Recessed card groups Favorites + Recent so they read as one secondary
   cluster, capped so they can never crowd out the Files tree below. */
.quick-access {
  flex-shrink: 0;
  max-height: 44%;
  overflow-y: auto;
  scrollbar-width: none;
  background: var(--bg-2);
  border-radius: var(--radius, 8px);
}

.quick-access .nav-section {
  padding: 2px 0;
}

.quick-access .nav-section + .nav-section {
  border-top: 1px solid var(--border);
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
  padding: 4px 6px;
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
  padding: 7px 10px;
  cursor: pointer;
  user-select: none;
  transition: background .1s ease;
}

.section-header:hover {
  background: var(--bg-3);
}

.section-chevron {
  width: 14px;
  height: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-3);
  flex-shrink: 0;
  transition: transform .15s ease;
}

.section-chevron.collapsed {
  transform: rotate(-90deg);
}

.section-title {
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.07em;
  text-transform: uppercase;
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
  padding: 2px 6px;
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
  font-size: 13px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.nav-time {
  font-size: 11px;
  color: var(--text-3);
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
  font-size: 13px;
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

.file-list {
  padding: 4px 6px;
}

.file-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius, 8px);
  cursor: pointer;
  transition: background .1s ease, box-shadow .1s ease;
  user-select: none;
  position: relative;
}

.file-item:hover {
  background: var(--bg-2);
}

/* Selected must read differently from hover — same background as hover
   made them indistinguishable before. Accent tint + a left rule fixes that. */
.file-item.is-selected {
  background: var(--accent-light, var(--bg-2));
}

.file-item.is-selected:hover {
  background: var(--accent-light, var(--bg-2));
}

.file-item.is-selected .file-name {
  color: var(--accent);
  font-weight: 500;
}

.file-item.child {
  padding-left: 24px;
  position: relative;
}

/* Indent guide: a hairline that ties a child row back to its parent folder */
.file-item.child::before {
  content: '';
  position: absolute;
  left: 13px;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--border);
}

.file-item:hover .copy-path-btn,
.file-item:hover .star-btn:not(.favorited) {
  opacity: 1;
}

.copy-path-btn {
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
  border-radius: var(--radius-sm, 6px);
  opacity: 0;
  transition: opacity .15s ease, color .12s ease, background .12s ease;
  flex-shrink: 0;
  /* 脱离文档流，让 file-meta(flex:1) 撑满，file-size 才能靠最右；
     与 file-size 互斥切换（同 nav-remove） */
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
}

.copy-path-btn:hover {
  background: var(--border-2);
  color: var(--text);
}

.star-btn {
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
  border-radius: var(--radius-sm, 6px);
  opacity: 0;
  transition: opacity .15s ease, color .12s ease, background .12s ease;
  flex-shrink: 0;
}

.star-btn:hover {
  background: var(--border-2);
  color: var(--c-star, #D4A017);
}

.star-btn.favorited {
  opacity: 1;
  color: var(--c-star, #D4A017);
}

.file-icon {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-3);
  align-self: center;
}

.file-item.is-selected .file-icon {
  color: var(--accent);
}

.file-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  align-self: flex-start;
  margin-top: 7px;
}

.file-dot.live {
  background: #1D9E75;
}

.file-dot.off {
  background: var(--border-2);
}

.file-dir-icon {
  width: 18px;
  height: 18px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-2);
  flex-shrink: 0;
}

.file-item.is-selected .file-dir-icon {
  color: var(--accent);
}

.file-meta {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
}

.file-name {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.4;
}

/* 同 nav-time/nav-remove 的互斥模式：默认 size 靠右显示，hover 时让位给 copy-path-btn */
.file-size {
  font-size: 11px;
  color: var(--text-3);
  flex-shrink: 0;
  transition: opacity .15s ease;
}

.file-item:hover .file-size {
  opacity: 0;
}

.file-empty {
  padding: 16px 12px;
  color: var(--text-3);
  font-size: 13px;
  text-align: center;
  flex: 1;
}

.file-empty-child {
  padding: 6px 10px 6px 52px;
  color: var(--text-3);
  font-size: 12px;
  font-style: italic;
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