<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<template>
  <div class="placeholder">
    <p>{{ t('settings.tabs.logLevels') }} — coming soon</p>
  </div>
</template>

<style scoped>
.placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-3);
  font-size: 13px;
}
</style>
