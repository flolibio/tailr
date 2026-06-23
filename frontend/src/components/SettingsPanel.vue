<script setup lang="ts">
import { ref, watch, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { loadLocale } from '../locales'
import { healthCheck } from '../services/api'

export interface Settings {
  fontSize: number
  fontFamily: string
  autoScroll: boolean
  maxVisibleLines: number
  darkTheme: boolean
}

const props = defineProps<{
  settings: Settings
}>()

const emit = defineEmits<{
  update: [settings: Settings]
  collapse: []
}>()

const { t, locale } = useI18n()

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

const currentLocale = computed(() => locale.value)
const version = ref('')

onMounted(async () => {
  try {
    const { version: v } = await healthCheck()
    version.value = v
  } catch {}
})

async function switchLocale(newLocale: string): Promise<void> {
  await loadLocale(newLocale)
}
</script>

<template>
  <div class="settings-panel-inner">
    <div class="settings-body">
      <!-- Font size -->
      <div class="s-group">
        <div class="s-label">
          <span class="s-lname">{{ t('settings.fontSize') }}</span>
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
          <span class="s-lname">{{ t('settings.maxVisibleLines') }}</span>
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
          <span class="toggle-name">{{ t('settings.autoScroll') }}</span>
          <button
            class="toggle"
            :class="{ on: local.autoScroll }"
            @click="update('autoScroll', !local.autoScroll)"
            :aria-pressed="local.autoScroll"
          ></button>
        </div>
      </div>

      <div class="s-divider"></div>

      <!-- Theme -->
      <div class="s-group">
        <div class="s-label"><span class="s-lname">{{ t('settings.theme') }}</span></div>
        <div class="theme-opts">
          <button
            class="theme-opt"
            :class="{ on: currentTheme === 'light' }"
            @click="setTheme('light')"
          >{{ t('settings.light') }}</button>
          <button
            class="theme-opt"
            :class="{ on: currentTheme === 'dark' }"
            @click="setTheme('dark')"
          >{{ t('settings.dark') }}</button>
          <button
            class="theme-opt"
            :class="{ on: currentTheme === 'system' }"
            @click="setTheme('system')"
          >{{ t('settings.system') }}</button>
        </div>
      </div>

      <div class="s-divider"></div>

      <!-- Language -->
      <div class="s-group">
        <div class="s-label"><span class="s-lname">{{ t('settings.language') }}</span></div>
        <div class="theme-opts">
          <button
            class="theme-opt"
            :class="{ on: currentLocale === 'en-US' }"
            @click="switchLocale('en-US')"
          >English</button>
          <button
            class="theme-opt"
            :class="{ on: currentLocale === 'zh-CN' }"
            @click="switchLocale('zh-CN')"
          >中文</button>
        </div>
      </div>
    </div>
    <div class="settings-footer">
      <span class="footer-version">v{{ version }}</span>
      <a class="footer-link" href="https://github.com/flolib-org/tailr" target="_blank" rel="noopener">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
        GitHub
      </a>
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
  height: 18px;
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
.collapse-btn {
  width: 24px;
  height: 24px;
  border-radius: 5px;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: background .12s, color .12s;
}

.collapse-btn:hover {
  background: var(--bg-2);
  color: var(--text);
}

.settings-footer {
  padding: 12px 16px;
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
}

.footer-version {
  color: var(--text-3);
  font-family: var(--font-mono);
}

.footer-link {
  color: var(--text-2);
  text-decoration: none;
  transition: color .12s;
  display: flex;
  align-items: center;
  gap: 4px;
}

.footer-link:hover {
  color: var(--accent);
}
</style>
