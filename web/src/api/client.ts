//! REST client。
//!
//! - baseUrl 用相对路径（不变量 10）：dev 靠 vite 代理，build 产物同源直连，
//!   dist 同一份多形态通用。
//! - 方法保持通用（get/post），新增面板不需要改这里。

import { useRef } from 'react'
import { ApiError } from './types'

export class ApiClient {
  constructor(private readonly getToken: () => string) {}

  get<T>(path: string, signal?: AbortSignal): Promise<T> {
    return this.request<T>('GET', path, undefined, signal)
  }

  post<T>(path: string, body: unknown, signal?: AbortSignal): Promise<T> {
    return this.request<T>('POST', path, body, signal)
  }

  private async request<T>(
    method: 'GET' | 'POST',
    path: string,
    body: unknown,
    signal?: AbortSignal,
  ): Promise<T> {
    const headers: Record<string, string> = { Authorization: `Bearer ${this.getToken()}` }
    const init: RequestInit = { method, headers, signal }
    if (body !== undefined) {
      headers['Content-Type'] = 'application/json'
      init.body = JSON.stringify(body)
    }

    const res = await fetch(path, init)
    if (!res.ok) {
      // 不变量 11：body 只读一次。拿到文本后自己解析，绝不二次消费 res。
      const raw = await res.text()
      let detail = raw
      try {
        const parsed: unknown = JSON.parse(raw)
        if (parsed !== null && typeof parsed === 'object' && 'error' in parsed) {
          const value = (parsed as { error: unknown }).error
          if (typeof value === 'string') detail = value
        }
      } catch {
        // 响应不是 JSON（例如 401 的纯文本）——原样带上文本，context 不丢
      }
      throw new ApiError(res.status, detail)
    }
    return (await res.json()) as T
  }
}

/**
 * 不变量 12：client 只在 ref 为 null 时构造一次，不随每次渲染重建；
 * token 经 getter 读取，配合 tokenRef 每次渲染刷新，读到的永远是最新的。
 */
export function useApiClient(token: string): ApiClient {
  const clientRef = useRef<ApiClient | null>(null)
  const tokenRef = useRef(token)
  tokenRef.current = token

  if (clientRef.current === null) {
    clientRef.current = new ApiClient(() => tokenRef.current)
  }
  return clientRef.current
}
