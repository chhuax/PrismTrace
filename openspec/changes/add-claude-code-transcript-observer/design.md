# Design: add-claude-code-transcript-observer

## Summary

新增 `ClaudeCodeTranscriptSource`，通过被动发现、读取并持续跟踪 `Claude Code` 默认落地到本机的 transcript `jsonl` 文件，将其中的高层会话事实投影到统一 observer 事件层。

本轮实现边界进一步固定为：

- 只做 host / CLI / artifact 闭环
- 不接 console observer 展示
- 不引入 attach、受控启动或 raw payload 抓取

这条路线的核心前提是：

- 不要求用户改启动方式
- 不要求用户重启已有会话
- 不要求 `PrismTrace` 受控启动 `Claude Code`

因此第一版的产品定义应明确为：

`Claude Code transcript observer`

而不是：

- `Claude Code attach observer`
- `Claude Code app server observer`
- `Claude Code raw payload capture`

## Source strategy

第一版优先使用 `Claude Code` 默认本地 transcript 目录：

- `~/.claude/projects/**/*.jsonl`

接入过程分两段：

1. 启动时扫描现有 transcript 文件，补齐近期历史上下文
2. 对活跃文件执行 follow，读取后续追加的增量事件

其中：

- 历史扫描负责解决“PrismTrace 晚于 Claude 启动”的场景
- 增量 follow 负责提供近实时 observer 体验

第一版不要求：

- 目录外自定义 transcript 路径
- 云端 session 回放
- 通过 remote control 或其他在线协议回溯历史

## Session discovery

建议以 transcript 文件为 `session` 的一等发现对象，而不是进程：

- 通过目录扫描发现候选 session 文件
- 以文件路径、session id、最近修改时间构建候选 source 列表
- 优先跟踪最近仍在写入的 transcript

第一版允许 `PrismTrace` 在没有发现 live `Claude Code` 进程的情况下，仍然展示可读取的 transcript session。

这和现有 attach 模型不同，但符合 transcript observer 的本质：

- attach 路线围绕“进程是否可连接”
- transcript 路线围绕“session 文件是否可读取且仍有增量”

## Event normalization

第一版不要求把 transcript 事件强行还原成 request / response 模型，而是直接映射到统一 observer 语义。

建议的最小映射如下：

- `user` -> `turn`
- `assistant` -> `item`
- `progress` -> `item`
- `system/local_command` -> `tool`
- `system/stop_hook_summary` -> `hook`
- `permission-mode` -> `approval`
- `attachment` -> `item`
- 未识别或暂未稳定映射的类型 -> `unknown`

每条归一化事件最少应包含：

- `channel_kind = claude-code-transcript`
- `event_kind`
- `session_id`
- 在可见时保留 `parent_uuid`、`tool_use_id` 等关联键
- `timestamp`
- `summary`
- `raw_json`

第一版必须保留 `raw_json`，原因是 transcript 事件种类和字段密度可能随 `Claude Code` 版本演化，过早压缩字段会导致后续分析能力受限。

## Observer semantics

`ClaudeCodeTranscriptSource` 应被视为统一 observer abstraction 中的一类 file-backed source。

它与现有 source 的关系建议如下：

- `AttachProbeSource`
  - 面向 Node / Electron live attach
- `CodexAppServerSource`
  - 面向 `Codex` 官方 socket / app server
- `OpencodeServerSource`
  - 面向 `opencode` 官方 server
- `ClaudeCodeTranscriptSource`
  - 面向 `Claude Code` 本地 transcript

也就是说，统一层收敛的是高层 `ObserverEvent` 语义，不是底层 transport 形态。

## CLI entry

第一版建议新增独立入口，例如：

- `--claude-observe`
- `--claude-transcript-root <path>`

其中：

- `--claude-observe` 默认使用 `~/.claude/projects`
- `--claude-transcript-root <path>` 主要用于调试和测试，不要求普通用户主动配置

这条入口不应复用现有 `--attach`，避免把 transcript observer 和进程注入模型混淆。

同时本轮 CLI 只要求完成：

- startup summary
- transcript root 发现
- 历史扫描
- 活跃 transcript 的最小增量 follow
- handshake / event 输出
- artifact 落盘

不要求：

- console UI 接线
- 独立 replay 命令
- 面向前端的额外交互协议

## Non-observable boundaries

第一版需要明确几个不可观测或弱可观测场景：

### `--no-session-persistence`

如果用户以关闭 session 持久化的方式运行 `Claude Code`，本地 transcript 不会稳定落盘，这条 observer 路线应返回结构化“不可观测”结果，而不是假装无数据。

### 历史文件已被清理或移动

如果 transcript 已被用户清理、迁移或目录权限不足，第一版只要求结构化报错与提示，不要求自动恢复。

### 无法获得 raw model payload

transcript observer 只承诺读取 `Claude Code` 已落地的高层会话事实，不承诺还原模型后端的原始 HTTP request / response。

## Persistence and replay

第一版建议把 handshake、发现到的 session 元数据和归一化事件继续写入 host artifact 体系。

这样可以满足两个目标：

- 为控制台和后续 analysis 层复用统一 event artifact
- 避免每次打开控制台都重新直接扫描全部 transcript 才能显示内容

但这一轮不要求：

- 设计新的长期数据库 schema
- 完成复杂 replay UI

## Risks

### 风险 1：transcript 事件类型随版本变化

应对：

- 保留 `raw_json`
- 未识别事件映射为 `unknown`
- 映射层按“最小高层语义”而不是按字段全量绑定

### 风险 2：follow 语义与文件轮转存在细节差异

应对：

- 第一版先覆盖 append-only 主路径
- 对文件消失、重建、停止写入返回结构化状态

### 风险 3：历史扫描量过大

应对：

- 第一版按最近修改时间优先
- 允许限制首次扫描窗口或候选文件数量
- 先保证“最近 session 可见”，再逐步扩展全量历史能力
