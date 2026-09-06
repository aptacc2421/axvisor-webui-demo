//! terminal 面板 —— 对应真实 webui 的串口终端。
//!
//! 演示两件事：
//! 1. 独占：第二个标签开终端会被 server 409，面板给出明确文案；
//! 2. 背压：点「暂停」后本会话停止消费，通道 4s（容量 8 × 500ms）后溢出，
//!    生产者只丢不堵；「恢复」先收到 dropped 帧，且数据序号跳变。

import { useEffect, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { TermSocket, type ControlFrame, type TermStatus } from '@/api/ws'
import type { PanelProps } from '@/api/types'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'

const STATUS_TEXT: Record<TermStatus, string> = {
  connecting: '连接中…',
  open: '已连接',
  busy: '已被占用',
  closed: '已断开',
}

export default function TerminalPanel({ token }: PanelProps) {
  const hostRef = useRef<HTMLDivElement>(null)
  const [status, setStatus] = useState<TermStatus>('connecting')
  const [detail, setDetail] = useState<string | null>(null)
  const [paused, setPaused] = useState(false)
  const [dropped, setDropped] = useState(0)
  const socketRef = useRef<TermSocket | null>(null)

  useEffect(() => {
    const host = hostRef.current
    if (!host) return

    const term = new Terminal({
      convertEol: true,
      fontSize: 13,
      theme: { background: '#0b1021', foreground: '#c8d3f5' },
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(host)
    try {
      fit.fit()
    } catch {
      // 标签处于 hidden 状态时容器尺寸为 0，fit 会拒绝——激活后再由 ResizeObserver 补上
    }

    const socket = new TermSocket(token, {
      onData: (text) => term.write(text),
      onControl: (frame: ControlFrame) => {
        if (frame.type === 'hello') {
          term.writeln(`\x1b[36m[控制] hello proto=${frame.proto}\x1b[0m`)
        } else if (frame.type === 'dropped') {
          term.writeln(`\x1b[33m[控制] dropped ${frame.count}\x1b[0m`)
          setDropped((d) => d + frame.count)
        }
        // ping 是心跳，不刷屏
      },
      onStatus: (s, d) => {
        setStatus(s)
        setDetail(d ?? null)
      },
    })
    socketRef.current = socket

    // 用户输入走 Binary 帧，回显由 server 代演
    const inputDisp = term.onData((data) => socket.send(data))
    const ro = new ResizeObserver(() => {
      try {
        fit.fit()
      } catch {
        // 同上：0 尺寸时跳过
      }
    })
    ro.observe(host)

    return () => {
      ro.disconnect()
      inputDisp.dispose()
      socket.close()
      term.dispose()
    }
  }, [token])

  return (
    <Card className="flex h-full flex-col">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          终端
          <Badge variant={status === 'open' ? 'default' : 'secondary'}>
            {STATUS_TEXT[status]}
          </Badge>
          {dropped > 0 && (
            <Badge variant="destructive">丢帧 {dropped}</Badge>
          )}
        </CardTitle>
        <CardDescription>
          hello 流每 500ms 一行；独占订阅位——两个终端标签同时开会撞上 409。
        </CardDescription>
      </CardHeader>
      <CardContent className="flex min-h-0 flex-1 flex-col gap-3">
        <div className="flex gap-2">
          {paused ? (
            <Button
              size="sm"
              onClick={() => {
                setPaused(false)
                socketRef.current?.resume()
              }}
            >
              恢复
            </Button>
          ) : (
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                setPaused(true)
                socketRef.current?.pause()
              }}
            >
              暂停（模拟慢消费者）
            </Button>
          )}
        </div>

        {status === 'busy' && (
          <p className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {detail ?? '该终端已被占用（独占）'}——关掉另一个终端标签，或用「+」再开一个会话。
          </p>
        )}
        {status === 'closed' && detail && (
          <p className="rounded-md bg-muted px-3 py-2 text-sm text-muted-foreground">{detail}</p>
        )}

        <div ref={hostRef} className="min-h-[240px] flex-1 overflow-hidden rounded-md border" />
      </CardContent>
    </Card>
  )
}
