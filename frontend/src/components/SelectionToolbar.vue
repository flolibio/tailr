<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useCopyFeedback } from '../composables/useClipboard'
import { Check, Copy, Search } from 'lucide-vue-next'

const { t } = useI18n()

const emit = defineEmits<{
  follow: [text: string]
}>()

const visible = ref(false)
const x = ref(0)
const y = ref(0)
const selectedText = ref('')
const { copied, copy: copyText } = useCopyFeedback()

function updateSelection(): void {
  const sel = window.getSelection()
  if (!sel || sel.isCollapsed || !sel.toString().trim()) {
    visible.value = false
    return
  }

  const range = sel.getRangeAt(0)
  const rect = range.getBoundingClientRect()
  selectedText.value = sel.toString()

  x.value = rect.left + rect.width / 2
  y.value = rect.top - 8
  visible.value = true
  copied.value = false
}

function onMouseUp(e: MouseEvent): void {
  if ((e.target as HTMLElement).closest('.selection-toolbar')) return
  setTimeout(() => {
    const sel = window.getSelection()
    if (sel && !sel.isCollapsed && sel.toString().trim()) {
      const range = sel.getRangeAt(0)
      const container = range.commonAncestorContainer
      const logViewer = container instanceof HTMLElement
        ? container.closest('.log-viewer')
        : container.parentElement?.closest('.log-viewer')
      if (!logViewer) {
        visible.value = false
        return
      }
      updateSelection()
    } else {
      visible.value = false
    }
  }, 10)
}

function onMouseDown(e: MouseEvent): void {
  if (visible.value && !(e.target as HTMLElement).closest('.selection-toolbar')) {
    visible.value = false
  }
}

async function copySelection(): Promise<void> {
  await copyText(selectedText.value)
}

function followSelection(): void {
  if (selectedText.value.trim()) {
    emit('follow', selectedText.value.trim())
    visible.value = false
    window.getSelection()?.removeAllRanges()
  }
}

onMounted(() => {
  document.addEventListener('mouseup', onMouseUp)
  document.addEventListener('mousedown', onMouseDown)
})

onUnmounted(() => {
  document.removeEventListener('mouseup', onMouseUp)
  document.removeEventListener('mousedown', onMouseDown)
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="visible"
      class="selection-toolbar"
      :style="{ left: x + 'px', top: y + 'px' }"
    >
      <button class="selection-btn" @click="copySelection">
        <Check v-if="copied" :size="14" :stroke-width="2.5" />
        <Copy v-else :size="14" :stroke-width="2" />
        <span>{{ copied ? t('selection.copied') : t('selection.copy') }}</span>
      </button>
      <div class="selection-sep"></div>
      <button class="selection-btn" @click="followSelection">
        <Search :size="14" :stroke-width="2" />
        <span>{{ t('selection.follow') }}</span>
      </button>
    </div>
  </Teleport>
</template>

<style>
.selection-toolbar {
  position: fixed;
  transform: translate(-50%, -100%);
  z-index: 9999;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 4px;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
  display: flex;
  align-items: center;
  gap: 2px;
  pointer-events: auto;
}

.selection-sep {
  width: 1px;
  height: 18px;
  background: var(--border);
  margin: 0 2px;
}

.selection-btn {
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 0 8px;
  border-radius: 4px;
  font-size: 12px;
  font-family: var(--font-sans);
  white-space: nowrap;
  transition: background .12s;
}

.selection-btn:hover {
  background: var(--bg-2);
}
</style>
