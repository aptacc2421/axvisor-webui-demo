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

## 9. 代码来源：哪些是手写、哪些是生成

仓库里只有两类东西：**人写源码**（进 git）与**生成产物**（不进 git，构建/运行时产生）。
没有脚手架模板代码——web 脚手架的产物只有一次性的初始骨架，本 demo 全部重写。

### 手写源码（进 git）

| 位置 | 内容 | 说明 |
| --- | --- | --- |
| `server/src/*.rs` | main / state / shell / terminal / events / manifest / vm | 全部手写；`#[cfg(test)]` 单元测试也是 |
| `web/src/shell/*.tsx` | 壳（App/Nav/Tabs/TokenGate） | 手写 |
| `web/src/api/*.ts` | types / client / ws / events | 手写（契约层） |
| `web/src/panels/**/*.tsx` | vms / console 面板 | 手写 |
| `web/src/components/ui/*.tsx` | shadcn/ui 组件 | **shadcn CLI 生成**（`npx shadcn add button ...`）——官方设计是把标准组件源码拉进仓库「归你所有」，生成一次后当源码维护，不再自动更新 |
| 配置文件 | package.json / vite.config.ts / tsconfig.json / tailwind.config.js / postcss.config.js / Cargo.toml / index.html | 手写（shadcn init/CLI 有辅助，内容已审阅定型） |

### 生成产物（不进 git，.gitignore 排除）

| 位置 | 由谁生成 | 说明 |
| --- | --- | --- |
| `web/dist/` | `npm run build` → **vite（esbuild）** | **这就是 TS→JS 的地方**：TSX/TS 被转译、打包、压缩成 `assets/index-*.js`；`tsc --noEmit` 只做类型检查不产出文件。产物含 index.html + hash 文件名的 js/css |
| `server/target/` | `cargo build` | Rust 编译产物（二进制在 target/debug/） |
| `web/node_modules/` | `npm install` | 依赖安装目录 |
| `web/package-lock.json`、`server/Cargo.lock` | npm / cargo **自动生成但进 git** | 锁定依赖精确版本，保证可复现构建 |
| `server/data/` | 运行时证据落盘 | console.log / vms.log / vms.json（§7） |

### 构建流水线

```text
web/src/*.tsx ──tsc --noEmit──▶ 类型检查（不产出）
      │
      └─vite build（esbuild 转译 + rollup 打包 + 压缩）──▶ web/dist/（浏览器直接吃）
server/src/*.rs ──cargo build──▶ target/debug/webui_demo_server（运行时直读 dist）
```

要点：浏览器**从不接触 TS**——它只吃 dist 里编译后的 JS；server 直读 dist
（§7 平台差异），所以改前端要重新 `npm run build` 才在 8080 集成形态生效
（5173 dev 形态由 vite 即时转译，免 build）。

## 10. 目录树逐文件说明（非生成文件）

