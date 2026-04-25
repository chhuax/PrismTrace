# Design: add-observer-source-abstraction

## Summary

新增一层统一的 source backend 抽象，让 `PrismTrace` 只围绕当前可行的观测接入面扩展：

- `CodexAppServerSource`
- `OpencodeServerSource`
- `ClaudeCodeTranscriptSource`
- 后续其他 `ExportReplaySource` / `RuntimeEventSource`

统一层的目标是把这些不同来源投影到高层 `ObserverEvent` 语义，而不是继续围绕 attach 控制流设计产品和 host 架构。

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

### Legacy attach route

历史 attach/probe 路线不再属于当前 `prismtrace-host` 产品面，也不再作为统一 source abstraction 的目标实现。

### Codex

现有 `Codex observer` 实现继续推进，并对齐统一 source 抽象。

### opencode

后续新增 `OpencodeServerSource`，优先从 `server + event/export` 入手。

### Claude Code

后续新增 `ClaudeCodeTranscriptSource`，优先从 transcript / event / export 入手。

## Risks

### 风险 1：统一层过早设计过大

应对：

- 第一版只覆盖已经在 `Codex`、`opencode`、`Claude Code` 中都能自然表达的高层语义

### 风险 2：现有 request/response 体系和统一事件层重复

应对：

- 允许短期并行存在
- 等多个 source 跑通后再决定收敛策略

### 风险 3：不同 source 的信息密度差异大

应对：

- 统一层保留 `raw_json`
- 把“高层统一”限定在事件语义，而非字段强对齐
