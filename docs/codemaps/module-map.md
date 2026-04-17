# Module Map

_Last refreshed: 2026-04-18_

## Module Inventory

| Module | Path | Responsibility |
| --- | --- | --- |
| prismtrace-core | `crates/prismtrace-core` | 共享领域模型：runtime kind、process sample、process target、probe health |
| prismtrace-storage | `crates/prismtrace-storage` | 本地状态目录布局与初始化 |
| prismtrace-host | `crates/prismtrace-host` | host 启动入口、process discovery service、本地 discovery 报告 |

## Responsibility Slices

- `prismtrace-core` 应保持纯领域逻辑和可测试的标准化/分类逻辑，不应承载 host I/O 或文件系统副作用
- `prismtrace-storage` 只负责本地状态布局与目录创建，不应吸收业务流程
- `prismtrace-host` 是当前唯一运行时编排层，负责把 core 和 storage 接成可执行入口

## Dependency Direction

- `prismtrace-host` 依赖 `prismtrace-core`
- `prismtrace-host` 依赖 `prismtrace-storage`
- `prismtrace-core` 和 `prismtrace-storage` 当前互不依赖

当前方向是由 host 作为顶层协调者，core 保持无副作用领域层，storage 保持单一职责的本地状态层。

## Change Routing

- 新增或调整 runtime 分类、process target 字段：改 `prismtrace-core`
- 新增本地状态文件/目录、db 路径布局：改 `prismtrace-storage`
- 新增 host 参数、CLI 入口、discovery / attach / API surface：改 `prismtrace-host`
- 涉及产品边界、需求、实现任务拆分：改 `openspec/changes/<change>/`
