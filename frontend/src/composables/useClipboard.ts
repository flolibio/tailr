import { ref, shallowRef, onUnmounted } from 'vue'

export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(text)
      return true
    }
    const textarea = document.createElement('textarea')
    textarea.value = text
    textarea.style.position = 'fixed'
    textarea.style.left = '-9999px'
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
    return true
  } catch {
    return false
  }
}

export function useCopyFeedback(timeout = 1500) {
  const copied = ref(false)
  let timer: ReturnType<typeof setTimeout> | null = null

  async function copy(text: string): Promise<boolean> {
    const ok = await copyToClipboard(text)
    if (timer) clearTimeout(timer)
    if (ok) {
      copied.value = true
      timer = setTimeout(() => {
        copied.value = false
      }, timeout)
    }
    return ok
  }

  onUnmounted(() => {
    if (timer) clearTimeout(timer)
  })

  return { copied, copy }
}

export function useCopyFeedbackId<T>(timeout = 1500) {
  const copiedId = shallowRef<T | null>(null)
  let timer: ReturnType<typeof setTimeout> | null = null

  async function copy(text: string, id: T): Promise<boolean> {
    const ok = await copyToClipboard(text)
    if (timer) clearTimeout(timer)
    if (ok) {
      copiedId.value = id
      timer = setTimeout(() => {
        copiedId.value = null
      }, timeout)
    }
    return ok
  }

  onUnmounted(() => {
    if (timer) clearTimeout(timer)
  })

  return { copiedId, copy }
}
