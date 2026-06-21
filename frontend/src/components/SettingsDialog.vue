<script setup lang="ts">
import { ref, watch, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { loadLocale } from '../locales'
import { healthCheck } from '../services/api'
import { useLogLevels } from '../composables/useLogLevels'
import LogLevelSettings from './settings/LogLevelSettings.vue'
import type { Settings } from './SettingsPanel.vue'

const props = defineProps<{
  settings: Settings
}>()

const emit = defineEmits<{
  update: [settings: Settings]
  close: []
}>()

const { t, locale } = useI18n()

// ── Navigation ──
type NavSection = 'general' | 'logLevels' | 'appearance' | 'server' | 'aiModels' | 'about'
const activeNav = ref<NavSection>('general')

const navItems = computed<{ key: NavSection; label: string; icon: string; section: string }[]>(() => [
  {
    key: 'general',
    label: t('settings.general'),
    icon: 'settings',
    section: t('settings.general'),
  },
  {
    key: 'logLevels',
    label: t('settings.logLevels'),
    icon: 'edit',
    section: t('settings.general'),
  },
  {
    key: 'about',
    label: t('settings.about'),
    icon: 'info',
    section: t('settings.about'),
  },
])

const sections = computed(() => {
  const result: { name: string; items: typeof navItems.value }[] = []
  let current = ''
  for (const item of navItems.value) {
    if (item.section !== current) {
      current = item.section
      result.push({ name: item.section, items: [] })
    }
    result[result.length - 1].items.push(item)
  }
  return result
})

// ── General settings (from SettingsPanel) ──
const local = ref<Settings>({ ...props.settings })

watch(
  () => props.settings,
  (val) => {
    local.value = { ...val }
  },
  { deep: true },
)

function updateSetting<K extends keyof Settings>(key: K, value: Settings[K]): void {
  local.value[key] = value
  emit('update', { ...local.value })
}

const currentTheme = ref<'light' | 'dark' | 'system'>('light')
let systemThemeCleanup: (() => void) | null = null

function setTheme(theme: 'light' | 'dark' | 'system'): void {
  if (systemThemeCleanup) {
    systemThemeCleanup()
    systemThemeCleanup = null
  }

  if (theme === 'system') {
    currentTheme.value = 'system'
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const apply = () => { updateSetting('darkTheme', mq.matches) }
    mq.addEventListener('change', apply)
    systemThemeCleanup = () => mq.removeEventListener('change', apply)
    apply()
  } else {
    currentTheme.value = theme
    updateSetting('darkTheme', theme === 'dark')
  }
}

watch(() => local.value.darkTheme, (isDark) => {
  if (currentTheme.value !== 'system') {
    currentTheme.value = isDark ? 'dark' : 'light'
  }
}, { immediate: true })

function formatMaxLines(v: number): string {
  return v.toLocaleString()
}

const currentLocale = computed(() => locale.value)

async function switchLocale(newLocale: string): Promise<void> {
  await loadLocale(newLocale)
}

// ── Version & footer ──
const version = ref('')

onMounted(async () => {
  try {
    const { version: v } = await healthCheck()
    version.value = v
  } catch {}
})

// ── Log Levels save/reset ──
const logLevelsRef = ref<InstanceType<typeof LogLevelSettings> | null>(null)

const {
  applyThemeColors,
  isDark,
} = useLogLevels()

const saveState = ref<'idle' | 'saving' | 'success' | 'error'>('idle')

async function handleSave() {
  if (!logLevelsRef.value) return

  // Blur any active input to commit pending keyword edits
  const active = document.activeElement as HTMLElement | null
  if (active && active.tagName === 'INPUT') active.blur()

  saveState.value = 'saving'
  try {
    await logLevelsRef.value.syncToBackend()
    applyThemeColors(isDark.value)
    saveState.value = 'success'
    setTimeout(() => { saveState.value = 'idle' }, 1500)
  } catch {
    saveState.value = 'error'
    setTimeout(() => { saveState.value = 'idle' }, 2000)
  }
}

function handleReset() {
  if (logLevelsRef.value) {
    logLevelsRef.value.resetToDefault()
  }
}

// ── Close handling ──
function onOverlayClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('settings-overlay')) {
    emit('close')
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    emit('close')
  }
}

onMounted(() => {
  document.addEventListener('keydown', onKeydown)
})

import { onUnmounted } from 'vue'

