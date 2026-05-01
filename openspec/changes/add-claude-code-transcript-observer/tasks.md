## 1. CLI 入口与 observer 壳层接线

- [x] 1.1 新增 `--claude-observe` / `--claude-transcript-root` 参数解析
  - 验证：`claude_observe_args` 聚焦测试通过，覆盖缺省路径、显式路径和缺参报错

- [x] 1.2 让 host session 通过 storage 驱动 `claude-code` observer
  - 验证：`run_claude_observer_session` 测试通过，`ObserverChannelKind::ClaudeCodeTranscript` label 稳定为 `claude-code`

## 2. Transcript 发现、历史扫描与归一化

- [x] 2.1 接入 transcript 文件发现与最近会话优先扫描
  - 验证：`discover_transcript_files_*` 聚焦测试通过，覆盖最近优先、结果截断和文件消失场景

- [x] 2.2 完成 transcript 记录到统一 observer 事件的最小映射
  - 验证：`claude_observer` 聚焦测试通过，覆盖 `user -> turn`、未知类型回退、`parentUuid/item_id` 归一化

## 3. Artifact 持久化与最小 follow

- [x] 3.1 为 `observer_events/claude-code` 增加握手与事件落盘
  - 验证：artifact writer 与 `run_claude_observer_writes_artifact_records` 测试通过

- [x] 3.2 补齐 append-only transcript 的最小增量 follow 语义
  - 验证：聚焦测试覆盖 backlog 消费、无尾换行、追加多行与无新行超时返回

## 4. 集成校验与回填

- [x] 4.1 回填 story 实施状态与验证口径
  - 验证：`docs/stories/add-claude-code-transcript-observer/plan.md` 已补当前状态、通过项和既有红灯说明

- [x] 4.2 运行 Task 4 指定验证并记录结果
  - 验证：与本 story 直接相关的聚焦测试通过；`cargo run -p prismtrace-host -- --discover` 通过
  - 备注：用户给出的聚焦测试合并命令 `cargo test -p prismtrace-host claude_observe_args codex_observe_args opencode_observe_args console_target_filters_arg claude_observer observer_channel_kind_label -- --nocapture` 不符合 `cargo test` 参数语法，已拆分为等价单项命令执行

- [x] 4.3 区分本次验证通过项与仓库当前既有红灯
  - 验证：`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 的失败点已在 story 计划文档中单独记录，未在本任务中扩修无关问题
