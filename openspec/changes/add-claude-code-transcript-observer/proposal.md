# Proposal: add-claude-code-transcript-observer

## Why

`PrismTrace` 现在已经明确对 `Codex` 和 `opencode` 逐步转向官方 observer 路线，不再把 live attach 当作默认接入方式。

对 `Claude Code`，调研结果也指向同样结论：

- 不应继续尝试 `attach + probe`
- 不接受“受控启动 Claude 再由 PrismTrace 接管”的产品前提
- 第一版更适合复用 `Claude Code` 默认会落地到本机的 transcript 数据

这意味着 `Claude Code` 不应被建模成新的 attach 兼容性问题，也不应被误收敛为 `Codex App Server` 一类官方 socket observer。

因此需要新增 `add-claude-code-transcript-observer`，把 `PrismTrace` 对 `Claude Code` 的接入路线明确为：

`被动读取本地 transcript 的 observer source`

## What Changes

- 在 host 中新增 `ClaudeCodeTranscriptSource`
- 通过发现并读取 `~/.claude/projects/**/*.jsonl` 接入 `Claude Code`
- 第一版支持：
  - 已有 transcript session 的历史扫描
  - 活跃 transcript 文件的增量 follow
  - transcript 高层事件到统一 observer 事件层的最小映射
- 明确不可观测边界，例如 `--no-session-persistence`

## Capabilities

### New Capabilities

- `claude-code-transcript-observer`: 通过本地 transcript 文件读取 `Claude Code` 的高层会话事实

## Impact

- 影响代码：集中在 `crates/prismtrace-host`
- 影响架构：为统一 observer 抽象新增一类 file-backed source，而不是 socket/server-backed source
- 影响产品语义：`Claude Code` 可被观测，但观测对象是本地 transcript 事实，不是 raw HTTP payload
- 影响文档：新增 `add-claude-code-transcript-observer` 提案与设计稿

## Out of Scope

- 本 change 不要求实现 `Claude Code` live attach
- 不要求受控启动、SDK 托管或 telemetry 前置配置
- 不要求抓取 `Claude Code -> 模型后端` 的原始 HTTP 报文
- 不要求这一轮扩展复杂控制台 UI
