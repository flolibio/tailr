type Listener = (...args: unknown[]) => void

interface WSMessage {
  type: string
  path?: string
  entries?: Array<{
    lineNum: number
    raw: string
    level: string
    timestamp?: string
    rawTimestamp?: string
    fields?: Record<string, unknown>
  }>
  seq?: number
  // 后端 Catchup 消息用 last_seq（序列化为 camelCase 的 lastSeq），
  // 与 Append 的 seq 区分。两者都代表"客户端应记录的高水位 seq"。
  lastSeq?: number
  lineNum?: number
  message?: string
  // Subscribed 消息携带的精确总行数（LineIndex::build），用于修正 file_tail
  // 的估算行号坐标系。
  totalLines?: number
  // UpdateAvailable: server-pushed notification of a newer release.
  latestVersion?: string
  currentVersion?: string
  releaseUrl?: string
}

export class WSClient {
  private ws: WebSocket | null = null
  private subscriptions: Map<string, number> = new Map()
  private listeners: Map<string, Set<Listener>> = new Map()
  private reconnectDelay = 1000
  private maxReconnectDelay = 30000
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null
  private shouldConnect = false
  private isTabVisible = true
  private lastPongTime = Date.now()
  private readonly pongTimeout = 90000 // 3 missed heartbeats = dead connection

  constructor() {
    this.handleVisibilityChange = this.handleVisibilityChange.bind(this)
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', this.handleVisibilityChange)
    }
  }

  private getWsUrl(): string {
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
    const base = `${protocol}//${location.host}/ws`
    const token = localStorage.getItem('tailr-token')
    return token ? `${base}?token=${encodeURIComponent(token)}` : base
  }

  connect(): void {
    if (this.ws?.readyState === WebSocket.OPEN) return
    this.shouldConnect = true

    try {
      this.ws = new WebSocket(this.getWsUrl())

      this.ws.onopen = () => {
        this.reconnectDelay = 1000
        this.emit('open')
        this.startHeartbeat()
        this.resubscribeAll()
      }

      this.ws.onmessage = (event: MessageEvent) => {
        try {
          const msg = JSON.parse(event.data as string) as WSMessage
          this.handleMessage(msg)
        } catch {
          // ignore malformed messages
        }
      }

      this.ws.onclose = () => {
        this.stopHeartbeat()
        this.emit('close')
        this.scheduleReconnect()
      }

      this.ws.onerror = () => {
        this.emit('error', new Error('WebSocket error'))
      }
    } catch {
      this.scheduleReconnect()
    }
  }

  private handleMessage(msg: WSMessage): void {
    switch (msg.type) {
      case 'append':
        if (msg.path && msg.entries) {
          this.emit('append', msg.path, msg.entries, msg.seq)
          if (msg.seq !== undefined) {
            this.subscriptions.set(msg.path, msg.seq)
          }
        }
        break
      case 'catchup':
        if (msg.path && msg.entries) {
          // 后端 Catchup 用 last_seq（JSON: lastSeq）携带高水位；
          // 兼容回退到 seq，避免协议不一致时丢更新。
          const catchupSeq = msg.lastSeq ?? msg.seq
          this.emit('catchup', msg.path, msg.entries, catchupSeq)
          if (catchupSeq !== undefined) {
            this.subscriptions.set(msg.path, catchupSeq)
          }
        }
        break
      case 'subscribed':
        if (msg.path) {
          this.emit('subscribed', msg.path, msg.totalLines)
        }
        break
      case 'truncate':
        if (msg.path) {
          this.emit('truncate', msg.path)
        }
        break
      case 'delete':
        if (msg.path) {
          this.emit('delete', msg.path)
        }
        break
      case 'error':
        this.emit('error', new Error(msg.message ?? 'Unknown server error'))
        break
      case 'pong':
        this.lastPongTime = Date.now()
        break
      case 'updateAvailable':
        this.emit(
          'updateAvailable',
          msg.latestVersion,
          msg.currentVersion,
          msg.releaseUrl,
        )
        break
    }
  }

  subscribe(path: string, afterSeq?: number): void {
    this.subscriptions.set(path, afterSeq ?? 0)
    if (this.ws?.readyState === WebSocket.OPEN) {
      const msg: Record<string, unknown> = { type: 'subscribe', path }
      if (afterSeq !== undefined) {
        msg.afterSeq = afterSeq
      }
      this.ws.send(JSON.stringify(msg))
    }
  }

  unsubscribe(path: string): void {
    this.subscriptions.delete(path)
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'unsubscribe', path }))
    }
  }

  private resubscribeAll(): void {
    for (const [path, seq] of this.subscriptions) {
      const msg: Record<string, unknown> = { type: 'subscribe', path }
      if (seq > 0) {
        msg.afterSeq = seq
      }
      this.ws?.send(JSON.stringify(msg))
    }
  }

  on(event: string, callback: Listener): () => void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set())
    }
    this.listeners.get(event)!.add(callback)
    return () => {
      this.listeners.get(event)?.delete(callback)
    }
  }

  private emit(event: string, ...args: unknown[]): void {
    this.listeners.get(event)?.forEach((fn) => fn(...args))
  }

  private startHeartbeat(): void {
    this.stopHeartbeat()
    this.lastPongTime = Date.now()
    this.heartbeatTimer = setInterval(() => {
      if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify({ type: 'ping' }))

        if (Date.now() - this.lastPongTime > this.pongTimeout) {
          this.ws.close()
        }
      }
    }, 30000)
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer !== null) {
      clearInterval(this.heartbeatTimer)
      this.heartbeatTimer = null
    }
  }

  private scheduleReconnect(): void {
    if (!this.shouldConnect) return
    if (!this.isTabVisible) return
    if (this.reconnectTimer !== null) return

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      this.connect()
      this.reconnectDelay = Math.min(this.reconnectDelay * 2, this.maxReconnectDelay)
    }, this.reconnectDelay)
  }

  private handleVisibilityChange(): void {
    this.isTabVisible = !document.hidden
    if (!this.isTabVisible || !this.shouldConnect) return

    this.reconnectDelay = 1000
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }

    if (this.ws) {
      this.ws.onclose = null
      this.ws.onerror = null
      this.ws.close()
      this.ws = null
    }
    this.connect()
  }

  disconnect(): void {
    this.shouldConnect = false
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    this.stopHeartbeat()
    if (this.ws) {
      this.ws.close()
      this.ws = null
    }
  }

  // Note: there is intentionally no destroy() method. The WSClient is an
  // app-scoped singleton (instantiated once by useTabs); it owns a
  // visibilitychange listener on `document` for the entire page lifetime and
  // is never torn down. If you need short-lived clients, add teardown here.
}
