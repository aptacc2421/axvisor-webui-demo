//! vm 组合面板 —— 对应真实 webui 的 VM 详情页：生命周期控制 + console 同屏。
//!
//! 插拔的第四种姿势：不新增任何后端触点，纯粹组合 panels/ 里已有的面板组件。
//! 壳对此一无所知——它只认 manifest 里的 kind:"vm"，渲染照旧走注册表。
//!
//! 注意终端的独占订阅位是全局唯一的：本面板里的 console 占住它之后，
//! 再用「+」开一个独立终端标签会撞上 409——这正是真实系统的语义
//! （一台 VM 的 console 只有一个，两个键盘同时敲 = 竞态）。

import type { PanelProps } from '@/api/types'
import CounterPanel from '../counter/CounterPanel'
import TerminalPanel from '../terminal/TerminalPanel'

export default function VmPanel(props: PanelProps) {
  return (
    <div className="flex h-full flex-col gap-3">
      <p className="text-xs text-muted-foreground">
        组合面板：控制面（counter task）与数据面（hello 流）同屏。对应真实 webui 的
        VM 详情页——左边生命周期操作，右边 guest console，且 console 全局独占。
      </p>
      <div className="grid min-h-0 flex-1 grid-cols-1 gap-4 xl:grid-cols-[380px_1fr]">
        <div className="min-h-0 overflow-auto">
          <CounterPanel {...props} />
        </div>
        <div className="min-h-0">
          <TerminalPanel {...props} />
        </div>
      </div>
    </div>
  )
}
