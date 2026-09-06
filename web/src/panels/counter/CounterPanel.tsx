//! counter 面板 —— 对应真实 webui 的 VM 生命周期交互语义：
//! 每个操作一个 pending 态（防重）、破坏性操作过确认框、返回 async 后轮询到终态、
//! 超时显式报错、全程 AbortController 贯穿、错误展示 HTTP 状态 + 后端 error 两维。

import { useCallback, useEffect, useRef, useState } from 'react'
import { describeError, type PanelProps } from '@/api/types'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

const RESET_TIMEOUT_MS = 10_000
const POLL_INTERVAL_MS = 300

interface CounterValue {
  value: number
}

interface AsyncAccepted {
  ok: boolean
  async: boolean
  status: string
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) return reject(new DOMException('aborted', 'AbortError'))
    const timer = setTimeout(resolve, ms)
    signal.addEventListener(
      'abort',
      () => {
        clearTimeout(timer)
        reject(new DOMException('aborted', 'AbortError'))
      },
      { once: true },
    )
  })
}

export default function CounterPanel({ api }: PanelProps) {
  const [value, setValue] = useState<number | null>(null)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [confirming, setConfirming] = useState(false)
  const abortRef = useRef<AbortController | null>(null)

  // 每个操作独占一个 controller：开始新操作或卸载时，中止上一个（含它的轮询）
  const begin = useCallback(() => {
    abortRef.current?.abort()
    const ac = new AbortController()
    abortRef.current = ac
    return ac
  }, [])

  useEffect(() => () => abortRef.current?.abort(), [])

  const load = useCallback(async () => {
    const ac = begin()
    setError(null)
    try {
      const res = await api.get<CounterValue>('/api/counter', ac.signal)
      setValue(res.value)
    } catch (e) {
      if (!ac.signal.aborted) setError(describeError(e))
    }
  }, [api, begin])

  useEffect(() => {
    void load()
  }, [load])

  const apply = useCallback(
    async (delta: number) => {
      const ac = begin()
      setPending(true)
      setError(null)
      try {
        const res = await api.post<CounterValue>('/api/counter', { delta }, ac.signal)
        setValue(res.value)
      } catch (e) {
        if (!ac.signal.aborted) setError(describeError(e))
      } finally {
        if (!ac.signal.aborted) setPending(false)
      }
    },
    [api, begin],
  )

  const reset = useCallback(async () => {
    const ac = begin()
    setPending(true)
    setError(null)
    const startedAt = Date.now()
    try {
      await api.post<AsyncAccepted>('/api/counter', { reset: true }, ac.signal)
      // async 接受：轮询到终态 value === 0，或 10s 超时显式报错
      for (;;) {
        if (Date.now() - startedAt > RESET_TIMEOUT_MS) {
          throw new Error('reset 超时：10s 内未归零')
        }
        const res = await api.get<CounterValue>('/api/counter', ac.signal)
        setValue(res.value)
        if (res.value === 0) break
        await sleep(POLL_INTERVAL_MS, ac.signal)
      }
    } catch (e) {
      if (!ac.signal.aborted) setError(describeError(e))
    } finally {
      if (!ac.signal.aborted) setPending(false)
    }
  }, [api, begin])

  return (
    <Card className="max-w-xl">
      <CardHeader>
        <CardTitle>计数器</CardTitle>
        <CardDescription>
          值由执行层 task 独占拥有；面板只能经 REST 发命令。reset 返回 async，前端轮询到归零。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="font-mono text-5xl tabular-nums">{value ?? '—'}</div>

        <div className="flex gap-2">
          <Button disabled={pending} onClick={() => void apply(1)}>
            +1
          </Button>
          <Button disabled={pending} variant="secondary" onClick={() => void apply(-1)}>
            -1
          </Button>
          <Button
            disabled={pending}
            variant="destructive"
            onClick={() => setConfirming(true)}
          >
            {pending ? '处理中…' : 'reset'}
          </Button>
        </div>

        {error && (
          // 不变量 11：HTTP 状态 + 后端 error 两个维度都给出来
          <p className="rounded-md bg-destructive/10 px-3 py-2 font-mono text-sm text-destructive">
            {error}
          </p>
        )}
      </CardContent>

      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>确认 reset？</DialogTitle>
            <DialogDescription>
              破坏性操作：counter 会在 2s 后归零。对应真实 webui 里 stop VM 的确认。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(false)}>
              取消
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                setConfirming(false)
                void reset()
              }}
            >
              确认 reset
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  )
}
