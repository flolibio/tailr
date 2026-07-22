<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAuth } from '../composables/useAuth'
import { verifyToken } from '../services/api'
import { Lock } from 'lucide-vue-next'

const { t } = useI18n()
const { token, saveToken } = useAuth()

const inputValue = ref(token.value)
const errorMsg = ref('')
const verifying = ref(false)

async function handleSave(): Promise<void> {
  // Verify the token BEFORE saving. Without this, a wrong token was silently
  // stored and the dialog closed as if it succeeded — the user only learned it
  // was wrong when the next API call 401'd and re-prompted.
  errorMsg.value = ''
  verifying.value = true
  try {
    const ok = await verifyToken(inputValue.value)
    if (!ok) {
      errorMsg.value = t('tokenDialog.invalidToken')
      return
    }
    saveToken(inputValue.value) // saves + closes the dialog
  } catch {
    // Network error during verification — can't confirm; let the user retry.
    errorMsg.value = t('tokenDialog.verifyFailed')
  } finally {
    verifying.value = false
  }
}

function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter') {
    handleSave()
  }
  // Escape deliberately NOT handled: the dialog only appears on 401 (token
  // missing or invalid), so there's no valid prior state to dismiss to.
  // Allowing Esc/overlay-click/Cancel would close the dialog and immediately
  // re-trigger on the next API call, creating an annoying close→reopen loop.
  // The only way out is entering a valid token.
}
</script>

<template>
  <!-- No @click.self on overlay: clicking outside must NOT dismiss, see
       handleKeydown comment above. -->
  <div class="token-overlay">
    <div class="token-dialog">
      <div class="token-header">
        <Lock :size="16" :stroke-width="2" />
        <span>{{ t('tokenDialog.title') }}</span>
      </div>
      <div class="token-body">
        <p class="token-message">{{ t('tokenDialog.message') }}</p>
        <input
          v-model="inputValue"
          type="password"
          class="token-input"
          :class="{ error: errorMsg }"
          :placeholder="t('tokenDialog.placeholder')"
          autofocus
          @keydown="handleKeydown"
        />
        <p v-if="errorMsg" class="token-error">{{ errorMsg }}</p>
      </div>
      <div class="token-footer">
        <!-- No Cancel button: same rationale as the missing overlay click
             handler — there is no valid state to cancel to. -->
        <button class="btn-save" @click="handleSave" :disabled="verifying">
          {{ verifying ? t('tokenDialog.verifying') : t('tokenDialog.save') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.token-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1100;
}

.token-dialog {
  width: 400px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  overflow: hidden;
}

.token-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-2);
  color: var(--text);
  font-size: 14px;
  font-weight: 600;
}

.token-body {
  padding: 20px 16px;
}

.token-message {
  margin: 0 0 16px;
  font-size: 14px;
  color: var(--text-2);
  line-height: 1.5;
}

.token-input {
  width: 100%;
  height: 30px;
  font-size: 14px;
  font-family: var(--font-mono);
  background: var(--bg);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0 12px;
  box-sizing: border-box;
}

.token-input:focus {
  outline: none;
  border-color: var(--accent);
}

.token-input.error {
  border-color: var(--c-error-text);
}

.token-error {
  margin: 8px 0 0;
  font-size: 12px;
  color: var(--c-error-text);
  line-height: 1.4;
}

.token-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--border);
  background: var(--bg-2);
}

.btn-cancel {
  height: 30px;
  padding: 0 16px;
  font-size: 12px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--bg-3);
  color: var(--text-2);
  cursor: pointer;
  transition: all 0.12s;
}

.btn-cancel:hover {
  background: var(--bg-hover, var(--bg-3));
  border-color: var(--border-2);
  color: var(--text);
}

.btn-save {
  height: 30px;
  padding: 0 16px;
  font-size: 12px;
  border-radius: var(--radius);
  border: 1px solid var(--accent);
  background: var(--accent);
  color: #fff;
  cursor: pointer;
  transition: all 0.12s;
}

.btn-save:hover {
  opacity: 0.9;
}
</style>
