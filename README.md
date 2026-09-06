# webui_demo

axvisor webui 目标架构在 Linux 上的**可运行等价物**：前端与真实 webui 同栈同构
（React 18 + TS + Vite 5 + shadcn/ui + Tailwind，目录三分 `shell/ panels/ api/`），
后端接口全部替代（counter 任务 + hello 流模拟 VM 生命周期与串口终端）。

完整规格见 [SPEC.md](./SPEC.md)。本 README 每分支更新：如何运行 + 该分支演示什么思想。

---

## 分支路线

```text
main
└─ step-0-shell      骨架：单根 Router + manifest + 壳 + 未知 kind 降级
   └─ step-1-counter 插入功能 A：POST 控制面 + VM 交互语义
      └─ step-2-terminal  插入功能 B：ws 终端（背压 + 独占）
         └─ step-3-unplug 拔出：manifest 删一个节点，后端能力无损
```

## 当前分支：step-2-terminal（插入功能 B）

这一步在 step-1 的基础上**再插一个功能**（ws 终端），改动面：

| 触点 | 文件 | 改了什么 |
| --- | --- | --- |
| 执行层 | `server/src/state.rs` | hello task：500ms 一行、try_send 进有界通道、丢弃计数 |
| HTTP 层 | `server/src/terminal.rs`（新增） | ws upgrade、帧协议 v1、独占 409、pause/resume |
| 挂载 | `server/src/main.rs` | `mod terminal;` + 一条 route |
| 资源层 | `server/src/manifest.rs` | 加 terminal 节点（probe 退役：降级路径由未知 kind 继续保证） |
| 传输层 | `web/src/api/ws.ts`（新增） | 帧协议客户端（Binary=数据 / Text=控制） |
| 渲染器 | `web/src/panels/terminal/`（新增） | xterm 终端 + 慢消费者按钮 |
| 注册表 | `web/src/panels/registry.ts` | 一行 `terminal: TerminalPanel` |
| 壳 | `web/src/shell/App.tsx` `Tabs.tsx` | 「+」新开实例（每标签一个独立面板实例，§6 声明的触点） |

演示的思想：

- **不反压执行侧（不变量 3）**：hello 生产者每 500ms 产一行，`try_send` 进有界
  通道；满了就丢弃 + 计数，永不阻塞、永不重试。点「暂停（模拟慢消费者）」，
  会话停止消费，约 4s 后通道溢出；点「恢复」，先收到 `dropped` 帧，
  数据序号跳变——证明暂停期间生产从未停止，只是丢了帧。
- **独占订阅位（不变量 4）**：receiver 的单一所有权就是独占语义（§7「语义即代码」）。
  第二个终端标签的连接在 upgrade 之前被 409 拒绝，面板显示「该终端已被占用」。
  浏览器 WebSocket 拿不到握手状态码，所以前端先用不带 Upgrade 头的
  普通 fetch 探测同一路由（被占 → 409；空闲 → 426）。
- **curl 一等公民**：以上行为全部可以用下面的命令复现。

### step-2 追加验收

```bash
# 探测：订阅位空闲时 426（欢迎升级），被占时 409
curl -s -w '\n%{http_code}\n' 'localhost:8080/ws/term?token=demo-token'
# 无 token 401
curl -s -o /dev/null -w '%{http_code}\n' 'localhost:8080/ws/term?token=wrong'

# 浏览器：终端持续滚动 hello world #n；输入 abc 有回显；
# 暂停 5s+ 恢复 → dropped 帧计数 > 0 且序号跳变；
# 「+」再开一个终端标签 → 显示「该终端已被占用（独占）」。
```

> **SPEC 冲突记录（§9.4）**：§4.3 写「容量 32 的通道」，§6 验收要求「暂停 5s
> 后丢帧 > 0」——32 × 500ms = 16s 才填满，两者不相容。经确认取 §6 的可观察
> 行为，容量定为 8（4s 开始丢帧）；要改回 32 只需动
> `server/src/state.rs` 里的 `HELLO_CHANNEL_CAPACITY` 一个常量。

