<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useCopyFeedbackId } from '../../composables/useClipboard'
import { useAuth } from '../../composables/useAuth'
import { healthCheck } from '../../services/api'
import { Copy, Check, BookOpen } from 'lucide-vue-next'

const { t } = useI18n()
const { copiedId, copy } = useCopyFeedbackId<string>()

const endpoint = computed(() => `${window.location.origin}/mcp`)

// 服务端 MCP 的认证/启用状态（来自 /api/health）：
// - mcpAuthRequired：/mcp 实际需要认证（专用 token 已设置，或继承全局）
// - mcpTokenDedicated：设置了专用 [mcp] token → 登录 token 无效，用占位符
// - mcpEnabled：[mcp] enabled（默认 true）
// 继承全局时登录 token 就是有效凭证（只读展示 + 填进片段）；专用 token
// 的值永不回传前端，只能由用户在服务端查看后手动替换占位符。
const mcpAuthRequired = ref(false)
const mcpTokenDedicated = ref(false)
const mcpEnabled = ref(true)
const probing = ref(true)
onMounted(async () => {
  try {
    const health = await healthCheck()
    mcpAuthRequired.value = health.mcpAuthRequired ?? false
    mcpTokenDedicated.value = health.mcpTokenDedicated ?? false
    mcpEnabled.value = health.mcpEnabled ?? true
  } catch {
    mcpAuthRequired.value = false
    mcpTokenDedicated.value = false
    mcpEnabled.value = true
  } finally {
    probing.value = false
  }
})

// 继承全局时的有效凭证 = 登录 token（只读展示）。
const loginToken = useAuth().getToken()

type Client = 'claudeCode' | 'cursor' | 'codex' | 'opencode' | 'generic'
const client = ref<Client>('claudeCode')

const clients: { key: Client; label: string; path: string }[] = [
  { key: 'claudeCode', label: 'Claude Code', path: '~/.claude.json' },
  { key: 'cursor', label: 'Cursor', path: '~/.cursor/mcp.json' },
  { key: 'codex', label: 'Codex', path: '~/.codex/config.toml' },
  { key: 'opencode', label: 'OpenCode', path: '~/.config/opencode/opencode.json' },
  { key: 'generic', label: t('settings.mcpGenericClient'), path: '' },
]

const configPath = computed(
  () => clients.find((c) => c.key === client.value)?.path ?? '',
)

const snippet = computed(() => {
  const url = endpoint.value
  const auth = !mcpAuthRequired.value
    ? null
    : { Authorization: `Bearer ${mcpTokenDedicated.value ? '<MCP_TOKEN>' : loginToken}` }
  const tk = mcpTokenDedicated.value ? '<MCP_TOKEN>' : loginToken
  switch (client.value) {
    case 'claudeCode':
      // Claude Code requires type:"http" — a bare url entry is skipped.
      return JSON.stringify(
        { mcpServers: { tailr: { type: 'http', url, ...(auth ? { headers: auth } : {}) } } },
        null,
        2,
      )
    case 'cursor':
      return JSON.stringify(
        { mcpServers: { tailr: { url, ...(auth ? { headers: auth } : {}) } } },
        null,
        2,
      )
    case 'codex':
      return [
        '[mcp_servers.tailr]',
        `url = "${url}"`,
        ...(auth
          ? ['# Streamable-HTTP auth (recent Codex versions):', 'http_headers = { Authorization = "Bearer ' + tk + '" }']
          : []),
      ].join('\n')
    case 'opencode':
      return JSON.stringify(
        { mcp: { tailr: { type: 'http', url, ...(auth ? { headers: auth } : {}) } } },
        null,
        2,
      )
    case 'generic':
      // Standard mcpServers shape accepted by most MCP-capable clients
      // (zcode, DeepSeek harness, ...); consult your client's docs for the
      // config file location.
      return JSON.stringify(
        { mcpServers: { tailr: { type: 'http', url, ...(auth ? { headers: auth } : {}) } } },
        null,
        2,
      )
  }
})

