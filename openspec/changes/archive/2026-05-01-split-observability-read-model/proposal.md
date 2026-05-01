## Why

PrismTrace 的产品主线已经从 attach-first 收敛到 observer-first，并且已经开始同时承载 Codex App Server、opencode server/export、Claude Code transcript 等 source。这个方向是正确的，但当前实现中的读链已经开始超载：

- `prismtrace-host` 同时承担 CLI、observer、artifact 读取、session reconstruction、console API 和页面服务
- `crates/prismtrace-host/src/console/observer.rs` 混合了数据访问、Codex transcript parser、session indexer、payload renderer 和 console projection
- `prismtrace-storage` 目前主要负责目录布局，没有真正承担索引、查询、缓存职责
- console API 每次请求仍以扫描 artifacts / transcript 文件为主，难以支撑后续分页、详情懒加载和分析能力

这会让后续 prompt diff、tool visibility、skill diagnostics 继续堆在 console 层，最终拖慢性能并模糊产品架构边界。

## What Changes

新增 `split-observability-read-model` change，先完成第一刀架构拆分：

- 从 `console/observer.rs` 中拆出 observer artifact / Codex rollout transcript 的读取和解析边界
- 在 storage/read-model 侧引入 session index 与 event index 的最小职责
- 让 console API 消费统一 read model，而不是直接扫描源 artifacts
- 保持现有 Web console UI 和对外路由基本不变，只稳定其背后的数据模型

第一轮按 **B -> A -> C** 推进：

1. 先拆清 `console/observer.rs` 的职责边界
2. 再补最小 index/cache，减少重复扫描
3. 最后固定可分页的 sessions/events/details API 形态

## Capabilities

### New Capabilities
- `observability-read-model`: 为 observer-first console 提供统一 session/event read model、索引和详情查询入口

### Modified Capabilities
- `local-console`: console API 从直接扫描 artifacts 迁移为消费 read model
- `observer-source-abstraction`: source 继续负责采集与归一化事件，但不直接承担 console projection

## Impact

- 影响代码：
  - `crates/prismtrace-host/src/console/observer.rs`
  - `crates/prismtrace-host/src/console/mod.rs`
  - `crates/prismtrace-host/src/console/server.rs`
  - `crates/prismtrace-storage/src/lib.rs`
  - 后续可能新增 focused modules，例如 `read_model` / `index`
- 影响系统行为：
  - console sessions / details 的返回内容保持兼容优先
  - 内部读取路径从“每次 API 扫文件”逐步迁移为“构建 read model 后查询”
- 影响文档：
  - 新增 `observability-read-model` OpenSpec 能力说明
- 边界说明：
  - 本次不引入云端服务
  - 本次不改成 React 或重写前端
  - 本次不强制引入异步 HTTP server
  - 本次不交付 prompt/tool/skill diagnostics，只为这些分析能力清理读链地基
