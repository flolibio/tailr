/**
 * 日志级别配置管理
 *
 * 管理预设数据、级别配置、动态 CSS 变量、localStorage 持久化。
 */
import { ref, computed } from 'vue'
import type { LevelDef, LogLevelConfig } from '../services/api'
import { getLogLevelConfig, saveLogLevelConfig } from '../services/api'

// ── 预设数据 ──────────────────────────────────────────────

export const PRESETS: Record<string, LevelDef[]> = {
  general: [
    { name: 'ERROR', keywords: ['ERROR'], colorLight: '#A32D2D', colorDark: '#F09595' },
    { name: 'WARN', keywords: ['WARN'], colorLight: '#854F0B', colorDark: '#EF9F27' },
    { name: 'INFO', keywords: ['INFO'], colorLight: '#0C447C', colorDark: '#85B7EB' },
    { name: 'DEBUG', keywords: ['DEBUG'], colorLight: '#3B6D11', colorDark: '#97C459' },
  ],
  java: [
    { name: 'FATAL', keywords: ['FATAL'], colorLight: '#CC2D26', colorDark: '#FF6B63' },
    { name: 'ERROR', keywords: ['ERROR'], colorLight: '#A32D2D', colorDark: '#F09595' },
    { name: 'WARN', keywords: ['WARN'], colorLight: '#854F0B', colorDark: '#EF9F27' },
    { name: 'INFO', keywords: ['INFO'], colorLight: '#0C447C', colorDark: '#85B7EB' },
    { name: 'DEBUG', keywords: ['DEBUG'], colorLight: '#3B6D11', colorDark: '#97C459' },
    { name: 'TRACE', keywords: ['TRACE'], colorLight: '#5F5E5A', colorDark: '#B4B2A9' },
  ],
  python: [
    { name: 'CRITICAL', keywords: ['CRITICAL'], colorLight: '#CC2D26', colorDark: '#FF6B63' },
    { name: 'ERROR', keywords: ['ERROR'], colorLight: '#A32D2D', colorDark: '#F09595' },
    { name: 'WARNING', keywords: ['WARNING'], colorLight: '#854F0B', colorDark: '#EF9F27' },
    { name: 'INFO', keywords: ['INFO'], colorLight: '#0C447C', colorDark: '#85B7EB' },
    { name: 'DEBUG', keywords: ['DEBUG'], colorLight: '#3B6D11', colorDark: '#97C459' },
  ],
  php: [
    { name: 'ALERT', keywords: ['ALERT'], colorLight: '#CC2D26', colorDark: '#FF6B63' },
    { name: 'ERROR', keywords: ['ERROR'], colorLight: '#A32D2D', colorDark: '#F09595' },
    { name: 'WARNING', keywords: ['WARNING'], colorLight: '#854F0B', colorDark: '#EF9F27' },
    { name: 'NOTICE', keywords: ['NOTICE'], colorLight: '#0C447C', colorDark: '#85B7EB' },
    { name: 'INFO', keywords: ['INFO'], colorLight: '#3B6D11', colorDark: '#97C459' },
    { name: 'DEBUG', keywords: ['DEBUG'], colorLight: '#5F5E5A', colorDark: '#B4B2A9' },
  ],
  go: [
    { name: 'ERROR', keywords: ['ERROR'], colorLight: '#A32D2D', colorDark: '#F09595' },
    { name: 'WARN', keywords: ['WARN'], colorLight: '#854F0B', colorDark: '#EF9F27' },
    { name: 'INFO', keywords: ['INFO'], colorLight: '#0C447C', colorDark: '#85B7EB' },
    { name: 'DEBUG', keywords: ['DEBUG'], colorLight: '#3B6D11', colorDark: '#97C459' },
  ],
  rust: [
    { name: 'ERROR', keywords: ['ERROR'], colorLight: '#A32D2D', colorDark: '#F09595' },
    { name: 'WARN', keywords: ['WARN'], colorLight: '#854F0B', colorDark: '#EF9F27' },
    { name: 'INFO', keywords: ['INFO'], colorLight: '#0C447C', colorDark: '#85B7EB' },
    { name: 'DEBUG', keywords: ['DEBUG'], colorLight: '#3B6D11', colorDark: '#97C459' },
    { name: 'TRACE', keywords: ['TRACE'], colorLight: '#5F5E5A', colorDark: '#B4B2A9' },
  ],
  syslog: [
    { name: 'EMERG', keywords: ['EMERG'], colorLight: '#CC2D26', colorDark: '#FF6B63' },
    { name: 'ALERT', keywords: ['ALERT'], colorLight: '#D4421E', colorDark: '#FF8A65' },
    { name: 'CRIT', keywords: ['CRIT'], colorLight: '#A32D2D', colorDark: '#F09595' },
    { name: 'ERR', keywords: ['ERR'], colorLight: '#854F0B', colorDark: '#EF9F27' },
    { name: 'WARNING', keywords: ['WARNING'], colorLight: '#0C447C', colorDark: '#85B7EB' },
    { name: 'NOTICE', keywords: ['NOTICE'], colorLight: '#3B6D11', colorDark: '#97C459' },
    { name: 'INFO', keywords: ['INFO'], colorLight: '#5F5E5A', colorDark: '#B4B2A9' },
    { name: 'DEBUG', keywords: ['DEBUG'], colorLight: '#5F5E5A', colorDark: '#B4B2A9' },
  ],
}

