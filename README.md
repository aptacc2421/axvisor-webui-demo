# axvisor webui demo

axvisor（Rust hypervisor）Web 管理界面目标架构的**可运行等价物**——在 Linux 上
用与真实 webui 相同的技术栈（React 18 + TS + Vite + shadcn/ui + Tailwind + xterm，
后端 Rust + axum 0.8 + tokio）演示控制面架构的三条核心主张：

1. **一切皆插件**：面板是独立组件，经注册表（slot）装配，互相零 import；
   插一个面板 = 目录 + 一行注册 + manifest 一个节点，存量代码零改动。
2. **资源是事件驱动的**：左栏是实时资源列表，`/ws/events` 用 wake/yield 推送
   （有变更 wake，无变更 yield），全 WebSocket（传输选型对齐 axvisor 真身）。
3. **终端是真交互 shell**：每台 VM 一份内存虚拟文件系统，支持
   `pwd / ls / mkdir / cd / echo > / cat`；大终端 + 标签拖拽融合，独占订阅位。

## 当前分支：full——模拟 shell + VM 终端（详细说明见下）。

## 分支剧本（三幕）

| 分支 | 有什么 | 演示什么 |
| --- | --- | --- |
| [`bare`](tree/bare) | 单根 Router + 空 manifest + 壳 | 「壳本身」：curl 一等公民、fallback、资源区空转 |
| [`with-vms`](compare/bare...with-vms) | + VmManager + 虚拟机管理面板 | 「后端挂上 = 界面出现」：事件驱动、证据落盘 |
| [`full`](compare/with-vms...full)（默认推荐看这个） | + 模拟 shell + VM 终端面板 | 「资源有了终端」：每 VM 独占 console、命令输出即回执 |

插拔成本看 diff：`git diff bare with-vms` / `git diff with-vms full`——
壳几乎零改动，95% 是新文件（功能本体）。

## 运行

```bash
git checkout full            # 或 bare / with-vms 看对应形态
cd server && cargo run       # :8080
cd web && npm i && npm run dev   # :5173（/api、/ws 代理）
```

浏览器 http://localhost:5173，token：`demo-token`。

- **虚拟机**面板：创建/停止 VM（异步 stop + 事件流推送终态）
- **VM 终端**面板：每台 VM 一个模拟 shell；标签可拖拽融合成同屏分列
- 独立复核：`tail -f server/data/console.log`、`cat server/data/vms.json`

## 文档

- [SPEC.md](SPEC.md)：最初的执行规格（12 条不变量），历史规格。
- [full 分支 README](blob/full/README.md)：三幕剧本与每步的实现决定。
- 设计文档对照：axvisor 仓库 `docs/design/axvisor-webui-control-plane-architecture.v3.md`。

---

## full 分支详述

面板收敛为两个：

- **「虚拟机」**（`panels/vms/`）：创建/停止/列表（纯管理页，零终端）；
- **「VM 终端」**（`panels/console/`）：每台 VM 一个模拟 shell（纯终端宿主，零管理按钮）。

shell 后端（`server/src/shell.rs`，架构详见 [ARCHITECTURE.md](ARCHITECTURE.md)）在内存虚拟文件系统上支持：
`pwd | ls | mkdir <dir> | cd <path> | echo <text> [> file | >> file] | cat <file>`。
每台 VM 独立文件系统与工作目录，跨连接持久。命令输出即「输入到达并被执行」的
界面内证据（终端右上角有 ↑/↓ 字节计数）；`console.log` 落盘可独立复核。

大终端 + 标签拖拽融合：拖一个终端标签到另一个上 = 融合成同屏分列（每格带
「分离」按钮拆回）。console 在面板挂载时即全部连接——后台默认流向浏览器。

历史：本仓库曾以 step-0..10 逐步演进（每步一个分支验证一类问题），收敛为
三幕时旧链打 tag `archive/steps-0-10` 存档，`git log archive/steps-0-10` 可考。
