<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { FolderOpen, Folder, File as FileIcon, Check, Copy, Loader2 } from 'lucide-vue-next'
import { useCopyFeedbackId } from '../composables/useClipboard'
import type { TreeNode } from './FileBrowser.vue'

const props = withDefaults(defineProps<{
  node: TreeNode
  selectedFile: string | null
  /** Indentation level (0 = top). Drives left padding + guide line. */
  level?: number
}>(), {
  level: 0,
})

const emit = defineEmits<{
  select: [path: string]
  toggle: [node: TreeNode]
}>()

const { t } = useI18n()
const { copiedId: copiedPath, copy: copyToText } = useCopyFeedbackId<string>()

function onSelect(): void {
  if (props.node.isDir) {
    emit('toggle', props.node)
  } else {
    emit('select', props.node.path)
  }
}

async function copyPath(event: MouseEvent): Promise<void> {
  event.stopPropagation()
  await copyToText(props.node.path, props.node.path)
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
}
</script>

<template>
  <div class="file-node">
    <div
      class="file-item"
      :class="{
        'is-dir': node.isDir,
        'is-selected': !node.isDir && selectedFile === node.path,
      }"
      :style="{ paddingLeft: 8 + level * 16 + 'px' }"
      @click="onSelect"
    >
      <div v-if="!node.isDir" class="file-icon">
        <FileIcon :size="14" :stroke-width="2" />
      </div>
      <div v-else class="file-dir-icon">
        <Loader2 v-if="node.loading" :size="14" :stroke-width="2" class="spin" />
        <FolderOpen v-else-if="node.expanded" :size="14" :stroke-width="2" />
        <Folder v-else :size="14" :stroke-width="2" />
      </div>
      <div class="file-meta">
        <div class="file-name">{{ node.name }}</div>
        <span v-if="!node.isDir && node.size != null" class="file-size">{{ formatSize(node.size) }}</span>
      </div>
      <button class="copy-path-btn" @click="copyPath($event)" :title="t('fileBrowser.copyPath')">
        <Check v-if="copiedPath === node.path" :size="14" :stroke-width="2.5" />
        <Copy v-else :size="14" :stroke-width="2" />
      </button>
    </div>

    <!-- Expanded directory: render children recursively -->
    <template v-if="node.isDir && node.expanded">
      <div v-if="node.loading" class="file-empty-child">{{ t('fileBrowser.loading') }}</div>
      <div v-else-if="node.children.length === 0" class="file-empty-child">{{ t('fileBrowser.emptyDir') }}</div>
      <template v-else>
        <FileTreeNode
          v-for="child in node.children"
          :key="child.path"
          :node="child"
          :selected-file="selectedFile"
          :level="level + 1"
          @select="emit('select', $event)"
          @toggle="emit('toggle', $event)"
        />
      </template>
    </template>
  </div>
</template>

<style scoped>
.file-item {
  display: flex;
  align-items: center;
  gap: 8px;
  /* paddingLeft is set inline per level; keep other padding here */
  padding-top: 8px;
  padding-right: 8px;
  padding-bottom: 8px;
  border-radius: var(--radius, 8px);
  cursor: pointer;
  transition: background .1s ease, box-shadow .1s ease;
  user-select: none;
  position: relative;
  height: 40px;
}

.file-item:hover {
  background: var(--bg-3);
}

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

.file-item:hover .copy-path-btn {
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
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
}

.copy-path-btn:hover {
  background: var(--border-2);
  color: var(--text);
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
  font-size: 14px;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.4;
}

.file-size {
  font-size: 12px;
  color: var(--text-2);
  flex-shrink: 0;
  transition: opacity .15s ease;
}

.file-item:hover .file-size {
  opacity: 0;
}

.file-empty-child {
  padding: 6px 10px 6px 36px;
  color: var(--text-3);
  font-size: 12px;
  font-style: italic;
}

.spin {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
