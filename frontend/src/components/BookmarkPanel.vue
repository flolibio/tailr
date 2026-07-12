<script setup lang="ts">
import { ref, computed } from 'vue'
import { useBookmarks, type Bookmark } from '../composables/useBookmarks'
import { useI18n } from 'vue-i18n'
import { ChevronDown } from 'lucide-vue-next'

const { t } = useI18n()

const props = defineProps<{
  filePath: string | null
  levelColors?: Record<string, string>
  validRange?: { min: number; max: number }
}>()

const emit = defineEmits<{
  scrollTo: [lineNum: number]
}>()

const { getBookmarks, remove } = useBookmarks()
const collapsed = ref(true)

const bookmarks = computed(() =>
  props.filePath ? getBookmarks(props.filePath) : [],
)

function isValid(lineNum: number): boolean {
  if (!props.validRange) return true
  return lineNum >= props.validRange.min && lineNum <= props.validRange.max
}

function handleClick(bm: Bookmark): void {
  if (isValid(bm.lineNum)) {
    emit('scrollTo', bm.lineNum)
  } else if (props.filePath) {
    remove(props.filePath, bm.lineNum)
  }
}
</script>

<template>
  <div class="bm-section">
    <div class="section-header" @click="collapsed = !collapsed">
      <span class="section-title">{{ t('bookmark.title') }} ({{ bookmarks.length }})</span>
      <div class="section-chevron" :class="{ collapsed }">
        <ChevronDown :size="14" :stroke-width="2.5" />
      </div>
    </div>
    <div v-show="!collapsed" class="bm-body">
      <div v-if="bookmarks.length === 0" class="bm-empty">{{ t('bookmark.empty') }}</div>
      <div
        v-for="bm in bookmarks"
        :key="bm.lineNum"
        class="bm-item"
        :class="{ 'bm-invalid': !isValid(bm.lineNum) }"
        :title="bm.preview"
        @click="handleClick(bm)"
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
  background: var(--bg-2);
  padding: 0 8px;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  cursor: pointer;
  user-select: none;
  transition: background .1s;
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
  transition: transform .15s;
}

.section-chevron.collapsed {
  transform: rotate(-90deg);
}

.section-title {
  font-size: 14px;
  color: var(--text-3);
}

.bm-body {
  overflow-y: auto;
  height: 180px;
  padding: 2px 8px 8px;
}

.bm-empty {
  padding: 16px 12px;
  font-size: 12px;
  color: var(--text-3);
  text-align: center;
}

.bm-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 8px;
  border-radius: var(--radius);
  cursor: pointer;
  transition: background .1s;
  user-select: none;
}

.bm-item:hover {
  background: var(--bg-3);
}

.bm-item.bm-invalid {
  opacity: 0.4;
}

.bm-item.bm-invalid .bm-preview {
  text-decoration: line-through;
  text-decoration-color: var(--text-3);
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
