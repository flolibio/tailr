/**
 * 历史文件过滤开关
 *
 * 控制 file-list 中是否显示 logrotate 产生的历史日志文件
 *（编号轮转、日期命名、.bak/.old 等旧文件标记）。
 *
 * 默认隐藏（仅看实时日志），用户可 toggle 打开。
 * 持久化到 localStorage，记住用户选择。
 *
 * 影响范围：仅 file-list，不影响 Recent。
 */
import { ref } from 'vue'

// ── localStorage ───────────────────────────────────────────

const STORAGE_KEY = 'tailr-show-historical'

function loadFromStorage(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'true'
  } catch {
    return false
  }
}

function saveToStorage(value: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(value))
  } catch {}
}

// ── logrotate 识别正则 ─────────────────────────────────────

/**
 * 匹配 logrotate 常见命名模式的历史文件。
 *
 * | 正则                     | 匹配示例                        | 说明              |
 * |-------------------------|--------------------------------|------------------|
 * | `\.\d+$`                | app.log.1, app.log.23          | 编号轮转           |
 * | `[-_.]\d{8}[._]`        | app-20240630.log, demo.20260701.log | 紧凑日期 YYYYMMDD |
 * | `[-_.]\d{4}-\d{2}-\d{2}[._]`| app-2024-06-30.log             | ISO 日期          |
 * | `\.(bak|old|prev|save)$`| app.log.bak, app.log.old       | 旧文件标记         |
 */
const HISTORICAL_PATTERNS: RegExp[] = [
  /\.\d+$/,
  /[-_.]\d{8}[._]/,
  /[-_.]\d{4}-\d{2}-\d{2}[._]/,
  /\.(bak|old|prev|save)$/,
]

// ── 模块级状态（单例）─────────────────────────────────────

const showHistorical = ref(loadFromStorage())

// ── Composable ────────────────────────────────────────────

export function useHistoricalFilter() {
  function isHistoricalFile(name: string): boolean {
    return HISTORICAL_PATTERNS.some((p) => p.test(name))
  }

  function toggle(): void {
    showHistorical.value = !showHistorical.value
    saveToStorage(showHistorical.value)
  }

  return {
    showHistorical,
    isHistoricalFile,
    toggle,
  }
}
