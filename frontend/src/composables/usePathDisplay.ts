/**
 * 文件列表全路径显示开关
 *
 * 文件树默认只显示文件名，遇到重名目录（多个项目都有 app/access.log）
 * 时无法区分。打开后文件行与 Recent 列表直接显示完整路径
 *（超宽省略号截断，遵循横向不滚动的布局规则）。
 *
 * 默认关闭（保持原有文件名视图）。持久化到 localStorage，记住用户选择。
 */
import { ref } from 'vue'

// ── localStorage ───────────────────────────────────────────

const STORAGE_KEY = 'tailr-show-full-path'

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

// ── 模块级状态（单例）─────────────────────────────────────

const showFullPath = ref(loadFromStorage())

// ── Composable ────────────────────────────────────────────

export function usePathDisplay() {
  function toggle(): void {
    showFullPath.value = !showFullPath.value
    saveToStorage(showFullPath.value)
  }

  return {
    showFullPath,
    toggle,
  }
}
