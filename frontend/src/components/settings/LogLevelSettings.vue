<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useLogLevels, PRESET_NAMES, COLOR_PALETTE } from '../../composables/useLogLevels'
import ColorPicker from './ColorPicker.vue'

const { t } = useI18n()

const {
  config,
  switchPreset,
  addLevel,
  removeLevel,
  updateLevel,
  resetToDefault,
  syncToBackend,
} = useLogLevels()

const presetOptions = computed(() =>
  Object.entries(PRESET_NAMES).map(([key, label]) => ({ key, label }))
)

const currentPresetLabel = computed(() => {
  const found = presetOptions.value.find(p => p.key === config.value.preset)
  return found ? found.label : config.value.preset
})

const showPresetDropdown = ref(false)
const presetDropdownRef = ref<HTMLElement | null>(null)

function togglePresetDropdown() {
  showPresetDropdown.value = !showPresetDropdown.value
}

function selectPreset(key: string) {
  switchPreset(key)
  showPresetDropdown.value = false
}

function onClickOutside(e: MouseEvent) {
  if (presetDropdownRef.value && !presetDropdownRef.value.contains(e.target as Node)) {
    showPresetDropdown.value = false
  }
}

onMounted(() => document.addEventListener('click', onClickOutside))
onUnmounted(() => document.removeEventListener('click', onClickOutside))

const showColorPicker = ref(false)
const colorPickerIndex = ref(0)
const colorPickerTarget = ref<'light' | 'dark'>('light')

// 保存状态
const saveState = ref<'idle' | 'saving' | 'success' | 'error'>('idle')
const countdown = ref(3)
let countdownTimer: ReturnType<typeof setInterval> | null = null

function openColorPicker(index: number, target: 'light' | 'dark') {
  colorPickerIndex.value = index
  colorPickerTarget.value = target
  showColorPicker.value = true
}

function onColorPick(color: string) {
  const level = config.value.levels[colorPickerIndex.value]
  if (!level) return
  if (colorPickerTarget.value === 'light') {
    updateLevel(colorPickerIndex.value, { colorLight: color })
  } else {
    updateLevel(colorPickerIndex.value, { colorDark: color })
  }
}

function updateKeywords(index: number, value: string) {
  const keywords = value.split(',').map(k => k.trim()).filter(Boolean).slice(0, 3)
  updateLevel(index, { keywords })
}

async function handleSave() {
  saveState.value = 'saving'
  try {
    await syncToBackend()
    saveState.value = 'success'
    countdown.value = 3
    countdownTimer = setInterval(() => {
      countdown.value--
      if (countdown.value <= 0) {
        if (countdownTimer) clearInterval(countdownTimer)
        window.location.reload()
      }
    }, 1000)
  } catch {
    saveState.value = 'error'
    setTimeout(() => { saveState.value = 'idle' }, 2000)
  }
}

const isDragging = ref(false)
const dragIndex = ref(-1)

function onDragStart(index: number) {
  isDragging.value = true
  dragIndex.value = index
}

function onDragOver(e: DragEvent, index: number) {
  e.preventDefault()
  if (dragIndex.value === index) return
  const levels = [...config.value.levels]
  const [moved] = levels.splice(dragIndex.value, 1)
  levels.splice(index, 0, moved)
  config.value = { ...config.value, levels }
  dragIndex.value = index
}

function onDragEnd() {
  isDragging.value = false
  dragIndex.value = -1
}
</script>