async function copyText(text: string, id: string): Promise<void> {
  await copy(text, id)
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

    <div class="mcp-row">
      <div class="mcp-label">Endpoint</div>
      <div class="mcp-value">
        <code>{{ endpoint }}</code>
        <button class="mcp-copy-btn" :title="t('settings.mcpCopy')" @click="copyText(endpoint, 'endpoint')">
          <Check v-if="copiedId === 'endpoint'" :size="13" />
          <Copy v-else :size="13" />
        </button>
      </div>
    </div>

    <div v-if="!probing && mcpAuthRequired && !mcpTokenDedicated" class="mcp-row">
      <div class="mcp-label">Token</div>
      <div class="mcp-value">
        <code>{{ loginToken }}</code>
        <button class="mcp-copy-btn" :title="t('settings.mcpCopy')" @click="copyText(loginToken, 'token')">
          <Check v-if="copiedId === 'token'" :size="13" />
          <Copy v-else :size="13" />
        </button>
      </div>
    </div>

    <div class="mcp-clients" role="tablist">
      <button
        v-for="c in clients"
        :key="c.key"
        :class="['mcp-client-tab', { active: client === c.key }]"
        role="tab"
        :aria-selected="client === c.key"
        @click="client = c.key"
      >
        {{ c.label }}
      </button>
    </div>

    <p v-if="configPath" class="mcp-path">
      {{ t('settings.mcpConfigPath') }}:
      <code>{{ configPath }}</code>
      <button class="mcp-copy-btn" :title="t('settings.mcpCopy')" @click="copyText(configPath, 'path')">
        <Check v-if="copiedId === 'path'" :size="13" />
        <Copy v-else :size="13" />
      </button>
    </p>

    <div class="mcp-snippet">
      <pre>{{ snippet }}</pre>
      <button class="mcp-copy-btn mcp-copy-snippet" @click="copyText(snippet, client)">
        <Check v-if="copiedId === client" :size="13" />
        <Copy v-else :size="13" />
        {{ copiedId === client ? t('settings.mcpCopied') : t('settings.mcpCopy') }}
      </button>
    </div>

    <p v-if="!probing && !mcpEnabled" class="mcp-warn-hint">{{ t('settings.mcpDisabledHint') }}</p>
    <p v-else-if="!probing && mcpAuthRequired" class="mcp-token-hint">{{ t('settings.mcpDedicatedHint') }}</p>
    <p v-else-if="!probing" class="mcp-token-hint">{{ t('settings.mcpTokenHint') }}</p>
  </div>
</template>

<style scoped>
.mcp-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.mcp-desc {
  margin: 0;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-2);
}

.mcp-docs-link {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  margin-left: 4px;
  color: var(--accent);
  text-decoration: none;
}

.mcp-row {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.mcp-label {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 90px;
  font-size: 12px;
  color: var(--text);
}

.mcp-value {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  flex: 1;
  min-width: 0;
}

.mcp-value code {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
}

.mcp-clients {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: var(--space-1);
}

.mcp-client-tab {
  padding: 4px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-2);
  font-size: 12px;
  cursor: pointer;
}

.mcp-client-tab:hover {
  border-color: var(--border-2);
}

.mcp-client-tab.active {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  border-color: var(--accent);
  color: var(--accent);
}

.mcp-path {
  margin: 0;
  font-size: 12px;
  color: var(--text-3);
  display: flex;
  align-items: center;
  gap: 6px;
}

.mcp-path code {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-2);
}

.mcp-snippet {
  position: relative;
}

.mcp-snippet pre {
  margin: 0;
  padding: var(--space-3);
  padding-right: 70px;
  border-radius: var(--radius-sm);
  background: var(--bg-2);
  border: 1px solid var(--border);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.6;
  overflow-x: auto;
  color: var(--text);
}

.mcp-copy-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 2px;
  border: none;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
}

.mcp-copy-btn:hover {
  color: var(--accent);
}

.mcp-copy-snippet {
  position: absolute;
  top: var(--space-2);
  right: var(--space-2);
  font-size: 11px;
}

.mcp-token-hint {
  margin: 0;
  font-size: 12px;
  color: var(--text-3);
}

.mcp-warn-hint {
  margin: 0;
  font-size: 12px;
  color: var(--warn, #e0a800);
}

</style>
