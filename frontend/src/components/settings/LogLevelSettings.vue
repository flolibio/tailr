<script setup lang="ts">
import { ref, computed } from 'vue'
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

function onPresetChange(e: Event) {
  const val = (e.target as HTMLSelectElement).value
  switchPreset(val)
}

function updateKeywords(index: number, value: string) {
  const keywords = value.split(',').map(k => k.trim()).filter(Boolean)
  updateLevel(index, { keywords })
}

async function handleSave() {
  await syncToBackend()
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
    <div class="section">
      <label class="section-label">{{ t('settings.preset') }}</label>
      <select class="preset-select" :value="config.preset" @change="onPresetChange">
        <option
          v-for="p in presetOptions"
          :key="p.key"
          :value="p.key"
        >{{ p.label }}</option>
        <option value="custom" disabled>Custom</option>
      </select>
    </div>

    <!-- 级别列表 -->
    <div class="section">
      <label class="section-label">{{ t('settings.levels') }}</label>
      <div class="level-list">
        <div
          v-for="(level, index) in config.levels"
          :key="index"
          class="level-row"
          :class="{ dragging: isDragging && dragIndex === index }"
          draggable="true"
          @dragstart="onDragStart(index)"
          @dragover="onDragOver($event, index)"
          @dragend="onDragEnd"
        >
          <!-- 拖拽手柄 -->
          <span class="drag-handle" :title="t('settings.dragToReorder')">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
              <circle cx="8" cy="6" r="2"/><circle cx="16" cy="6" r="2"/>
              <circle cx="8" cy="12" r="2"/><circle cx="16" cy="12" r="2"/>
              <circle cx="8" cy="18" r="2"/><circle cx="16" cy="18" r="2"/>
            </svg>
          </span>

          <!-- 级别名称 -->
          <input
            class="level-name"
            :value="level.name"
            @input="updateLevel(index, { name: ($event.target as HTMLInputElement).value })"
            :placeholder="t('settings.levelName')"
          />

          <!-- 关键词 -->
          <input
            class="level-keywords"
            :value="level.keywords.join(', ')"
            @input="updateKeywords(index, ($event.target as HTMLInputElement).value)"
            :placeholder="t('settings.keywords')"
          />

          <!-- 浅色颜色 -->
          <button
            class="color-dot"
            :style="{ background: level.colorLight }"
            :title="t('settings.lightColor')"
            @click="openColorPicker(index, 'light')"
          ></button>

          <!-- 深色颜色 -->
          <button
            class="color-dot"
            :style="{ background: level.colorDark }"
            :title="t('settings.darkColor')"
            @click="openColorPicker(index, 'dark')"
          ></button>

          <!-- 删除按钮 -->
          <button
            class="remove-btn"
            @click="removeLevel(index)"
            :title="t('settings.removeLevel')"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
          </button>
        </div>
      </div>

      <button class="add-btn" @click="addLevel">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
        </svg>
        {{ t('settings.addLevel') }}
      </button>
    </div>

    <!-- 操作按钮 -->
    <div class="section actions">
      <button class="primary" @click="handleSave">{{ t('settings.save') }}</button>
      <button @click="resetToDefault">{{ t('settings.resetDefault') }}</button>
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
  </div>
</template>

<style scoped>
.log-level-settings {
  display: flex;
  flex-direction: column;
  gap: 20px;
  height: 100%;
}

.section {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.section-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-3);
}

.preset-select {
  width: 100%;
  height: 32px;
  font-size: 13px;
}

.level-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.level-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 6px;
  border-radius: 6px;
  border: 1px solid transparent;
  transition: border-color .12s, background .12s;
}

.level-row:hover {
  background: var(--bg-2);
  border-color: var(--border);
}

.level-row.dragging {
  opacity: 0.5;
  border-color: var(--accent);
}

.drag-handle {
  color: var(--text-3);
  cursor: grab;
  display: flex;
  align-items: center;
  flex-shrink: 0;
  padding: 2px;
}

.drag-handle:active {
  cursor: grabbing;
}

.level-name {
  width: 90px;
  height: 28px;
  font-size: 12px;
  font-weight: 600;
  padding: 0 6px;
  flex-shrink: 0;
}

.level-keywords {
  flex: 1;
  min-width: 0;
  height: 28px;
  font-size: 11px;
  padding: 0 6px;
}

.color-dot {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 2px solid var(--bg);
  cursor: pointer;
  flex-shrink: 0;
  padding: 0;
  transition: transform .12s;
}

.color-dot:hover {
  transform: scale(1.2);
}

.remove-btn {
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 4px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity .12s, background .12s, color .12s;
}

.level-row:hover .remove-btn {
  opacity: 1;
}

.remove-btn:hover {
  background: var(--c-error-bg);
  color: var(--c-error-text);
}

.add-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  width: fit-content;
  height: 28px;
  font-size: 11px;
  padding: 0 10px;
  border: 1px dashed var(--border);
  background: transparent;
  color: var(--text-2);
  cursor: pointer;
  border-radius: 6px;
  transition: all .12s;
}

.add-btn:hover {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-light);
}

.actions {
  flex-direction: row;
  gap: 8px;
  margin-top: auto;
  padding-top: 16px;
  border-top: 1px solid var(--border);
}
</style>
