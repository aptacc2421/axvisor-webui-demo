//! vms 面板 —— 纯管理页：计数、创建、列表、停止。零终端。
//!
//! 与 console 面板（VM 终端）是两个独立组件：本面板对 TerminalPanel 零 import，
//! 两者只共享壳注入的资源事件流（props.resources）。终端去「VM 终端」面板看，
//! 或直接点左栏资源区的某台 VM。

import { useState } from 'react'
import { VM_STATE_TEXT } from '@/api/events'
import { describeError, type PanelProps } from '@/api/types'
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

const STOP_DELAY_HINT = '2s 后停产'

export default function VmsPanel({ api, resources = [] }: PanelProps) {
  const [error, setError] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [confirmingStop, setConfirmingStop] = useState<number | null>(null)

  const create = async () => {
    setCreating(true)
    setError(null)
    try {
      // 同步创建（轻量操作）：列表变化由资源事件流自动推送
      await api.post<{ ok: boolean; id: number }>('/api/vms', { action: 'create' })
    } catch (e) {
      setError(describeError(e))
    } finally {
      setCreating(false)
    }
  }

  const stop = async (id: number) => {
    setConfirmingStop(null)
    setError(null)
    try {
      // async 接受：stopping → stopped 由事件流推送
      await api.post<{ ok: boolean; async: boolean; status: string }>(
        `/api/vms/${id}/stop`,
        { action: 'stop' },
      )
    } catch (e) {
      setError(describeError(e))
    }
  }

  return (
    <Card className="max-w-xl">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          虚拟机
          <Badge variant="secondary">{resources.length} 台</Badge>
        </CardTitle>
        <CardDescription>
          纯管理页。终端在「VM 终端」面板，或直接点左栏资源区的某台 VM。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2">
          <Button size="sm" disabled={creating} onClick={() => void create()}>
            {creating ? '创建中…' : '创建 VM（+1）'}
          </Button>
          {error && (
            <span className="font-mono text-xs text-destructive">{error}</span>
          )}
        </div>

        <ul className="divide-y rounded-md border">
          {resources.map((v) => (
            <li key={v.id} className="flex items-center justify-between px-3 py-2">
              <span className="flex items-center gap-2 font-mono text-sm">
                VM #{v.id}
                <Badge
                  variant={v.state === 'running' ? 'default' : 'outline'}
                  className="text-[10px]"
                >
                  {VM_STATE_TEXT[v.state]}
                </Badge>
              </span>
              {v.state === 'running' && (
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={() => setConfirmingStop(v.id)}
                >
                  停止
                </Button>
              )}
            </li>
          ))}
          {resources.length === 0 && (
            <li className="px-3 py-2 text-sm text-muted-foreground">
              还没有 VM——点「创建 VM」
            </li>
          )}
        </ul>
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
              破坏性操作：{STOP_DELAY_HINT}。console 页签会保留，但不再有输出——对应真实
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
