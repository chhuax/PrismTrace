# Design: add-observer-source-abstraction

## Summary

新增一层统一的 source backend 抽象，让 `PrismTrace` 可以同时承载：

- `AttachProbeSource`
- `CodexAppServerSource`
- 后续的 `OpencodeServerSource`

并将这些不同来源统一投影到高层 `ObserverEvent` 语义，而不是强迫所有 source 都先还原成 HTTP request/response。

## Architecture

### Source backend

每个 source backend 负责：

- 连接目标
- 读取源协议
- 产出 source 原始事件

### Normalize

由 host 内统一映射逻辑将 source 原始事件转换为高层 `ObserverEvent`：

- `session_started`
- `session_updated`
- `session_completed`
- `turn_started`
- `turn_completed`
- `item_observed`
- `tool_call_observed`
- `approval_observed`
- `hook_observed`
- `capability_snapshot_observed`
- `observer_error_observed`

### Product consumption

后续 artifact、timeline、inspector 和 analysis 层只消费统一 `ObserverEvent`，不直接依赖底层 source 私有协议。

## Migration

### Attach route

现有 attach/probe 路线继续保留，但其定位改为：

- `AttachProbeSource`

### Codex

现有 `Codex observer` 最小实现继续推进，但后续应对齐到统一 source 抽象。

### opencode

后续新增 `OpencodeServerSource`，优先从 `server + event/export` 入手。

## Risks

### 风险 1：统一层过早设计过大

应对：

- 第一版只覆盖已经在 `Codex` 和 `opencode` 中都能自然表达的高层语义

### 风险 2：现有 request/response 体系和统一事件层重复

应对：

- 允许短期并行存在
- 等多个 source 跑通后再决定收敛策略

### 风险 3：不同 source 的信息密度差异大

应对：

- 统一层保留 `raw_json`
- 把“高层统一”限定在事件语义，而非字段强对齐