```text
webui_demo/
├── README.md                    门面导览（三幕剧本 + 运行方式）
├── ARCHITECTURE.md              本文档
├── SPEC.md                      最初的执行规格（历史）
│
├── server/                      后端（Rust + axum）
│   ├── Cargo.toml               依赖：axum/tokio/tower-http/serde（§9 清单）
│   └── src/
│       ├── main.rs              唯一 router() 装配：REST + ws + 静态 + JSON 404
│       │                        兜底 + Bearer 鉴权中间件 + / 入口
│       ├── state.rs             AppState；Evidence 证据落盘（console.log/
│       │                        vms.log/vms.json）；VmManager（创建/异步 stop/
│       │                        资源 watch 广播/console 独占座位）
│       ├── shell.rs             模拟 shell：内存虚拟文件系统 + 命令执行
│       │                        （pwd/ls/mkdir/cd/echo>/cat）+ 单元测试
│       ├── terminal.rs          /ws/vms/{id}/console：独占检查（409/426）+
│       │                        行式会话（命令→执行→输出+提示符）
│       ├── events.rs            /ws/events：资源事件广播（wake/yield）
│       ├── manifest.rs          GET /api/manifest：面板清单（静态 JSON）
│       └── vm.rs                /api/vms REST：列表/创建/异步停止
│
└── web/                         前端（React + TS + Vite）
    ├── index.html               挂载点 #root
    ├── package.json             依赖清单 + dev/build/test 脚本
    ├── vite.config.ts           dev 代理（/api、/ws → 8080）+ @ 别名
    ├── tsconfig.json            TS 选项 + @/* 路径映射
    ├── tailwind.config.js       Tailwind 主题（shadcn 色板变量映射）
    ├── postcss.config.js        tailwind/autoprefixer 插件
    ├── components.json          shadcn CLI 配置（生成 ui 组件用）
    └── src/
        ├── main.tsx             ★ 唯一接线点：挂 <App/> 并注入 registry
        ├── index.css            Tailwind 指令 + shadcn CSS 变量
        │
        ├── shell/               壳：不知道任何面板的存在（不变量 5）
        │   ├── App.tsx          token 门 → 布局；manifest 拉取；页签状态机；
        │   │                    资源点击路由（openVm）
        │   ├── TokenGate.tsx    token 输入页（内存态，不落 storage）
        │   ├── Nav.tsx          左栏：能力区（manifest 驱动）+ 资源区·实时
        │   │                    （事件流驱动，含连接状态徽章）
        │   └── Tabs.tsx         页签条 + 面板渲染区（全挂载 + Suspense）
        │
        ├── api/                 「怎么跟服务端说话」——所有跨网络代码
        │   ├── types.ts         契约类型：PanelMeta/Manifest/PanelProps/
        │   │                    ApiError（错误对象 = HTTP 状态 + 后端 error）
        │   ├── client.ts        ApiClient：REST 封装（Bearer 注入、body 只读
        │   │                    一次、错误封装）+ useApiClient（client 单例）
        │   ├── ws.ts            wsUrl/httpUrl 助手 + TermSocket（console 通道：
        │   │                    探测 409/426、行式收发、状态回调）
        │   └── events.ts        EventSocket（/ws/events + 指数退避重连）+
        │                        useResourceFeed（资源列表状态 hook）+ VmInfo
        │
        ├── lib/                 「跟业务无关的通用小工具」
        │   └── utils.ts         cn()：clsx + tailwind-merge——条件类名合并
        │                        与冲突消解（shadcn 组件全靠它）
        │
        ├── panels/              面板（互相零 import，只消费 props）
        │   ├── registry.ts      ★ kind → 懒加载组件注册表；未注册 → fallback
        │   ├── FallbackPanel.tsx 未知 kind 的 JSON 降级视图（前向兼容）
        │   ├── vms/VmsPanel.tsx     「虚拟机」管理页：计数/创建/列表/停止
        │   └── console/
        │       ├── ConsolePanel.tsx   「VM 终端」宿主：分组/拖拽融合/分离
        │       └── TerminalPanel.tsx  单个终端：xterm + 行式输入编辑 +
        │                              ↑/↓ 字节计数条
        │
        └── components/ui/       shadcn 生成的基础组件（badge/button/card/
                                dialog/input）——生成一次后当源码维护
```

### api/ 与 lib/ 为什么分两处

- **`api/` 有业务语义**：它描述的是「本系统与外界的契约」——路径、鉴权方式、
  帧格式、错误形状。协议一变，改的就是这里（且只在这里）。
- **`lib/` 无业务语义**：`cn()` 换个项目照样用。它不知道服务端存在。
- 分开放是为了让 diff 说话：「改协议」的提交只动 `api/`，「调样式」的提交
  只动 `lib/` 或组件——两类变更永不混在一个文件里。

## 11. 边界（明确不做）

多用户/RBAC、token 轮换、SSL、历史回放、观察者/键盘分离（真实 console mux
的形状，候选后续）、manifest 运行时热更新（当前靠分支 + 重启 + 刷新键）。
