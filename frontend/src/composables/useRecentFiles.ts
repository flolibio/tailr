import { ref } from 'vue'

const MAX_RECENT = 15
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
    const filtered = recentFiles.value.filter((f) => f.path !== path)
    filtered.unshift({ path, openedAt: Date.now() })
    if (filtered.length > MAX_RECENT) {
      filtered.splice(MAX_RECENT)
    }
    recentFiles.value = filtered
    saveToStorage(filtered)
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
