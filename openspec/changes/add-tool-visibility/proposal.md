## Why

PrismTrace 当前已经能在控制台里检查 request payload 与 matching response，但还缺一层关键事实：

- 这次 request 最终暴露给模型的 tools / functions 集合是什么

没有这一层事实，PrismTrace 还不能完整回答“发了什么、收到了什么、当时可见什么工具”，也就很难自然过渡到后续的 session reconstruction、tool visibility diff 与 skill diagnostics。

因此需要单独开 `add-tool-visibility`，先把最小可用的 tool visibility fact 收进系统。

## What Changes

- 从已捕获的 request payload 中提取 request-embedded tool visibility
- 将 tool visibility 持久化为独立 artifact
- 在 request inspector 中展示 final tools、tool count 和 tool choice
- 更新路线图当前状态与下一步建议

## Capabilities

### New Capabilities
- `tool-visibility`: 记录并展示单次 request 中最终发送给模型的 tool / function 集合

### Modified Capabilities
- `request-inspector`: 从 request / response 检查器扩展为 request / tool / response 三类事实检查器

## Impact

- 影响代码：`crates/prismtrace-host/src/request_capture.rs`、`crates/prismtrace-host/src/console.rs` 以及新增 `tool_visibility` 模块
- 影响系统行为：匹配到 tools / functions 的 request 将生成独立 visibility artifact，并在控制台详情中可见
- 影响文档：新增 `add-tool-visibility` 设计、计划与 OpenSpec 变更文档
- 边界说明：本次不交付多阶段 visibility、session reconstruction、visibility diff 或 skill diagnostics
