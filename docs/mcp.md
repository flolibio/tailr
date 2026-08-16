# MCP — let AI agents read your logs

tailr ships a built-in [Model Context Protocol](https://modelcontextprotocol.io) server. AI agents (Claude Code, Cursor, and any MCP-capable client) can list, search, and analyze your server logs directly — no SSH, no copy-pasting.

The endpoint is **streamable HTTP** at `http://<your-server>:7700/mcp`, protected by the same Bearer token as the web UI (when one is configured).

## Quick start

### Claude Code

Add to your MCP configuration (`~/.claude.json` or project `.mcp.json`):

```json
{
  "mcpServers": {
    "tailr": {
      "type": "http",
      "url": "http://your-server:7700/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_TOKEN"
      }
    }
  }
}
```

> `"type": "http"` is required by Claude Code — a bare `"url"` entry is skipped.

### Cursor

Add to `~/.cursor/mcp.json` (same shape):

```json
{
  "mcpServers": {
    "tailr": {
      "url": "http://your-server:7700/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_TOKEN"
      }
    }
  }
}
```

If tailr runs without a token (`token = ""` in config), omit the `headers` block.

### Multiple servers

Configure one entry per machine — agents distinguish results by the `host` field in every response:

```json
{
  "mcpServers": {
    "tailr-web1": { "type": "http", "url": "http://web1:7700/mcp", "headers": { "Authorization": "Bearer TOKEN" } },
    "tailr-web2": { "type": "http", "url": "http://web2:7700/mcp", "headers": { "Authorization": "Bearer TOKEN" } },
    "tailr-db":   { "type": "http", "url": "http://db:7700/mcp",   "headers": { "Authorization": "Bearer TOKEN" } }
  }
}
```

Set a friendly display name per instance so agents can tell them apart:

```toml
# ~/.tailr/config.toml on web1
[mcp]
host_name = "web1-prod"
```

## Tools

| Tool | What it does |
|---|---|
| `list_log_files` | Files available on this server (start here) |
| `get_log_stats` | Line count, size, per-level counts — understand a file before searching it |
| `search_logs` | AND keyword search with context windows; `count_only=true` for "how many" questions; paginated via `resumeCursor` |
| `read_log_range` | Sequential reading from a cursor |
| `tail_log` | Last N lines with absolute line numbers |

Everything is budget-capped server-side (matches, output bytes, scan time), so even multi-GB files can't blow up your context window — oversized results page through `resumeCursor`.

## Configuration

```toml
# ~/.tailr/config.toml
[mcp]
# Expose the /mcp endpoint (default true). Set false to disable AI access —
# the endpoint then returns 404 as if it never existed.
# enabled = true

# Display name in tool responses (defaults to the system hostname).
# host_name = "web1-prod"

# Dedicated Bearer token for /mcp, layered on the global `token` above:
# - unset  → /mcp requires the global token (default — a locked server
#            never silently opens a machine interface)
# - set    → only this token unlocks /mcp; rotate agent credentials
#            independently of your web login
# - ""     → /mcp is open even when the web UI requires a token
#            (explicit opt-in; logs a startup warning)
# token = "mcp-specific-secret"
```

All keys are optional; existing configs work unchanged.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| Claude Code logs `has a "url" but no "type"` | Add `"type": "http"` to the entry |
| 401 on connect | Token mismatch — with a `[mcp] token` set, only that token unlocks `/mcp`; otherwise the global token applies |
| 404 on connect | `[mcp] enabled = false` on that server |
| Client can't reach the server | Check `bind` address and firewall; the port is the same as the web UI |
| Search returns partial results | That's the timeout budget paging — the agent continues via `resumeCursor` automatically; final answers use `more: false` |
