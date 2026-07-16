<script setup lang="ts">
import { ref, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  checkUpgrade,
  performUpgrade,
  healthCheck,
  type UpdateInfo,
} from '../../services/api'
import { Download, RefreshCw, Upload, AlertCircle, CheckCircle2 } from 'lucide-vue-next'

defineProps<{
  version: string
}>()

const { t } = useI18n()

// ── Upgrade state ──
const checking = ref(false)
const updateInfo = ref<UpdateInfo | null>(null)
const checkError = ref('')

const upgrading = ref(false)
const upgradeMessage = ref('')
const upgradeError = ref('')

// Flags for the terminal states shown after a completed/failed upgrade flow.
const upgradeSucceeded = ref(false)

let pollTimer: ReturnType<typeof setTimeout> | null = null

async function handleCheck() {
  checking.value = true
  checkError.value = ''
  updateInfo.value = null
  try {
    updateInfo.value = await checkUpgrade()
  } catch (e) {
    checkError.value = e instanceof Error ? e.message : String(e)
  } finally {
    checking.value = false
  }
}

async function handleUpgrade() {
  if (!updateInfo.value?.hasUpdate || !updateInfo.value.supported) return

  const confirmed = window.confirm(
    t('settings.upgradeConfirm', { version: updateInfo.value.latestVersion }),
  )
  if (!confirmed) return

  upgrading.value = true
  upgradeError.value = ''
  upgradeMessage.value = t('settings.upgrading')

  try {
    await performUpgrade()
    upgradeMessage.value = t('settings.restarting')
    // Poll /api/health until the server comes back after restart.
    await pollUntilReady()
    upgradeSucceeded.value = true
    // Reload to pick up the new frontend bundle baked into the new binary.
    window.location.reload()
  } catch (e) {
    upgradeError.value = e instanceof Error ? e.message : String(e)
    upgradeMessage.value = ''
    upgrading.value = false
  }
}

/// Poll health every 1s for up to 30s. The server is unreachable during restart,
/// so HTTP polling is more reliable than WS (which would also be severed).
async function pollUntilReady(): Promise<void> {
  const maxAttempts = 30
  for (let i = 0; i < maxAttempts; i++) {
    try {
      await healthCheck()
      return // server is back
    } catch {
      await new Promise((r) => setTimeout(r, 1000))
    }
  }
  throw new Error(t('settings.restartTimeout'))
}

onUnmounted(() => {
  if (pollTimer) clearTimeout(pollTimer)
})
</script>

<template>
  <div class="about-content">
    <div class="about-logo">
      <img src="/logo-192x192.png" alt="tailr" width="48" height="48" />
    </div>
    <div class="about-name">tailr</div>
    <div class="about-version">v{{ version }}</div>
    <div class="about-desc">{{ t('settings.description') }}</div>
    <a class="about-link" href="https://github.com/flolibio/tailr" target="_blank" rel="noopener">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
      GitHub
    </a>

    <!-- ── Update check / upgrade ── -->
    <div class="update-section">
      <!-- Initial: check button -->
      <button
        v-if="!updateInfo && !checking"
        class="btn-check"
        @click="handleCheck"
      >
        <RefreshCw :size="14" :stroke-width="2" />
        {{ t('settings.checkUpdate') }}
      </button>

      <!-- Checking -->
      <div v-else-if="checking" class="update-status">
        <span class="spinner" />
        <span>{{ t('settings.checking') }}</span>
      </div>

      <!-- Check error -->
      <div v-else-if="checkError" class="update-status error">
        <AlertCircle :size="14" :stroke-width="2" />
        <span>{{ checkError }}</span>
        <button class="btn-retry" @click="handleCheck">{{ t('settings.checkUpdate') }}</button>
      </div>

