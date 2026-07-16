<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useToast, type Toast, type ToastPosition } from '../composables/useToast'
import {
  CheckCircle2,
  Info,
  AlertTriangle,
  AlertCircle,
  Loader2,
  X,
} from 'lucide-vue-next'

const { t: tt } = useI18n()

const { toasts, dismiss } = useToast()

// Default position for the container; individual toasts may override.
// Kept simple: render a container per distinct position present in the queue.
const POSITION_GROUPS: ToastPosition[] = [
  'top-left',
  'top-center',
  'top-right',
  'bottom-left',
  'bottom-center',
  'bottom-right',
]

const groups = computed(() =>
  POSITION_GROUPS.map((pos) => ({
    pos,
    items: toasts.value.filter((t) => t.position === pos),
  })).filter((g) => g.items.length > 0),
)

const TYPE_ICON = {
  info: Info,
  success: CheckCircle2,
  warning: AlertTriangle,
  error: AlertCircle,
  loading: Loader2,
} as const

function iconFor(t: Toast) {
  return TYPE_ICON[t.type]
}

function stopTimer(_t: Toast) {
  // Pause-on-hover is handled per-item via CSS; timer pausing would require
  // composable support. Kept out of scope: auto-dismiss continues.
}
</script>

<template>
  <teleport to="body">
    <div
      v-for="g in groups"
      :key="g.pos"
      class="toast-region"
      :class="g.pos"
      role="region"
      aria-live="polite"
    >
      <transition-group name="toast">
        <div
          v-for="t in g.items"
          :key="t.id"
          class="toast"
          :class="[t.type]"
          @mouseenter="stopTimer(t)"
        >
          <component
            :is="iconFor(t)"
            :size="16"
            :stroke-width="2"
            class="toast-icon"
            :class="{ spin: t.type === 'loading' }"
          />
          <div class="toast-body">
            <div v-if="t.title" class="toast-title">{{ t.title }}</div>
            <div class="toast-message">{{ t.message }}</div>
          </div>
          <button
            v-if="t.action"
            class="toast-action"
            @click="t.action!.onClick(); dismiss(t.id)"
          >
            {{ t.action!.label }}
          </button>
          <button
            v-if="t.closeButton"
            class="toast-close"
            :title="tt('toast.close')"
            @click="dismiss(t.id)"
          >
            <X :size="14" :stroke-width="2" />
          </button>
        </div>
      </transition-group>
    </div>
  </teleport>
</template>

<style scoped>
/* ── Positioning regions ── */
.toast-region {
  position: fixed;
  z-index: 2000;
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: 360px;
  pointer-events: none; /* let clicks pass through gaps; toasts re-enable */
}

.toast-region.top-left { top: 16px; left: 16px; }
.toast-region.top-center { top: 16px; left: 50%; transform: translateX(-50%); }
.toast-region.top-right { top: 16px; right: 16px; }
.toast-region.bottom-left { bottom: 16px; left: 16px; flex-direction: column-reverse; }
.toast-region.bottom-center { bottom: 16px; left: 50%; transform: translateX(-50%); flex-direction: column-reverse; }
.toast-region.bottom-right { bottom: 16px; right: 16px; flex-direction: column-reverse; }

/* ── Toast card ── */
.toast {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  background: var(--bg);
  border: 1px solid var(--border-2);
  border-radius: var(--radius-lg);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12), 0 2px 6px rgba(0, 0, 0, 0.06);
  pointer-events: auto;
  min-width: 260px;
  max-width: 360px;
}

.toast-icon {
  flex-shrink: 0;
  margin-top: 1px;
}

.toast.info .toast-icon { color: var(--accent); }
.toast.success .toast-icon { color: #22c55e; }
.toast.warning .toast-icon { color: #f59e0b; }
.toast.error .toast-icon { color: var(--c-error-text); }
.toast.loading .toast-icon { color: var(--text-3); }

.toast-icon.spin {
  animation: spin 0.8s linear infinite;
}

.toast-body {
  flex: 1;
  min-width: 0;
}

.toast-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  margin-bottom: 2px;
}

.toast-message {
  font-size: 13px;
  color: var(--text-2);
  line-height: 1.4;
  word-break: break-word;
}

/* ── Action button ── */
.toast-action {
  flex-shrink: 0;
  align-self: center;
  height: 26px;
  padding: 0 10px;
  border: 1px solid var(--border-2);
  border-radius: var(--radius);
  background: transparent;
  color: var(--accent);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.12s;
  white-space: nowrap;
}

.toast-action:hover {
  background: var(--accent-light);
  border-color: var(--accent);
}

/* ── Close button ── */
.toast-close {
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: all 0.12s;
  padding: 0;
  margin-top: 1px;
}

.toast-close:hover {
  background: var(--bg-3);
  color: var(--text);
}

/* ── Transitions (vue-sonner-inspired slide + fade) ── */
.toast-enter-active,
.toast-leave-active {
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.25s ease;
}

.toast-enter-from {
  opacity: 0;
}

.toast-leave-to {
  opacity: 0;
  transform: translateX(0) scale(0.96);
}

/* Slide-in direction matches the region */
.top-left .toast-enter-from,
.bottom-left .toast-enter-from {
  transform: translateX(-120%);
}
.top-right .toast-enter-from,
.bottom-right .toast-enter-from {
  transform: translateX(120%);
}
.top-center .toast-enter-from {
  transform: translateY(-120%);
}
.bottom-center .toast-enter-from {
  transform: translateY(120%);
}

/* Leave animation for list reflow */
.toast-leave-active {
  position: absolute;
  width: 100%;
}

.toast-move {
  transition: transform 0.25s ease;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
