## Why

PrismTrace 已经可以发现候选进程，但当前结果仍然更像“系统进程清单”而不是“下一步可操作的 attach 目标列表”。在真正进入 attach 和 probe 注入之前，产品需要先回答一个更贴近用户的问题：哪些目标现在值得 attach，哪些目标不值得，以及原因是什么。

## What Changes

- 为 PrismTrace 增加第一版 attach readiness 能力
- 在候选进程列表之上，输出可附着性判断、失败分类和可读的 readiness 结果
- 提供一个可本地运行验证的 readiness 入口，用于查看目标当前状态
- 本次 change 只覆盖 readiness 判断，不包含真正的 attach、probe 注入和 payload 采集

## Capabilities

### New Capabilities
- `attach-readiness`: 基于候选进程输出结构化的 attach readiness 结果，告诉用户某个目标当前是否适合进入 attach 流程

### Modified Capabilities
- `process-discovery`: discovery 结果将被 attach readiness 消费，但其原有 requirement 不改变

## Impact

- 影响代码：`crates/prismtrace-core`、`crates/prismtrace-host`
- 影响系统：host 本地目标选择流程，从“发现候选进程”提升到“发现可行动目标”
- 依赖影响：第一版尽量不引入新的注入或 attach 依赖，优先用现有进程信息和可扩展的 readiness 规则建模

## Docs Impact

- 在 `openspec/changes/add-attach-readiness/specs/` 下新增 `attach-readiness` capability spec
- 若本地演示入口变化，则更新 README 与 codemap
