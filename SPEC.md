# webui_demo 执行文档（SPEC）

> 本文档是给编码执行者的完整规格。执行者**不需要** axvisor 上下文——每条架构
> 思想都以「不变量」形式内嵌在 §5，照做即可。

## 0. 定位：demo 是 webui 目标架构的可运行等价物

真实项目 axvisor（Rust hypervisor）有一个 Web 管理界面（webui）架构设计。
本 demo 在 Linux 上构建该架构的**等价物**：

- **前端与真实 webui 同栈同构**：React 18 + TypeScript + Vite 5 + shadcn/ui
  + Tailwind，目录三分 `shell/ panels/ api/`，manifest 驱动导航，面板注册表
  渲染。demo 的前端代码就是未来真实 webui 的骨架，可直接移植。
- **后端接口全部替代**：webui 需要的接口（VM 生命周期、串口终端、鉴权）在
  demo 里由 counter 任务与 hello 流模拟——形态（REST + ws + manifest + 帧
  协议）完全一致，执行层内容不同。
- **演示目的**：在浏览器里看见 webui 架构如何运转——分层、可插拔、背压、
  独占订阅、未知 kind 降级、curl 一等公民。

## 1. 技术栈（与真实 webui 分支对齐）

| 项 | 选择 | 说明 |
| --- | --- | --- |
| 后端 | Rust + axum 0.8 + tokio | 与 axvisor 同构 |
| 前端 | React 18.3.1 + TypeScript 5.6 + Vite 5.4.11 | 同 webui |
| UI 组件 | shadcn/ui（radix-ui + cva + clsx + tailwind-merge + lucide-react） | 同 webui |
| 样式 | Tailwind 3.4 + tailwindcss-animate + postcss | 同 webui |
| 终端 | @xterm/xterm + @xterm/addon-fit | 预演真实终端面板 |
| 测试 | vitest | 逻辑层测试；组件测试矩阵不做（§8） |
| 端口 | server 8080；vite dev 5173 | |
| engines | node >= 24, npm >= 11.12.1 | 同 webui |
| 鉴权 | 固定 token `demo-token`（内存态） | 传输层统一鉴权的占位 |

## 2. 架构与接口替代表

```text
                      ┌─────────── 同一个 axum Router ───────────┐
浏览器壳(React+shadcn) ──GET /──→ 静态资产(直读 web/dist，Linux page cache 天然按需)
  面板A: counter ──POST /api/counter──→ counter.rs ──命令通道──→ ┐
  面板B: terminal ──ws /ws/term?token=──→ terminal.rs ──独占订阅──┤
                                    hello.rs(生产者，永不被反压) ←┘ 执行层
curl ──(同一路由树，一等公民)──────────────────────────────────┘
```

**webui 接口 → demo 替代**（核心对照，执行者按左列的「形态」实现右列）：

| webui 需要的接口 | demo 替代 | demo 执行层 |
| --- | --- | --- |
| `GET /api/vms`（资源列表） | `GET /api/counter` | counter task（值 + 命令通道） |
| `POST /api/vms/{id}/{action}`（start/stop/pause/resume，**异步接受 + 轮询到终态 + 超时**） | `POST /api/counter`（delta 同步；**reset 异步接受 + 轮询到归零 + 超时**） | 同 |
| `/ws/vms/{id}/console`（guest 串口流，独占，M3 规划） | `/ws/term`（hello 流，独占） | hello task + 有界通道 |
| `GET /api/manifest`（资源层发现） | 同名同构 | manifest.rs 静态 JSON |
| Bearer token 鉴权 | 固定 `demo-token` | 同 |
| stop 需确认 Dialog | reset 需确认 Dialog | 同语义 |

**demo ↔ webui ↔ axvisor 三层映射**（写给未来回看的你，执行者可忽略）：

| demo | webui 目标架构 | axvisor 真身 |
| --- | --- | --- |
| counter task（独占值，经通道服务请求） | 领域操作面板 | AxvmManager 生命周期 |
| reset 异步接受 + 轮询 + 超时 + AbortController | VM action 交互语义 | 同 |
| hello task + 有界通道 + try_send | —（数据面） | guest 串口 + console_mux 环 + 泵 |
| terminal.rs 独占订阅 | 终端面板桥 | 网络订阅位（每 VM 唯一） |
| manifest.rs | web/resources/ | 资源层 manifest |
| web/src/shell/ | web-ui/src/shell/ | 应用壳 |
| web/src/panels/registry.ts | 渲染器注册表 | 同 |
| 直读 fs 服务静态资产 | assets.rs（fs 一次性载入） | 同（Linux 有 page cache，直读即按需） |

