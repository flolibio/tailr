import { ref } from 'vue'

const MAX_RECENT = 10
const STORAGE_KEY = 'tailr-recent-files'

export interface RecentFile {
  path: string
  openedAt: number
}

function loadFromStorage(): RecentFile[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw)
  } catch {}
  return []
}

function saveToStorage(data: RecentFile[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data))
  } catch {}
}

const recentFiles = ref<RecentFile[]>(loadFromStorage())

export function useRecentFiles() {
  function recordOpen(path: string): void {
    const now = Date.now()
    const existing = recentFiles.value.find((f) => f.path === path)
    if (existing) {
      // Rebuild the array rather than mutating the element in place — `ref`
      // tracks the `.value` reference, so a full replacement guarantees
      // subscribers re-render (e.g. the relative-time label).
      recentFiles.value = recentFiles.value.map((f) =>
        f.path === path ? { ...f, openedAt: now } : f,
      )
      saveToStorage(recentFiles.value)
      return
    }
    recentFiles.value = [{ path, openedAt: now }, ...recentFiles.value]
    if (recentFiles.value.length > MAX_RECENT) {
      recentFiles.value = recentFiles.value.slice(0, MAX_RECENT)
    }
    saveToStorage(recentFiles.value)
  }

  function remove(path: string): void {
    recentFiles.value = recentFiles.value.filter((f) => f.path !== path)
    saveToStorage(recentFiles.value)
  }

  return {
    recentFiles,
    recordOpen,
    remove,
  }
}
