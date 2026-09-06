//! 终端组件（大终端形态）：全黑占满、顶部细条放状态与字节计数、零说明文字。
//!
//! 输入到达服务器的界面内证据：↑ 计数 = 已上行字节（客户端发出），
//! ↓ 计数 = 已下行字节；服务端对每笔输入原样回显（§7 代演）——
//! 回显字符出现在屏幕上 + ↓ 随之增长 = round-trip 完成 = 确实到达了服务器。
//! 独立复核：tail -f server/data/console.log。

import { useEffect, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { TermSocket, type ControlFrame, type TermStatus } from '@/api/ws'
import type { PanelProps } from '@/api/types'

const STATUS_TEXT: Record<TermStatus, string> = {
  connecting: '连接中…',
  open: '已连接',
  busy: '已被占用',
  closed: '已断开',
}

export default function TerminalPanel({
  token,
  consolePath,
  title,
}: PanelProps & {
  /** 要连哪条 console：/ws/vms/{id}/console */
  consolePath: string
  title: string
}) {
  const hostRef = useRef<HTMLDivElement>(null)
  const [status, setStatus] = useState<TermStatus>('connecting')
  const [detail, setDetail] = useState<string | null>(null)
  const [dropped, setDropped] = useState(0)
  const [txBytes, setTxBytes] = useState(0)
  const [rxBytes, setRxBytes] = useState(0)
  const socketRef = useRef<TermSocket | null>(null)
  const lineRef = useRef('')

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
      // 容器处于 hidden 状态时尺寸为 0，fit 会拒绝——激活后由 ResizeObserver 补上
    }

    let tx = 0
    let rx = 0
    const socket = new TermSocket(
      token,
      {
        onData: (text) => {
          rx += text.length
          setRxBytes(rx)
          term.write(text)
        },
        onControl: (frame: ControlFrame) => {
          if (frame.type === 'dropped') {
            term.writeln(`\x1b[33m[控制] dropped ${frame.count}\x1b[0m`)
            setDropped((d) => d + frame.count)
          }
          // hello / ping：握手与心跳，不刷屏
        },
        onStatus: (s, d) => {
          setStatus(s)
          setDetail(d ?? null)
        },
      },
      consolePath,
    )
    socketRef.current = socket

    // 行式输入：本地行编辑（回显 + 退格），回车整行发给 server；
    // server 执行后推回输出 + 提示符——输出出现 = 命令确实到达并被执行
    const inputDisp = term.onData((data) => {
      if (data === '\r') {
        const line = lineRef.current
        lineRef.current = ''
        term.write('\r\n')
        if (line.length > 0) {
          tx += line.length
          setTxBytes(tx)
          socket.send(line)
        } else {
          socket.send('')
        }
      } else if (data === '\u007f') {
        if (lineRef.current.length > 0) {
          lineRef.current = lineRef.current.slice(0, -1)
          term.write('\b \b')
        }
      } else if (data >= ' ') {
        lineRef.current += data
        term.write(data)
      }
      // 方向键等控制序列：demo 的 shell 不需要，忽略
    })
    const ro = new ResizeObserver(() => {
      try {
        fit.fit()
      } catch {
        // 0 尺寸时跳过
      }
    })
    ro.observe(host)

    return () => {
      ro.disconnect()
      inputDisp.dispose()
      socket.close()
      term.dispose()
    }
  }, [token, consolePath])

  return (
    <div className="flex h-full min-w-0 flex-col overflow-hidden bg-[#0b1021]">
      <div className="flex shrink-0 items-center justify-between border-b border-zinc-800 px-2 py-1 text-xs text-zinc-400">
        <div className="flex items-center gap-2">
          <span className="font-mono text-zinc-200">{title}</span>
          <span className={status === 'open' ? 'text-emerald-400' : 'text-zinc-500'}>
            {STATUS_TEXT[status]}
          </span>
          {dropped > 0 && <span className="text-red-400">丢帧 {dropped}</span>}
        </div>
        <span
          className="font-mono text-[10px] tabular-nums"
          title="↑ 已上行字节（命令行）；↓ 已下行字节（输出+提示符）。输出出现 = 命令确实到达并被执行"
        >
          ↑{txBytes}B ↓{rxBytes}B
        </span>
      </div>

      {status === 'busy' && (
        <p className="bg-red-950/60 px-2 py-1 text-xs text-red-300">
          {detail ?? '该终端已被占用（独占）'}
        </p>
      )}
      {status === 'closed' && detail && (
        <p className="bg-zinc-900 px-2 py-1 text-xs text-zinc-400">{detail}</p>
      )}

      <div ref={hostRef} className="min-h-0 flex-1 p-1" />
    </div>
  )
}
