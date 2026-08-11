<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  checkUpgrade,
  performUpgrade,
  healthCheck,
  ApiError,
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

/// On mount, check if an upgrade is already in progress (e.g. after page
/// refresh). If so, resume the "waiting for restart" polling so the user sees
/// the spinner and cannot trigger a second upgrade.
onMounted(async () => {
  try {
    const health = await healthCheck()
    if (health.upgradeInProgress) {
      upgrading.value = true
      upgradeMessage.value = t('settings.upgrading')
      // Resume polling — the detached task is still running server-side.
      waitForRestart(health.version).catch((e) => {
        upgradeError.value = e instanceof Error ? e.message : String(e)
        upgrading.value = false
      })
    }
  } catch {
    // Server unreachable on mount — not our concern, App.vue handles connection state.
  }
})

async function handleCheck() {
  checking.value = true
  checkError.value = ''
  updateInfo.value = null
  try {
    // Manual check must bypass the backend cache — otherwise a stale "up to
    // date" result (cached before a new release was published) hides real
    // updates for up to 6h. The background poll still uses the cache.
    updateInfo.value = await checkUpgrade(true)
  } catch (e) {
    checkError.value = e instanceof Error ? e.message : String(e)
  } finally {
    checking.value = false
  }
}

async function handleUpgrade() {
  if (!updateInfo.value?.hasUpdate || !updateInfo.value.supported) return
  if (upgrading.value) return // double-click guard

  upgrading.value = true
  upgradeError.value = ''
  upgradeMessage.value = t('settings.upgrading')

  const currentVersion = updateInfo.value.currentVersion

  try {
    await performUpgrade()
    // Detached mode: backend returns immediately with status:"started".
    // Don't reload — poll health until the upgrade completes + server restarts.
    await waitForRestart(currentVersion)
    upgradeSucceeded.value = true
    // Reload to pick up the new frontend bundle baked into the new binary.
    window.location.reload()
  } catch (e) {
    // ApiError carries a stable SCREAMING_SNAKE code from the backend; map it
    // to a localized message. Unknown codes / network errors fall back to the
    // raw message string.
    const code = e instanceof ApiError && e.code ? e.code : (e instanceof Error ? e.message : String(e))
    upgradeError.value = mapUpgradeError(code)
    upgradeMessage.value = ''
    upgrading.value = false
  }
}

/// Poll /api/health through the full upgrade lifecycle:
///   Phase 1: upgradeInProgress=true  → "downloading + installing" (spinner)
///   Phase 2: requests fail           → server restarting (spinner)
///   Phase 3: requests succeed + version changed → upgrade done, caller reloads
///   Timeout: 6 minutes (5min backend UPGRADE_TIMEOUT + 1min restart buffer)
///
/// `preUpgradeVersion` is the version before the upgrade, used to detect when
/// the server has restarted with the new binary.
async function waitForRestart(preUpgradeVersion: string): Promise<void> {
  const maxAttempts = 360 // 6 min @ 1s interval
  let sawUpgradeInProgress = false
  let sawServerDown = false

  for (let i = 0; i < maxAttempts; i++) {
    try {
      const health = await healthCheck()

      if (health.upgradeInProgress) {
        // Phase 1: detached task still running (download/replace).
        sawUpgradeInProgress = true
        upgradeMessage.value = t('settings.upgrading')
        await sleep(1000)
        continue
      }

      if (sawUpgradeInProgress || sawServerDown) {
        // upgradeInProgress flipped to false (or server came back after restart).
        // If the version changed → upgrade succeeded. If version is the same
        // but we saw the flag → upgrade may have failed mid-way; treat as done
        // and let the caller reload to reflect reality.
        if (health.version !== preUpgradeVersion) {
          upgradeMessage.value = t('settings.restarting')
          return // success — new version is live
        }
        // Flag cleared but version unchanged — upgrade task ended without
        // replacing the binary (e.g. ALREADY_UP_TO_DATE race). Stop waiting.
        throw new Error(t('settings.upgradeNoChange'))
      }

      // No upgrade in progress and we never saw it — the request was very fast
      // or we missed the flag. Check version: if changed, done; else keep waiting.
      if (health.version !== preUpgradeVersion) {
        return
      }
      // Still on old version, flag never seen — give it a few seconds in case
      // we raced ahead of the detached task setting the flag.
      await sleep(1000)
    } catch {
      // Phase 2: server unreachable → restarting.
      sawServerDown = true
      upgradeMessage.value = t('settings.restarting')
      await sleep(1000)
    }
  }
  throw new Error(t('settings.restartTimeout'))
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms))
}

/// Map backend upgrade error codes to upgrade-specific localized messages.
/// These are richer than the generic errors.* keys (e.g. they mention sudo,
/// Token configuration) — tailored to the upgrade panel's context.
/// Unknown codes / network errors fall back to the error's message string.
const UPGRADE_ERROR_CODE_MAP: Record<string, string> = {
  UNSUPPORTED_PLATFORM: 'settings.upgradeUnsupported',
  PERMISSION_DENIED: 'settings.permissionDenied',
  TOKEN_REQUIRED: 'settings.upgradeRequiresToken',
  UPGRADE_IN_PROGRESS: 'settings.upgradeInProgress',
}

