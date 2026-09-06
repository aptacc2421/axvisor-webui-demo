//! console 面板 —— VM 终端宿主：同屏网格 / 分页子页签展示所有 VM 的 console。
//!
//! 与 vms 面板（管理页）是两个独立组件：本面板零生命周期按钮，只有终端。
//! 每个 VM console 复用 TerminalPanel（props 传入 consolePath），面板挂载时
//! 即全部连接——字符后台默认流向浏览器（不看也在排水，防止缓冲区积压）。
//! 停止是「停产不停服」：页签不消失，连接保持，只是安静下来。

import { useEffect, useState } from 'react'
import { VM_STATE_TEXT } from '@/api/events'
import type { PanelProps } from '@/api/types'
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
import { cn } from '@/lib/utils'

export default function ConsolePanel({
  token,
  meta,
  api,
  resources = [],
  focusVm = null,
}: PanelProps) {
  const [activeId, setActiveId] = useState<number | null>(null)
  const [layout, setLayout] = useState<'tabs' | 'grid'>('tabs')

  // 左栏资源区点击某台 VM → 分页模式下激活它的 console
  useEffect(() => {
    if (focusVm !== null) {
      setLayout('tabs')
      setActiveId(focusVm)
    }
  }, [focusVm])

  // 分页模式至少亮一台：打开面板就能看到字符在流
  useEffect(() => {
    if (activeId === null && resources.length > 0) {
      setActiveId(resources[0].id)
    }
  }, [activeId, resources])

  return (
    <Card className="flex h-full flex-col">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          VM 终端
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
          停止后的页签不消失，只是安静下来。
        </CardDescription>
      </CardHeader>

      <CardContent className="flex min-h-0 flex-1 flex-col gap-3">
        {layout === 'tabs' && (
          <div className="flex flex-wrap items-center gap-1 border-b pb-2">
            {resources.map((v) => (
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
                  {VM_STATE_TEXT[v.state]}
                </Badge>
              </button>
            ))}
            {resources.length === 0 && (
              <span className="text-xs text-muted-foreground">
                还没有 VM——去「虚拟机」面板创建，或点左栏资源区
              </span>
            )}
          </div>
        )}

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
    </Card>
  )
}
