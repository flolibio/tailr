import { ref } from 'vue'
import { searchLogs } from '../services/api'
import type { LogEntry } from '../services/api'

export interface SearchResult {
  matches: LogEntry[]
  totalMatches: number
  query: string
  elapsedMs: number
}

export function useSearch() {
  const query = ref('')
  const isRegex = ref(false)
  const levelFilter = ref<string[]>([])
  const contextLines = ref(3)
  const results = ref<SearchResult | null>(null)
  const isSearching = ref(false)
  const error = ref<string | null>(null)

  async function search(path: string): Promise<void> {
    if (!query.value.trim()) {
      results.value = null
      return
    }
    isSearching.value = true
    error.value = null
    try {
      const data = await searchLogs(path, query.value, {
        regex: isRegex.value,
        levels: levelFilter.value.length > 0 ? levelFilter.value : undefined,
        context: contextLines.value,
      })
      results.value = data
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Search failed'
      results.value = null
    } finally {
      isSearching.value = false
    }
  }

  function clear(): void {
    query.value = ''
    results.value = null
    error.value = null
  }

  return {
    query,
    isRegex,
    levelFilter,
    contextLines,
    results,
    isSearching,
    error,
    search,
    clear,
  }
}
