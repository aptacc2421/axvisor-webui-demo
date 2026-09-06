//! 渲染器注册表：kind → 懒加载组件。
//!
//! 不变量 6：新增一个面板 = panels/ 加目录 + 本文件加一行 + manifest 加一个节点，
//! 共三处，其余（壳、路由、client）零改动。

import type { PanelComponent, PanelRegistry } from '@/api/types'
import { FallbackPanel } from './FallbackPanel'

const renderers: Record<string, PanelComponent> = {
  // step-0 没有任何渲染器——probe 故意没有，用来演示降级
}

export function resolvePanel(kind: string): PanelComponent {
  return renderers[kind] ?? FallbackPanel
}

export const panelRegistry: PanelRegistry = { resolve: resolvePanel }