<template>
  <div class="log-level-settings">
    <!-- 预设选择 -->
    <div class="field-group">
      <label class="field-label">{{ t('settings.preset') }}</label>
      <div class="preset-dropdown" ref="presetDropdownRef">
        <button class="preset-trigger" @click.stop="togglePresetDropdown">
          <span class="preset-current">{{ currentPresetLabel }}</span>
          <svg class="preset-arrow" :class="{ open: showPresetDropdown }" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6 9 12 15 18 9"/>
          </svg>
        </button>
        <div v-if="showPresetDropdown" class="preset-menu">
          <button
            v-for="p in presetOptions"
            :key="p.key"
            class="preset-option"
            :class="{ active: config.preset === p.key }"
            @click.stop="selectPreset(p.key)"
          >
            <svg v-if="config.preset === p.key" class="check-icon" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
            <span v-else class="check-placeholder"></span>
            {{ p.label }}
          </button>
        </div>
      </div>
    </div>

    <!-- 级别列表 -->
    <div class="field-group">
      <label class="field-label">{{ t('settings.levels') }}</label>
      <div class="level-list">
        <div
          v-for="(level, index) in config.levels"
          :key="index"
          class="level-card"
          :class="{ dragging: isDragging && dragIndex === index }"
          draggable="true"
          @dragstart="onDragStart(index)"
          @dragover="onDragOver($event, index)"
          @dragend="onDragEnd"
        >
          <div class="level-card-top">
            <span class="drag-handle" :title="t('settings.dragToReorder')">
              <svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor">
                <circle cx="8" cy="6" r="2"/><circle cx="16" cy="6" r="2"/>
                <circle cx="8" cy="12" r="2"/><circle cx="16" cy="12" r="2"/>
                <circle cx="8" cy="18" r="2"/><circle cx="16" cy="18" r="2"/>
              </svg>
            </span>
            <input
              class="level-name"
              :value="level.name"
              @input="updateLevel(index, { name: ($event.target as HTMLInputElement).value })"
              :placeholder="t('settings.levelName')"
            />
            <div class="level-card-right">
              <button
                class="color-dot"
                :style="{ background: level.colorLight }"
                :title="t('settings.lightColor')"
                @click="openColorPicker(index, 'light')"
              ></button>
              <button
                class="color-dot"
                :style="{ background: level.colorDark }"
                :title="t('settings.darkColor')"
                @click="openColorPicker(index, 'dark')"
              ></button>
              <button
                class="remove-btn"
                @click="removeLevel(index)"
                :title="t('settings.removeLevel')"
              >
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
                </svg>
              </button>
            </div>
          </div>
          <div class="level-card-bottom">
            <input
              class="level-keywords"
              :value="level.keywords.join(', ')"
              @input="updateKeywords(index, ($event.target as HTMLInputElement).value)"
              :placeholder="t('settings.keywords') + ' (≤3)'"
            />
          </div>
        </div>
      </div>

      <button class="add-btn" @click="addLevel">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
        </svg>
        {{ t('settings.addLevel') }}
      </button>
    </div>

    <!-- 操作按钮 -->
    <div class="actions">
      <button
        class="save-btn"
        :class="{ success: saveState === 'success', error: saveState === 'error' }"
        @click="handleSave"
        :disabled="saveState === 'saving'"
      >
        <template v-if="saveState === 'saving'">
          <span class="spinner"></span>
          {{ t('settings.saving') }}
        </template>
        <template v-else-if="saveState === 'success'">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
            <polyline points="20 6 9 17 4 12"/>
          </svg>
          {{ t('settings.saved') }} ({{ countdown }}s)
        </template>
        <template v-else-if="saveState === 'error'">
          {{ t('settings.saveError') }}
        </template>
        <template v-else>
          {{ t('settings.save') }}
        </template>
      </button>
      <button class="reset-btn" @click="resetToDefault">{{ t('settings.resetDefault') }}</button>
    </div>
  </div>

  <!-- 颜色选择器模态框 -->
  <ColorPicker
    v-if="showColorPicker"
    :palette="COLOR_PALETTE"
    :current="colorPickerTarget === 'light'
      ? config.levels[colorPickerIndex]?.colorLight
      : config.levels[colorPickerIndex]?.colorDark"
    @pick="onColorPick"
    @close="showColorPicker = false"
  />
</template>

<style scoped>
.log-level-settings {
  display: flex;
  flex-direction: column;
  gap: 20px;
  padding: 16px;
  height: 100%;
  overflow-y: auto;
}

.field-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.field-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-3);
}

.preset-dropdown {
  position: relative;
  width: 100%;
}

.preset-trigger {
  width: 100%;
  height: 34px;
  font-size: 13px;
  font-family: var(--font-mono);
  background: var(--bg-2);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0 10px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
  transition: border-color .15s, background .15s;
}

