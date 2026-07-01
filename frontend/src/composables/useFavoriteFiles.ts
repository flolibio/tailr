import { ref } from 'vue'

const STORAGE_KEY = 'tailr-favorite-files'

export interface FavoriteFile {
  path: string
  favoritedAt: number
}

function loadFromStorage(): FavoriteFile[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) return JSON.parse(raw)
  } catch {}
  return []
}

function saveToStorage(data: FavoriteFile[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data))
  } catch {}
}

const favoriteFiles = ref<FavoriteFile[]>(loadFromStorage())

export function useFavoriteFiles() {
  function isFavorite(path: string): boolean {
    return favoriteFiles.value.some((f) => f.path === path)
  }

  function add(path: string): void {
    if (isFavorite(path)) return
    favoriteFiles.value = [{ path, favoritedAt: Date.now() }, ...favoriteFiles.value]
    saveToStorage(favoriteFiles.value)
  }

  function remove(path: string): void {
    favoriteFiles.value = favoriteFiles.value.filter((f) => f.path !== path)
    saveToStorage(favoriteFiles.value)
  }

  function toggle(path: string): void {
    if (isFavorite(path)) {
      remove(path)
    } else {
      add(path)
    }
  }

  return {
    favoriteFiles,
    isFavorite,
    add,
    remove,
    toggle,
  }
}
