# Claude Code Transcript Observer 设计稿

日期：2026-04-26  
状态：已收敛，待进入实施计划

## 1. 背景

`PrismTrace` 当前产品主线已经从 `attach-first` 收口到 `observer-first`。

对 `Claude Code`，现阶段已知事实也已经足够明确：

- 不应继续尝试 `attach + probe`
- 不接受“由 PrismTrace 受控启动 Claude Code”作为产品前提
- 第一版最稳定的观测面是 `Claude Code` 默认落地到本机的 transcript `jsonl`

因此，这一轮应把 `Claude Code` 定义为一条新的 file-backed observer source，而不是新的 attach 兼容性问题，也不是 `Codex` 那类官方 app server observer。

## 2. 本轮目标

本 story 第一版只做底层采集闭环，不扩散到控制台展示：

1. 在 host 中新增 `ClaudeCodeTranscriptSource`
2. 提供独立 CLI 入口读取 `~/.claude/projects/**/*.jsonl`
3. 完成历史扫描、活跃 transcript follow、统一事件归一化
4. 将握手和事件稳定落盘到统一 `observer_events` artifact 目录
5. 对不可观测场景返回结构化错误，而不是静默无数据

## 3. 非目标

这一版明确不做：

- 不做 `Claude Code` live attach
- 不做受控启动、session 托管或 SDK 集成
- 不承诺获取 `Claude Code -> 模型后端` 的原始 HTTP request / response
- 不接本地 console observer 视图
- 不在这一轮做 transcript 以外的数据源发现
- 不顺手重构 `Codex` / `opencode` 既有 observer 流程

## 4. 选定方案

本轮采用 `observer-shell + transcript-source` 方案。

也就是说：

- 继续复用现有 `observer.rs` 抽象
- 新增 `ClaudeCodeTranscriptSource` 和对应 `ObserverSession`
- 由 source 负责 transcript 文件发现、历史补扫和增量 follow
- 由 session 负责把 transcript 记录投影成统一 `ObservedEvent`
- artifact 落盘直接对齐 `Codex` 的 `observer_events/<channel>/*.jsonl` 结构

这样做的原因是：

- 避免先写临时脚本式入口，后续再重接一次
- 保证 `Claude Code` 从第一版开始就进入统一 observer 壳层
- 后续需要补 console 展示时，只消费同一批 artifact 即可

## 5. 接入边界

### 5.1 不复用 attach 语义

`Claude Code` 接入不应经过：

- `AttachController`
- `InstrumentationRuntime`
- probe bootstrap
- `HttpRequestObserved / HttpResponseObserved`

原因是 transcript observer 的观测对象不是目标进程内的网络路径，而是本地已落地的高层会话事实。

### 5.2 作为新的 file-backed observer source 接入

host 内 observer source 至少应允许三类通道并列：

1. `CodexAppServerSource`
2. `OpencodeServerSource`
3. `ClaudeCodeTranscriptSource`

三者共享：

- `ObserverSource`
- `ObserverSession`
- `ObserverHandshake`
- `ObservedEvent`

但底层 transport 可以不同：

- `Codex` 是 socket / app server
- `opencode` 是 HTTP server
- `Claude Code` 是 transcript file

## 6. CLI 行为

第一版新增最小采集入口：

- `--claude-observe`
- `--claude-transcript-root <path>`

默认行为是：

1. 输出 host startup summary
2. 以 `~/.claude/projects` 作为默认 transcript root
3. 扫描候选 transcript 文件
4. 读取最近可用 session 的历史记录
5. 对仍在追加写入的 transcript 做最小增量 follow
6. 输出 handshake 和归一化事件
7. 同步将 handshake / event 落盘到 artifact

`--claude-transcript-root <path>` 主要用于测试和调试，不要求普通用户主动配置。

## 7. Session 发现策略

第一版把 transcript 文件视为 session 发现对象，而不是进程。

发现流程建议如下：

1. 递归扫描 transcript root 下的 `*.jsonl`
2. 以文件路径、最近修改时间、可解析状态构建候选列表
3. 优先处理最近仍在写入的 transcript
4. 对每个候选文件建立独立 session 读取状态

这意味着即使当前没有 live `Claude Code` 进程，只要 transcript 文件仍存在，`PrismTrace` 仍可以观测历史 session。

## 8. 事件归一化

第一版不强行还原成 request / response 语义，只做高层保守映射：

- `user` -> `ObservedEventKind::Turn`
- `assistant` -> `ObservedEventKind::Item`
- `progress` -> `ObservedEventKind::Item`
- `system/local_command` -> `ObservedEventKind::Tool`
- `system/stop_hook_summary` -> `ObservedEventKind::Hook`
- `permission-mode` -> `ObservedEventKind::Approval`
- `attachment` -> `ObservedEventKind::Item`
- 未识别类型 -> `ObservedEventKind::Unknown`

每条 `ObservedEvent` 至少保留：

- `channel_kind = ClaudeCodeTranscript`
- `event_kind`
- `summary`
- `thread_id`
- `turn_id`
- `item_id`
- `timestamp`
- `raw_json`

`raw_json` 是这一轮的强约束，不允许省略。

## 9. Artifact 持久化

artifact 策略直接对齐现有 observer 通道：

- 路径：`.prismtrace/artifacts/observer_events/claude-code/*.jsonl`
- 第一行写 `record_type = handshake`
- 后续逐行追加 `record_type = event`

每条记录至少包含：

- `record_type`
- `channel`
- `event_kind`
- `summary`
- `thread_id`
- `turn_id`
- `item_id`
- `timestamp`
- `recorded_at_ms`
- `raw_json`

这样即使这一轮还没有 console，也能保证后续视图层直接复用同一批 artifact。

## 10. 不可观测边界

第一版需要明确暴露几个失败场景：

### 10.1 `--no-session-persistence`

如果用户关闭了 session 持久化，或者 transcript 没有稳定落盘，observer 应返回结构化“不可观测”错误，而不是输出空成功结果。

### 10.2 transcript 根目录不可读

如果默认目录不存在、权限不足或路径被清理，observer 应明确报告目录问题和当前使用的 root 路径。

### 10.3 无法获得 raw model payload

transcript observer 只承诺读取 `Claude Code` 已落地的高层会话事实，不承诺还原模型后端原始报文。

## 11. 风险与降级

### 风险 1：transcript 事件类型会随版本变化

应对：

- 保留 `raw_json`
- 未识别类型统一回退到 `unknown`
- 映射层只绑定最小高层语义

### 风险 2：follow 与文件轮转细节存在差异

应对：

- 第一版先覆盖 append-only 主路径
- 对文件消失、停止写入、重新创建给出结构化状态

### 风险 3：首次历史扫描量过大

应对：

- 优先最近修改的 transcript
- 限制首次扫描候选数量或历史窗口
- 先保证“最近 session 可见”，再扩展全量历史

## 12. 验证策略

本轮验证分三层：

1. transcript 解析测试
   - 覆盖已知事件类型、未知类型、损坏行、空文件
2. host 集成测试
   - 覆盖 CLI 参数解析、握手输出、artifact 落盘、增量 follow
3. 本地基线
   - `cargo fmt --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo run -p prismtrace-host -- --discover`
