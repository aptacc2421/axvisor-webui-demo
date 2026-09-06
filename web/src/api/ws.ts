//! 帧协议 v1 客户端（不变量 9）：Binary = 数据，Text = 控制。
//!
//! 浏览器 WebSocket 拿不到握手失败的状态码（只有一个 1006），所以先用
//! 不带 Upgrade 头的普通 fetch 探测一次同一路由：server 在订阅位被占时
//! 会返回 409——独占语义就是这样在 UI 上变成一句明确的话的。

export type ControlFrame =
  | { type: 'hello'; proto: number }
  | { type: 'ping' }
  | { type: 'dropped'; count: number }

export type TermStatus = 'connecting' | 'open' | 'busy' | 'closed'

export interface TermHandlers {
  onData: (text: string) => void
  onControl: (frame: ControlFrame) => void
  onStatus: (status: TermStatus, detail?: string) => void
}

export function parseControl(raw: string): ControlFrame | null {
  try {
    const parsed: unknown = JSON.parse(raw)
    if (parsed !== null && typeof parsed === 'object' && 'type' in parsed) {
      return parsed as ControlFrame
    }
  } catch {
    // 不是 JSON 的 Text 帧：帧协议 v1 里没有这种东西，忽略
  }
  return null
}

/** 同源 ws(s) URL（不变量 10）：dev 走 5173 代理，集成形态直连 8080 */
export function wsUrl(token: string, path: string): string {
  const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${scheme}//${location.host}${path}?token=${encodeURIComponent(token)}`
}

/** 探测用的 http(s) 版：fetch 不认 ws scheme（浏览器实测抓到的 bug） */
export function httpUrl(token: string, path: string): string {
  const scheme = location.protocol === 'https:' ? 'https:' : 'http:'
  return `${scheme}//${location.host}${path}?token=${encodeURIComponent(token)}`
}

export class TermSocket {
  private ws: WebSocket | null = null
  private closedByUs = false

  constructor(
    private readonly token: string,
    private readonly handlers: TermHandlers,
    private readonly path = '/ws/term',
  ) {
    void this.connect()
  }

  /** 上行 Binary：用户输入 */
  send(text: string) {
    this.ws?.send(new TextEncoder().encode(text))
  }

  /** 上行 Text 控制（演示慢消费者用） */
  pause() {
    this.sendControl('pause')
  }

  resume() {
    this.sendControl('resume')
  }

  close() {
    this.closedByUs = true
    this.ws?.close()
    this.ws = null
  }

  private sendControl(type: 'pause' | 'resume') {
    this.ws?.send(JSON.stringify({ type }))
  }

  private async connect() {
    this.handlers.onStatus('connecting')

    try {
      const probe = await fetch(httpUrl(this.token, this.path))
      if (probe.status === 409) {
        this.handlers.onStatus('busy', '该终端已被占用（独占）')
        return
      }
    } catch {
      // 探测失败（比如 server 刚重启）就照常尝试 upgrade，让 ws 自己报错
    }

    // 探测期间可能已被 close()（组件卸载 / StrictMode 二次挂载）
    if (this.closedByUs) return

    const ws = new WebSocket(wsUrl(this.token, this.path))
    this.ws = ws
    ws.binaryType = 'arraybuffer'

    ws.onopen = () => this.handlers.onStatus('open')

    ws.onmessage = (ev: MessageEvent) => {
      if (typeof ev.data === 'string') {
        // Text = 控制帧
        const frame = parseControl(ev.data)
        if (frame) this.handlers.onControl(frame)
        return
      }
      // Binary = 数据帧
      const bytes =
        ev.data instanceof ArrayBuffer ? new Uint8Array(ev.data) : new Uint8Array()
      this.handlers.onData(new TextDecoder().decode(bytes))
    }

    ws.onclose = () => {
      if (!this.closedByUs) this.handlers.onStatus('closed', '连接已断开')
    }
    ws.onerror = () => {
      if (!this.closedByUs) this.handlers.onStatus('closed', '连接错误')
    }
  }
}
