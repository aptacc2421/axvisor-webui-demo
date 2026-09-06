//! console 面板 —— VM 终端宿主（大终端形态）：顶部细标签条 + 全黑终端占满。
//!
//! 拖动标签页到另一个标签上 = 融合（同屏分列，像浏览器的标签合并）；
//! 融合视图里每格有「分离」按钮可拆回。所有 console 在面板挂载时即全部
//! 连接——后台默认流向浏览器，不看也在排水；停止的页签不消失，只是安静。
//!
//! 与 vms 面板（管理页）互相零 import，只共享壳注入的资源事件流。

import { useEffect, useRef, useState } from 'react'
import type { VmState } from '@/api/events'
import type { PanelProps } from '@/api/types'
import TerminalPanel from '../terminal/TerminalPanel'
import { cn } from '@/lib/utils'

export default function ConsolePanel({
  token,
  meta,
  api,
  resources = [],
  focusVm = null,
}: PanelProps) {
  /** 每组是一个「融合视图」：单元素组 = 普通标签，多元素组 = 同屏分列 */
  const [groups, setGroups] = useState<number[][]>([])
  const [activeGroup, setActiveGroup] = useState(0)
  const dragIdxRef = useRef<number | null>(null)
  const [dragging, setDragging] = useState<number | null>(null)

  // 资源列表变化 → 同步分组：新增的追加为独立标签，消失的剔除，不破坏已有融合
  useEffect(() => {
    setGroups((prev) => {
      const alive = resources.map((v) => v.id)
      const kept = prev
        .map((g) => g.filter((id) => alive.includes(id)))
        .filter((g) => g.length > 0)
      const inGroups = new Set(kept.flat())
      const added = alive.filter((id) => !inGroups.has(id)).map((id) => [id])
      return [...kept, ...added]
    })
  }, [resources])

  // 活动组越界夹紧
  useEffect(() => {
    if (activeGroup >= groups.length) {
      setActiveGroup(Math.max(0, groups.length - 1))
    }
  }, [groups, activeGroup])

  // 左栏资源区点击某台 VM → 激活包含它的组
  useEffect(() => {
    if (focusVm !== null) {
      const gi = groups.findIndex((g) => g.includes(focusVm))
      if (gi >= 0) setActiveGroup(gi)
    }
  }, [focusVm, groups])

  // 拖 A 到 B 上 = 融合：两组并一组（按 id 排序），活动组跟到 B
  const mergeGroups = (from: number, into: number) => {
    setGroups((prev) => {
      const next = prev.map((g) => [...g])
      const [moved] = next.splice(from, 1)
      const targetIdx = into > from ? into - 1 : into
      next[targetIdx] = [...next[targetIdx], ...moved].sort((a, b) => a - b)
      return next
    })
    setActiveGroup(into > from ? into - 1 : into)
  }

  // 融合视图里拆出一台 → 回到独立标签
  const splitOut = (id: number) => {
    setGroups((prev) => {
      const next = prev.map((g) => g.filter((x) => x !== id))
      next.push([id])
      return next.filter((g) => g.length > 0)
    })
    setActiveGroup(groups.length) // 新组追加在末尾
  }

  const stateOf = (id: number): VmState =>
    resources.find((v) => v.id === id)?.state ?? 'stopped'

  return (
    <div className="flex h-full min-w-0 flex-col bg-[#0b1021]">
      {/* 标签条：可拖动融合 */}
      <div
        className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-zinc-800 bg-zinc-950 px-1 py-1"
        onDragOver={(e) => e.preventDefault()}
      >
        {groups.map((g, i) => {
          const anyRunning = g.some((id) => stateOf(id) === 'running')
          const label =
            g.length > 1 ? g.map((id) => `#${id}`).join('+') : `VM #${g[0]}`
          return (
            <button
              key={g.join('-')}
              type="button"
              draggable
              onDragStart={() => {
                dragIdxRef.current = i
                setDragging(i)
              }}
              onDragEnd={() => setDragging(null)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault()
                if (dragIdxRef.current !== null && dragIdxRef.current !== i) {
                  mergeGroups(dragIdxRef.current, i)
                }
                dragIdxRef.current = null
                setDragging(null)
              }}
              onClick={() => setActiveGroup(i)}
              title="拖动到另一个标签上可融合为同屏分列"
              className={cn(
                'flex shrink-0 cursor-grab items-center gap-1.5 rounded-md px-2.5 py-1 font-mono text-xs',
                i === activeGroup
                  ? 'bg-zinc-800 text-zinc-100'
                  : 'text-zinc-500 hover:bg-zinc-900 hover:text-zinc-300',
                dragging === i && 'opacity-40',
              )}
            >
              <span
                className={cn(
                  'inline-block h-1.5 w-1.5 rounded-full',
                  anyRunning ? 'bg-emerald-400' : 'bg-zinc-600',
                )}
              />
              {label}
              {g.some((id) => stateOf(id) === 'stopping') && (
                <span className="text-[10px] text-amber-400">…</span>
              )}
            </button>
          )
        })}
        {groups.length === 0 && (
          <span className="px-2 text-xs text-zinc-600">
            还没有 VM——去「虚拟机」面板创建，或点左栏资源区
          </span>
        )}
      </div>

      {/* 终端区：活动组的所有 console 分列占满（大终端） */}
      <div className="min-h-0 flex-1">
        {groups.map((g, i) =>
          i === activeGroup ? (
            <div key={i} className="flex h-full gap-px bg-zinc-800">
              {g.map((id) => (
                <div key={id} className="relative flex min-w-0 flex-1 flex-col">
                  {g.length > 1 && (
                    <button
                      type="button"
                      onClick={() => splitOut(id)}
                      className="absolute right-1 top-1 z-10 rounded bg-zinc-950/80 px-1.5 py-0.5 text-[10px] text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
                      title="从融合视图分离出去"
                    >
                      分离
                    </button>
                  )}
                  <TerminalPanel
                    token={token}
                    api={api}
                    meta={meta}
                    title={`VM #${id}`}
                    consolePath={`/ws/vms/${id}/console`}
                  />
                </div>
              ))}
            </div>
          ) : null,
        )}
      </div>
    </div>
  )
}
