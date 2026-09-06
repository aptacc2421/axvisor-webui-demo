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
            └─ step-4-compose 组合：控制 + console 同屏（零后端触点）
               └─ step-5-evidence 落盘：状态可 cat / tail -f 独立验证
```

## 当前分支：step-5-evidence（演示证据落盘）

**UI 会骗人，文件不会。** 执行层把状态写进 `server/data/`（已 gitignore），
每一条演示主张都有了浏览器之外的证据：

```bash
tail -f server/data/console.log
#   浏览器终端里敲 abc → 立刻看到 [..] IN "abc"——输入确实到了 server
#   [..] OUT hello world #57        ← 数据面确实在生产
#   [..] CTL pause                  ← 点了暂停
#   （5 秒空洞：OUT 一行都没有——会话停止消费，但生产没停）
#   [..] CTL resume
#   [..] OUT hello world #58 ... #65（缓冲一次性吐出）
#   [..] OUT hello world #68        ← 序号跳变：#66/#67 被丢帧
cat server/data/counter.json
#   点 +1 后变 {"value":1,...}；reset 异步期间仍是旧值，2s 后归零也记录在案
```

改动只在执行层（state.rs 的 Evidence + counter task 落盘、terminal.rs 会话
记 IN/OUT/CTL），HTTP 层、前端、契约零变化——证据是执行层的副产品，不是新接口。

## 当前分支：step-4-compose（组合面板）

**插拔的第四种姿势：不加后端，只拼已有面板。**

`kind:"vm"` 是一个组合面板——左边 counter（控制面），右边 terminal（数据面）
同屏，对应真实 webui 的 VM 详情页（生命周期操作 + guest console）。

```bash
git diff step-3-unplug step-4-compose --stat
# → panels/vm/ 新增 + registry 一行 + manifest 一个节点，后端零改动
```

演示的思想：

- **壳对组合一无所知**：VmPanel 直接 import 已有的 CounterPanel/TerminalPanel，
  不经过注册表——面板可以组合面板，壳依然只认 manifest 的 kind。
- **独占是全局的**：组合页里的 console 占住唯一的订阅位后，再用「+」开独立
  终端标签会撞上 409——一台 VM 的 console 只有一个，两个键盘同时敲 = 竞态。
- **step-3 的回响**：上一步拔掉的 counter 节点没有让能力消失，这一步它以
  组合的形式回到界面——拔掉的是挂载，留下的是积木。
- 懒加载照常生效：VmPanel 自己只有 0.64 kB 的 chunk，counter/terminal 的
  chunk 用到才下载。

### step-4 验收

```bash
A='Authorization: Bearer demo-token'
curl -s -H "$A" localhost:8080/api/manifest
# → {"proto":1,"panels":[{"kind":"terminal",...},{"kind":"vm",...}]}
curl -s -X POST -H "$A" -d '{"delta":3}' localhost:8080/api/counter   # → {"value":3}
```

浏览器：导航出现「虚拟机」→ 点开是同屏的计数器 + 终端；此时「+」再开一个
「终端」标签 → 显示「该终端已被占用（独占）」。

## 历史分支

前几步各自演示的思想（完整改动面见各分支的 README 与 commit message）：

- **step-1-counter**：插功能 A。counter task（值 + 命令通道）+ HTTP 薄壳 +
  面板 + registry 一行 + manifest 一节点。控制面是薄壳，reset 的
  「async 接受 → 轮询到终态 → 10s 超时」就是 VM 生命周期的交互语义。
- **step-2-terminal**：插功能 B。hello task + ws 终端。两条铁律落地：
  不反压执行侧（try_send 满则丢弃 + 计数，恢复时先发 dropped 帧、序号跳变），
  独占订阅位（第二连接 upgrade 前被 409）。
  > SPEC 冲突记录（§9.4）：§4.3 的「通道容量 32」与 §6 的「暂停 5s 后丢帧 > 0」
  > 不相容（32 × 500ms = 16s 才填满）。经确认取 §6 的可观察行为，容量定为 8；
  > 改回只需动 `state.rs` 的 `HELLO_CHANNEL_CAPACITY` 一个常量。
- **step-3-unplug**：拔出演示，唯一改动是 manifest.rs 少一个 counter 节点。
- **step-4-compose**：组合面板 vm——控制 + console 同屏，零后端触点。壳对组合
  一无所知（VmPanel 直接 import 已有面板组件）；独占是全局的（组合页的 console
  占住订阅位后，独立终端标签撞 409）。

### step-3 验收

```bash
A='Authorization: Bearer demo-token'
curl -s -H "$A" localhost:8080/api/manifest
# → {"proto":1,"panels":[{"kind":"terminal","title":"终端","verbs":["read","write","stream"]}]}
curl -s -X POST -H "$A" -d '{"delta":5}' localhost:8080/api/counter   # → {"value":5}
```

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
                + vm/（组合面板：控制 + console 同屏）
  api/          types.ts（契约）、client.ts（REST，相对路径）、ws.ts（帧协议）
  components/   shadcn 生成：button / card / dialog / input / badge
  lib/utils.ts  cn()
```

接线点只有一处：`web/src/main.tsx` 把 `panels/registry.ts` 注入 `shell/App`。
新增面板改 `panels/` + registry 一行 + manifest 一个节点，壳零改动。
