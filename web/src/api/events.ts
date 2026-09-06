//! 壳级资源事件流客户端：`/ws/events`（WebSocket，Text=状态帧，无 Binary）。
//!
//! 事件驱动（wake/yield）：服务端有资源变更就推一帧，没有就挂着；
//! 客户端不再轮询。断线自动重连（指数退避）——这是放弃 EventSource
//! 内建重连后需要自己补的唯一一块（传输选型对齐 axvisor 真身：全 ws）。

import { useEffect, useState } from 'react'
import { wsUrl } from './ws'

export type VmState = 'running' | 'stopping' | 'stopped'

export interface VmInfo {
  id: number
  state: VmState
}

export const VM_STATE_TEXT: Record<VmState, string> = {
  running: '运行中',
  stopping: '停止中…',
  stopped: '已停止',
}

type ResourceFrame =
  | { type: 'hello'; proto: number }
  | { type: 'vms'; vms: VmInfo[] }
  | { type: 'ping' }

export function useResourceFeed(token: string): { vms: VmInfo[]; live: boolean } {
  const [vms, setVms] = useState<VmInfo[]>([])
  const [live, setLive] = useState(false)

  useEffect(() => {
    let ws: WebSocket | null = null
    let closedByUs = false
    let retries = 0
    let timer: number | undefined

    const connect = () => {
      if (closedByUs) return
      ws = new WebSocket(wsUrl(token, '/ws/events'))
      ws.onopen = () => {
        retries = 0
        setLive(true)
      }
      ws.onmessage = (ev: MessageEvent) => {
        if (typeof ev.data !== 'string') return
        try {
          const frame = JSON.parse(ev.data) as ResourceFrame
          if (frame.type === 'vms') setVms(frame.vms)
          // hello / ping：握手与心跳，无需额外处理
        } catch {
          // 非 JSON 帧：忽略
        }
      }
      ws.onclose = () => {
        if (closedByUs) return
        setLive(false)
        const delay = Math.min(8000, 1000 * 2 ** retries++)
        timer = window.setTimeout(connect, delay)
      }
    }
    connect()

    return () => {
      closedByUs = true
      if (timer !== undefined) clearTimeout(timer)
      ws?.close()
    }
  }, [token])

  return { vms, live }
}
