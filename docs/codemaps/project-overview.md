# Project Overview

_Last refreshed: 2026-04-26_

## Project Summary

PrismTrace（棱镜观测）是一个本地优先的 AI 应用可观测性工具。当前仓库已经从早期 attach-first 试探，推进到 observer-first 路线：以 Rust host、macOS 进程发现、本地 artifacts、官方 observer source 接入和本地控制台为主，为后续 session timeline 和分析能力打底。

## First-Read Paths

- `README.md`
- `README.zh-CN.md`
- `docs/总体设计与V1方案.md`
- `openspec/changes/add-codex-app-server-observer/`
- `openspec/changes/add-observer-source-abstraction/`
- `crates/prismtrace-host/src/main.rs`
- `crates/prismtrace-host/src/discovery.rs`
- `crates/prismtrace-host/src/codex_observer.rs`
- `crates/prismtrace-host/src/console/mod.rs`

## System Architecture

当前工程是一个 Rust workspace，按职责分成三个 crate：

- `prismtrace-core`：共享领域模型，定义 runtime kind、process sample、process target 与 IPC message
- `prismtrace-storage`：本地状态目录布局与初始化，负责 `.prismtrace/state` 下的 db/artifacts/tmp/logs 结构
- `prismtrace-host`：可运行的 host 入口，负责 bootstrap、本地 discovery service、observer source 接入、artifact 聚合和本地控制台

设计文档和需求变更通过 `docs/` 与 `openspec/` 双轨维护：

- `docs/` 保存高层产品设计与历史实现计划
- `openspec/changes/` 保存变更级 proposal/design/spec/tasks，并驱动后续实现

## Main Flows

### 1. Host bootstrap

`crates/prismtrace-host/src/main.rs` 调用 `bootstrap()`，根据当前工作目录创建 `.prismtrace/state` 的本地状态结构，然后输出启动摘要。

### 2. Process discovery

当执行 `cargo run -p prismtrace-host -- --discover` 时，host 使用 `PsProcessSampleSource` 调用 `ps -axo pid=,comm=`，解析出 `ProcessSample`，再在 `prismtrace-core` 中进行 runtime 分类和目标标准化，最后生成文本 discovery report。

### 3. Observer intake

当执行 `cargo run -p prismtrace-host -- --codex-observe` 时，host 会连接 `Codex` 官方 observer 面，归一化事件并将结果落到本地 artifacts。

### 4. Local console

当执行 `cargo run -p prismtrace-host -- --console` 时，host 会聚合 request / session / observer artifacts，输出 observer-first 的本地控制台。

### 5. OpenSpec-driven development

功能开发默认先在 `openspec/changes/<change>/` 下完成 proposal/design/spec/tasks，再通过 apply 执行。当前产品主线围绕 observer source、console 和 session reconstruction 推进；历史 attach 变更已归档，不再代表当前产品方向。

## External Surfaces And Dependencies

- 当前主外部 surface 是本地 CLI：`prismtrace-host`
- 当前依赖仅限 Rust 标准库和 workspace 内部 path dependencies
- 当前真实 macOS 进程发现依赖系统 `ps` 命令

## Problem Routing

- 如果问题涉及 runtime 类型、进程目标标准化、IPC message：先看 `prismtrace-core/src/lib.rs`
- 如果问题涉及本地状态目录、db/artifacts 位置：先看 `prismtrace-storage/src/lib.rs`
- 如果问题涉及 host 启动流程、本地 discovery / observer / console 行为、CLI 参数：先看 `prismtrace-host/src/lib.rs`、`src/discovery.rs`、`src/codex_observer.rs`、`src/console/` 与 `src/main.rs`
- 如果问题涉及某个功能为什么存在、边界是什么、下一步该怎么做：先看对应 `openspec/changes/<change>/`

## Constraints And Repo Rules

- 主要开发流程使用 OpenSpec；中大型功能先走 `propose/explore`，实现走 `apply`
- OpenSpec 结构性关键词如 `Why`、`Decision`、`Requirement`、`Scenario` 保持英文，其余正文优先中文
- 当前仓库仍在早期阶段，不要假设已经有完整的多 source API 编排或最终分析工作台
