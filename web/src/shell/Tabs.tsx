//! 多标签：每标签一个面板实例。
//!
//! 所有已打开的标签都保持挂载（非激活的用 hidden 隐藏），这样终端这类
//! 「独占订阅」的面板在两个标签同时打开时才会真的撞上 409。
//!
//! 「+」新开实例：从 manifest 里的面板中挑一个，无条件新开一个标签。
//! 壳天生带这个能力——后面任何 step 插入面板，这里一行都不用改。

import { Suspense, useState } from 'react'
import type { ApiClient } from '@/api/client'
import type { PanelMeta, PanelRegistry } from '@/api/types'
import { cn } from '@/lib/utils'
import type { TabState } from './App'

interface TabsProps {
  panels: PanelMeta[]
  tabs: TabState[]
  activeId: string | null
  registry: PanelRegistry
  api: ApiClient
  token: string
  onActivate: (id: string) => void
  onClose: (id: string) => void
  onNew: (kind: string) => void
}

export function Tabs(props: TabsProps) {
  const { panels, tabs, activeId, registry, api, token, onActivate, onClose, onNew } = props
  const [pickerOpen, setPickerOpen] = useState(false)

  // 同 kind 多实例时给标签编号，方便辨认是哪个会话
  const counts = new Map<string, number>()
  for (const t of tabs) counts.set(t.kind, (counts.get(t.kind) ?? 0) + 1)
  const seen = new Map<string, number>()

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="flex items-center gap-1 border-b px-2 py-1">
        {tabs.map((t) => {
          const meta = panels.find((p) => p.kind === t.kind)
          if (!meta) return null
          const index = (seen.get(t.kind) ?? 0) + 1
          seen.set(t.kind, index)
          const multi = (counts.get(t.kind) ?? 0) > 1
          return (
            <div
              key={t.id}
              className={cn(
                'flex items-center gap-2 rounded-md px-3 py-1.5 text-sm',
                t.id === activeId ? 'bg-secondary' : 'text-muted-foreground hover:bg-accent',
              )}
            >
              <button type="button" onClick={() => onActivate(t.id)}>
                {meta.title}
                {multi && ` ·${index}`}
              </button>
              <button
                type="button"
                aria-label={`关闭 ${meta.title}`}
                className="text-muted-foreground hover:text-foreground"
                onClick={() => onClose(t.id)}
              >
                ×
              </button>
            </div>
          )
        })}
        <div className="relative">
          <button
            type="button"
            aria-label="新开面板实例"
            title="新开面板实例"
            className="rounded-md px-2.5 py-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
            onClick={() => setPickerOpen((o) => !o)}
          >
            +
          </button>
          {pickerOpen && (
            <div className="absolute right-0 top-full z-10 mt-1 w-60 rounded-md border bg-popover p-2 shadow-md">
              <p className="px-2 pb-1 text-xs text-muted-foreground">
                新开一个面板实例（每标签独立）
              </p>
              {panels.map((p) => (
                <button
                  key={p.kind}
                  type="button"
                  className="flex w-full items-center justify-between rounded-md px-2 py-1.5 text-sm hover:bg-accent"
                  onClick={() => {
                    setPickerOpen(false)
                    onNew(p.kind)
                  }}
                >
                  {p.title}
                  <span className="font-mono text-xs text-muted-foreground">{p.kind}</span>
                </button>
              ))}
              {panels.length === 0 && (
                <p className="px-2 py-1 text-xs text-muted-foreground">manifest 未暴露任何面板</p>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {tabs.length === 0 && (
          <p className="text-sm text-muted-foreground">从左侧导航打开一个面板。</p>
        )}
        {tabs.map((t) => {
          const meta = panels.find((p) => p.kind === t.kind)
          if (!meta) return null
          const Panel = registry.resolve(t.kind)
          return (
            <div key={t.id} className={cn('h-full', t.id === activeId ? 'block' : 'hidden')}>
              <Suspense fallback={<p className="text-sm text-muted-foreground">加载面板…</p>}>
                <Panel meta={meta} api={api} token={token} />
              </Suspense>
            </div>
          )
        })}
      </div>
    </div>
  )
}
