# Claude Code Transcript Observer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `Claude Code` 作为新的 transcript-backed observer source 接入 `prismtrace-host`，打通 `--claude-observe`、历史扫描、最小增量 follow 和 `observer_events/claude-code` artifact 落盘。

**Architecture:** 继续复用 `observer.rs` 的统一抽象，不走 attach 语义。`main.rs` 负责 CLI 解析，`lib.rs` 负责 host session 接线，新增的 `claude_observer.rs` 负责 transcript 目录发现、`jsonl` 解析、增量 follow、事件归一化和 artifact 写入。

**Tech Stack:** Rust, `serde_json`, `std::fs`, `std::io`, inline unit tests, PrismTrace local artifact storage

---

## 当前状态

状态：Task 1、Task 2、Task 3 已完成并通过前两轮 review；Task 4 已完成 OpenSpec 回填与集成校验记录。

当前已落地范围：

- `--claude-observe` / `--claude-transcript-root` CLI 入口已接入
- `ClaudeCodeTranscript` observer channel 已接入统一 host shell
- transcript 发现、历史扫描、事件归一化与 `observer_events/claude-code` artifact 落盘已实现
- 最小增量 follow 已覆盖 backlog、无尾换行、`parentUuid` 和空目录语义

## Task 4 验证结果

与本 story 直接相关的验证：

- PASS: `cargo test -p prismtrace-host claude_observe_args -- --nocapture`
- PASS: `cargo test -p prismtrace-host claude_observer -- --nocapture`
- PASS: `cargo test -p prismtrace-host observer_channel_kind_label -- --nocapture`
- PASS: `cargo fmt --check`
- PASS: `cargo run -p prismtrace-host -- --discover`

补充的共享 CLI 回归验证：

- PASS: `cargo test -p prismtrace-host codex_observe_args -- --nocapture`
- PASS: `cargo test -p prismtrace-host opencode_observe_args -- --nocapture`

说明：

- 用户给出的聚焦测试命令
  - `cargo test -p prismtrace-host claude_observe_args codex_observe_args opencode_observe_args claude_observer observer_channel_kind_label -- --nocapture`
- 该命令会直接报 `unexpected argument 'codex_observe_args' found`，因为 `cargo test` 只接受单个过滤串。
- 为获得有效验证证据，本次按等价单项命令逐条执行并记录结果。

## 当前仓库基线红灯

以下失败发生在当前工作区既有改动上，本次任务未扩修无关问题，只做记录：

- FAIL: `cargo clippy --workspace --all-targets -- -D warnings`
  - 失败点 1：`crates/prismtrace-core/src/lib.rs:478` 存在 duplicated `#[test]` attribute
  - 失败点 2：`crates/prismtrace-host/src/console/observer.rs:584` 命中 `clippy::items-after-test-module`

- FAIL: `cargo test --workspace`
  - `prismtrace-core` 有 duplicated `#[test]` warning
  - `prismtrace-host` 共有 7 个既有 console 相关测试失败：
    - `console::tests::console_server_serves_homepage_over_http`
    - `console::tests::render_console_homepage_exposes_theme_switcher`
    - `console::tests::render_console_homepage_includes_title_and_heading`
    - `console::tests::render_console_homepage_renders_empty_regions_and_refresh_script`
    - `console::tests::render_console_homepage_renders_request_detail_and_health_panel_regions`
    - `console::tests::render_console_homepage_seeds_initial_session_selection_for_js_hydration`
    - `console::tests::render_console_homepage_uses_observer_first_shell_copy`

结论：

- Claude transcript observer 这条 story 的实现与聚焦测试目前可用
- 仓库全量基线尚未恢复为全绿，红灯集中在 core 测试属性和 console 页面/测试改动，不应在本任务中顺手修复

## 文件边界

- `crates/prismtrace-host/src/main.rs`
  - 新增 `--claude-observe` / `--claude-transcript-root` 参数解析
- `crates/prismtrace-host/src/lib.rs`
  - 暴露 `run_claude_observer_session`
- `crates/prismtrace-host/src/observer.rs`
  - 新增 `ObserverChannelKind::ClaudeCodeTranscript`
- `crates/prismtrace-host/src/claude_observer.rs`
  - 新增 transcript observer 实现、artifact writer 和测试
- `openspec/changes/add-claude-code-transcript-observer/tasks.md`
  - 回填本轮执行项

### Task 1: CLI 入口与 observer 壳层接线

**Files:**
- Modify: `crates/prismtrace-host/src/main.rs`
- Modify: `crates/prismtrace-host/src/lib.rs`
- Modify: `crates/prismtrace-host/src/observer.rs`
- Test: `crates/prismtrace-host/src/main.rs` inline tests
- Test: `crates/prismtrace-host/src/observer.rs` inline tests