## 3. 目录结构（按此创建，不增不减顶层）

```text
webui_demo/
├── SPEC.md                     ← 本文档
├── README.md                   ← 运行方式（每分支写清楚）
├── server/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             ← 唯一 router() 装配：/api + /ws + / 静态，一个 Router
│       ├── state.rs            ← 执行层：counter task + hello task 的创建与句柄
│       ├── manifest.rs         ← GET /api/manifest（静态 JSON）
│       ├── counter.rs          ← GET/POST /api/counter（经命令通道，不直捣状态）
│       └── terminal.rs         ← GET /ws/term（ws upgrade、帧协议、独占、echo）
└── web/
    ├── package.json / package-lock.json
    ├── vite.config.ts          ← dev 代理 /api、/ws → http://localhost:8080
    ├── tailwind.config.js / postcss.config.js / tsconfig.json / index.html
    └── src/
        ├── shell/
        │   ├── App.tsx         ← 壳：token 门 → 布局 → 标签栏 → 导航（manifest 驱动）
        │   ├── TokenGate.tsx   ← token 输入（内存态 useToken，不落 storage）
        │   ├── Nav.tsx         ← 导航项完全由 manifest 生成（零硬编码）
        │   └── Tabs.tsx        ← 多标签：每标签一个面板实例
        ├── panels/
        │   ├── registry.ts     ← kind → 懒加载组件；未命中 → JSON 降级组件
        │   ├── counter/        ← counter 面板（值、±1、reset 确认框、轮询）
        │   └── terminal/       ← terminal 面板（xterm + 慢消费者按钮）
        ├── api/
        │   ├── types.ts        ← 契约类型（manifest/counter/帧）
        │   ├── client.ts       ← REST client（baseUrl="" 相对路径）
        │   └── ws.ts           ← 帧协议客户端（Binary=数据 / Text=控制）
        ├── components/ui/      ← shadcn 生成的组件（button/card/dialog/input/badge）
        └── lib/
            └── utils.ts        ← cn()（clsx + tailwind-merge）
```

## 4. 接口契约

### 4.1 manifest

`GET /api/manifest` →

```json
{ "proto": 1, "panels": [
  { "kind": "counter",  "title": "计数器", "verbs": ["read", "write"] },
  { "kind": "terminal", "title": "终端",   "verbs": ["read", "write", "stream"] }
] }
```

- step-0 阶段 panels 只有一项 `{"kind":"probe","title":"探针","verbs":["read"]}`（无渲染器，演示降级）。
- step-3（拔插演示）时删掉 counter 节点——前端面板代码保留，仅 manifest 不再暴露。

### 4.2 counter（POST 控制面，对应 webui 的 VM 生命周期）

- `GET /api/counter` → `{"value": N}`
- `POST /api/counter` body `{"delta": 1}` 或 `{"delta": -1}` → 同步返回
  `{"value": 新值}`（对应 webui 的轻量操作）。
- `POST /api/counter` body `{"reset": true}` → **异步接受**
  `{"ok": true, "async": true, "status": "resetting"}`；server 延迟 2s 后归零
  （对应 webui 的 start/stop：返回 async，前端轮询）。
- 值由执行层 task 独占拥有（`state.rs` spawn 一个 task 持有 `i64` +
  `mpsc<Cmd>`）；HTTP handler 只能经通道发 `Cmd::Add(delta, oneshot)` /
  `Cmd::Reset(delay, oneshot)` 请求-响应。
- 鉴权：`Authorization: Bearer demo-token`，错误 401。

### 4.3 ws 终端（/ws/term，对应 webui 的串口终端）

- 连接：`GET /ws/term?token=demo-token`（浏览器 ws 无法带 header——这正是真实
  系统用 ticket 的原因；demo 用固定 token 查询参数简化）。
