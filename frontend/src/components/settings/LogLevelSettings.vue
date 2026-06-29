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
  applyThemeColors,
  isDark,
} = useLogLevels()

defineExpose({
  syncToBackend,
  resetToDefault,
  applyThemeColors,
  isDark,
})

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

const isDragging = ref(false)
const dragIndex = ref(-1)

function onDragStart(e: DragEvent, index: number) {
  isDragging.value = true
  dragIndex.value = index
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = 'move'
    const target = e.target as HTMLElement
    e.dataTransfer.setDragImage(target.closest('.level-card')!, -10, -10)
  }
}

function onDragOver(e: DragEvent, index: number) {
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
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
    <!-- Preset -->
    <div class="setting-group">
      <div class="setting-label">{{ t('settings.preset') }}</div>
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

    <!-- Levels -->
    <div class="setting-group">
      <div class="level-header">
        <span class="col-handle"></span>
        <span class="col-name">{{ t('settings.levelName') }}</span>
        <span class="col-keywords">{{ t('settings.keywordsHeader') }}</span>
        <span class="col-light"></span>
        <span class="col-dark"></span>
        <span class="col-remove"></span>
      </div>

      <div
        v-for="(level, index) in config.levels"
        :key="index"
        class="level-card"
        :class="{ dragging: isDragging && dragIndex === index }"
        @dragover="onDragOver($event, index)"
        @dragend="onDragEnd"
      >
        <span
          class="drag-handle"
          :title="t('settings.dragToReorder')"
          draggable="true"
          @dragstart="onDragStart($event, index)"
        >
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
        <input
          class="level-keywords"
          :value="level.keywords.join(', ')"
          @blur="updateKeywords(index, ($event.target as HTMLInputElement).value)"
          :placeholder="t('settings.keywords')"
        />
        <div class="color-dot-group">
          <button
            class="color-dot"
            :style="{ background: level.colorLight }"
            :title="t('settings.lightColor')"
            @click="openColorPicker(index, 'light')"
          ></button>
          <span class="color-dot-label">{{ t('settings.light') }}</span>
        </div>
        <div class="color-dot-group">
          <button
            class="color-dot"
            :style="{ background: level.colorDark }"
            :title="t('settings.darkColor')"
            @click="openColorPicker(index, 'dark')"
          ></button>
          <span class="color-dot-label">{{ t('settings.dark') }}</span>
        </div>
        <button
          class="remove-btn"
          @click="removeLevel(index)"
          :title="t('settings.removeLevel')"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>

      <button class="add-btn" @click="addLevel">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
        </svg>
        {{ t('settings.addLevel') }}
      </button>
    </div>
  </div>

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
}

.setting-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.setting-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-2);
}

.hint {
  font-weight: 400;
  color: var(--text-3);
  font-size: 12px;
}

/* ── Preset ── */
.preset-dropdown {
  position: relative;
  width: 200px;
}

.preset-trigger {
  width: 100%;
  font-size: 12px;
  background: var(--bg-2);
  color: var(--text);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0 8px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
  transition: border-color .15s;
}

.preset-trigger:hover {
  border-color: var(--border-2);
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
  height: 30px;
  font-size: 12px;
  background: transparent;
  color: var(--text-2);
  border: none;
  border-radius: 0;
  padding: 0 8px;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 6px;
  cursor: pointer;
  transition: background .1s;
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

/* ── Level Header ── */
.level-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px 6px;
  font-size: 10px;
  color: var(--text-3);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.col-handle { width: 10px; }
.col-name { width: 100px; flex-shrink: 0; }
.col-keywords { flex: 1; }
.col-light { width: 40px; text-align: center; }
.col-dark { width: 40px; text-align: center; }
.col-remove { width: 20px; }

/* ── Level Card ── */
.level-card {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  background: var(--bg-2);
  /* border: 1px solid var(--border); */
  border-radius: var(--radius);
  transition: border-color .12s;
}

.level-card:hover {
  border-color: var(--border-2);
}

.level-card.dragging {
  opacity: 0.4;
  border: 2px dashed var(--accent);
  background: var(--accent-light);
}

.drag-handle {
  color: var(--text-3);
  cursor: grab;
  opacity: 0.4;
  transition: opacity .12s;
  flex-shrink: 0;
}

.level-card:hover .drag-handle {
  opacity: 0.8;
}

.drag-handle:active {
  cursor: grabbing;
}

.level-name {
  width: 100px;
  flex-shrink: 0;
  font-size: 12px;
  font-weight: 600;
  font-family: var(--font-mono);
  background: var(--bg);
  color: var(--text);
  border: 1px solid transparent;
  border-radius: 4px;
  padding: 0 6px;
  transition: border-color .12s;
  height: 30px;
}

.level-name:hover {
  border-color: var(--border);
}

.level-name:focus {
  outline: none;
  border-color: var(--accent);
}

.level-keywords {
  flex: 1;
  min-width: 0;
  font-size: 10px;
  background: var(--bg);
  color: var(--text-2);
  border: 1px solid transparent;
  border-radius: 4px;
  padding: 0 6px;
  transition: border-color .12s;
  height: 30px;
}

.level-keywords:hover {
  border-color: var(--border);
}

.level-keywords:focus {
  outline: none;
  border-color: var(--accent);
}

.color-dot-group {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
}

.color-dot {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 2px solid var(--bg-3);
  cursor: pointer;
  flex-shrink: 0;
  padding: 0;
  transition: transform .12s;
}

.color-dot:hover {
  transform: scale(1.2);
}

.color-dot-label {
  font-size: 10px;
  color: var(--text-3);
  text-align: center;
  line-height: 1;
  white-space: nowrap;
}

.remove-btn {
  width: 20px;
  height: 20px !important;
  border: none !important;
  background: transparent !important;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  opacity: 0;
  transition: opacity .12s, background .12s, color .12s;
  flex-shrink: 0;
  padding: 0 !important;
}

.level-card:hover .remove-btn {
  opacity: 0.6;
}

.remove-btn:hover {
  opacity: 1 !important;
  background: var(--c-error-bg);
  color: var(--c-error-text);
}

/* ── Add Button ── */
.add-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  width: 100%;
  height: 30px;
  font-size: 12px;
  padding: 0 12px;
  border: 1px dashed var(--border);
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  border-radius: var(--radius);
  margin-top: 8px;
  transition: all .12s;
}

.add-btn:hover {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-light);
}
</style>
