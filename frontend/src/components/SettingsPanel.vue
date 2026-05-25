<script setup lang="ts">
import { ref, watch } from 'vue'

export interface Settings {
  fontSize: number
  autoScroll: boolean
  lineWrap: boolean
  maxVisibleLines: number
  darkTheme: boolean
}

const props = defineProps<{
  settings: Settings
  collapsed: boolean
}>()

const emit = defineEmits<{
  update: [settings: Settings]
  toggleCollapse: []
}>()

const local = ref<Settings>({ ...props.settings })

watch(
  () => props.settings,
  (val) => {
    local.value = { ...val }
  },
  { deep: true },
)

function update<K extends keyof Settings>(key: K, value: Settings[K]): void {
  local.value[key] = value
  emit('update', { ...local.value })
}
</script>

<template>
  <div class="settings-panel" :class="{ collapsed: collapsed }">
    <div class="sidebar-header">
      <span v-if="!collapsed">Settings</span>
      <button @click="emit('toggleCollapse')" :title="collapsed ? 'Open settings' : 'Close settings'">
        {{ collapsed ? '◀' : '▶' }}
      </button>
    </div>
    <div v-if="!collapsed" class="settings-body">
      <div class="setting-group">
        <div class="setting-label">Font Size: {{ local.fontSize }}px</div>
        <input
          type="range"
          :value="local.fontSize"
          min="12"
          max="20"
          step="1"
          @input="update('fontSize', +($event.target as HTMLInputElement).value)"
        />
      </div>

      <div class="setting-group">
        <label class="setting-checkbox">
          <input
            type="checkbox"
            :checked="local.autoScroll"
            @change="update('autoScroll', ($event.target as HTMLInputElement).checked)"
          />
          <span>Auto-scroll</span>
        </label>
      </div>

      <div class="setting-group">
        <label class="setting-checkbox">
          <input
            type="checkbox"
            :checked="local.lineWrap"
            @change="update('lineWrap', ($event.target as HTMLInputElement).checked)"
          />
          <span>Line wrap</span>
        </label>
      </div>

      <div class="setting-group">
        <div class="setting-label">Max visible lines: {{ local.maxVisibleLines }}</div>
        <input
          type="range"
          :value="local.maxVisibleLines"
          min="1000"
          max="100000"
          step="1000"
          @input="update('maxVisibleLines', +($event.target as HTMLInputElement).value)"
        />
      </div>

      <div class="setting-group">
        <label class="setting-checkbox">
          <input
            type="checkbox"
            :checked="local.darkTheme"
            @change="update('darkTheme', ($event.target as HTMLInputElement).checked)"
          />
          <span>Dark theme</span>
        </label>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.settings-panel.collapsed {
  width: 0;
  min-width: 0;
}

.settings-body {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
}

.setting-group {
  margin-bottom: 16px;
}

.setting-label {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}

.setting-group input[type="range"] {
  width: 100%;
}

.setting-checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  font-size: 13px;
}
</style>
