import type { LogEntry } from '../services/api'

/**
 * Filter log entries by level and keyword. Shared by App.vue (statusbar counts)
 * and LogPanel.vue (viewer rendering) so the two never silently desync.
 *
 * - Levels: if a non-full subset is selected, only matching levels pass.
 *   A full selection (all levels) is treated as "no filter" for backward compat.
 * - Keywords: case-insensitive substring match, ALL keywords must match (AND).
 */
export function filterEntries(
  entries: LogEntry[],
  selectedLevels: string[],
  filterKeywords: string[],
  allLevels: string[],
): LogEntry[] {
  let result = entries

  if (selectedLevels.length > 0 && selectedLevels.length < allLevels.length) {
    const levelSet = new Set(selectedLevels)
    result = result.filter((e) => levelSet.has(e.level))
  }

  if (filterKeywords.length > 0) {
    const lowerKws = filterKeywords.map((k) => k.toLowerCase())
    result = result.filter((e) => {
      const lower = e.raw.toLowerCase()
      return lowerKws.every((kw) => lower.includes(kw))
    })
  }

  return result
}
