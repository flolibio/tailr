/**
 * 历史文件过滤开关
 *
 * 控制 file-list 中是否显示 logrotate 产生的历史日志文件
 *（编号轮转、日期命名、.bak/.old 等旧文件标记）。
 *
 * 默认隐藏（仅看实时日志），用户可 toggle 打开。
 * 持久化到 localStorage，记住用户选择。
 *
 * 例外：按日期命名且日期为当天的文件（app-20260820.log）是正在写入的
 * 活跃日志，即使开关处于隐藏状态也不过滤。
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

/** 明确的旧文件后缀标记 —— 无论日期如何都视为历史文件。 */
const OLD_FILE_MARKER: RegExp = /\.(bak|old|prev|save)$/

/**
 * 从文件名提取日期命名中的日期（YYYYMMDD 紧凑 / YYYY-MM-DD ISO）。
 * 分隔符必须是 - 或 _ 或 .（避免把任意长数字串误读为日期），
 * 日期后必须紧跟 . 或 _ 或结尾（logrotate dateext 的常见形态，
 * 如 app-20260820.log、access.log-20260820、app.log.20260820）。
 */
const DATE_NAMED_PATTERNS: RegExp[] = [
  /[-_.](\d{4})(\d{2})(\d{2})(?=[._]|$)/,
  /[-_.](\d{4})-(\d{2})-(\d{2})(?=[._]|$)/,
]

interface FileDate {
  year: number
  month: number
  day: number
}

function extractDateFromName(name: string): FileDate | null {
  for (const re of DATE_NAMED_PATTERNS) {
    const m = re.exec(name)
    if (m) {
      const year = Number(m[1])
      const month = Number(m[2])
      const day = Number(m[3])
      if (month >= 1 && month <= 12 && day >= 1 && day <= 31) {
        return { year, month, day }
      }
    }
  }
  return null
}

/** 文件名中的日期是否是今天（按本地时区）。按日期命名的日志中，当天文件仍是活跃日志。 */
function isNamedToday(name: string): boolean {
  const d = extractDateFromName(name)
  if (!d) return false
  const now = new Date()
  return d.year === now.getFullYear() && d.month === now.getMonth() + 1 && d.day === now.getDate()
}

// ── 模块级状态（单例）─────────────────────────────────────

const showHistorical = ref(loadFromStorage())

// ── Composable ────────────────────────────────────────────

export function useHistoricalFilter() {
  function isHistoricalFile(name: string): boolean {
    // .bak/.old 等标记明确表示旧文件，即使带着今天的日期也过滤
    if (OLD_FILE_MARKER.test(name)) return true
    // 按日期命名的日志（app-20260820.log）：当天的文件是正在写入的活跃日志，不过滤
    if (isNamedToday(name)) return false
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