- [ ] **Step 1: 先写 CLI 参数解析和 channel label 的失败测试**

```rust
#[test]
fn claude_observe_args_returns_none_when_flag_is_missing() {
    let args = vec!["--discover".to_string()];

    assert!(
        claude_observe_args(&args)
            .expect("parse should succeed")
            .is_none()
    );
}

#[test]
fn claude_observe_args_uses_default_transcript_root() {
    let args = vec!["--claude-observe".to_string()];
    let options = claude_observe_args(&args)
        .expect("parse should succeed")
        .expect("options should exist");

    assert_eq!(options.transcript_root, None);
}

#[test]
fn claude_observe_args_parses_explicit_transcript_root() {
    let args = vec![
        "--claude-observe".to_string(),
        "--claude-transcript-root".to_string(),
        "/tmp/claude-projects".to_string(),
    ];
    let options = claude_observe_args(&args)
        .expect("parse should succeed")
        .expect("options should exist");

    assert_eq!(
        options.transcript_root,
        Some(std::path::PathBuf::from("/tmp/claude-projects"))
    );
}
```

```rust
#[test]
fn observer_channel_kind_label_covers_claude_transcript() {
    assert_eq!(
        ObserverChannelKind::ClaudeCodeTranscript.label(),
        "claude-code"
    );
}
```

- [ ] **Step 2: 跑参数解析和 channel label 测试，确认当前实现缺少 Claude 入口**

Run: `cargo test -p prismtrace-host claude_observe_args observer_channel_kind_label -- --nocapture`

Expected: FAIL，提示 `claude_observe_args`、`ClaudeCodeTranscript` 或对应 label 尚未实现。

- [ ] **Step 3: 实现最小 CLI / lib / observer 接线**

```rust
if let Some(options) = claude_observe_args(&args)? {
    let mut stdout = std::io::stdout().lock();
    prismtrace_host::run_claude_observer_session(&result, options, &mut stdout)?;
    return Ok(());
}
```

```rust
fn claude_observe_args(
    args: &[String],
) -> std::io::Result<Option<prismtrace_host::claude_observer::ClaudeObserverOptions>> {
    if !args.iter().any(|arg| arg == "--claude-observe") {
        return Ok(None);
    }

    let transcript_root =
        arg_value(args, "--claude-transcript-root").map(std::path::PathBuf::from);

    Ok(Some(
        prismtrace_host::claude_observer::ClaudeObserverOptions {
            transcript_root,
            ..Default::default()
        },
    ))
}
```

```rust
pub fn run_claude_observer_session(
    result: &BootstrapResult,
    options: claude_observer::ClaudeObserverOptions,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(output, "{}", startup_summary(result))?;
    claude_observer::run_claude_observer(&result.storage, output, options)
}
```

```rust
pub enum ObserverChannelKind {
    CodexAppServer,
    OpencodeServer,
    ClaudeCodeTranscript,
}
```

- [ ] **Step 4: 重跑参数解析和 channel label 测试**

Run: `cargo test -p prismtrace-host claude_observe_args observer_channel_kind_label -- --nocapture`

Expected: PASS，`main.rs` 和 `observer.rs` inline tests 通过。

- [ ] **Step 5: Commit**

```bash
git add crates/prismtrace-host/src/main.rs crates/prismtrace-host/src/lib.rs crates/prismtrace-host/src/observer.rs
git commit -m "feat: add claude transcript observer entrypoints"
```

### Task 2: transcript 发现、历史扫描和事件归一化

**Files:**
- Create: `crates/prismtrace-host/src/claude_observer.rs`
- Modify: `crates/prismtrace-host/src/lib.rs`
- Test: `crates/prismtrace-host/src/claude_observer.rs` inline tests

- [ ] **Step 1: 先写 transcript 扫描与归一化的失败测试**

```rust
#[test]
fn discover_transcript_files_orders_recent_jsonl_first() -> io::Result<()> {
    let root = unique_test_dir();
    let older = root.join("project-a").join("session-a.jsonl");
    let newer = root.join("project-b").join("session-b.jsonl");
    std::fs::create_dir_all(older.parent().expect("parent"))?;
    std::fs::create_dir_all(newer.parent().expect("parent"))?;
    std::fs::write(&older, b"{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n")?;
    std::fs::write(&newer, b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":\"hello\"}}\n")?;

    let files = discover_transcript_files(&root, 10)?;

    assert_eq!(files.len(), 2);
    assert_eq!(files[0], newer);
    assert_eq!(files[1], older);
    Ok(())
}

#[test]
fn transcript_user_record_maps_to_turn_event() {
    let event = normalize_transcript_record(
        "session-1",
        &json!({
            "type": "user",
            "uuid": "msg-1",
            "timestamp": "2026-04-26T10:00:00Z",
            "message": {
                "role": "user",
                "content": "Inspect this repo"
            }
        }),
    )
    .expect("event should exist");

    assert_eq!(event.event_kind, ObservedEventKind::Turn);
    assert_eq!(event.thread_id.as_deref(), Some("session-1"));
    assert_eq!(event.item_id.as_deref(), Some("msg-1"));
}

#[test]
fn transcript_unknown_record_falls_back_to_unknown_event() {
    let event = normalize_transcript_record(
        "session-1",
        &json!({
            "type": "weird-event",
            "timestamp": "2026-04-26T10:00:00Z"
        }),
    )
    .expect("event should exist");

    assert_eq!(event.event_kind, ObservedEventKind::Unknown);
}
```

