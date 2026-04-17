## Why

PrismTrace 现在已经有了可运行的 Rust workspace 骨架，但还缺少从“当前机器上的真实进程”进入产品流程的第一步。下一步最有价值的能力，是让 host 能发现正在运行的 Node / Electron AI 进程，为后续 attach 和观测能力提供候选目标列表。

## What Changes

- 为 PrismTrace host 增加第一版进程发现能力
- 定义 PrismTrace 在 macOS 上识别 Node / Electron 候选 AI 进程的方式
- 返回结构化的进程目标数据，供后续 CLI、HTTP API 或本地 Web UI 使用
- 本次 change 只覆盖 discovery，不包含 live attach、probe injection 或 payload capture

## Capabilities

### New Capabilities
- `process-discovery`：发现当前 macOS 上正在运行的候选进程，并将可能的 Node / Electron AI 目标分类返回给 PrismTrace

### Modified Capabilities

## Impact

- 影响代码：`crates/prismtrace-core`、`crates/prismtrace-host`，以及新增的 discovery 支撑模块
- 影响系统：本地 macOS 进程检查流程、host 启动后的目标发现流程
- 依赖影响：第一版尽量不引入新依赖；如果后续实现需要，可再评估 macOS 进程检查库

## Docs Impact

- 在 `openspec/changes/add-process-discovery/specs/` 下新增 `process-discovery` capability spec
- 仅当本地开发入口或 V1 对外范围发生变化时，再更新仓库 README
