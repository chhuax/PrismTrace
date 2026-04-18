# Project Overview

_Last refreshed: 2026-04-18_

## Project Summary

PrismTrace（棱镜观测）是一个本地优先的 AI 应用可观测性工具。当前仓库的第一阶段实现重点是 Rust host 骨架、macOS 进程发现能力、attach readiness 判断和第一版 attach controller，为后续 payload capture、session timeline 和分析能力打底。

## First-Read Paths

- `README.md`
- `README.zh-CN.md`
- `docs/2026-04-17-ai-observability-v1-design.md`
- `openspec/specs/process-discovery/spec.md`
- `openspec/specs/attach-readiness/spec.md`
- `openspec/specs/attach-controller/spec.md`
- `crates/prismtrace-host/src/main.rs`
- `crates/prismtrace-host/src/attach.rs`
- `crates/prismtrace-host/src/discovery.rs`
- `crates/prismtrace-host/src/readiness.rs`

## System Architecture

当前工程是一个 Rust workspace，按职责分成三个 crate：

- `prismtrace-core`：共享领域模型，定义 runtime kind、process sample、process target、attach readiness、attach session 和 probe health
- `prismtrace-storage`：本地状态目录布局与初始化，负责 `.prismtrace/state` 下的 db/artifacts/tmp/logs 结构
- `prismtrace-host`：可运行的 host 入口，负责 bootstrap、本地 discovery service、readiness service、attach controller 和本地报告输出

设计文档和需求变更通过 `docs/` 与 `openspec/` 双轨维护：

- `docs/` 保存高层产品设计与历史实现计划
- `openspec/changes/` 保存变更级 proposal/design/spec/tasks，并驱动后续实现

## Main Flows

### 1. Host bootstrap

`crates/prismtrace-host/src/main.rs` 调用 `bootstrap()`，根据当前工作目录创建 `.prismtrace/state` 的本地状态结构，然后输出启动摘要。

### 2. Process discovery

当执行 `cargo run -p prismtrace-host -- --discover` 时，host 使用 `PsProcessSampleSource` 调用 `ps -axo pid=,comm=`，解析出 `ProcessSample`，再在 `prismtrace-core` 中进行 runtime 分类和目标标准化，最后生成文本 discovery report。

### 3. Attach readiness

当执行 `cargo run -p prismtrace-host -- --readiness` 时，host 先运行 discovery，再把 `ProcessTarget` 转化为结构化 attach readiness 结果，输出当前是否值得进入 attach 流程以及原因说明。

### 4. Attach control path

当执行 `cargo run -p prismtrace-host -- --attach <pid>` 时，host 会先运行 discovery 和 readiness，找到目标 pid 对应的 readiness 结果，再通过一个受控 attach backend 发起最小 attach 流程。只有在 backend 和最小握手都完成后，报告才会进入 `attached`。

### 5. OpenSpec-driven development

功能开发默认先在 `openspec/changes/<change>/` 下完成 proposal/design/spec/tasks，再通过 apply 执行。当前已经完成 `add-process-discovery`、`add-attach-readiness` 与 `add-attach-controller`，对应能力已合并到 `openspec/specs/`，归档记录位于 `openspec/changes/archive/`。

## External Surfaces And Dependencies

- 当前主外部 surface 是本地 CLI：`prismtrace-host`
- 当前依赖仅限 Rust 标准库和 workspace 内部 path dependencies
- 当前真实 macOS 进程发现依赖系统 `ps` 命令

## Problem Routing

- 如果问题涉及 runtime 类型、进程目标标准化、readiness / attach session 领域模型：先看 `prismtrace-core/src/lib.rs`
- 如果问题涉及本地状态目录、db/artifacts 位置：先看 `prismtrace-storage/src/lib.rs`
- 如果问题涉及 host 启动流程、本地 discovery / readiness / attach 行为、CLI 参数：先看 `prismtrace-host/src/lib.rs`、`src/attach.rs`、`src/discovery.rs`、`src/readiness.rs` 与 `src/main.rs`
- 如果问题涉及某个功能为什么存在、边界是什么、下一步该怎么做：先看对应 `openspec/changes/<change>/`

## Constraints And Repo Rules

- 主要开发流程使用 OpenSpec；中大型功能先走 `propose/explore`，实现走 `apply`
- OpenSpec 结构性关键词如 `Why`、`Decision`、`Requirement`、`Scenario` 保持英文，其余正文优先中文
- 当前仓库仍在早期阶段，不要假设已经有 HTTP API、Web UI 或 attach/instrumentation runtime