- [ ] **Step 2: 跑 `claude_observer` 测试，确认模块尚未实现**

Run: `cargo test -p prismtrace-host claude_observer -- --nocapture`

Expected: FAIL，提示 `claude_observer.rs`、`discover_transcript_files` 或 `normalize_transcript_record` 尚不存在。

- [ ] **Step 3: 实现最小 transcript source / session / 历史扫描逻辑**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeObserverOptions {
    pub transcript_root: Option<PathBuf>,
    pub max_files: usize,
    pub max_events: usize,
    pub follow_timeout: Duration,
}

impl Default for ClaudeObserverOptions {
    fn default() -> Self {
        Self {
            transcript_root: None,
            max_files: 8,
            max_events: 64,
            follow_timeout: Duration::from_millis(500),
        }
    }
}

fn normalize_transcript_record(session_id: &str, value: &Value) -> Option<ObservedEvent> {
    let record_type = value.get("type").and_then(Value::as_str).unwrap_or("unknown");
    let event_kind = match record_type {
        "user" => ObservedEventKind::Turn,
        "assistant" | "progress" | "attachment" => ObservedEventKind::Item,
        "system" if value.get("subtype").and_then(Value::as_str) == Some("local_command") => {
            ObservedEventKind::Tool
        }
        "system" if value.get("subtype").and_then(Value::as_str) == Some("stop_hook_summary") => {
            ObservedEventKind::Hook
        }
        "permission-mode" => ObservedEventKind::Approval,
        _ => ObservedEventKind::Unknown,
    };

    Some(ObservedEvent {
        channel_kind: ObserverChannelKind::ClaudeCodeTranscript,
        event_kind,
        summary: summarize_transcript_record(record_type, value),
        method: Some("transcript-jsonl".into()),
        thread_id: Some(session_id.to_string()),
        turn_id: value.get("parentUuid").and_then(Value::as_str).map(str::to_string),
        item_id: value.get("uuid").and_then(Value::as_str).map(str::to_string),
        timestamp: value.get("timestamp").and_then(Value::as_str).map(str::to_string),
        raw_json: value.clone(),
    })
}
```

- [ ] **Step 4: 重跑 `claude_observer` 测试**

Run: `cargo test -p prismtrace-host claude_observer -- --nocapture`

Expected: PASS，扫描、归一化和默认 options 相关测试通过。

- [ ] **Step 5: Commit**

```bash
git add crates/prismtrace-host/src/claude_observer.rs crates/prismtrace-host/src/lib.rs
git commit -m "feat: add claude transcript observer source"
```

### Task 3: artifact 落盘与最小增量 follow

**Files:**
- Modify: `crates/prismtrace-host/src/claude_observer.rs`
- Test: `crates/prismtrace-host/src/claude_observer.rs` inline tests

- [ ] **Step 1: 先写 artifact writer 与 follow 的失败测试**

```rust
#[test]
fn claude_observer_artifact_writer_persists_handshake_and_events() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = crate::bootstrap(&workspace_root)?;
    let handshake = ObserverHandshake {
        channel_kind: ObserverChannelKind::ClaudeCodeTranscript,
        transport_label: "/tmp/claude-projects".into(),
        server_label: "claude transcript".into(),
        raw_json: json!({ "root": "/tmp/claude-projects" }),
    };
    let writer = ClaudeObserverArtifactWriter::create(&result.storage, &handshake)?;
    writer.append_event(&ObservedEvent {
        channel_kind: ObserverChannelKind::ClaudeCodeTranscript,
        event_kind: ObservedEventKind::Turn,
        summary: "user prompt".into(),
        method: Some("transcript-jsonl".into()),
        thread_id: Some("session-1".into()),
        turn_id: None,
        item_id: Some("msg-1".into()),
        timestamp: Some("2026-04-26T10:00:00Z".into()),
        raw_json: json!({ "type": "user" }),
    })?;

    let artifact = std::fs::read_to_string(writer.artifact_path())?;
    assert!(artifact.contains("\"record_type\":\"handshake\""));
    assert!(artifact.contains("\"record_type\":\"event\""));
    Ok(())
}

