<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const emit = defineEmits<{
  follow: [text: string]
}>()

const visible = ref(false)
const x = ref(0)
const y = ref(0)
const selectedText = ref('')
const copied = ref(false)

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

async function copySelection(): Promise<void> {
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(selectedText.value)
    } else {
      const textarea = document.createElement('textarea')
      textarea.value = selectedText.value
      textarea.style.position = 'fixed'
      textarea.style.left = '-9999px'
      document.body.appendChild(textarea)
      textarea.select()
      document.execCommand('copy')
      document.body.removeChild(textarea)
    }
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 1500)
  } catch {}
}

function followSelection(): void {
  if (selectedText.value.trim()) {
    emit('follow', selectedText.value.trim())
    visible.value = false
    window.getSelection()?.removeAllRanges()
  }
}

onMounted(() => {
  document.addEventListener('selectionchange', updateSelection)
  document.addEventListener('mousedown', (e) => {
    if (visible.value && !(e.target as HTMLElement).closest('.selection-toolbar')) {
      visible.value = false
    }
  })
})

onUnmounted(() => {
  document.removeEventListener('selectionchange', updateSelection)
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
        <svg v-if="copied" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
        <svg v-else width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
        <span>{{ copied ? t('selection.copied') : t('selection.copy') }}</span>
      </button>
      <div class="selection-sep"></div>
      <button class="selection-btn" @click="followSelection">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/></svg>
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