onUnmounted(() => {
  document.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div class="settings-overlay" @click="onOverlayClick">
    <div class="settings-dialog">
      <!-- Header -->
      <div class="dialog-header">
        <span class="dialog-title">{{ t('settings.title') }}</span>
        <button class="close-btn" @click="emit('close')">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>

      <!-- Body -->
      <div class="dialog-body">
        <!-- Sidebar Nav -->
        <div class="dialog-sidebar">
          <template v-for="section in sections" :key="section.name">
            <div class="nav-section">{{ section.name }}</div>
            <button
              v-for="item in section.items"
              :key="item.key"
              class="nav-item"
              :class="{ active: activeNav === item.key }"
              @click="activeNav = item.key"
            >
              <!-- Settings icon -->
              <svg v-if="item.icon === 'settings'" class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/>
              </svg>
              <!-- Edit icon -->
              <svg v-else-if="item.icon === 'edit'" class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 013 3L7 19l-4 1 1-4L16.5 3.5z"/>
              </svg>
              <!-- Info icon -->
              <svg v-else-if="item.icon === 'info'" class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><circle cx="12" cy="8" r="1" fill="currentColor" stroke="none"/>
              </svg>
              {{ item.label }}
            </button>
          </template>
        </div>

        <!-- Content Area -->
        <div class="dialog-content">
          <!-- General -->
          <template v-if="activeNav === 'general'">
            <div class="section-title">{{ t('settings.basicSettings') }}</div>

            <!-- Font size -->
            <div class="setting-row">
              <div class="setting-info">
                <div class="setting-name">{{ t('settings.fontSize') }}</div>
                <div class="setting-description">{{ local.fontSize }}px</div>
              </div>
              <div class="setting-control">
                <input
                  type="range"
                  :value="local.fontSize"
                  min="10"
                  max="20"
                  step="1"
                  @input="updateSetting('fontSize', +($event.target as HTMLInputElement).value)"
                />
              </div>
            </div>

            <!-- Max visible lines -->
            <div class="setting-row">
              <div class="setting-info">
                <div class="setting-name">{{ t('settings.maxVisibleLines') }}</div>
                <div class="setting-description">{{ formatMaxLines(local.maxVisibleLines) }}</div>
              </div>
              <div class="setting-control">
                <input
                  type="range"
                  :value="local.maxVisibleLines"
                  min="1000"
                  max="100000"
                  step="1000"
                  @input="updateSetting('maxVisibleLines', +($event.target as HTMLInputElement).value)"
                />
              </div>
            </div>

            <!-- Auto-scroll -->
            <div class="setting-row">
              <div class="setting-info">
                <div class="setting-name">{{ t('settings.autoScroll') }}</div>
              </div>
              <div class="setting-control">
                <button
                  class="toggle"
                  :class="{ on: local.autoScroll }"
                  @click="updateSetting('autoScroll', !local.autoScroll)"
                  :aria-pressed="local.autoScroll"
                ></button>
              </div>
            </div>

            <!-- Theme -->
            <div class="setting-row">
              <div class="setting-info">
                <div class="setting-name">{{ t('settings.theme') }}</div>
              </div>
              <div class="setting-control">
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
            </div>

            <!-- Language -->
            <div class="setting-row">
              <div class="setting-info">
                <div class="setting-name">{{ t('settings.language') }}</div>
              </div>
              <div class="setting-control">
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
                  >{{ '\u4E2D\u6587' }}</button>
                </div>
              </div>
            </div>
          </template>

          <!-- Log Levels -->
          <template v-else-if="activeNav === 'logLevels'">
            <div class="section-title">{{ t('settings.logLevels') }}</div>
            <LogLevelSettings ref="logLevelsRef" />
          </template>

          <!-- About -->
          <template v-else-if="activeNav === 'about'">
            <div class="section-title">{{ t('settings.about') }}</div>
            <div class="about-content">
              <div class="about-logo">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/>
                </svg>
              </div>
              <div class="about-name">tailr</div>
              <div class="about-version">v{{ version }}</div>
              <div class="about-desc">{{ t('settings.description') }}</div>
              <a class="about-link" href="https://github.com/wunamesst/tailr" target="_blank" rel="noopener">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
                GitHub
              </a>
            </div>
          </template>
        </div>
      </div>

      <!-- Footer -->
      <div class="dialog-footer">
        <div class="footer-right">
          <button
            v-if="activeNav === 'logLevels'"
            class="btn-reset"
            @click="handleReset"
          >{{ t('settings.resetDefault') }}</button>
          <button
            v-if="activeNav === 'logLevels'"
            class="btn-save"
            :class="{ success: saveState === 'success', error: saveState === 'error' }"
            @click="handleSave"
            :disabled="saveState === 'saving'"
          >
            <template v-if="saveState === 'saving'">
              <span class="spinner"></span>
              {{ t('settings.saving') }}
            </template>
            <template v-else-if="saveState === 'success'">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
              {{ t('settings.saved') }}
            </template>
            <template v-else-if="saveState === 'error'">
              {{ t('settings.saveError') }}
            </template>
            <template v-else>
              {{ t('settings.save') }}
            </template>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* ── Overlay ── */
.settings-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

/* ── Dialog ── */
.settings-dialog {
  width: 860px;
  height: 580px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* ── Header ── */
.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-2);
  flex-shrink: 0;
}

.dialog-title {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.02em;
  color: var(--text);
}

.close-btn {
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius);
  transition: background 0.12s, color 0.12s;
  padding: 0;
}

.close-btn:hover {
  background: #c42b1c;
  color: #fff;
}

