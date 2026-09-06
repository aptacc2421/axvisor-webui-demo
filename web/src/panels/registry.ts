//! 渲染器注册表：kind → 懒加载组件。
//!
//! 不变量 6：新增一个面板 = panels/ 加目录 + 本文件加一行 + manifest 加一个节点，
//! 共三处，其余（壳、路由、client）零改动。
//! bare 阶段：没有任何面板——注册表为空，导航能力区为空，演示「壳本身」。

import type { PanelComponent, PanelRegistry } from '@/api/types'
import { FallbackPanel } from './FallbackPanel'

const renderers: Record<string, PanelComponent> = {
  // ← 插入一个面板，这里就加一行（如 `vms: VmsPanel`）
}

export function resolvePanel(kind: string): PanelComponent {
  return renderers[kind] ?? FallbackPanel
}

export const panelRegistry: PanelRegistry = { resolve: resolvePanel }
