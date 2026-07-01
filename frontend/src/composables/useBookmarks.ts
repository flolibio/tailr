import { ref } from 'vue'
import type { LogEntry } from '../services/api'

const MAX_BOOKMARKS = 50
const STORAGE_KEY = 'tailr-bookmarks'

export interface Bookmark {
  lineNum: number
  preview: string
  level: string
  createdAt: number
}

function loadFromStorage(): Record<string, Bookmark[]> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw)
  } catch {}
  return {}
}

function saveToStorage(data: Record<string, Bookmark[]>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data))
  } catch {}
}

const bookmarksMap = ref<Record<string, Bookmark[]>>(loadFromStorage())

function save(): void {
  saveToStorage(bookmarksMap.value)
}

export function useBookmarks() {
  function getBookmarks(path: string): Bookmark[] {
    return bookmarksMap.value[path] ?? []
  }

  function add(path: string, entry: LogEntry): void {
    const list = bookmarksMap.value[path] ?? []
    if (list.some((b) => b.lineNum === entry.lineNum)) return
    const bookmark: Bookmark = {
      lineNum: entry.lineNum,
      preview: entry.raw.slice(0, 80),
      level: entry.level,
      createdAt: Date.now(),
    }
    list.push(bookmark)
    if (list.length > MAX_BOOKMARKS) {
      list.splice(0, list.length - MAX_BOOKMARKS)
    }
    bookmarksMap.value = { ...bookmarksMap.value, [path]: list }
    save()
  }

  function remove(path: string, lineNum: number): void {
    const list = bookmarksMap.value[path]
    if (!list) return
    const filtered = list.filter((b) => b.lineNum !== lineNum)
    if (filtered.length === 0) {
      const next = { ...bookmarksMap.value }
      delete next[path]
      bookmarksMap.value = next
    } else {
      bookmarksMap.value = { ...bookmarksMap.value, [path]: filtered }
    }
    save()
  }

  function isBookmarked(path: string, lineNum: number): boolean {
    return (bookmarksMap.value[path] ?? []).some((b) => b.lineNum === lineNum)
  }

  return {
    bookmarksMap,
    getBookmarks,
    add,
    remove,
    isBookmarked,
  }
}
