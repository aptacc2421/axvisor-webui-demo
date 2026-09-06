//! 导航项 100% 来自 manifest（不变量 5）：这里没有任何 kind 的硬编码，
//! 后端 manifest 加一个节点，导航就多一项。

import { Badge } from '@/components/ui/badge'
import type { PanelMeta } from '@/api/types'

interface NavProps {
  panels: PanelMeta[]
  activeKind: string | null
  onOpen: (kind: string) => void
}

export function Nav({ panels, activeKind, onOpen }: NavProps) {
  return (
    <nav className="flex w-60 shrink-0 flex-col border-r bg-muted/30">
      <div className="px-4 py-3 text-xs font-medium uppercase tracking-wide text-muted-foreground">
        面板 · 来自 manifest
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
    </nav>
  )
}
