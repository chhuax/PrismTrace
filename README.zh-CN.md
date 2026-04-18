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

当前设计文档在 [docs/2026-04-17-ai-observability-v1-design.md](./docs/2026-04-17-ai-observability-v1-design.md)。
当前骨架实现计划在 [docs/2026-04-18-workspace-skeleton-implementation-plan.md](./docs/2026-04-18-workspace-skeleton-implementation-plan.md)。

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