function mapUpgradeError(code: string): string {
  const key = UPGRADE_ERROR_CODE_MAP[code]
  return key ? t(key) : code
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

    <!-- Version + inline check-update control -->
    <div class="version-row">
      <span class="about-version">v{{ version }}</span>

      <!-- Initial: check button -->
      <button
        v-if="!updateInfo && !checking"
        class="btn-check"
        @click="handleCheck"
      >
        <RefreshCw :size="12" :stroke-width="2" />
        {{ t('settings.checkUpdate') }}
      </button>

      <!-- Checking -->
      <span v-else-if="checking" class="version-inline-status">
        <span class="spinner" />
        {{ t('settings.checking') }}
      </span>

      <!-- Up to date -->
      <span v-else-if="updateInfo && !updateInfo.hasUpdate" class="version-inline-status ok">
        <CheckCircle2 :size="12" :stroke-width="2" />
        {{ t('settings.latestVersion') }}
      </span>

      <!-- Has update: text hint only (version details in the action panel below) -->
      <span v-else-if="updateInfo && updateInfo.hasUpdate" class="version-inline-status new">
        {{ t('settings.updateDetected') }}
      </span>

      <!-- Check error -->
      <span v-else-if="checkError" class="version-inline-status error">
        <AlertCircle :size="12" :stroke-width="2" />
        <button class="btn-retry-inline" @click="handleCheck">{{ t('settings.checkUpdate') }}</button>
      </span>
    </div>

    <div class="about-desc">{{ t('settings.description') }}</div>
    <a class="about-link" href="https://github.com/flolibio/tailr" target="_blank" rel="noopener">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
      GitHub
    </a>

    <!-- ── Upgrade action panel (only when an update is available) ── -->
    <div v-if="updateInfo && updateInfo.hasUpdate" class="update-section">
      <!-- Has update, platform supported: upgrade -->
      <div v-if="updateInfo.supported" class="update-available">
        <div class="update-info">
          <span class="update-label">{{ t('settings.newVersionAvailable') }}</span>
          <span class="version-pair">
            <span class="version-old">v{{ updateInfo.currentVersion }}</span>
            <span class="version-arrow">→</span>
            <span class="version-new">v{{ updateInfo.latestVersion }}</span>
          </span>
          <!-- 固定说明：想看更新内容，跳转 GitHub release 页面 -->
          <a
            class="release-notes-link"
            :href="updateInfo.releaseUrl"
            target="_blank"
            rel="noopener"
          >
            {{ t('settings.viewReleaseNotes') }}
          </a>
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
      <div v-else class="update-available unsupported">
        <div class="update-info">
          <span class="update-label">{{ t('settings.newVersionAvailable') }}</span>
          <span class="version-pair">
            <span class="version-old">v{{ updateInfo.currentVersion }}</span>
            <span class="version-arrow">→</span>
            <span class="version-new">v{{ updateInfo.latestVersion }}</span>
          </span>
          <span class="update-hint">{{ t('settings.upgradeUnsupported') }}</span>
          <!-- 固定说明：想看更新内容，跳转 GitHub release 页面 -->
          <a
            class="release-notes-link"
            :href="updateInfo.releaseUrl"
            target="_blank"
            rel="noopener"
          >
            {{ t('settings.viewReleaseNotes') }}
          </a>
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

/* ── Version row: version + inline check-update control ── */
.version-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.about-version {
  font-size: 14px;
  color: var(--text-2);
}

.version-inline-status {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--text-3);
}

.version-inline-status.ok {
  color: #22c55e;
}

.version-inline-status.new {
  color: var(--accent);
  font-weight: 500;
  font-family: var(--font-mono);
}

.version-inline-status.error {
  color: var(--c-error-text);
}

.version-arrow {
  color: var(--text-3);
}

.btn-check {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 8px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-2);
  font-size: 11px;
  cursor: pointer;
  transition: all 0.12s;
}

.btn-check:hover {
  background: var(--bg-3);
  border-color: var(--border-2);
  color: var(--text);
}

.btn-retry-inline {
  border: none;
  background: transparent;
  color: inherit;
  font-size: 11px;
  cursor: pointer;
  text-decoration: underline;
  padding: 0;
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

/* ── Upgrade action panel (only when an update is available) ── */
.update-section {
  margin-top: 16px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  width: 100%;
  max-width: 360px;
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

.update-status.error {
  color: var(--c-error-text);
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

/* ── Release notes 跳转链接（统一固定说明，引导去 GitHub 查看） ── */
.release-notes-link {
  margin-top: 8px;
  font-size: 11px;
  color: var(--text-3);
  text-decoration: none;
  transition: color 0.12s;
}

.release-notes-link:hover {
  color: var(--text);
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
