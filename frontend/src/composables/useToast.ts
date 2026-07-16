import { ref, readonly } from 'vue'

// ── Types ──────────────────────────────────────────────────

export type ToastType = 'info' | 'success' | 'warning' | 'error' | 'loading'

export type ToastPosition =
  | 'top-left'
  | 'top-center'
  | 'top-right'
  | 'bottom-left'
  | 'bottom-center'
  | 'bottom-right'

export interface ToastAction {
  label: string
  onClick: () => void
}

export interface ToastOptions {
  /** Where the toast appears. Defaults to the container's position. */
  position?: ToastPosition
  /** Auto-dismiss after N ms. Set 0 or Infinity to persist. Default 4000. */
  duration?: number
  /** Optional action button rendered inline (vue-sonner style). */
  action?: ToastAction
  /** Show a close (×) button. Default false. */
  closeButton?: boolean
  /** Optional title shown above the description. */
  title?: string
}

export interface Toast extends Required<Omit<ToastOptions, 'action' | 'title' | 'duration'>> {
  id: string
  type: ToastType
  message: string
  action?: ToastAction
  title?: string
  duration: number
}

// ── Singleton state (module-level, same pattern as useAuth) ──

const toasts = ref<Toast[]>([])
const DEFAULT_DURATION = 4000
const MAX_VISIBLE = 5

let counter = 0
function nextId(): string {
  counter += 1
  return `toast-${Date.now()}-${counter}`
}

const timers = new Map<string, ReturnType<typeof setTimeout>>()

function clearTimer(id: string): void {
  const t = timers.get(id)
  if (t) {
    clearTimeout(t)
    timers.delete(id)
  }
}

function scheduleDismiss(id: string, duration: number): void {
  if (!duration || duration === Infinity) return
  clearTimer(id)
  timers.set(
    id,
    setTimeout(() => dismiss(id), duration),
  )
}

function push(type: ToastType, message: string, opts: ToastOptions = {}): string {
  const id = nextId()
  const toast: Toast = {
    id,
    type,
    message,
    position: opts.position ?? 'top-right',
    duration: opts.duration ?? DEFAULT_DURATION,
    closeButton: opts.closeButton ?? false,
    action: opts.action,
    title: opts.title,
  }
  toasts.value.push(toast)
  // Cap visible toasts; drop the oldest (and clear its timer).
  while (toasts.value.length > MAX_VISIBLE) {
    const oldest = toasts.value.shift()
    if (oldest) clearTimer(oldest.id)
  }
  scheduleDismiss(id, toast.duration)
  return id
}

function dismiss(id: string): void {
  clearTimer(id)
  toasts.value = toasts.value.filter((t) => t.id !== id)
}

function dismissAll(): void {
  for (const id of [...timers.keys()]) clearTimer(id)
  toasts.value = []
}

// ── Public API (mirrors vue-sonner's toast() surface) ──────

export function useToast() {
  return {
    toasts: readonly(toasts),
    /** Generic push. `toast('msg')` style like vue-sonner. */
    show: (message: string, opts?: ToastOptions) => push('info', message, opts),
    info: (message: string, opts?: ToastOptions) => push('info', message, opts),
    success: (message: string, opts?: ToastOptions) => push('success', message, opts),
    warning: (message: string, opts?: ToastOptions) => push('warning', message, opts),
    error: (message: string, opts?: ToastOptions) => push('error', message, opts),
    loading: (message: string, opts?: ToastOptions) => push('loading', message, opts),
    dismiss,
    dismissAll,
  }
}