- **独占**：同一时刻只允许一个连接。已有连接时，第二个 upgrade 请求返回
  HTTP 409（在 upgrade 之前拒绝）。连接断开即释放订阅位。
- **帧协议 v1**：
  - 下行 Text（控制）：
    `{"type":"hello","proto":1}` 连接建立后先发；
    `{"type":"ping"}` 每 20s（有数据流量时重置计时）；
    `{"type":"dropped","count":N}` 恢复发送后先发（N 为暂停期间累计丢帧数）；
  - 下行 Binary（数据）：UTF-8 文本行；
  - 上行 Binary：用户输入（server 原样回发下行 Binary——模拟终端回显；
    注：真实系统里回显是 guest 做的，此处由 server 代演）；
  - 上行 Text（演示用控制）：`{"type":"pause"}` / `{"type":"resume"}`。
- **数据流（背压核心）**：hello task 每 500ms 产出一行 `hello world #n`，
  `try_send` 进**容量 32 的有界通道**（tokio mpsc，receiver 由当前 ws 会话持有）：
  - 满则丢弃 + `dropped_count += 1`（**生产者永不被反压、永不 await 重试**）；
  - 会话处于 pause 状态时不向 ws 发送（数据继续在通道里堆积、溢出丢弃）；
  - resume 后先发 dropped 帧再恢复数据流；
  - ws 断开 → receiver drop → send 失败仅计数，**hello task 继续跑**；
  - 新连接到来 → 建新通道从头订阅（demo 不做历史回放）。

## 5. 十二条不变量（架构思想的落地，验收时逐条对照）

