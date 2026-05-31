<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { listFiles } from '../services/api'
import type { FileEntry } from '../services/api'

const emit = defineEmits<{
  select: [path: string]
  collapse: []
}>()

const props = defineProps<{
  selectedFile: string | null
}>()

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

const filteredTree = computed(() => {
  const q = filterText.value.trim().toLowerCase()
  if (!q) return tree.value
  return tree.value
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

async function refresh(): Promise<void> {
  loading.value = true
  try {
    const entries = await listFiles()
    tree.value = entriesToTree(entries)
  } catch (e) {
    console.error('Failed to load files:', e)
  } finally {
    loading.value = false
  }
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
          placeholder="Filter files…"
          class="filter-input"
        />
        <button v-if="filterText" class="filter-clear" @click="filterText = ''">✕</button>
      </div>
      <div class="sidebar-actions">
        <button class="icon-btn" @click="refresh" title="Refresh">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M1 4v6h6"/><path d="M23 20v-6h-6"/>
            <path d="M20.49 9A9 9 0 0 0 5.64 5.64L1 10m22 4-4.64 4.36A9 9 0 0 1 3.51 15"/>
          </svg>
        </button>
        <button class="icon-btn" @click="emit('collapse')" title="Collapse">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="11 17 6 12 11 7"/><polyline points="18 17 13 12 18 7"/>
          </svg>
        </button>
      </div>
    </div>
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
          <div v-if="!node.isDir" class="file-dot" :class="props.selectedFile === node.path ? 'live' : 'off'"></div>
          <div v-else class="file-dir-icon">
            <svg v-if="node.expanded" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
            <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
          </div>
          <div class="file-meta">
            <div class="file-name">{{ node.name }}</div>
            <div v-if="!node.isDir && node.size !== undefined" class="file-size">{{ formatSize(node.size) }}</div>
          </div>
        </div>
        <template v-if="node.isDir && node.expanded">
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
            <div v-if="!child.isDir" class="file-dot" :class="props.selectedFile === child.path ? 'live' : 'off'"></div>
            <div v-else class="file-dir-icon">
              <svg v-if="child.expanded" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
              <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
            </div>
            <div class="file-meta">
              <div class="file-name">{{ child.name }}</div>
              <div v-if="!child.isDir && child.size !== undefined" class="file-size">{{ formatSize(child.size) }}</div>
            </div>
          </div>
        </template>
      </template>
    </div>
    <div v-else-if="loading" class="file-empty">Loading…</div>
    <div v-else-if="filterText" class="file-empty">No matching files</div>
    <div v-else class="file-empty">No files found</div>
  </div>
</template>

<style scoped>
.file-browser {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.sidebar-header {
  height: 44px;
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
  height: 30px;
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
  font-size: 11px;
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

.file-list {
  padding: 8px;
  overflow-y: auto;
  flex: 1;
}

.file-item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 9px 10px;
  border-radius: 8px;
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

.file-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
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
  font-size: 13px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-size {
  font-size: 11px;
  color: var(--text-3);
  margin-top: 2px;
}

.file-empty {
  padding: 16px 12px;
  color: var(--text-3);
  font-size: 13px;
  text-align: center;
  flex: 1;
}
</style>