export const PRESET_NAMES: Record<string, string> = {
  general: 'General',
  java: 'Java (Log4j/SLF4J)',
  python: 'Python (logging)',
  php: 'PHP (error_log)',
  go: 'Go (slog/zerolog)',
  rust: 'Rust (tracing)',
  syslog: 'syslog',
}

// ── 预设色板 ──────────────────────────────────────────────

export const COLOR_PALETTE = [
  // 红
  { light: '#CC2D26', dark: '#FF6B63' },
  { light: '#A32D2D', dark: '#F09595' },
  // 橙
  { light: '#CC5500', dark: '#FFB340' },
  { light: '#854F0B', dark: '#EF9F27' },
  // 黄
  { light: '#8B6914', dark: '#FFD600' },
  { light: '#664D03', dark: '#FFE066' },
  // 绿
  { light: '#2E7D32', dark: '#66BB6A' },
  { light: '#3B6D11', dark: '#97C459' },
  // 青
  { light: '#00695C', dark: '#4DD0E1' },
  { light: '#004D40', dark: '#26A69A' },
  // 蓝
  { light: '#0C447C', dark: '#85B7EB' },
  { light: '#283593', dark: '#7986CB' },
  // 紫
  { light: '#6A1B9A', dark: '#CE93D8' },
  { light: '#4527A0', dark: '#B39DDB' },
  // 灰
  { light: '#616161', dark: '#B4B2A9' },
  { light: '#455A64', dark: '#90A4AE' },
]

// ── localStorage 持久化 ───────────────────────────────────

const STORAGE_KEY = 'tailr-log-levels'

function loadFromStorage(): LogLevelConfig | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw)
  } catch {}
  return null
}

function saveToStorage(config: LogLevelConfig): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config))
  } catch {}
}

// ── Composable ────────────────────────────────────────────

let isDark = false

export function useLogLevels() {
  const config = ref<LogLevelConfig>(loadFromStorage() ?? {
    preset: 'general',
    levels: PRESETS.general.map(l => ({ ...l })),
  })

  const levelNames = computed(() => config.value.levels.map(l => l.name))

  // 动态 CSS 变量
  function applyThemeColors(dark: boolean) {
    isDark = dark
    const root = document.documentElement
    for (const level of config.value.levels) {
      const key = level.name.toLowerCase()
      const color = dark ? level.colorDark : level.colorLight
      // 计算背景色（颜色降低透明度）
      root.style.setProperty(`--c-${key}-text`, color)
      root.style.setProperty(`--c-${key}-bg`, `${color}18`)
      root.style.setProperty(`--c-${key}-border`, `${color}40`)
    }
  }

  // 切换预设
  function switchPreset(presetName: string, preserveColors = true) {
    const preset = PRESETS[presetName]
    if (!preset) return

    const newLevels = preset.map(l => {
      if (!preserveColors) return { ...l }
      const oldLevel = config.value.levels.find(ol => ol.name === l.name)
      return {
        ...l,
        colorLight: oldLevel?.colorLight ?? l.colorLight,
        colorDark: oldLevel?.colorDark ?? l.colorDark,
      }
    })

    config.value = { preset: presetName, levels: newLevels }
    applyThemeColors(isDark)
    saveToStorage(config.value)
  }

  // 添加级别
  function addLevel() {
    config.value.levels.push({
      name: 'NEW_LEVEL',
      keywords: ['NEW_LEVEL'],
      colorLight: '#616161',
      colorDark: '#B4B2A9',
    })
    config.value = { ...config.value, preset: 'custom' }
  }

  // 删除级别
  function removeLevel(index: number) {
    config.value.levels.splice(index, 1)
    config.value = { ...config.value, preset: 'custom' }
    applyThemeColors(isDark)
    saveToStorage(config.value)
  }

  // 更新级别
  function updateLevel(index: number, updates: Partial<LevelDef>) {
    const level = config.value.levels[index]
    if (!level) return
    Object.assign(level, updates)
    config.value = { ...config.value, preset: 'custom' }
    applyThemeColors(isDark)
    saveToStorage(config.value)
  }

  // 重置为默认
  function resetToDefault() {
    switchPreset('general', false)
  }

  // 同步到后端
  async function syncToBackend() {
    await saveLogLevelConfig(config.value)
  }

  // 从后端加载
  async function loadFromBackend() {
    try {
      const remote = await getLogLevelConfig()
      config.value = remote
      saveToStorage(remote)
      applyThemeColors(isDark)
    } catch {
      // 后端不可用时使用本地配置
    }
  }

  return {
    config,
    levelNames,
    switchPreset,
    addLevel,
    removeLevel,
    updateLevel,
    resetToDefault,
    applyThemeColors,
    syncToBackend,
    loadFromBackend,
  }
}