| # | 不变量 | 为什么 |
| --- | --- | --- |
| 1 | 所有路由（/api、/ws、静态）merge 进**一个 Router 实例**，一个 serve | 单一根：curl 与浏览器同一入口 |
| 2 | counter 的值只被执行层 task 拥有；HTTP 层经通道通信，零直捣 | 分层：控制面是薄壳，领域在执行层 |
| 3 | hello 生产者 `try_send`，失败计数，永不阻塞/重试 | 不反压执行侧是铁律 |
| 4 | 终端单连接独占；第二连接 409 | 独占订阅位（两人同时操作 = 输入竞态） |
| 5 | shell/ 不 import 任何 panels/*；导航项 100% 来自 manifest | 壳零业务逻辑，UI 可插拔的前提 |
| 6 | 新面板 = panels/ 加目录 + registry 加一行 + manifest 加节点，共三处，其余零改动 | 可插拔的可验证定义 |
| 7 | manifest 里未知 kind → 渲染 JSON 降级视图，不报错 | 前向兼容（前端与后端不同代） |
| 8 | 一切浏览器功能都能用 curl 复现（验收命令全是 curl） | curl 是一等客户端 |
| 9 | ws：Binary=数据、Text=控制；连接先收 hello 帧 | 帧协议 v1 |
| 10 | 前端 baseUrl 用相对路径（同源）；dev 形态靠 vite 代理 | dist 同一份，多运行形态通用 |
| 11 | client 错误路径**只读一次 body**，错误对象同时携带 HTTP 状态码与后端 error 字段 | 错误上下文保真（真实 webui 分支此处有 body 二次消费 bug，demo 是正确写法的参考实现） |
| 12 | ApiClient 用 `useRef` + 仅在 null 时创建，token 经 getter 传入保持新鲜 | client 构造稳定（真实 webui 分支此处有每渲染重建 bug，demo 是正确写法的参考实现） |

**交互语义不变量**（对应真实 webui 的 VM 操作语义，集中在 counter 面板）：

- 每个操作维护 pending 状态，pending 期间全部操作按钮 disabled（防重）；
- reset（破坏性操作）必须过 shadcn Dialog 确认；
- reset 返回 async 后**轮询** GET 直到 value==0 或超时 10s（超时显式报错）；
- 卸载/刷新/取消经 AbortController 的 signal 贯穿请求链；
- 错误消息展示「HTTP 状态 + 后端 error」两个维度。

## 6. 分支计划（每步一个分支，从上一步长出）

```bash
git checkout -b step-0-shell main
# 完成 step-0 后：
git checkout -b step-1-counter step-0-shell   # 依此类推
```

### step-0-shell（骨架：router 单根 + manifest + 壳 + 降级）

- server：axum 起 8080；一个 Router 挂 `/api/manifest`（返回 probe 节点）+
  `/` 静态服务（`tower-http ServeDir` 指向 `web/dist`，目录不存在时 `/` 返回
  503、`/api/*` 照常）；固定 token 鉴权中间件（401）。
- web：Tailwind + shadcn 初始化（button/card/input/badge 组件，Dialog 留给
  step-1）；壳（TokenGate → 导航 + 标签栏）；registry + JSON 降级组件；导航
  点击 probe → 显示该节点的 JSON。
- 验收：
  - `curl -s -H 'Authorization: Bearer demo-token' localhost:8080/api/manifest`
    返回 probe 节点 JSON；无 token 401。
  - `curl -s -o /dev/null -w '%{http_code}' localhost:8080/` → 200（build 后）
    或 503（无 dist 时）。
  - 浏览器：输入 token → 导航显示「探针」→ 点开 JSON 降级视图。
  - `cd web && npm run dev` → 5173 同样可用（代理形态）。

### step-1-counter（插入功能 A：POST 控制面 + VM 交互语义）

- server：state.rs 的 counter task（值 + 命令通道 + reset 延迟）；counter.rs
  三个操作（GET / delta / reset-async）；manifest 增 counter 节点。
- web：panels/counter/（当前值 + 「+1」「-1」「reset」按钮；reset 过确认
  Dialog；reset 后轮询到归零，期间按钮 pending 态；超时 10s 报错）；
  registry 注册 `counter`。
- 验收：
  - `curl -s -X POST -H 'Authorization: Bearer demo-token' -d '{"delta":1}' localhost:8080/api/counter` 两次 → `{"value":2}`。
  - `curl -s -X POST -H 'Authorization: Bearer demo-token' -d '{"reset":true}' localhost:8080/api/counter` → `{"ok":true,"async":true,...}`；2s 后 GET 为 0。
  - 浏览器点按钮值变化；reset 有确认框、轮询期间防重；导航自动多出「计数器」
    （未改 shell 任何代码）。

### step-2-terminal（插入功能 B：ws 终端）

- server：state.rs 的 hello task（500ms 一行、有界通道 32、try_send+计数）；
  terminal.rs（upgrade、hello/ping 帧、echo、pause/resume、独占 409）。
- web：panels/terminal/（xterm 显示数据流 + 输入回显 + 「模拟慢消费者」
  按钮组 + 丢帧计数显示）；registry 注册 `terminal`；壳支持「+」新标签
  （每标签独立面板实例）。
- 验收：
  - 浏览器终端持续滚动 `hello world #n`；输入 `abc` 回显。
  - 点「暂停」等 5s 点「恢复」→ 收到 dropped 帧，面板显示丢帧数 > 0，
    且恢复后序号跳变（证明期间生产未停、只丢了帧）。
  - 第二个标签开终端 → 面板显示「该终端已被占用（独占）」。
  - `websocat` 或等价工具验证第二连接 409。

### step-3-unplug（拔出演示）

- 仅一处改动：manifest.rs 删 counter 节点（面板代码保留在仓库）。
- 验收：导航只剩「终端」；counter 面板文件还在但不被加载；
  `curl` 证实 `POST /api/counter` 仍可用（拔的是 UI 挂载，不是后端能力）；
  终端不受影响。

## 6.5 演示方式：git diff 证明改动面，checkout 验证效果

本 demo 的最终用途是**向他人现场演示可插拔**。演示脚本（按此顺序讲最有说服力，
每步先 diff 论证、再 checkout 验证）：

```bash
# ① 插入 counter 的成本
git diff step-0-shell step-1-counter --stat
#    期望：panels/counter/ 新增 + registry.ts 一行 + manifest.rs 一节点
#         + server/src/counter.rs——其余零改动
git diff step-0-shell step-1-counter -- server/src/manifest.rs
#    → 就一个节点："后端挂上 = 界面出现"

# ② 切过去看行为
git checkout step-1-counter   # cargo run 重启 server；vite dev 挂着自动热更前端
#    → 刷新浏览器：导航多出「计数器」；curl POST /api/counter 同权可用

# ③ 插入 terminal 同理：diff → checkout → 演示慢消费者按钮 + 独占 409

# ④ 拔出：diff 极端小是亮点
git diff step-2-terminal step-3-unplug --stat
#    期望：仅 manifest.rs 一行 JSON——拔 = 删一个节点
git checkout step-3-unplug
#    → 刷新：counter 从导航消失；curl POST /api/counter 仍 200
#    → 收尾语："拔的是 UI 挂载，不是后端能力——代码还在，能力还在"
```

核心主张一句话：**插一个功能 = 三个触点加一行注册；拔一个功能 = 删一行
JSON，且后端能力无损。** 质疑哪一步就现场 curl 哪一步。

实操要点（执行者按此保证演示可复现）：

- 前端全程 `npm run dev` 挂着不关——git checkout 换前端文件，vite 自动热更，
  无需每分支重新 build；仅 server 侧变化需重跑 `cargo run`。
- **分支 diff 干净是硬要求**（也是 §9 纪律 2 的原因）：每个分支相对父分支
  的 diff 必须严格落在该步声明的触点内，不得混入无关改动（格式化、依赖
  升级、无关文件）——否则 `--stat` 演示失去说服力。
- 演示时 diff 与 checkout 是两个互补视角：diff = 改动面（可插拔性的度量，
  开发者视角），checkout + 运行 = 效果（使用者视角）；若插拔只能靠切分支
  重编译才能展示，那它是版本演进而非插拔——真正的插拔证据是 manifest
  数据驱动导航（不变量 5）。

## 7. 设计说明（执行者必读）

- **demo 即参考实现**：真实 webui 分支经 review 发现两个前端 bug——
  client 错误路径 body 二次消费、ApiClient 每渲染重建。demo 的不变量 11/12
  就是这两个 bug 的正确写法；demo 前端将来移植回真实 webui 时，这两个文件
  可直接作为模板。
- **慢消费者演示的意义**：暂停期间通道满 → 丢弃计数 → 恢复时报告。这演示
  「网络侧慢不影响执行侧生产」——真实系统里生产者是 guest 串口输出路径，
  反压它会拖死 guest。
- **独占的意义**：真实系统里一个 VM 终端被两个键盘同时输入 = 竞态。demo 用
  mpsc receiver 的单一所有权天然实现，语义即代码。
- **reset 异步 + 轮询的意义**：对应真实 webui 的 VM 生命周期（POST 返回
  async，前端轮询到终态或超时）。demo 用「2s 后归零」把这个语义做成看得见
  的最小形态。
- **Linux 与 axvisor 的实现差异（有意为之）**：demo 静态资产直读 fs——Linux
  有 page cache，按需缺页是内核送的；axvisor 无此基础设施且禁止 handler
  同步读，故真实实现改为「启动时一次性载入内存」。demo 保留直读正是展示
  这条平台差异。
- **echo 的归属**：demo 里 server 回显输入；真实系统回显由 guest 终端驱动
  做，传输层不回显。demo 代演是为了让面板行为完整。

## 8. 明确不做（防过度设计）

多用户/RBAC、token 过期与轮换、真 ticket 机制、历史回放/重连续传、SSL、
生产构建优化、错误国际化、Testing Library 组件测试矩阵（属真实 webui 的
验收范围，demo 不做）、多 counter 资源/多终端通道。
依赖清单之外**不新增任何依赖**（shadcn 生成组件按官方 CLI 方式引入，不手写
新组件库）。

## 9. 给执行 AI 的工作纪律

1. 严格按 §3 目录结构、§4 契约、§5 不变量实现；SPEC 未覆盖的决定选最小
   实现，并在 commit message 注明。
2. 每完成一个 step：先跑通该步全部验收命令，再 commit（一个 step 一个分支，
   commit 前确认在上一步基础上 diff 最小）。
3. 禁止引入 SPEC 未列的依赖；禁止用脚手架模板代码填充未要求的功能。
4. 若某验收项无法通过且原因在 SPEC 冲突——停下来，在 commit message 或
   README 记录冲突点，不要擅自改契约。
5. README.md 每分支更新：如何运行（server / dev / 集成三种形态）+ 该分支
   演示什么思想。
6. 分支 diff 必须干净（§6.5）：只落该步声明的触点，无关改动（格式化、
   依赖升级）禁止混入——diff 是最终演示的证据。
