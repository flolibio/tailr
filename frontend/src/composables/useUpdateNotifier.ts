import { ref, readonly } from 'vue'

// localStorage keys for dedup: once a version has been notified (toast shown)
// or dismissed, we don't re-notify until a different version appears.
const NOTIFIED_KEY = 'tailr-notified-update'
const DISMISSED_KEY = 'tailr-dismissed-update'

/** Latest version we've been notified about (toast shown at least once). */
const notifiedVersion = ref<string | null>(localStorage.getItem(NOTIFIED_KEY))
/** Latest version the user explicitly dismissed (badge cleared). */
const dismissedVersion = ref<string | null>(localStorage.getItem(DISMISSED_KEY))
/** Whether the Settings gear should show the update badge dot. */
const hasUpdateBadge = ref(false)

function setNotified(v: string): void {
  notifiedVersion.value = v
  localStorage.setItem(NOTIFIED_KEY, v)
}

function setDismissed(v: string): void {
  dismissedVersion.value = v
  localStorage.setItem(DISMISSED_KEY, v)
  hasUpdateBadge.value = false
}

export interface UpdateNotice {
  latestVersion: string
  currentVersion: string
  releaseUrl: string
}

export function useUpdateNotifier() {
  return {
    notifiedVersion: readonly(notifiedVersion),
    dismissedVersion: readonly(dismissedVersion),
    hasUpdateBadge: readonly(hasUpdateBadge),
    /**
     * Called when an UpdateAvailable is received (or on load if a cached update
     * exists). Returns true if a toast should be shown for this version
     * (i.e. not previously notified).
     */
    shouldShowToast(latest: string): boolean {
      return notifiedVersion.value !== latest
    },
    /** Record that the toast was shown for this version. */
    markNotified(latest: string): void {
      setNotified(latest)
      hasUpdateBadge.value = true
    },
    /** User dismissed the badge / opened the update panel. */
    dismiss(latest: string): void {
      setDismissed(latest)
    },
    /** Clear all state (used after a successful upgrade resets versions). */
    reset(): void {
      notifiedVersion.value = null
      dismissedVersion.value = null
      hasUpdateBadge.value = false
      localStorage.removeItem(NOTIFIED_KEY)
      localStorage.removeItem(DISMISSED_KEY)
    },
  }
}
