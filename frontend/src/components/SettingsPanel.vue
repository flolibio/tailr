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
}>()

const emit = defineEmits<{
  update: [settings: Settings]
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

function setTheme(theme: 'light' | 'dark' | 'system'): void {
  if (theme === 'system') {
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    update('darkTheme', prefersDark)
  } else {
    update('darkTheme', theme === 'dark')
  }
}

const currentTheme = ref<'light' | 'dark' | 'system'>('light')

watch(() => local.value.darkTheme, (isDark) => {
  currentTheme.value = isDark ? 'dark' : 'light'
}, { immediate: true })

function formatMaxLines(v: number): string {
  return v.toLocaleString()
}
</script>

<template>
  <div class="settings-panel-inner">
    <div class="settings-header">
      <span class="settings-title">Settings</span>
    </div>
    <div class="settings-body">
      <!-- Font size -->
      <div class="s-group">
        <div class="s-label">
          <span class="s-lname">Font size</span>
          <span class="s-lval">{{ local.fontSize }}px</span>
        </div>
        <input
          type="range"
          :value="local.fontSize"
          min="10"
          max="20"
          step="1"
          @input="update('fontSize', +($event.target as HTMLInputElement).value)"
        />
      </div>

      <!-- Max visible lines -->
      <div class="s-group">
        <div class="s-label">
          <span class="s-lname">Max visible lines</span>
          <span class="s-lval">{{ formatMaxLines(local.maxVisibleLines) }}</span>
        </div>
        <input
          type="range"
          :value="local.maxVisibleLines"
          min="1000"
          max="100000"
          step="1000"
          @input="update('maxVisibleLines', +($event.target as HTMLInputElement).value)"
        />
      </div>

      <div class="s-divider"></div>

      <!-- Toggles -->
      <div class="s-group">
        <div class="toggle-row">
          <span class="toggle-name">Auto-scroll</span>
          <button
            class="toggle"
            :class="{ on: local.autoScroll }"
            @click="update('autoScroll', !local.autoScroll)"
            :aria-pressed="local.autoScroll"
          ></button>
        </div>
        <div class="toggle-row">
          <span class="toggle-name">Line wrap</span>
          <button
            class="toggle"
            :class="{ on: local.lineWrap }"
            @click="update('lineWrap', !local.lineWrap)"
            :aria-pressed="local.lineWrap"
          ></button>
        </div>
      </div>

      <div class="s-divider"></div>

      <!-- Theme -->
      <div class="s-group">
        <div class="s-label"><span class="s-lname">Theme</span></div>
        <div class="theme-opts">
          <button
            class="theme-opt"
            :class="{ on: currentTheme === 'light' }"
            @click="setTheme('light')"
          >Light</button>
          <button
            class="theme-opt"
            :class="{ on: currentTheme === 'dark' }"
            @click="setTheme('dark')"
          >Dark</button>
          <button
            class="theme-opt"
            :class="{ on: currentTheme === 'system' }"
            @click="setTheme('system')"
          >System</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-panel-inner {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.settings-body {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 20px;
  overflow-y: auto;
  flex: 1;
}

.s-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.s-label {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.s-lname {
  font-size: 12px;
  color: var(--text-2);
}

.s-lval {
  font-size: 12px;
  font-weight: 500;
  font-family: var(--font-mono);
  color: var(--text);
}

.s-divider {
  height: 1px;
  background: var(--border);
}

.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 5px 0;
}

.toggle-name {
  font-size: 12.5px;
  color: var(--text);
}

.toggle {
  width: 34px;
  height: 19px;
  border-radius: 10px;
  background: var(--border-2);
  position: relative;
  cursor: pointer;
  border: none;
  outline: none;
  padding: 0;
  transition: background .18s;
  flex-shrink: 0;
}

.toggle::after {
  content: '';
  position: absolute;
  width: 13px;
  height: 13px;
  border-radius: 50%;
  background: white;
  top: 3px;
  left: 3px;
  transition: left .16s;
}

.toggle.on {
  background: var(--accent);
}

.toggle.on::after {
  left: 18px;
}

.theme-opts {
  display: flex;
  gap: 5px;
}

.theme-opt {
  flex: 1;
  padding: 6px 0;
  border-radius: 6px;
  border: 1px solid var(--border);
  font-size: 11px;
  font-weight: 500;
  text-align: center;
  cursor: pointer;
  color: var(--text-2);
  background: transparent;
  transition: all .12s;
  height: auto;
}

.theme-opt:hover {
  background: var(--bg-2);
}

.theme-opt.on {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-light);
}
</style>
