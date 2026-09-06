//! 渲染器注册表：kind → 懒加载组件。
//!
//! 不变量 6：新增一个面板 = panels/ 加目录 + 本文件加一行 + manifest 加一个节点，
//! 共三处，其余（壳、路由、client）零改动。

import { lazy } from 'react'
import type { PanelComponent, PanelRegistry } from '@/api/types'
import { FallbackPanel } from './FallbackPanel'

// 懒加载：注册表里有 kind，不等于用户点了它——用到了才下载那一块代码
const CounterPanel = lazy(() => import('./counter/CounterPanel'))
const TerminalPanel = lazy(() => import('./terminal/TerminalPanel'))

const renderers: Record<string, PanelComponent> = {
  counter: CounterPanel, // ← 插入一个面板，这里就加这一行
  terminal: TerminalPanel,
}

export function resolvePanel(kind: string): PanelComponent {
  return renderers[kind] ?? FallbackPanel
}

export const panelRegistry: PanelRegistry = { resolve: resolvePanel }