/* ── Body ── */
.dialog-body {
  display: flex;
  flex: 1;
  overflow: hidden;
}

/* ── Sidebar ── */
.dialog-sidebar {
  width: 200px;
  flex-shrink: 0;
  border-right: 1px solid var(--border);
  background: var(--bg-2);
  padding: 8px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-section {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-3);
  padding: 12px 10px 4px;
}

.nav-item {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 8px;
  padding: 7px 10px;
  border-radius: var(--radius);
  border: none;
  background: transparent;
  color: var(--text-2);
  font-size: 12.5px;
  font-family: var(--font-sans);
  cursor: pointer;
  transition: background 0.1s, color 0.1s;
  text-align: left;
  width: 100%;
  height: auto;
}

.nav-item:hover {
  background: var(--bg-3);
  color: var(--text-2);
}

.nav-item.active {
  background: var(--accent-light);
  color: var(--accent);
}

.nav-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  opacity: 0.7;
}

.nav-item.active .nav-icon {
  opacity: 1;
}

/* ── Content ── */
.dialog-content {
  flex: 1;
  padding: 24px 28px;
  overflow-y: auto;
}

.section-title {
  font-size: 16px;
  font-weight: 600;
  margin-bottom: 20px;
  color: var(--text);
}

.placeholder-text {
  font-size: 13px;
  color: var(--text-3);
  padding: 20px 0;
}

/* ── Setting Row ── */
.setting-row {
  display: flex;
  align-items: flex-start;
  gap: 16px;
  padding: 12px 0;
  border-bottom: 1px solid var(--border);
}

.setting-row:last-child {
  border-bottom: none;
}

.setting-info {
  flex: 1;
  min-width: 0;
}

.setting-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text);
  margin-bottom: 2px;
}

.setting-description {
  font-size: 11.5px;
  color: var(--text-3);
  line-height: 1.5;
}

.setting-control {
  flex-shrink: 0;
  display: flex;
  justify-content: flex-start;
}

/* ── Toggle Switch ── */
.toggle {
  position: relative;
  width: 36px;
  height: 20px;
  flex-shrink: 0;
  border: none;
  padding: 0;
}

.toggle input {
  opacity: 0;
  width: 0;
  height: 0;
}

.toggle::after {
  content: '';
  position: absolute;
  width: 14px;
  height: 14px;
  left: 2px;
  top: 2px;
  background: var(--text-3);
  border-radius: 50%;
  transition: transform 0.2s, background 0.2s;
}

.toggle.on {
  background: var(--accent);
  border-color: var(--accent);
}

.toggle.on::after {
  transform: translateX(16px);
  background: #fff;
}

/* ── Theme / Language opts ── */
.theme-opts {
  display: flex;
  gap: 5px;
}

.theme-opt {
  min-width: 70px;
  padding: 6px 12px;
  border-radius: 6px;
  border: 1px solid var(--border);
  font-size: 11px;
  font-weight: 500;
  text-align: center;
  cursor: pointer;
  color: var(--text-2);
  background: transparent;
  transition: all 0.12s;
  height: auto;
}

.theme-opt:hover {
  background: var(--bg-2);
  color: var(--text-2);
}

.theme-opt.on {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-light);
}

/* ── Footer ── */
.dialog-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 10px 16px;
  border-top: 1px solid var(--border);
  background: var(--bg-2);
  flex-shrink: 0;
}

.footer-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* ── About ── */
.about-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 40px 0;
}

.about-logo {
  color: var(--accent);
  opacity: 0.8;
}

.about-name {
  font-size: 20px;
  font-weight: 600;
  color: var(--text);
}

.about-version {
  font-size: 13px;
  font-family: var(--font-mono);
  color: var(--text-3);
}

.about-desc {
  font-size: 13px;
  color: var(--text-2);
}

.about-link {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  padding: 8px 16px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-2);
  text-decoration: none;
  font-size: 12px;
  transition: all 0.12s;
}

.about-link:hover {
  background: var(--bg-3);
  border-color: var(--border-2);
  color: var(--text);
}

/* ── Footer buttons ── */
.btn-reset {
  height: 30px;
  font-size: 12px;
  padding: 0 14px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--bg-3);
  color: var(--text-2);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: all 0.12s;
}

.btn-reset:hover {
  background: var(--bg-hover, var(--bg-3));
  border-color: var(--border-2);
  color: var(--text);
}

.btn-save {
  height: 30px;
  font-size: 12px;
  padding: 0 14px;
  border-radius: var(--radius);
  border: 1px solid var(--accent);
  background: var(--accent);
  color: #fff;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  transition: all 0.12s;
}

.btn-save:hover:not(:disabled) {
  opacity: 0.9;
}

.btn-save:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}

.btn-save.success {
  background: #22c55e;
  border-color: #22c55e;
}

.btn-save.error {
  background: var(--c-error-text);
  border-color: var(--c-error-text);
}

/* ── Spinner ── */
.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-top-color: white;
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
