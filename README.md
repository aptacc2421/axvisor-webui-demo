# webui_demo

axvisor webui 目标架构在 Linux 上的**可运行等价物**：前端与真实 webui 同栈同构
（React 18 + TS + Vite 5 + shadcn/ui + Tailwind，目录三分 `shell/ panels/ api/`），
后端接口全部替代（counter 任务 + hello 流模拟 VM 生命周期与串口终端）。

完整规格见 [SPEC.md](./SPEC.md)。本 README 每分支更新：如何运行 + 该分支演示什么思想。

---

## 当前分支：step-0-shell（骨架）

演示的思想：**单根 Router + manifest 驱动导航 + 未知 kind 降级**。

- 一个 Router、一次 serve：`/api`、`/ws`、静态资产全在同一棵路由树上，
  curl 和浏览器走同一个入口。
- 导航项 100% 来自 `GET /api/manifest`，壳里没有一处 kind 硬编码。
- step-0 的 manifest 只暴露 `probe`——**故意没有渲染器**，用来演示降级：
  未知 kind 渲染成 JSON 视图，不白屏、不报错。

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

浏览器：输入 `demo-token` → 左侧导航出现「探针」→ 点开是 JSON 降级视图
（标题旁有「未注册 kind：probe」徽章）。

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
