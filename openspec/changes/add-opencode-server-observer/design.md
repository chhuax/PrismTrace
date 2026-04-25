# Design: add-opencode-server-observer

## Summary

新增 `OpencodeServerSource`，通过 `opencode` 官方 `server + attach(url) + export + event` 能力接入 `PrismTrace`，并将其投影到统一 observer 事件层。

## Source strategy

第一版优先使用四类官方面：

1. `GET /global/health`
2. session list
3. session export
4. `GET /global/event`

其中：

- health 用于验证连接
- session list / export 用于快速获得高信息密度离线结构
- global event 用于逐步扩展实时观测能力

## Event normalization

建议将 `opencode` 数据映射到统一高层语义：

- `session_*`
- `item_observed`
- `tool_call_observed`
- `approval_observed`
- `observer_error_observed`
- `capability_snapshot_observed`（如后续接 plugin / MCP / tool visibility）

## CLI entry

建议新增独立入口，例如：

- `--opencode-observe`
- `--opencode-url <url>`
- 可选后续 `--opencode-export <session_id>`

不复用现有 `--attach`。

## Risks

### 风险 1：实时事件流信息密度不足

应对：

- 第一版不只依赖 event stream，同时接 export

### 风险 2：server 需要认证或附加配置

应对：

- 第一版先支持本地最小无密码 server
- 后续再补安全与认证配置

### 风险 3：export 与 event 语义不完全一致

应对：

- 统一层保留 `raw_json`
- 允许短期并行存在“离线结构”和“实时事件”
