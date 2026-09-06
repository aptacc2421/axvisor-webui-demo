//! vm 组合面板 —— 对应真实 webui 的 VM 详情页：资源列表 + 生命周期 + 每 VM console。
//!
//! 耦合纪律（积木式组合，不焊接）：
//! - VM 数量来自 GET /api/vms（资源层），不是去读 counter 面板的值；
//! - 创建/停止走 POST /api/vms 系列接口，stop 复用「async 接受 + 轮询到终态」
//!   的语义（2s 后停产）；
//! - console 子页签在本面板内部管理，壳毫不知情；每个 console 用
//!   TerminalPanel 连自己的 /ws/vms/{id}/console——组件复用走 props，
//!   不摸对方内部。
//! - 停止是「停产不停服」：页签不消失，console 连接保持，只是不再有输出。

import { useCallback, useEffect, useRef, useState } from 'react'
import { describeError, type PanelProps } from '@/api/types'
import TerminalPanel from '../terminal/TerminalPanel'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { cn } from '@/lib/utils'

type VmState = 'running' | 'stopping' | 'stopped'

interface VmInfo {
  id: number
  state: VmState
}

const STATE_TEXT: Record<VmState, string> = {
  running: '运行中',
  stopping: '停止中…',
  stopped: '已停止',
}

export default function VmPanel({ api, token, meta }: PanelProps) {
  const [vms, setVms] = useState<VmInfo[]>([])
  const [error, setError] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [activeId, setActiveId] = useState<number | null>(null)
  const [confirmingStop, setConfirmingStop] = useState<number | null>(null)
  const pollAbortRef = useRef<AbortController | null>(null)

  // 资源列表轮询：1s 一次（顺带捕捉 stopping → stopped 的终态推进）
  useEffect(() => {
    let timer: number | undefined
    const tick = async () => {
      pollAbortRef.current?.abort()
      const ac = new AbortController()
      pollAbortRef.current = ac
      try {
        const res = await api.get<{ vms: VmInfo[] }>('/api/vms', ac.signal)
        setVms(res.vms)
        setError(null)
        // 初次加载聚焦最新一台；之后不再抢用户的焦点
        setActiveId((cur) => cur ?? res.vms.at(-1)?.id ?? null)
      } catch (e) {
        if (!ac.signal.aborted) setError(describeError(e))
      }
    }
    void tick()
    timer = window.setInterval(() => void tick(), 1000)
    return () => {
      window.clearInterval(timer)
      pollAbortRef.current?.abort()
    }
  }, [api])

  const create = useCallback(async () => {
    setCreating(true)
    setError(null)
    try {
      // 同步创建（轻量操作）：+1 台，立即聚焦它的新 console 页签
      const res = await api.post<{ ok: boolean; id: number }>('/api/vms', {
        action: 'create',
      })
      setActiveId(res.id)
      const fresh = await api.get<{ vms: VmInfo[] }>('/api/vms')
      setVms(fresh.vms)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setCreating(false)
    }
  }, [api])

  const stop = useCallback(
    async (id: number) => {
      setConfirmingStop(null)
      setError(null)
      try {
        // async 接受：状态由 1s 轮询推进（stopping → stopped）
        await api.post<{ ok: boolean; async: boolean; status: string }>(
          `/api/vms/${id}/stop`,
          { action: 'stop' },
        )
      } catch (e) {
        setError(describeError(e))
      }
    },
    [api],
  )

  const active = vms.find((v) => v.id === activeId) ?? null

  return (
    <Card className="flex h-full flex-col">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          虚拟机
          {/* 计数 = 现有 VM 数量：来自资源列表，不是兄弟面板的值 */}
          <Badge variant="secondary">{vms.length} 台</Badge>
        </CardTitle>
        <CardDescription>
          +1 台就多一个 console 页签；停止后页签不消失，但不再有输出。
          资源证据：<code>server/data/vms.json</code>、
          <code>vms.log</code>、<code>console.log</code>（vm&#123;id&#125; 行）。
        </CardDescription>
      </CardHeader>

      <CardContent className="flex min-h-0 flex-1 flex-col gap-3">
        <div className="flex items-center gap-2">
          <Button size="sm" disabled={creating} onClick={() => void create()}>
            {creating ? '创建中…' : '创建 VM（+1）'}
          </Button>
          {error && (
            <span className="font-mono text-xs text-destructive">{error}</span>
          )}
        </div>

        {/* console 子页签：面板内部管理，壳对此一无所知 */}
        <div className="flex flex-wrap items-center gap-1 border-b pb-2">
          {vms.map((v) => (
            <button
              key={v.id}
              type="button"
              onClick={() => setActiveId(v.id)}
              className={cn(
                'flex items-center gap-1.5 rounded-md px-2.5 py-1 text-sm',
                v.id === activeId
                  ? 'bg-secondary'
                  : 'text-muted-foreground hover:bg-accent',
              )}
            >
              VM #{v.id}
              <Badge
                variant={v.state === 'running' ? 'default' : 'outline'}
                className="text-[10px]"
              >
                {STATE_TEXT[v.state]}
              </Badge>
            </button>
          ))}
          {vms.length === 0 && (
            <span className="text-xs text-muted-foreground">
              还没有 VM——点「创建 VM」
            </span>
          )}
        </div>

        {/* 活跃 VM 的生命周期操作（破坏性操作过确认框） */}
        <div className="flex items-center gap-2">
          {active && active.state === 'running' && (
            <Button
              size="sm"
              variant="destructive"
              onClick={() => setConfirmingStop(active.id)}
            >
              停止 VM #{active.id}
            </Button>
          )}
          {active?.state === 'stopping' && (
            <span className="text-sm text-muted-foreground">
              VM #{active.id} 停止中…（async，轮询到终态）
            </span>
          )}
        </div>

        {/* console 区：全部挂载、非激活隐藏——每台 VM 独占自己的订阅位 */}
        <div className="min-h-[280px] flex-1">
          {vms.map((v) => (
            <div
              key={v.id}
              className={cn('h-full', v.id === activeId ? 'block' : 'hidden')}
            >
              <TerminalPanel
                token={token}
                api={api}
                meta={meta}
                consolePath={`/ws/vms/${v.id}/console`}
              />
            </div>
          ))}
        </div>
      </CardContent>

      <Dialog
        open={confirmingStop !== null}
        onOpenChange={(open) => {
          if (!open) setConfirmingStop(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>确认停止 VM #{confirmingStop}？</DialogTitle>
            <DialogDescription>
              破坏性操作：2s 后停产。console 页签会保留，但不再有输出——对应真实
              webui 里 stop VM 的确认。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmingStop(null)}>
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={() => confirmingStop !== null && void stop(confirmingStop)}
            >
              确认停止
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  )
}
