# Design: add-opencode-server-observer

## Summary

新增 `OpencodeServerSource`，通过 `opencode` 官方 server 接口把 `opencode` 接入 `PrismTrace` 的统一 observer 层。第一版只做底层采集闭环，不做 UI 接入，也不复用 attach 语义。

## Chosen approach

本 change 采用 `observer-parity` 路径，而不是继续保留 snapshot 原型：

- 沿用 `observer.rs` 的 `ObserverSource / ObserverSession / ObservedEvent` 抽象
- 参照 `codex_observer.rs`，把 `opencode_observer.rs` 补成正式 source 实现
- 补齐 artifact writer 与 CLI 入口
- 先连接现有 `opencode` server，不负责自动拉起 server

## Source strategy

第一版固定四类官方面：

1. `GET /global/health`
2. session list
3. session export 或 session message
4. `GET /global/event`

其中：

- health 仅用于握手
- session list + export/message 用于生成高信息密度快照事件
- global event 用于补最小实时事件

## Boundary

`opencode` 不经过：

- `AttachController`
- `LiveAttachBackend`
- `InstrumentationRuntime`
- probe bootstrap
- `HttpRequestObserved / HttpResponseObserved`

它在 host 内与 `CodexAppServerSource` 并列，作为新的 observer source。

## Event normalization

第一版只做保守归一化：

- session -> `thread`
- message / text / reasoning / part -> `item`
- tool -> `tool`
- permission / approval -> `approval`
- agent -> `agent`
- MCP server -> `mcp`
- provider -> `provider`
- plugin -> `plugin`
- command -> `command`
- app -> `app`
- 其他未识别事件 -> `unknown`

`agent`、MCP server、provider 是 opencode 自身的协议域，不在底层伪装成 Codex 的 `skill`、`plugin`、`app`。如果上层 diagnostics 需要跨 source 对齐，应在 analysis/read-model 层显式定义等价规则。

所有事件必须保留 `raw_json`。

## Artifact strategy

事件统一写入：

- `.prismtrace/artifacts/observer_events/opencode/*.jsonl`

记录格式与 `Codex` 对齐：

- 握手写一条 `record_type = handshake`
- 事件逐条写 `record_type = event`

## CLI entry

第一版新增：

- `--opencode-observe`
- `--opencode-url <url>`

CLI 行为：

1. 输出 host startup summary
2. 连接现有 server
3. 输出 handshake
4. 拉一轮 snapshot 事件
5. 读取最小实时事件
6. 同步持久化到 artifact

## Risks

### 风险 1：实时事件流信息密度不足

应对：

- 第一版不依赖纯 event stream
- 用 session list + export/message 建立快照基线

### 风险 2：event 与 export 语义不完全一致

应对：

- 统一层保留 `raw_json`
- 无法稳定识别时回退到 `unknown`

### 风险 3：server 连接参数未来可能扩展

应对：

- 第一版只支持连接现有本地 server
- 后续再补认证、自动发现、自动拉起