#[test]
fn follow_transcript_reads_appended_lines() -> io::Result<()> {
    let root = unique_test_dir();
    let transcript = root.join("project").join("session.jsonl");
    std::fs::create_dir_all(transcript.parent().expect("parent"))?;
    std::fs::write(
        &transcript,
        b"{\"type\":\"user\",\"uuid\":\"msg-1\",\"timestamp\":\"2026-04-26T10:00:00Z\"}\n",
    )?;

    let appended = b"{\"type\":\"assistant\",\"uuid\":\"msg-2\",\"timestamp\":\"2026-04-26T10:00:01Z\"}\n";
    let events = follow_transcript_file(&transcript, appended, "session")?;

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_kind, ObservedEventKind::Item);
    Ok(())
}
```

- [ ] **Step 2: 跑 `claude_observer` 测试，确认 artifact 与 follow 仍未实现**

Run: `cargo test -p prismtrace-host claude_observer -- --nocapture`

Expected: FAIL，提示 `ClaudeObserverArtifactWriter`、`follow_transcript_file` 或签名不匹配。

- [ ] **Step 3: 实现 artifact writer、`run_claude_observer` 和最小增量 follow**

```rust
pub fn run_claude_observer(
    storage: &StorageLayout,
    output: &mut impl Write,
    options: ClaudeObserverOptions,
) -> io::Result<()> {
    let source = ClaudeObserverSource::from_options(&options)?;
    let mut session = source.connect()?;
    let handshake = session.initialize()?;
    let artifact_writer = ClaudeObserverArtifactWriter::create(storage, &handshake)?;

    writeln!(output, "{}", serde_json::to_string(&json!({
        "type": "claude_observer_handshake",
        "channel": handshake.channel_kind.label(),
        "transport": handshake.transport_label,
        "server_label": handshake.server_label,
        "raw": handshake.raw_json,
    }))?)?;

    for event in session.collect_capability_events()? {
        artifact_writer.append_event(&event)?;
        writeln!(output, "{}", serde_json::to_string(&event_as_json(&event))?)?;
    }

    while let Some(event) = session.next_event(options.follow_timeout)? {
        artifact_writer.append_event(&event)?;
        writeln!(output, "{}", serde_json::to_string(&event_as_json(&event))?)?;
    }

    Ok(())
}
```

- [ ] **Step 4: 重跑 `claude_observer` 测试**

Run: `cargo test -p prismtrace-host claude_observer -- --nocapture`

Expected: PASS，artifact writer、历史扫描和增量 follow 测试通过。

- [ ] **Step 5: Commit**

```bash
git add crates/prismtrace-host/src/claude_observer.rs
git commit -m "feat: persist claude transcript observer events"
```

### Task 4: 集成校验与 OpenSpec 回填

**Files:**
- Modify: `openspec/changes/add-claude-code-transcript-observer/tasks.md`
- Modify: `docs/stories/add-claude-code-transcript-observer/plan.md`

- [ ] **Step 1: 回填 OpenSpec tasks**

```md
## 1. CLI and source plumbing

- [x] 1.1 新增 `--claude-observe` / `--claude-transcript-root` 入口
- [x] 1.2 让 host session 通过 storage 驱动 `Claude Code` observer

## 2. Transcript and artifact pipeline

- [x] 2.1 补 transcript 文件发现、历史扫描和最小增量 follow
- [x] 2.2 为握手与事件补 `observer_events/claude-code` artifact 落盘

## 3. Verification

- [x] 3.1 覆盖 transcript 解析、未知类型和 follow 行为
- [x] 3.2 跑本地 CI 基线
```

- [ ] **Step 2: 跑定向测试**

Run: `cargo test -p prismtrace-host claude_observe_args claude_observer observer_channel_kind_label -- --nocapture`

Expected: PASS，CLI、observer channel、transcript observer 定向测试通过。

- [ ] **Step 3: 跑仓库 CI 基线**

Run: `cargo fmt --check`
Expected: PASS

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

Run: `cargo test --workspace`
Expected: PASS

Run: `cargo run -p prismtrace-host -- --discover`
Expected: PASS，并输出当前 host summary 和发现到的目标。

- [ ] **Step 4: 更新计划状态**

```md
## 状态

- [x] 计划内任务已完成
- [x] 本地基线已通过
```

- [ ] **Step 5: Commit**

```bash
git add openspec/changes/add-claude-code-transcript-observer/tasks.md docs/stories/add-claude-code-transcript-observer/plan.md
git commit -m "docs: finalize claude transcript observer plan"
```