.preset-trigger:hover {
  border-color: var(--border-2);
  background: var(--bg);
}

.preset-current {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preset-arrow {
  flex-shrink: 0;
  color: var(--text-3);
  transition: transform .2s;
}

.preset-arrow.open {
  transform: rotate(180deg);
}

.preset-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
  z-index: 100;
  overflow: hidden;
}

.preset-option {
  width: 100%;
  height: 34px;
  font-size: 13px;
  font-family: var(--font-mono);
  background: transparent;
  color: var(--text-2);
  border: none;
  border-radius: 0;
  padding: 0 10px;
  display: flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  transition: background .1s;
  text-align: left;
}

.preset-option:hover {
  background: var(--bg-2);
}

.preset-option.active {
  color: var(--accent);
  background: var(--accent-light);
}

.check-icon {
  flex-shrink: 0;
  color: var(--accent);
}

.check-placeholder {
  width: 12px;
  flex-shrink: 0;
}

/* ── 级别列表 ── */
.level-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.level-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-2);
  transition: border-color .15s, background .15s, box-shadow .15s;
  min-width: 0;
}

.level-card:hover {
  border-color: var(--border-2);
  background: var(--bg);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
}

.level-card.dragging {
  opacity: 0.5;
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-light);
}

.level-card-top {
  display: flex;
  align-items: center;
  gap: 6px;
}

.level-card-bottom {
  margin-top: 6px;
}

.level-card-right {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
  flex-shrink: 0;
}

.drag-handle {
  color: var(--text-3);
  cursor: grab;
  display: flex;
  align-items: center;
  flex-shrink: 0;
  opacity: 0.5;
  transition: opacity .15s;
}

.level-card:hover .drag-handle {
  opacity: 1;
}

.drag-handle:active {
  cursor: grabbing;
}

.level-name {
  flex: 1;
  min-width: 0;
  height: 28px;
  font-size: 12px;
  font-weight: 600;
  padding: 0 8px;
}

.level-keywords {
  width: 100%;
  height: 28px;
  font-size: 11px;
  padding: 0 8px;
}

.color-dot {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid var(--bg-2);
  cursor: pointer;
  flex-shrink: 0;
  padding: 0;
  transition: transform .15s, box-shadow .15s;
}

.level-card:hover .color-dot {
  border-color: var(--border);
}

.color-dot:hover {
  transform: scale(1.15);
  box-shadow: 0 0 0 2px var(--bg), 0 0 0 3px var(--border);
}

.remove-btn {
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 5px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity .15s, background .15s, color .15s;
}

.level-card:hover .remove-btn {
  opacity: 0.6;
}

.remove-btn:hover {
  opacity: 1 !important;
  background: var(--c-error-bg);
  color: var(--c-error-text);
}

.add-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  width: 100%;
  height: 32px;
  font-size: 11px;
  padding: 0 12px;
  border: 1px dashed var(--border);
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  border-radius: 8px;
  transition: all .15s;
  margin-top: 4px;
}

.add-btn:hover {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-light);
}

/* ── 操作按钮 ── */
.actions {
  display: flex;
  gap: 10px;
  margin-top: auto;
  padding-top: 16px;
  border-top: 1px solid var(--border);
}

.save-btn {
  flex: 1;
  height: 36px;
  font-size: 13px;
  font-weight: 500;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  background: var(--accent);
  color: white;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  transition: all .2s;
}

.save-btn:hover:not(:disabled) {
  opacity: 0.9;
  transform: translateY(-1px);
  box-shadow: 0 2px 8px rgba(24, 95, 165, 0.3);
}

.save-btn:active:not(:disabled) {
  transform: translateY(0);
}

.save-btn:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}

.save-btn.success {
  background: #22c55e;
}

.save-btn.error {
  background: var(--c-error-text);
}

.reset-btn {
  height: 36px;
  font-size: 13px;
  padding: 0 16px;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-2);
  border-radius: 8px;
  cursor: pointer;
  transition: all .15s;
}

.reset-btn:hover {
  background: var(--bg-2);
  border-color: var(--border-2);
  color: var(--text);
}

/* ── 旋转加载指示器 ── */
.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-top-color: white;
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}
</style>
