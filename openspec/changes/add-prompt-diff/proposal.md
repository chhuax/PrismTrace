## Why

PrismTrace 当前已经完成 request capture、response capture、local console、request inspector、tool visibility 和第一版 session reconstruction。系统已经能够回答：

- 它发了什么 request
- 它收到了什么 response
- 这次 request 暴露了哪些 tools
- 这些调用在同一 session 里是怎样串起来的

但用户仍然很难直接回答一个更贴近分析层的问题：

- 这次调用的 prompt 相比上一条到底变了什么

没有这一层能力，PrismTrace 仍然停留在“能浏览事实”，还没有进入“能解释变化”的第一刀。

因此需要单独开 `add-prompt-diff`，先把同一 session 内相邻 request 的 prompt 变化提炼成可读 diff。

## What Changes

- 从 request artifact 中提取 prompt-bearing 文本 projection
- 在同一 session 内将当前 request 与上一条 request 做 prompt diff
- 在 request inspector 中展示 diff 状态、上一条 request 引用和 diff 文本
- 更新路线图当前状态与下一步建议

## Capabilities

### New Capabilities
- `prompt-diff`: 在 request inspector 中比较同一 session 内相邻两次 request 的 prompt 变化

### Modified Capabilities
- `request-inspector`: 从“检查单次 exchange 的事实”扩展为“检查单次 exchange 及其相对上一条调用的 prompt 变化”

## Impact

- 影响代码：主要集中在 `crates/prismtrace-host/src/console.rs`
- 影响系统行为：request detail 将新增 prompt diff 结果；控制台详情区将新增 `Prompt Diff` 区域
- 影响文档：新增 `add-prompt-diff` story 与 OpenSpec 变更文档
- 边界说明：本次不交付 tool visibility diff、response diff、failure attribution 或 skill diagnostics
