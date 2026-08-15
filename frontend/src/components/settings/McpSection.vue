<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useCopyFeedbackId } from '../../composables/useClipboard'
import { useAuth } from '../../composables/useAuth'
import { Copy, Check, Bot, BookOpen } from 'lucide-vue-next'

const { t } = useI18n()
const { copiedId, copy } = useCopyFeedbackId<string>()

const endpoint = computed(() => `${window.location.origin}/mcp`)

// The user's own token (already in their browser via the auth dialog) —
// embedded into the copy snippet so the config works out of the box.
const token = useAuth().getToken()
const authHeaders = computed(() => (token ? { Authorization: `Bearer ${token}` } : null))

type Client = 'claudeCode' | 'cursor'
const client = ref<Client>('claudeCode')

function mcpConfig(withType: boolean): string {
  const entry: Record<string, unknown> = {
    ...(withType ? { type: 'http' } : {}),
    url: endpoint.value,
    ...(authHeaders.value ? { headers: authHeaders.value } : {}),
  }
  return JSON.stringify({ mcpServers: { tailr: entry } }, null, 2)
}

const snippet = computed(() =>
  client.value === 'claudeCode' ? mcpConfig(true) : mcpConfig(false),
)
// Claude Code requires type:"http"; Cursor takes a bare url.
const configPath = computed(() =>
  client.value === 'claudeCode' ? '~/.claude.json' : '~/.cursor/mcp.json',
)

async function copySnippet(): Promise<void> {
  await copy(snippet.value, client.value)
}
</script>

<template>
  <div class="mcp-section">
    <p class="mcp-desc">
      {{ t('settings.mcpDesc') }}
      <a
        class="mcp-docs-link"
        href="https://github.com/flolibio/tailr/blob/main/docs/mcp.md"
        target="_blank"
        rel="noopener"
      >
        <BookOpen :size="13" />
        {{ t('settings.mcpDocs') }}
      </a>
    </p>

    <div class="mcp-endpoint">
      <Bot :size="16" />
      <code>{{ endpoint }}</code>
    </div>

    <div class="mcp-clients" role="tablist">
      <button
        :class="['mcp-client-tab', { active: client === 'claudeCode' }]"
        role="tab"
        :aria-selected="client === 'claudeCode'"
        @click="client = 'claudeCode'"
      >
        Claude Code
      </button>
      <button
        :class="['mcp-client-tab', { active: client === 'cursor' }]"
        role="tab"
        :aria-selected="client === 'cursor'"
        @click="client = 'cursor'"
      >
        Cursor
      </button>
    </div>

    <p class="mcp-path">{{ t('settings.mcpConfigPath') }}: <code>{{ configPath }}</code></p>

    <div class="mcp-snippet">
      <pre>{{ snippet }}</pre>
      <button class="mcp-copy-btn" :title="t('settings.mcpCopy')" @click="copySnippet()">
        <Check v-if="copiedId === client" :size="14" />
        <Copy v-else :size="14" />
        {{ copiedId === client ? t('settings.mcpCopied') : t('settings.mcpCopy') }}
      </button>
    </div>

    <p v-if="!token" class="mcp-token-hint">{{ t('settings.mcpTokenHint') }}</p>
  </div>
</template>

<style scoped>
.mcp-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.mcp-desc {
  margin: 0;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-secondary, #888);
}

.mcp-docs-link {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  margin-left: 4px;
  color: var(--accent, #4a9eff);
  text-decoration: none;
}

.mcp-endpoint {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-radius: 6px;
  background: var(--bg-secondary, #1e1e1e);
  border: 1px solid var(--border, #333);
  font-size: 13px;
}

.mcp-endpoint code {
  user-select: all;
}

.mcp-clients {
  display: flex;
  gap: 6px;
  margin-top: 4px;
}

.mcp-client-tab {
  padding: 5px 14px;
  border: 1px solid var(--border, #333);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary, #888);
  font-size: 12px;
  cursor: pointer;
}

.mcp-client-tab.active {
  background: var(--accent, #4a9eff);
  border-color: var(--accent, #4a9eff);
  color: #fff;
}

.mcp-path {
  margin: 0;
  font-size: 12px;
  color: var(--text-secondary, #888);
}

.mcp-snippet {
  position: relative;
}

.mcp-snippet pre {
  margin: 0;
  padding: 12px 14px;
  border-radius: 6px;
  background: var(--bg-secondary, #1a1a1a);
  border: 1px solid var(--border, #333);
  font-size: 12px;
  line-height: 1.6;
  overflow-x: auto;
  user-select: all;
}

.mcp-copy-btn {
  position: absolute;
  top: 8px;
  right: 8px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border: 1px solid var(--border, #333);
  border-radius: 5px;
  background: var(--bg-primary, #252525);
  color: var(--text-primary, #ddd);
  font-size: 11px;
  cursor: pointer;
}

.mcp-copy-btn:hover {
  border-color: var(--accent, #4a9eff);
  color: var(--accent, #4a9eff);
}

.mcp-token-hint {
  margin: 0;
  font-size: 12px;
  color: var(--warn, #e0a800);
}
</style>
