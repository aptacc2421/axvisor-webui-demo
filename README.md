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

## 当前分支：step-1-counter（插入功能 A）

这一步演示 **插一个功能的成本**：

```bash
git diff step-0-shell step-1-counter --stat
```

改动面就这些（其余零改动，shell 一行没碰）：

| 触点 | 文件 | 改了什么 |
| --- | --- | --- |
| 执行层 | `server/src/state.rs` | counter task：值 + 命令通道 + reset 延迟提交 |
| HTTP 薄壳 | `server/src/counter.rs`（新增） | GET / delta / reset-async 三个操作 |
| 挂载 | `server/src/main.rs` | `mod counter;` + 一条 route |
| 资源层 | `server/src/manifest.rs` | 加一个 counter 节点 |
| 渲染器 | `web/src/panels/counter/`（新增） | 面板本体 |
| 注册表 | `web/src/panels/registry.ts` | 一行 `counter: CounterPanel` |

演示的思想：

- **控制面是薄壳**：值只被 counter task 独占拥有（不变量 2），handler 只能经
  通道发命令，零直捣。
- **reset 的异步语义**：POST 只回「已接受」，2s 后才归零；前端轮询到终态、
  10s 超时显式报错。这就是真实 webui 里 start/stop VM 的交互形状。
- **后端挂上 = 界面出现**：manifest 多一个节点，导航就多一项「计数器」，
  壳没改一行。

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

## 验收

```bash
# manifest：有 token 拿到节点，无 token 401
curl -s -H 'Authorization: Bearer demo-token' localhost:8080/api/manifest
# → {"proto":1,"panels":[{"kind":"probe","title":"探针","verbs":["read"]}]}
curl -s -o /dev/null -w '%{http_code}\n' localhost:8080/api/manifest
# → 401

# 静态：构建后 200，无 dist 时 503
curl -s -o /dev/null -w '%{http_code}\n' localhost:8080/
# → 200（或 503）

# 未知的 /api 路径返回 JSON 404，不落进 SPA 兜底
curl -s -H 'Authorization: Bearer demo-token' localhost:8080/api/nope
# → {"error":"not found"}
```

浏览器：输入 `demo-token` → 左侧导航出现「探针」与「计数器」→
「探针」是 JSON 降级视图（标题旁有「未注册 kind：probe」徽章），
「计数器」是功能完整的面板。

### step-1 追加验收

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
  state.rs      执行层句柄（step-0 只有静态资产目录）
  manifest.rs   GET /api/manifest
web/src/
  shell/        App（token 门 → 布局 → 标签栏 → 导航）、TokenGate、Nav、Tabs
  panels/       registry.ts（kind → 组件）+ FallbackPanel（JSON 降级）
  api/          types.ts（契约）、client.ts（REST，相对路径）
  components/   shadcn 生成：button / card / input / badge
  lib/utils.ts  cn()
```

接线点只有一处：`web/src/main.tsx` 把 `panels/registry.ts` 注入 `shell/App`。
新增面板改 `panels/` + registry 一行 + manifest 一个节点，壳零改动。
