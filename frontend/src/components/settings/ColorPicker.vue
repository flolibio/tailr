<script setup lang="ts">
const props = defineProps<{
  palette: Array<{ light: string; dark: string }>
  current?: string
}>()

const emit = defineEmits<{
  pick: [color: string]
  close: []
}>()
</script>

<template>
  <div class="color-picker-overlay" @click.self="emit('close')">
    <div class="color-picker">
      <div class="picker-header">
        <span class="picker-title">选择颜色</span>
        <button class="close-btn" @click="emit('close')">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>
      <div class="palette-grid">
        <button
          v-for="(c, i) in palette"
          :key="i"
          class="palette-swatch"
          :class="{ active: current === c.light }"
          :style="{ background: c.light }"
          :title="c.light"
          @click="emit('pick', c.light)"
        ></button>
      </div>
      <div class="palette-grid">
        <button
          v-for="(c, i) in palette"
          :key="i"
          class="palette-swatch dark-swatch"
          :class="{ active: current === c.dark }"
          :style="{ background: c.dark }"
          :title="c.dark"
          @click="emit('pick', c.dark)"
        ></button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.color-picker-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
}

.color-picker {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: 16px;
  min-width: 280px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
}

.picker-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
}

.picker-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text);
}

.close-btn {
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 4px;
}

.close-btn:hover {
  background: var(--bg-2);
  color: var(--text);
}

.palette-grid {
  display: grid;
  grid-template-columns: repeat(8, 1fr);
  gap: 6px;
  margin-bottom: 8px;
}

.palette-swatch {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  border: 2px solid transparent;
  cursor: pointer;
  padding: 0;
  transition: transform .12s, border-color .12s;
}

.palette-swatch:hover {
  transform: scale(1.15);
}

.palette-swatch.active {
  border-color: var(--text);
}

.dark-swatch {
  border: 2px solid var(--border);
}
</style>
