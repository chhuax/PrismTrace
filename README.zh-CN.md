# PrismTrace / 棱镜观测

英文名：`PrismTrace`

[English README](./README.md)

`棱镜观测` 是一个 AI 应用可观测性工具，目标是在不重启应用、不打断当前会话的前提下，拿到 AI 应用真实发给大模型的内容，并逐步扩展到会话重建、分析和解释能力。

第一版重点关注 macOS 上基于 Node / Electron 的 AI CLI 和桌面应用。第一阶段先解决最核心的问题：

- 附着到正在运行的目标进程
- 采集附着后的真实 LLM 请求与响应
- 在本地可观测性控制台里查看 payload、tools 和元数据

## 当前状态

仓库目前处于设计和初始化阶段。

当前设计文档在 [docs/总体设计与V1方案.md](./docs/总体设计与V1方案.md)。
当前产品路线图在 [docs/产品迭代路线图.md](./docs/产品迭代路线图.md)。

## V1 范围

PrismTrace V1 当前边界是：

- 仅支持 macOS
- 优先支持已经在运行中的 Node / Electron AI 应用
- 不要求重启被观测应用
- 先解决 payload 可见性
- 采用本地优先的存储与隐私策略

## 长期方向

PrismTrace 不只是一个 payload 抓取工具，它的长期方向是完整的 AI 应用可观测性：

- 信息采集
- 会话重建
- 分析与解释

第一步先把可信的本地事实层搭起来，后续再在此基础上叠加分析能力。

## 当前仓库结构

- `crates/prismtrace-core`：共享的运行时与进程领域类型
- `crates/prismtrace-storage`：本地状态目录布局和存储初始化
- `crates/prismtrace-host`：可运行的 host 二进制和启动引导
- `docs/`：设计文档和实现计划

## 本地开发

```bash
cargo test
cargo run -p prismtrace-host
cargo run -p prismtrace-host -- --discover
cargo run -p prismtrace-host -- --readiness
cargo run -p prismtrace-host -- --attach <pid>
```

`--attach <pid>` 当前会进入前台 attach 会话，并对受支持的运行中 Node CLI 目标捕获 request 与 response artifacts。

## 本地控制台

使用下面的命令启动本地可观测性控制台：

```bash
cargo run -p prismtrace-host -- --console
```

如果只想看特定目标，可以重复传入 `--target`：

```bash
cargo run -p prismtrace-host -- --console --target opencode
cargo run -p prismtrace-host -- --console --target opencode --target codex
```

当传入 `--target` 后，首页和 `/api/*` payload 会保持一致的过滤视图；如果当前没有命中任何目标，控制台仍会正常打开，并显示带过滤上下文的空态说明，而不会回退到全局进程列表。

当前默认入口地址是 `http://127.0.0.1:7799`。

当前 bootstrap 阶段的控制台提供：

- target 摘要列表
- 最近活动时间线
- request 摘要列表
- 基础 request 详情与 observability health 面板
