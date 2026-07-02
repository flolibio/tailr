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
  collapse: []
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
              children: node.children.filter(
                (c) => c.isDir || !isHistoricalFile(c.name) || props.selectedFile === c.path
              ),
            }
          }
          if (isHistoricalFile(node.name) && props.selectedFile !== node.path) {
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
    toggleDir(node)
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
      <div class="sidebar-actions">
        <button class="icon-btn" @click="emit('collapse')" :title="t('fileBrowser.collapse')">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="11 17 6 12 11 7"/><polyline points="18 17 13 12 18 7"/>
          </svg>
        </button>
      </div>
    </div>
    <div class="nav-scroll">
      <div class="nav-section">
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

      <div class="nav-section">
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
        <div v-show="!filesCollapsed">
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
            <div v-if="!node.isDir" class="file-icon-spacer"></div>
            <div v-else class="file-dir-icon">
              <svg v-if="node.expanded" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
              <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
            </div>
            <div class="file-meta">
              <div class="file-name">{{ node.name }}</div>
              <div v-if="!node.isDir && node.size !== undefined" class="file-size">{{ formatSize(node.size) }}</div>
            </div>
            <button v-if="!node.isDir" class="star-btn" :class="{ favorited: isFavorite(node.path) }" @click.stop="toggleFavorite(node.path)" :title="t('fileBrowser.toggleFavorite')">
              <svg v-if="isFavorite(node.path)" width="12" height="12" viewBox="0 0 24 24" fill="currentColor" stroke="none"><polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26"/></svg>
              <svg v-else width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26"/></svg>
            </button>
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
              <div v-if="!child.isDir" class="file-icon-spacer"></div>
              <div v-else class="file-dir-icon">
                <svg v-if="child.expanded" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
                <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
              </div>
              <div class="file-meta">
                <div class="file-name">{{ child.name }}</div>
                <div v-if="!child.isDir && child.size !== undefined" class="file-size">{{ formatSize(child.size) }}</div>
              </div>
              <button v-if="!child.isDir" class="star-btn" :class="{ favorited: isFavorite(child.path) }" @click.stop="toggleFavorite(child.path)" :title="t('fileBrowser.toggleFavorite')">
                <svg v-if="isFavorite(child.path)" width="12" height="12" viewBox="0 0 24 24" fill="currentColor" stroke="none"><polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26"/></svg>
                <svg v-else width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="12,2 15.09,8.26 22,9.27 17,14.14 18.18,21.02 12,17.77 5.82,21.02 7,14.14 2,9.27 8.91,8.26"/></svg>
              </button>
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
  height: 36px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-2);
  font-family: var(--font-sans);
  font-size: 12px;
  color: var(--text);
  padding: 0 26px 0 28px;
  outline: none;
  transition: border-color .15s;
}

.filter-input:focus {
  border-color: var(--border-2);
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

.nav-scroll {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
}

.nav-section {
  padding: 4px 0;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 12px;
  cursor: pointer;
  user-select: none;
  transition: background .1s;
}

.section-header:hover {
  background: var(--bg-2);
}

.section-chevron {
  width: 14px;
  height: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-3);
  flex-shrink: 0;
  transition: transform .15s;
}

.section-chevron.collapsed {
  transform: rotate(-90deg);
}

.section-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
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
  border-radius: 5px;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: background .12s, color .12s, border-color .12s;
}

.section-icon-btn:hover {
  background: var(--bg-3);
  color: var(--text);
}

.section-icon-btn.active {
  background: var(--bg-2);
  color: var(--accent);
  border-color: var(--border-2);
}

.section-body {
  padding: 2px 8px;
  max-height: 200px;
  overflow-y: auto;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 10px;
  border-radius: 10px;
  cursor: pointer;
  transition: background .1s;
  user-select: none;
}

.nav-item:hover {
  background: var(--bg-2);
}

.nav-item:hover .nav-remove {
  opacity: 1;
}

.nav-text {
  flex: 1;
  min-width: 0;
  font-size: 14px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.nav-time {
  font-size: 12px;
  color: var(--text-3);
  flex-shrink: 0;
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
  transition: opacity .15s, color .12s, background .12s;
  flex-shrink: 0;
  font-size: 14px;
  line-height: 1;
}

.nav-remove:hover {
  background: var(--bg-3);
  color: var(--text);
}

.file-list {
  padding: 8px;
}

.file-item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 9px 10px;
  border-radius: 10px;
  cursor: pointer;
  transition: background .1s;
  user-select: none;
}

.file-item:hover {
  background: var(--bg-2);
}

.file-item.is-selected {
  background: var(--bg-2);
}

.file-item.is-selected .file-name {
  color: var(--text);
  font-weight: 500;
}

.file-item.child {
  padding-left: 24px;
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
  border-radius: 4px;
  opacity: 0;
  transition: opacity .15s, color .12s, background .12s;
  flex-shrink: 0;
}

.copy-path-btn:hover {
  background: var(--bg-3);
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
  border-radius: 4px;
  opacity: 0;
  transition: opacity .15s, color .12s, background .12s;
  flex-shrink: 0;
}

.star-btn:hover {
  background: var(--bg-3);
  color: var(--c-star, #D4A017);
}

.star-btn.favorited {
  opacity: 1;
  color: var(--c-star, #D4A017);
}

.file-icon-spacer {
  width: 18px;
  flex-shrink: 0;
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

.file-meta {
  flex: 1;
  min-width: 0;
}

.file-name {
  font-size: 14px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-size {
  font-size: 12px;
  color: var(--text-3);
  margin-top: 2px;
}

.file-empty {
  padding: 16px 12px;
  color: var(--text-3);
  font-size: 14px;
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
  width: 6px;
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
</style>
