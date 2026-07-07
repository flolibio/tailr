<script setup lang="ts">
import { useTabs } from '../composables/useTabs'

const { tabs, activeTabPath, switchTo, closeTab } = useTabs()

function basename(path: string): string {
  const parts = path.split('/')
  return parts[parts.length - 1] || path
}

function handleClose(path: string, event: MouseEvent): void {
  event.stopPropagation()
  closeTab(path)
}

function handleMiddleClick(path: string, event: MouseEvent): void {
  if (event.button === 1) {
    event.preventDefault()
    closeTab(path)
  }
}
</script>

<template>
  <div class="tabbar">
    <div
      v-for="tab in tabs"
      :key="tab.path"
      class="tab"
      :class="{ active: tab.path === activeTabPath }"
      :title="tab.path"
      @click="switchTo(tab.path)"
      @mouseup="handleMiddleClick(tab.path, $event)"
    >
      <span v-if="tab.hasUnread && tab.path !== activeTabPath" class="tab-dot"></span>
      <span class="tab-name">{{ basename(tab.path) }}</span>
      <button class="tab-close" @click="handleClose(tab.path, $event)">✕</button>
    </div>
  </div>
</template>

<style scoped>
.tabbar {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: stretch;
  overflow-x: auto;
  scrollbar-width: none;
  height: var(--tabbar-h);
}

.tabbar::-webkit-scrollbar {
  display: none;
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
  font-size: 14px;
  color: var(--text-3);
  cursor: pointer;
  border-right: 1px solid var(--border);
  transition: color .12s, background .12s;
  user-select: none;
  white-space: nowrap;
  position: relative;
  height: 100%;
}

.tab:hover {
  color: var(--text-2);
  background: var(--bg-2);
}

.tab.active {
  color: var(--text);
  background: var(--bg);
}

.tab.active::after {
  content: '';
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent);
  border-radius: 1px 1px 0 0;
}

.tab-close {
  width: 16px;
  height: 16px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-radius: 3px;
  opacity: 0.4;
  transition: opacity .12s, background .12s, color .12s;
  font-size: 14px;
  line-height: 1;
}

.tab:hover .tab-close {
  opacity: 1;
}

.tab-close:hover {
  background: var(--bg-3);
  color: var(--text);
}

.tab-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--c-unread, #5B9DFF);
  flex-shrink: 0;
}

.tab-name {
  font-weight: 500;
}
</style>
