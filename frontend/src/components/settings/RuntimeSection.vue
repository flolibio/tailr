<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { fetchRuntime, type RuntimeData } from '../../services/api'
import { Server, Package } from 'lucide-vue-next'

const { t } = useI18n()
const data = ref<RuntimeData | null>(null)
const loading = ref(true)
const error = ref('')
const lastUpdated = ref('')
const liveDotRef = ref<HTMLElement | null>(null)
let timer: number | undefined

async function load(): Promise<void> {
  try {
    data.value = await fetchRuntime()
    error.value = ''
    lastUpdated.value = new Date().toLocaleTimeString([], { hour12: false })
    // Flash the live dot to signal a fresh sample.
    await nextTick()
    flashDot()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

function flashDot(): void {
  const el = liveDotRef.value
  if (!el) return
  el.classList.remove('flash')
  // Force reflow so re-adding the class restarts the animation.
  void el.offsetWidth
  el.classList.add('flash')
}

onMounted(() => {
  void load()
  timer = window.setInterval(() => void load(), 5000)
})

onUnmounted(() => {
  if (timer !== undefined) clearInterval(timer)
})

/** Split a byte count into a numeric value + unit pair for inline display:
 *  e.g. 19293798 => "18.4" + "MB". Returns [value, unit]. */
function splitBytes(bytes: number): [string, string] {
  if (!bytes) return ['0', 'B']
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return [(bytes / Math.pow(1024, i)).toFixed(1), units[i]]
}

/** Split uptime into [primaryValue, unit] so the largest unit renders big
 *  (card-value) and the remainder small (card-unit), matching the value+unit
 *  style of other metric cards.
 *  e.g. 7560s → ["2", "h 6m"],  600s → ["10", "m"],  90061s → ["1", "d 1h"] */
function splitUptime(seconds: number): [string, string] {
  const d = Math.floor(seconds / 86400)
  const h = Math.floor((seconds % 86400) / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  if (d > 0) {
    const rest = h > 0 ? `${h}h` : ''
    return [`${d}`, rest ? `d ${rest}` : 'd']
  }
  if (h > 0) {
    const rest = m > 0 ? `${m}m` : ''
    return [`${h}`, rest ? `h ${rest}` : 'h']
  }
  return [`${m}`, 'm']
}

function percent(used: number, total: number): number {
  return total > 0 ? (used / total) * 100 : 0
}

// ── computed display values (each split called once per render, not twice) ──
const procMem = computed(() => splitBytes(data.value?.processMemoryBytes ?? 0))
const sysMemUsed = computed(() => splitBytes(data.value?.systemUsedMemoryBytes ?? 0))
const sysMemTotal = computed(() => splitBytes(data.value?.systemTotalMemoryBytes ?? 0))
const sysMemPct = computed(() => percent(data.value?.systemUsedMemoryBytes ?? 0, data.value?.systemTotalMemoryBytes ?? 0))
const diskUsed = computed(() => splitBytes(data.value?.diskUsedBytes ?? 0))
const diskTotal = computed(() => splitBytes(data.value?.diskTotalBytes ?? 0))
const diskPct = computed(() => percent(data.value?.diskUsedBytes ?? 0, data.value?.diskTotalBytes ?? 0))
const uptime = computed(() => splitUptime(data.value?.uptimeSeconds ?? 0))
const sysCpuClamped = computed(() => Math.min(data.value?.systemCpuPercent ?? 0, 100))
</script>

<template>
  <div class="runtime-section">
    <!-- Header: description + live indicator (pulsing dot + timestamp) -->
    <div class="content-header">
      <div class="content-sub">{{ t('settings.runtimeDesc') }}</div>
      <div v-if="data" class="live-indicator">
        <span ref="liveDotRef" class="live-dot" />
        <span class="live-time">{{ t('settings.runtimeUpdatedAt') }} {{ lastUpdated }}</span>
      </div>
    </div>

    <div v-if="loading" class="state-msg">{{ t('app.loading') }}</div>
    <div v-else-if="error" class="state-msg error">{{ t('errors.runtimeFetchFailed') }}</div>

    <template v-else-if="data">
      <!-- ── tailr process metrics (4-col grid, value-only cards) ── -->
      <div class="section-label">
        <Package :size="14" :stroke-width="2" />
        <span>{{ t('settings.runtimeProcessGroup') }}</span>
      </div>
      <div class="card-grid cols-4">
        <div class="card">
          <div class="card-label">{{ t('settings.runtimeProcessMemory') }}</div>
          <div class="card-value">
            {{ procMem[0] }}<span class="card-unit"> {{ procMem[1] }}</span>
          </div>
        </div>
        <div class="card">
          <div class="card-label">{{ t('settings.runtimeProcessCpu') }}</div>
          <div class="card-value">{{ data.processCpuPercent.toFixed(1) }}<span class="card-unit">%</span></div>
        </div>
        <div class="card">
          <div class="card-label">{{ t('settings.runtimeUptime') }}</div>
          <div class="card-value">
            {{ uptime[0] }}<span class="card-unit"> {{ uptime[1] }}</span>
          </div>
        </div>
        <div class="card">
          <div class="card-label">{{ t('settings.runtimeWsConnections') }}</div>
          <div class="card-value">{{ data.wsConnections }}</div>
        </div>
      </div>

      <!-- ── server metrics (3-col grid, cards with progress bars) ── -->
      <div class="section-label">
        <Server :size="14" :stroke-width="2" />
        <span>{{ t('settings.runtimeServerGroup') }}</span>
      </div>
      <div class="card-grid cols-3">
        <div class="card card--bar">
          <div class="card-label">{{ t('settings.runtimeSystemMemory') }}</div>
          <div class="card-value">
            {{ sysMemUsed[0] }}
            <span class="card-unit">/ {{ sysMemTotal[0] }} {{ sysMemTotal[1] }}</span>
          </div>
          <div class="bar-track">
            <div
              class="bar-fill"
              :style="{ width: sysMemPct + '%' }"
            />
          </div>
          <div class="bar-caption">{{ sysMemPct.toFixed(0) }}%</div>
        </div>

        <div class="card card--bar">
          <div class="card-label">{{ t('settings.runtimeSystemCpu') }}</div>
          <div class="card-value">{{ data.systemCpuPercent.toFixed(1) }}<span class="card-unit">%</span></div>
          <div class="bar-track">
            <div
              class="bar-fill"
              :style="{ width: sysCpuClamped + '%' }"
            />
          </div>
          <div class="bar-caption">{{ sysCpuClamped.toFixed(0) }}%</div>
        </div>

        <div class="card card--bar">
          <div class="card-label">{{ t('settings.runtimeDisk') }}</div>
          <div class="card-value">
            {{ diskUsed[0] }}
            <span class="card-unit">/ {{ diskTotal[0] }} {{ diskTotal[1] }}</span>
          </div>
          <div class="bar-track">
            <div
              class="bar-fill"
              :class="{ warning: diskPct > 80 }"
              :style="{ width: diskPct + '%' }"
            />
          </div>
          <div class="bar-caption">{{ diskPct.toFixed(0) }}%</div>
        </div>
      </div>

      <div class="footnote">{{ t('settings.runtimeServerHint') }}</div>
    </template>
  </div>
</template>

<style scoped>
.runtime-section {
  display: flex;
  flex-direction: column;
}

/* ── header: description + live indicator ── */
.content-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 18px;
}

.content-sub {
  font-size: 13px;
  color: var(--text-2);
}

.live-indicator {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px 3px 8px;
  border-radius: 999px;
  background: var(--bg-2);
  flex-shrink: 0;
}

.live-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #1d9e75;
  position: relative;
}

.live-dot::after {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: 50%;
  background: #1d9e75;
}

.live-dot.flash::after {
  animation: pulse-flash 0.6s ease-out;
}

@keyframes pulse-flash {
  0%   { transform: scale(1);   opacity: 0.7; }
  100% { transform: scale(2.8); opacity: 0;   }
}

.live-time {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-3);
  white-space: nowrap;
}

