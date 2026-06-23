import { ref, readonly } from 'vue'

const TOKEN_KEY = 'tailr-token'
const showTokenDialog = ref(false)
const token = ref(localStorage.getItem(TOKEN_KEY) || '')

export function useAuth() {
  function getToken(): string {
    return token.value
  }

  function saveToken(newToken: string): void {
    token.value = newToken
    if (newToken) {
      localStorage.setItem(TOKEN_KEY, newToken)
    } else {
      localStorage.removeItem(TOKEN_KEY)
    }
    showTokenDialog.value = false
  }

  function handleAuthError(): void {
    showTokenDialog.value = true
  }

  function dismissDialog(): void {
    showTokenDialog.value = false
  }

  return {
    token: readonly(token),
    showTokenDialog: readonly(showTokenDialog),
    getToken,
    saveToken,
    handleAuthError,
    dismissDialog,
  }
}
