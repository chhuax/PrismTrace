# Proposal: use-global-local-store

## Why

PrismTrace 的产品语义是“本机 AI observability”：observer 采集本机可观测 AI 会话，console 默认展示本机已有会话。

当前 host bootstrap 把当前启动目录当成 workspace root，并把状态写到 `cwd/.prismtrace/state`。这导致：

- console 和 observer 只要在不同目录启动，就读写不同状态库
- 用户会看到“暂无可用会话”，即使本机已经存在可观测 AI 会话
- project/cwd 从会话元数据错误升级成了数据边界

这和产品目标冲突，需要改成机器本地的全局 store。

## What Changes

- 默认 state root 改为用户级本机目录：`~/Library/Application Support/PrismTrace`
- CLI 支持 `--state-root <path>` 作为调试/测试/高级部署覆盖
- observer 与 console 默认读写同一个全局 store，不再依赖启动目录一致
- Codex thread/read-model 默认展示本机所有未归档交互会话；cwd/project 只作为 metadata 与后续筛选维度
- 启动时保守导入当前目录下旧版 `.prismtrace/state/artifacts`，避免 alpha 用户的已有 observer artifacts 立即不可见
- console 空态/启动摘要暴露当前读取的 state root，方便排查

## Impact

受影响 spec：

- `local-console`
- `observability-read-model`

受影响模块：

- `prismtrace-host` lifecycle / CLI bootstrap
- `prismtrace-host` console API
- `prismtrace-host` observability read model
- `prismtrace-storage` state layout compatibility helpers

## Out of Scope

- 不实现跨所有磁盘目录扫描历史 `.prismtrace`
- 不实现完整 project UI 筛选器
- 不改变 artifacts 内部 schema