## 运行（三种形态）

### 形态一：dev（推荐，改前端免重编译）

两个终端：

```bash
cd server && cargo run          # 8080
cd web    && npm run dev        # 5173，/api 与 /ws 代理到 8080
```

浏览器开 <http://localhost:5173>。改 `web/src` 下任意文件，vite 自动热更。

### 形态二：集成（server 直读构建产物）

```bash
cd web && npm run build         # 产出 web/dist
cd ../server && cargo run
```

浏览器开 <http://localhost:8080>——后端直读 `web/dist`，不再需要 vite。

### 形态三：只要后端（curl 一等公民）

不构建前端直接 `cargo run`：此时 `/` 返回 **503**（提示 dist 未构建），
但 `/api/*` 照常可用。后端能力不依赖前端产物。

## 验收（基础，step-0 起）

```bash
# manifest：有 token 拿到节点，无 token 401
curl -s -H 'Authorization: Bearer demo-token' localhost:8080/api/manifest
# → {"proto":1,"panels":[{"kind":"counter",...},{"kind":"terminal",...}]}
curl -s -o /dev/null -w '%{http_code}\n' localhost:8080/api/manifest
# → 401

# 静态：构建后 200，无 dist 时 503
curl -s -o /dev/null -w '%{http_code}\n' localhost:8080/
# → 200（或 503）

# 未知的 /api 路径返回 JSON 404，不落进 SPA 兜底
curl -s -H 'Authorization: Bearer demo-token' localhost:8080/api/nope
# → {"error":"not found"}
```

浏览器：输入 `demo-token` → 左侧导航出现「计数器」「终端」
（完全由 manifest 生成，壳里没有硬编码）。

### step-1 追加验收（counter）

```bash
A='Authorization: Bearer demo-token'

# delta 同步：两次 +1 后值为 2
curl -s -X POST -H "$A" -d '{"delta":1}' localhost:8080/api/counter
curl -s -X POST -H "$A" -d '{"delta":1}' localhost:8080/api/counter
# → {"value":1} → {"value":2}

# reset 异步接受：立刻返回，2s 后才归零
curl -s -X POST -H "$A" -d '{"reset":true}' localhost:8080/api/counter
# → {"ok":true,"async":true,"status":"resetting"}
curl -s -H "$A" localhost:8080/api/counter       # 立刻查：还是 2（没归零）
sleep 2.5
curl -s -H "$A" localhost:8080/api/counter       # → {"value":0}
```

浏览器：点 +1/-1 值立刻变；点 reset 弹确认框；确认后按钮进入 pending 且全部
禁用（防重），面板轮询到归零才解禁。错误时按「HTTP 状态 + 后端 error」两维显示。

## 目录

```text
server/src/
  main.rs       唯一 router() 装配：/api + /ws + / 静态，一个 Router
  state.rs      执行层：counter task + hello task 的创建与句柄
  manifest.rs   GET /api/manifest
  counter.rs    GET/POST /api/counter（经命令通道，不直捣状态）
  terminal.rs   GET /ws/term（ws upgrade、帧协议 v1、独占、echo）
web/src/
  shell/        App（token 门 → 布局 → 标签栏 → 导航）、TokenGate、Nav、Tabs
  panels/       registry.ts（kind → 组件）+ FallbackPanel（JSON 降级）
                + counter/（计数器面板）+ terminal/（xterm 终端面板）
  api/          types.ts（契约）、client.ts（REST，相对路径）、ws.ts（帧协议）
  components/   shadcn 生成：button / card / dialog / input / badge
  lib/utils.ts  cn()
```

接线点只有一处：`web/src/main.tsx` 把 `panels/registry.ts` 注入 `shell/App`。
新增面板改 `panels/` + registry 一行 + manifest 一个节点，壳零改动。
