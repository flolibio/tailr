<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { listFiles } from '../services/api'
import type { FileEntry } from '../services/api'

const emit = defineEmits<{
  select: [path: string]
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

onMounted(async () => {
  loading.value = true
  try {
    const entries = await listFiles()
    tree.value = entriesToTree(entries)
  } catch (e) {
    console.error('Failed to load files:', e)
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="file-browser">
    <div class="sidebar-header">
      <span>Files</span>
      <button @click="onMounted" title="Refresh">↻</button>
    </div>
    <div class="file-tree" v-if="tree.length > 0">
      <template v-for="node in tree" :key="node.path">
        <div
          class="tree-node"
          :class="{
            'is-dir': node.isDir,
            'is-file': !node.isDir,
            'is-selected': !node.isDir && props.selectedFile === node.path,
          }"
          @click="selectFile(node)"
        >
          <span class="node-icon">{{ node.isDir ? (node.expanded ? '📂' : '📁') : '📄' }}</span>
          <span class="node-name">{{ node.name }}</span>
          <span v-if="!node.isDir && node.size !== undefined" class="node-size">{{
            formatSize(node.size)
          }}</span>
        </div>
        <template v-if="node.isDir && node.expanded">
          <div
            v-for="child in node.children"
            :key="child.path"
            class="tree-node child"
            :class="{
              'is-dir': child.isDir,
              'is-file': !child.isDir,
              'is-selected': !child.isDir && props.selectedFile === child.path,
            }"
            @click="selectFile(child)"
          >
            <span class="node-icon">{{
              child.isDir ? (child.expanded ? '📂' : '📁') : '📄'
            }}</span>
            <span class="node-name">{{ child.name }}</span>
            <span v-if="!child.isDir && child.size !== undefined" class="node-size">{{
              formatSize(child.size)
            }}</span>
          </div>
        </template>
      </template>
    </div>
    <div v-else-if="loading" class="file-empty">Loading...</div>
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

.file-tree {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}

.tree-node {
  display: flex;
  align-items: center;
  padding: 3px 12px;
  cursor: pointer;
  user-select: none;
  gap: 6px;
  font-size: 13px;
  white-space: nowrap;
}

.tree-node:hover {
  background: var(--bg-hover);
}

.tree-node.is-selected {
  background: var(--bg-selected);
}

.tree-node.child {
  padding-left: 28px;
}

.node-icon {
  font-size: 14px;
  flex-shrink: 0;
  width: 18px;
  text-align: center;
}

.node-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.node-size {
  font-size: 11px;
  color: var(--text-muted);
  flex-shrink: 0;
}

.file-empty {
  padding: 16px 12px;
  color: var(--text-muted);
  font-size: 13px;
  text-align: center;
}
</style>
