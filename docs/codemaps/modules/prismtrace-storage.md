# Module Identity

`prismtrace-storage`

## Why It Exists

集中定义 PrismTrace 在本地工作区下的状态目录结构，避免 host 直接散落拼接 `.prismtrace/state`、`artifacts`、`tmp`、`logs` 等路径。

## Main Entrypoints

- `StorageLayout::new()`
- `StorageLayout::initialize()`

## Core Flows

- 根据 workspace root 推导 `.prismtrace/state`
- 推导 `observability.db` 和 artifact/tmp/logs 路径
- 初始化本地目录树

## Internal And External Dependencies

- 当前只依赖 Rust 标准库
- 被 `prismtrace-host` 在 bootstrap 流程中调用

## Edit Hazards And Debugging Notes

- 这里应继续保持“布局与初始化”职责，不要把更高层的事件模型或 DB 访问塞进来
- 如果未来切到 SQLite 实现，优先新增模块，而不是让这个文件膨胀成完整 storage runtime
