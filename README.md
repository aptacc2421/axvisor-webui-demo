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
