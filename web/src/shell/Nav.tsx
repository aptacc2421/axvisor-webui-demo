//! 导航项 100% 来自 manifest（不变量 5）：这里没有任何 kind 的硬编码，
//! 后端 manifest 加一个节点，导航就多一项。

import { Badge } from '@/components/ui/badge'
import { VM_STATE_TEXT, type VmInfo, type VmState } from '@/api/events'
import type { PanelMeta } from '@/api/types'

interface NavProps {
  panels: PanelMeta[]
  /** 壳级资源事件流的实时快照（wake/yield 推送，非轮询） */
  resources: VmInfo[]
  live: boolean
  activeKind: string | null
  onOpen: (kind: string) => void
  onOpenVm: (id: number) => void
}

export function Nav({ panels, resources, live, activeKind, onOpen, onOpenVm }: NavProps) {
  return (
    <nav className="flex w-60 shrink-0 flex-col overflow-y-auto border-r bg-muted/30">
      <div className="px-4 py-3 text-xs font-medium uppercase tracking-wide text-muted-foreground">
        能力 · 来自 manifest
      </div>
      <ul className="flex flex-col gap-1 px-2">
        {panels.map((p) => (
          <li key={p.kind}>
            <button
              type="button"
              onClick={() => onOpen(p.kind)}
              className={
                'flex w-full flex-col items-start gap-1 rounded-md px-3 py-2 text-left transition-colors ' +
                (p.kind === activeKind
                  ? 'bg-primary text-primary-foreground'
                  : 'hover:bg-accent hover:text-accent-foreground')
              }
            >
              <span className="text-sm font-medium">{p.title}</span>
              <span className="flex flex-wrap gap-1">
                {p.verbs.map((v) => (
                  <Badge key={v} variant="outline" className="text-[10px]">
                    {v}
                  </Badge>
                ))}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {panels.length === 0 && (
        <p className="px-4 py-2 text-sm text-muted-foreground">manifest 未暴露任何面板</p>
      )}

      <div className="mt-2 flex items-center justify-between px-4 py-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
        <span>资源 · 实时</span>
        <span
          className={
            'rounded-full px-1.5 py-0.5 text-[10px] ' +
            (live ? 'bg-green-100 text-green-700' : 'bg-amber-100 text-amber-700')
          }
          title="资源事件流连接状态"
        >
          {live ? '已连接' : '重连中'}
        </span>
      </div>
      <ul className="flex flex-col gap-1 px-2 pb-4">
        {resources.map((v) => (
          <li key={v.id}>
            <button
              type="button"
              onClick={() => onOpenVm(v.id)}
              className="flex w-full items-center justify-between rounded-md px-3 py-2 text-left text-sm hover:bg-accent hover:text-accent-foreground"
            >
              <span className="font-mono">VM #{v.id}</span>
              <StateBadge state={v.state} />
            </button>
          </li>
        ))}
        {resources.length === 0 && (
          <li className="px-4 py-1 text-sm text-muted-foreground">暂无资源</li>
        )}
      </ul>
    </nav>
  )
}

function StateBadge({ state }: { state: VmState }) {
  const variant = state === 'running' ? 'default' : state === 'stopping' ? 'secondary' : 'outline'
  return (
    <Badge variant={variant} className="text-[10px]">
      {VM_STATE_TEXT[state]}
    </Badge>
  )
}
