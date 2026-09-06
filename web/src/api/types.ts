//! 前后端契约类型：manifest / 面板渲染器契约 / 错误对象。
//! shell/ 只依赖这里，不依赖 panels/（不变量 5）。

import type { ComponentType } from 'react'
import type { ApiClient } from './client'

export interface PanelMeta {
  kind: string
  title: string
  verbs: string[]
}

export interface Manifest {
  proto: number
  panels: PanelMeta[]
}

/** 每个面板拿到的上下文：自己的 manifest 节点 + 一个 REST client。 */
export interface PanelProps {
  meta: PanelMeta
  token: string
  api: ApiClient
}

export type PanelComponent = ComponentType<PanelProps>

/** 注册表契约。具体实现在 panels/registry.ts，由入口注入进壳。 */
export interface PanelRegistry {
  resolve(kind: string): PanelComponent
}

/**
 * 错误对象同时携带 HTTP 状态码与后端 error 字段（不变量 11）。
 * 后端 body 只在 client 里读一次，读到的文本经这里两个维度呈现。
 */
export class ApiError extends Error {
  readonly status: number
  readonly detail: string

  constructor(status: number, detail: string) {
    super(`HTTP ${status} · ${detail}`)
    this.status = status
    this.detail = detail
  }
}

export function describeError(e: unknown): string {
  if (e instanceof ApiError) return `HTTP ${e.status} · ${e.detail}`
  return e instanceof Error ? e.message : String(e)
}
