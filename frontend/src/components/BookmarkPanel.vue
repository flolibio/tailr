<script setup lang="ts">
import { ref, computed } from 'vue'
import { useBookmarks } from '../composables/useBookmarks'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps<{
  filePath: string | null
  levelColors?: Record<string, string>
}>()

const emit = defineEmits<{
  scrollTo: [lineNum: number]
}>()

const { getBookmarks, remove } = useBookmarks()
const collapsed = ref(true)

const bookmarks = computed(() =>
  props.filePath ? getBookmarks(props.filePath) : [],
)
</script>

<template>
  <div class="bm-section">
    <div class="section-header" @click="collapsed = !collapsed">
      <div class="section-chevron" :class="{ collapsed }">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
      </div>
      <span class="section-title">{{ t('bookmark.title') }} ({{ bookmarks.length }})</span>
    </div>
    <div v-show="!collapsed" class="bm-body">
      <div
        v-for="bm in bookmarks"
        :key="bm.lineNum"
        class="bm-item"
        :title="bm.preview"
        @click="emit('scrollTo', bm.lineNum)"
      >
        <span class="bm-dot" :style="{ background: levelColors?.[bm.level] ?? 'var(--text-3)' }"></span>
        <span class="bm-preview">{{ bm.preview }}</span>
        <button class="bm-remove" @click.stop="props.filePath && remove(props.filePath, bm.lineNum)" :title="t('bookmark.remove')">✕</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.bm-section {
  flex-shrink: 0;
  border-top: 1px solid var(--border);
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

.bm-body {
  overflow-y: auto;
  height: 180px;
  padding: 2px 8px 8px;
}

.bm-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border-radius: 10px;
  cursor: pointer;
  transition: background .1s;
  user-select: none;
}

.bm-item:hover {
  background: var(--bg-2);
}

.bm-item:hover .bm-remove {
  opacity: 1;
}

.bm-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.bm-preview {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.bm-remove {
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

.bm-remove:hover {
  background: var(--bg-3);
  color: var(--text);
}
</style>
