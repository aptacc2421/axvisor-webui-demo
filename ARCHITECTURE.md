# 架构文档

> 对应代码：[`full`](https://github.com/aptacc2421/axvisor-webui-demo/tree/full) 分支（功能最全形态）。
> 本 demo 是 axvisor Web 管理界面目标架构的**可运行等价物**：前端与真实 webui
> 同栈（React 18 + TS + Vite + shadcn/ui + Tailwind + xterm），后端与 axvisor
> 同构（Rust + axum 0.8 + tokio，单 Router 门面）。

## 1. 全景

```text
┌───────────────────────────── 浏览器 ─────────────────────────────┐
│  壳 shell/（唯一接线点 main.tsx）      面板 panels/（互相零 import）│
│  ├─ TokenGate   token 门（内存态）     ├─ vms/     管理页          │
│  ├─ App         布局·页签·资源点击     │   └─ 创建/停止/列表        │
│  ├─ Nav         能力区 + 资源区·实时   ├─ console/ 终端宿主        │
│  ├─ Tabs        多实例页签（全挂载）    │   └─ 大终端·拖拽融合       │
│  ├─ useResourceFeed 事件订阅（ws）     └─ TerminalPanel（xterm）   │
│  └─ registry    kind → 懒加载组件（fallback 兜底）                 │
└─────┬──────────────────────┬───────────────────────┬──────────────┘
      │ REST（Bearer 头）     │ ws /ws/events          │ ws /ws/vms/{id}/console
      │ 请求/响应             │ 单向广播（wake/yield）  │ 双向行式协议（独占）
┌─────▼──────────────────────▼───────────────────────▼──────────────┐
│                 传输层：一个 Router，一个 serve（main.rs）          │
│  /api/manifest  /api/vms(+stop)  /ws/events  /ws/vms/{id}/console │
│  /api、/ws 未知路径 → JSON 404     / → web/dist（未构建时 503）     │
└─────┬──────────────────────┬───────────────────────┬──────────────┘
      │                      │                       │
┌─────▼─────────┐   ┌────────▼────────┐   ┌──────────▼──────────────┐
│ manifest      │   │ VmManager       │──▶│ /ws/events 会话          │
│ （静态面板清单）│   │ 创建/异步 stop  │   │ watch 变更 → 推快照帧    │
└───────────────┘   │ 资源快照 watch   │   │ （wake/yield，无轮询）   │
                    └────────┬────────┘   └─────────────────────────┘
                             │ 订阅座位（独占）
                    ┌────────▼────────┐
                    │ ConsoleSession  │
                    │ Arc<Mutex<Shell>>│  内存虚拟文件系统 + 工作目录
                    │ pwd/ls/mkdir/cd/ │  每台 VM 一份，跨连接持久
                    │ echo>/cat        │
                    └─────────────────┘
```

## 2. 三条通道，三种语义

| 通道 | 方向 | 职责 | 鉴权 | 协议 |
| --- | --- | --- | --- | --- |
| `REST /api/*` | 请求/响应 | 控制面（manifest、创建/停止/列表） | `Authorization: Bearer` | JSON；未知路径 JSON 404 |
| `ws /ws/events` | 服务端 → 壳，单向广播 | 资源列表变化（实例级，wake/yield） | 查询参数 token | Text 帧：`hello`/`vms`/`ping` |
| `ws /ws/vms/{id}/console` | 双向 | 第 id 台 VM 的终端 | 查询参数 token | Binary：命令行/输出；独占座位 |

传输选型对齐 axvisor 真身（v3 设计文档「传输选型」：WebSocket 已上线，
SSE 因单向下行被否）——事件广播与终端各走一条 ws，互不耦合。

## 3. 壳与面板的接缝（插件化的关键）

参考 deepseek-harness「一切皆插件」的模式，在本 demo 尺度上的等价物：

- **唯一接线点**：`web/src/main.tsx` 把 `panels/registry.ts` 注入 `shell/App`。
  除此之外 `shell/` 对 `panels/*` 零 import（grep 可验证）。
- **注册表**：`registry.ts` 的 `renderers[kind]` 表 + `React.lazy` 懒加载；
  未注册 kind 落入 `FallbackPanel`（JSON 降级视图）——后端可以比前端新。
- **面板契约**：`PanelProps { meta, token, api, resources?, focusVm? }`。
  壳只转发数据，不认识任何 kind 字面量；面板间零 import，只共享 props。
- **插一个面板 = 三处**：`panels/` 加目录 + registry 一行 + manifest 一个节点。
  `git diff bare with-vms` / `git diff with-vms full` 是实测证据：壳几乎零改动，
  95% 是新文件（功能本体）。

## 4. 事件驱动资源流（wake/yield）

```text
VmManager：资源变更（create/stop 收尾）→ vm_snapshot() →
  state_tx.send_replace(list)                    ← wake：所有订阅者被唤醒
/ws/events 会话：挂起等 rx.changed()             ← yield：无变更不占 CPU
  → 变更到达 → borrow 快照 → 推 {"type":"vms","vms":[...]} 帧
客户端 EventSocket：收帧 → setState → React 重渲染左栏资源区与面板
  （断线指数退避自动重连；无轮询）
```

要点：
- 列表快照直接存在 `watch` 通道里，`list()` 是 O(1) 的 `borrow()`，
  REST 与 ws 共用同一份真相，没有两套查询路径。
- 左栏资源区是**实例级**的（每台 VM 一个条目 + 实时状态徽章），与
  「能力区」（面板 kind，manifest 驱动）并列，都零硬编码。

## 5. 每 VM console：独占座位 + 模拟 shell

- **独占 = 单一所有权**：订阅时从 VmManager 领一个「座位通道」的 receiver，
  receiver 活着 = 订阅位被占；会话断开 receiver 被 drop，订阅位自动释放。
  第二路连接在 upgrade **之前**被 409（浏览器 ws 拿不到握手状态码，前端用
  不带 Upgrade 头的普通 fetch 探测同一路由：被占 → 409，空闲 → 426）。
- **模拟 shell**（`server/src/shell.rs`）：内存虚拟文件系统
  （BTreeMap 目录树），行式命令 `pwd / ls / mkdir / cd / echo > / >> / cat`，
  支持 `..`、`/` 前缀、追加写；sh 风格报错。每台 VM 一份，跨连接持久。
- **回执即证据**：一帧命令 → 执行 → 推回输出 + 提示符 `vm{id}:/path$ `。
  终端右上角 `↑/↓` 字节计数实时反映收发；输出出现 = 命令确实到达并被执行。
- **数据不丢**：console 在面板挂载时即全部连接，不看也在排水——实测隐藏的
  console 切回时内部缓冲序号连续无丢帧。

## 6. 大终端 + 拖拽融合

- 终端组件全黑占满、顶部细条（标题/状态/丢帧/字节计数），零说明文字。
- 标签条可拖拽：拖标签 A 到标签 B 上 = 融合成同屏分列（`groups: number[][]`
  分组模型，HTML5 DnD），每格带「分离」按钮拆回；数据层只是展示组合，
  每格 console 仍连自己的通道。

## 7. 证据落盘

「UI 会骗人，文件不会」——执行层把状态写进 `server/data/`，每条主张都有
浏览器之外的独立证据：

| 文件 | 内容 | 验证什么 |
| --- | --- | --- |
| `console.log` | 每笔 IN（命令）/OUT（输出+提示符），带时间戳与 vm 标记 | 输入确实到达、命令确实执行 |
| `vms.log` | CREATE / STOP / STOPPED 生命周期事件 | stop 真的执行了、异步时序 |
| `vms.json` | VM 列表快照 | 列表状态与 UI 一致 |

## 8. 平台差异（demo ↔ axvisor 真身）

| 维度 | demo（Linux） | axvisor 真身 | 出处 |
| --- | --- | --- | --- |
| 静态资产 | 直读 fs（page cache 天然按需） | 启动一次性载入内存（禁止 handler 同步读） | SPEC §7 |
| 传输 | axum 原生 ws | 同（#2211 已上线；SSE 单向被否） | v3 设计文档 |
| 文件系统证据 | 写 fs | 走 axvisor 自身机制 | §7 |
| shell | 内存模拟 VFS | guest 串口另一端（帧协议不变） | v3 设计文档 |

## 9. 边界（明确不做）

多用户/RBAC、token 轮换、SSL、历史回放、观察者/键盘分离（真实 console mux
的形状，候选后续）、manifest 运行时热更新（当前靠分支 + 重启 + 刷新键）。
