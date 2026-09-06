//! vm 组合面板 —— 对应真实 webui 的 VM 详情页：资源列表 + 生命周期 + 每 VM console。
//!
//! 数据是事件驱动的：VM 列表来自壳级资源事件流（props 注入，wake/yield 推送，
//! 本面板零轮询）；创建/停止走 POST /api/vms 系列接口，列表变化由事件流自动
//! 反映——「+1 台就多一个 console」不需要任何手动刷新。
//!
//! 耦合纪律（积木式组合，不焊接）：
//! - console 子页签/同屏布局在本面板内部管理，壳毫不知情；
//! - 每 VM console 复用 TerminalPanel（props 传入 consolePath），不摸对方内部；
//! - console 在面板挂载时即全部连接：字符后台默认流向浏览器（不论看没看，
//!   防止内容在缓冲区积压）。
//! - 停止是「停产不停服」：页签不消失，console 连接保持，只是不再有输出。

import { useEffect, useState } from 'react'
import { VM_STATE_TEXT } from '@/api/events'
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

const STOP_DELAY_HINT = '2s 后停产'

export default function VmPanel({
  api,
  token,
  meta,
  resources = [],
  focusVm = null,
}: PanelProps) {
  const [error, setError] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [activeId, setActiveId] = useState<number | null>(null)
  const [confirmingStop, setConfirmingStop] = useState<number | null>(null)
  const [layout, setLayout] = useState<'tabs' | 'grid'>('tabs')

  // 壳请求聚焦某台 VM（左栏资源区点击）→ 激活它的 console
  useEffect(() => {
    if (focusVm !== null) setActiveId(focusVm)
  }, [focusVm])

  const create = async () => {
    setCreating(true)
    setError(null)
    try {
      // 同步创建（轻量操作）：列表变化由资源事件流自动推送
      const res = await api.post<{ ok: boolean; id: number }>('/api/vms', {
        action: 'create',
      })
      setActiveId(res.id)
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
      // async 接受：stopping → stopped 的状态推进由事件流自动到达
      await api.post<{ ok: boolean; async: boolean; status: string }>(
        `/api/vms/${id}/stop`,
        { action: 'stop' },
      )
    } catch (e) {
      setError(describeError(e))
    }
  }

  const active = resources.find((v) => v.id === activeId) ?? null

  return (
    <Card className="flex h-full flex-col">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          虚拟机
          {/* 计数 = 现有 VM 数量：来自资源事件流，不是兄弟面板的值 */}
          <Badge variant="secondary">{resources.length} 台</Badge>
          <div className="ml-auto flex items-center gap-1">
            <Button
              size="sm"
              variant={layout === 'grid' ? 'secondary' : 'ghost'}
              onClick={() => setLayout('grid')}
            >
              同屏
            </Button>
            <Button
              size="sm"
              variant={layout === 'tabs' ? 'secondary' : 'ghost'}
              onClick={() => setLayout('tabs')}
            >
              分页
            </Button>
          </div>
        </CardTitle>
        <CardDescription>
          console 在面板挂载时即全部连接——后台默认流向浏览器，不看也在排水。
          停止后页签不消失，{STOP_DELAY_HINT}，只是安静下来。
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

        {/* 子页签 */}
        <div className="flex flex-wrap items-center gap-1 border-b pb-2">
          {resources.map((v) => (
            <button
              key={v.id}
              type="button"
              onClick={() => setActiveId(v.id)}
              className={cn(
                'flex items-center gap-1.5 rounded-md px-2.5 py-1 text-sm',
                v.id === activeId && layout === 'tabs'
                  ? 'bg-secondary'
                  : 'text-muted-foreground hover:bg-accent',
              )}
            >
              VM #{v.id}
              <Badge
                variant={v.state === 'running' ? 'default' : 'outline'}
                className="text-[10px]"
              >
                {VM_STATE_TEXT[v.state]}
              </Badge>
            </button>
          ))}
          {resources.length === 0 && (
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
              VM #{active.id} 停止中…（async，事件流推送终态）
            </span>
          )}
        </div>

        {/* console 区：全部挂载（不论看没看都在排水），同屏网格或分页切换 */}
        <div
          className={cn(
            'min-h-0 flex-1',
            layout === 'grid' ? 'grid grid-cols-1 gap-3 xl:grid-cols-2' : '',
          )}
        >
          {resources.map((v) => (
            <div
              key={v.id}
              className={cn(
                'min-h-[240px]',
                layout === 'grid' ? 'h-72' : 'h-full',
                (layout === 'tabs' && v.id === activeId) || layout === 'grid'
                  ? 'block'
                  : 'hidden',
              )}
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
