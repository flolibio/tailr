<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAuth } from '../composables/useAuth'

const { t } = useI18n()
const { token, saveToken, dismissDialog } = useAuth()

const inputValue = ref(token.value)

function handleSave(): void {
  saveToken(inputValue.value)
}

function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Enter') {
    handleSave()
  } else if (e.key === 'Escape') {
    dismissDialog()
  }
}
</script>

<template>
  <div class="token-overlay" @click.self="dismissDialog">
    <div class="token-dialog">
      <div class="token-header">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>
        </svg>
        <span>{{ t('tokenDialog.title') }}</span>
      </div>
      <div class="token-body">
        <p class="token-message">{{ t('tokenDialog.message') }}</p>
        <input
          v-model="inputValue"
          type="password"
          class="token-input"
          :placeholder="t('tokenDialog.placeholder')"
          autofocus
          @keydown="handleKeydown"
        />
      </div>
      <div class="token-footer">
        <button class="btn-cancel" @click="dismissDialog">{{ t('tokenDialog.cancel') }}</button>
        <button class="btn-save" @click="handleSave">{{ t('tokenDialog.save') }}</button>
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
  font-size: 13px;
  color: var(--text-2);
  line-height: 1.5;
}

.token-input {
  width: 100%;
  height: 36px;
  font-size: 13px;
  font-family: var(--font-mono);
  background: var(--bg-3);
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

.token-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--border);
  background: var(--bg-2);
}

.btn-cancel {
  height: 32px;
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
  height: 32px;
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