<!-- Up to date -->
      <div v-else-if="updateInfo && !updateInfo.hasUpdate" class="update-status ok">
        <CheckCircle2 :size="14" :stroke-width="2" />
        <span>{{ t('settings.latestVersion') }}</span>
      </div>

      <!-- Has update, platform supported: upgrade -->
      <div
        v-else-if="updateInfo && updateInfo.hasUpdate && updateInfo.supported"
        class="update-available"
      >
        <div class="update-info">
          <span class="update-label">{{ t('settings.newVersionAvailable') }}</span>
          <span class="version-pair">
            <span class="version-old">v{{ updateInfo.currentVersion }}</span>
            <span class="version-arrow">→</span>
            <span class="version-new">v{{ updateInfo.latestVersion }}</span>
          </span>
        </div>
        <button
          v-if="!upgrading && !upgradeSucceeded"
          class="btn-upgrade"
          @click="handleUpgrade"
        >
          <Upload :size="14" :stroke-width="2" />
          {{ t('settings.upgradeTo') }} v{{ updateInfo.latestVersion }}
        </button>
      </div>

      <!-- Has update, platform unsupported: download link only -->
      <div
        v-else-if="updateInfo && updateInfo.hasUpdate && !updateInfo.supported"
        class="update-available unsupported"
      >
        <div class="update-info">
          <span class="update-label">{{ t('settings.newVersionAvailable') }}</span>
          <span class="version-pair">
            <span class="version-old">v{{ updateInfo.currentVersion }}</span>
            <span class="version-arrow">→</span>
            <span class="version-new">v{{ updateInfo.latestVersion }}</span>
          </span>
          <span class="update-hint">{{ t('settings.upgradeUnsupported') }}</span>
        </div>
        <a
          class="btn-download"
          :href="updateInfo.releaseUrl"
          target="_blank"
          rel="noopener"
        >
          <Download :size="14" :stroke-width="2" />
          {{ t('settings.manualDownload') }}
        </a>
      </div>

      <!-- Upgrading / restarting progress -->
      <div v-if="upgrading" class="upgrade-progress">
        <span class="spinner" />
        <span>{{ upgradeMessage }}</span>
      </div>

      <!-- Upgrade error -->
      <div v-if="upgradeError" class="update-status error">
        <AlertCircle :size="14" :stroke-width="2" />
        <span>{{ upgradeError }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.about-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 40px 0;
}

.about-logo {
  color: var(--accent);
  opacity: 0.8;
}

.about-name {
  font-size: 20px;
  font-weight: 600;
  color: var(--text);
}

.about-version {
  font-size: 14px;
  color: var(--text-3);
}

.about-desc {
  font-size: 14px;
  color: var(--text-2);
}

.about-link {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  padding: 8px 16px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-2);
  text-decoration: none;
  font-size: 12px;
  transition: all 0.12s;
}

.about-link:hover {
  background: var(--bg-3);
  border-color: var(--border-2);
  color: var(--text);
}

/* ── Update / upgrade ── */
.update-section {
  margin-top: 16px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  width: 100%;
  max-width: 360px;
}

.btn-check {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 16px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-2);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.12s;
}

.btn-check:hover {
  background: var(--bg-3);
  border-color: var(--border-2);
  color: var(--text);
}

.update-status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-2);
  flex-wrap: wrap;
  justify-content: center;
}

.update-status.ok {
  color: #22c55e;
}

.update-status.error {
  color: var(--c-error-text);
}

.btn-retry {
  height: 24px;
  padding: 0 10px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-3);
  font-size: 11px;
  cursor: pointer;
}

.btn-retry:hover {
  color: var(--text);
}

.update-available {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 12px 16px;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-2);
  width: 100%;
}

.update-info {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.update-label {
  font-size: 12px;
  color: var(--text-3);
}

.version-pair {
  display: flex;
  align-items: center;
  gap: 8px;
  font-family: var(--font-mono);
  font-size: 13px;
}

.version-old {
  color: var(--text-3);
}

.version-arrow {
  color: var(--text-3);
}

.version-new {
  color: var(--accent);
  font-weight: 600;
}

.update-hint {
  font-size: 11px;
  color: var(--text-3);
  margin-top: 2px;
  text-align: center;
}

.btn-upgrade {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 16px;
  border-radius: var(--radius);
  border: 1px solid var(--accent);
  background: var(--accent);
  color: #fff;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.12s;
}

.btn-upgrade:hover:not(:disabled) {
  opacity: 0.9;
}

.btn-download {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 16px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-2);
  font-size: 12px;
  text-decoration: none;
  cursor: pointer;
  transition: all 0.12s;
}

.btn-download:hover {
  background: var(--bg-3);
  border-color: var(--border-2);
  color: var(--text);
}

.upgrade-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-2);
}

/* ── Spinner ── */
.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid var(--border-2);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
  flex-shrink: 0;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