/* ── section label ── */
.section-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-2);
  text-transform: uppercase;
  letter-spacing: 0.03em;
  margin-bottom: 10px;
}

.section-label :deep(svg) {
  color: var(--accent);
}

/* ── card grid ── */
.card-grid {
  display: grid;
  gap: 10px;
  margin-bottom: 22px;
}

.card-grid.cols-4 { grid-template-columns: repeat(4, minmax(0, 1fr)); }
.card-grid.cols-3 { grid-template-columns: repeat(3, minmax(0, 1fr)); }

.card {
  background: var(--bg-2);
  border-radius: 10px;
  padding: 12px 14px;
}

/* Cards with a progress bar use flex column so the bar-caption sticks to the
 * bottom, keeping bars aligned across the row. */
.card--bar {
  display: flex;
  flex-direction: column;
}

.card-label {
  font-size: 12px;
  color: var(--text-2);
  margin-bottom: 6px;
}

.card-value {
  font-family: var(--font-mono);
  font-size: 18px;
  font-weight: 600;
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

.card-unit {
  font-size: 12px;
  font-weight: 400;
  color: var(--text-2);
}

/* ── progress bar ── */
.bar-track {
  height: 4px;
  background: var(--border);
  border-radius: 2px;
  margin-top: 9px;
  overflow: hidden;
}

.bar-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 2px;
  transition: width 0.3s ease;
}

.bar-fill.warning {
  background: #f59e0b;
}

.bar-caption {
  font-size: 11px;
  color: var(--text-3);
  margin-top: 5px;
  font-family: var(--font-mono);
}

/* ── footnote ── */
.footnote {
  font-size: 12px;
  color: var(--text-3);
  margin-top: 4px;
}

/* ── loading / error state ── */
.state-msg {
  padding: 24px 0;
  text-align: center;
  font-size: 13px;
  color: var(--text-3);
}

.state-msg.error {
  color: var(--c-error-text);
}
</style>
