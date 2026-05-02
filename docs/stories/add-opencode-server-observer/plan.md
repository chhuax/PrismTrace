# Opencode Server Observer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `opencode` 升级为和 `Codex` 并列的正式 observer source，打通 `health + session list + export/message + global event + artifact 落盘` 的最小 CLI 采集闭环。

**Architecture:** 继续复用 `observer.rs` 的统一抽象，不走 attach 语义。`main.rs` 负责参数入口，`lib.rs` 负责 host session 调度，`opencode_observer.rs` 负责 HTTP 握手、快照归一化、实时事件归一化和 artifact 写入。

**Tech Stack:** Rust, `ureq`, `serde_json`, inline unit tests, PrismTrace local artifact storage

---

### Task 1: CLI 入口与 host session plumbing

**Files:**
- Modify: `crates/prismtrace-host/src/main.rs`
- Test: `crates/prismtrace-host/src/main.rs` inline tests

- [ ] **Step 1: 先写 CLI 参数解析的失败测试**

```rust
#[test]
fn opencode_observe_args_returns_none_when_flag_is_missing() {
    let args = vec!["--discover".to_string()];

    assert!(
        opencode_observe_args(&args)
            .expect("parse should succeed")
            .is_none()
    );
}

#[test]
fn opencode_observe_args_returns_default_options_without_url() {
    let args = vec!["--opencode-observe".to_string()];
    let options = opencode_observe_args(&args)
        .expect("parse should succeed")
        .expect("options should exist");

    assert_eq!(options.base_url, "http://127.0.0.1:4096");
}

#[test]
fn opencode_observe_args_parses_explicit_base_url() {
    let args = vec![
        "--opencode-observe".to_string(),
        "--opencode-url".to_string(),
        "http://127.0.0.1:4999".to_string(),
    ];
    let options = opencode_observe_args(&args)
        .expect("parse should succeed")
        .expect("options should exist");

    assert_eq!(options.base_url, "http://127.0.0.1:4999");
}
```

- [ ] **Step 2: 跑参数解析测试，确认当前实现还不支持 `opencode` 正式入口**

Run: `cargo test -p prismtrace-host opencode_observe_args -- --nocapture`

Expected: FAIL，提示 `opencode_observe_args` 未定义，或 `--opencode-observe` 未被 `main.rs` 解析。

- [ ] **Step 3: 实现 `main.rs` 的最小入口接线**

```rust
if let Some(options) = opencode_observe_args(&args)? {
    let mut stdout = std::io::stdout().lock();
    prismtrace_host::run_opencode_observer_session(&result, options, &mut stdout)?;
    return Ok(());
}

fn opencode_observe_args(
    args: &[String],
) -> std::io::Result<Option<prismtrace_host::opencode_observer::OpencodeObserverOptions>> {
    if !args.iter().any(|arg| arg == "--opencode-observe") {
        return Ok(None);
    }

    let base_url = arg_value(args, "--opencode-url")
        .map(str::to_string)
        .unwrap_or_else(|| "http://127.0.0.1:4096".to_string());

    Ok(Some(
        prismtrace_host::opencode_observer::OpencodeObserverOptions {
            base_url,
            ..Default::default()
        },
    ))
}
```

- [ ] **Step 4: 重跑参数解析测试**

Run: `cargo test -p prismtrace-host opencode_observe_args -- --nocapture`

Expected: PASS，`main.rs` inline tests 全部通过。

- [ ] **Step 5: Commit**

```bash
git add crates/prismtrace-host/src/main.rs
git commit -m "feat: add opencode observer cli entry"
```

### Task 2: 握手与 artifact 落盘骨架

**Files:**
- Modify: `crates/prismtrace-host/src/opencode_observer.rs`
- Modify: `crates/prismtrace-host/src/lib.rs`
- Test: `crates/prismtrace-host/src/opencode_observer.rs` inline tests

- [ ] **Step 1: 先写握手落盘测试**

```rust
#[test]
fn opencode_observer_artifact_writer_persists_handshake_and_event() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = crate::bootstrap(&workspace_root)?;

    let handshake = ObserverHandshake {
        channel_kind: ObserverChannelKind::OpencodeServer,
        transport_label: "http://127.0.0.1:4096".into(),
        server_label: "opencode test".into(),
        raw_json: json!({ "version": "test" }),
    };
    let writer = OpencodeObserverArtifactWriter::create(&result.storage, &handshake)?;
    writer.append_event(&ObservedEvent {
        channel_kind: ObserverChannelKind::OpencodeServer,
        event_kind: ObservedEventKind::Thread,
        summary: "demo".into(),
        method: Some("GET /session".into()),
        thread_id: Some("session-1".into()),
        turn_id: None,
        item_id: None,
        timestamp: Some("1".into()),
        raw_json: json!({ "id": "session-1" }),
    })?;

    let artifact = std::fs::read_to_string(writer.artifact_path())?;
    assert!(artifact.contains("\"record_type\":\"handshake\""));
    assert!(artifact.contains("\"record_type\":\"event\""));
    Ok(())
}
```

- [ ] **Step 2: 跑 `opencode_observer` 测试，确认目前还没有 artifact writer**

Run: `cargo test -p prismtrace-host opencode_observer -- --nocapture`

Expected: FAIL，提示 `OpencodeObserverArtifactWriter`、`artifact_path` 或 `run_opencode_observer` 签名不匹配。

- [ ] **Step 3: 实现握手与 event artifact writer，并把 `run_opencode_observer` / `run_opencode_observer_session` 改成接收 storage**

```rust
pub fn run_opencode_observer(
    storage: &StorageLayout,
    output: &mut impl std::io::Write,
    options: OpencodeObserverOptions,
) -> io::Result<()> {
    let factory = OpencodeObserverFactory;
    let source = factory
        .build_sources(&options)?
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no opencode source available"))?;

    writeln!(
        output,
        "[opencode-observer] attempting {} via {}",
        source.channel_kind().label(),
        source.transport_label()
    )?;

    let mut session = source.connect()?;
    let handshake = session.initialize()?;
    let artifact_writer = OpencodeObserverArtifactWriter::create(storage, &handshake)?;

    writeln!(output, "{}", serde_json::to_string(&json!({
        "type": "opencode_observer_handshake",
        "channel": handshake.channel_kind.label(),
        "transport": handshake.transport_label,
        "server_label": handshake.server_label,
        "raw": handshake.raw_json,
    }))?)?;

    for event in session.collect_capability_events()? {
        artifact_writer.append_event(&event)?;
        writeln!(output, "{}", serde_json::to_string(&event_as_json(&event))?)?;
    }

    Ok(())
}
```

```rust
pub fn run_opencode_observer_session(
    result: &BootstrapResult,
    options: opencode_observer::OpencodeObserverOptions,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(output, "{}", startup_summary(result))?;
    opencode_observer::run_opencode_observer(&result.storage, output, options)
}
```

```rust
struct OpencodeObserverArtifactWriter {
    artifact_path: PathBuf,
}

impl OpencodeObserverArtifactWriter {
    fn create(storage: &StorageLayout, handshake: &ObserverHandshake) -> io::Result<Self> {
        let observer_dir = storage.artifacts_dir.join("observer_events").join("opencode");
        fs::create_dir_all(&observer_dir)?;

        let artifact_path = observer_dir.join(format!("{}-{}.jsonl", current_time_ms()?, std::process::id()));
        let writer = Self { artifact_path };
        writer.append_json_line(&json!({
            "record_type": "handshake",
            "channel": handshake.channel_kind.label(),
            "transport": handshake.transport_label,
            "server_label": handshake.server_label,
            "recorded_at_ms": current_time_ms()?,
            "raw_json": handshake.raw_json,
        }))?;
        Ok(writer)
    }
}
```

- [ ] **Step 4: 重跑 `opencode_observer` 测试**

Run: `cargo test -p prismtrace-host opencode_observer -- --nocapture`

Expected: PASS，默认 options、截断逻辑、artifact writer 测试通过。

- [ ] **Step 5: Commit**

```bash
git add crates/prismtrace-host/src/opencode_observer.rs crates/prismtrace-host/src/lib.rs
git commit -m "feat: persist opencode observer artifacts"
```

### Task 3: session/export/message 快照归一化

**Files:**
- Modify: `crates/prismtrace-host/src/opencode_observer.rs`
- Test: `crates/prismtrace-host/src/opencode_observer.rs` inline tests

- [ ] **Step 1: 为快照归一化先写聚焦测试**

```rust
#[test]
fn session_snapshot_maps_to_thread_event() {
    let event = normalize_session_event(&json!({
        "id": "session-1",
        "title": "Debug API",
        "directory": "/tmp/demo",
        "updated": 1714000000
    }));

    assert_eq!(event.event_kind, ObservedEventKind::Thread);
    assert_eq!(event.thread_id.as_deref(), Some("session-1"));
    assert!(event.summary.contains("Debug API"));
}

#[test]
fn message_part_maps_tool_parts_to_tool_events() {
    let events = normalize_message_parts(
        "session-1",
        &json!({
            "info": {
                "id": "turn-1",
                "role": "assistant",
                "time": { "created": 1714000001 }
            },
            "parts": [
                { "id": "part-1", "type": "text", "text": "hello" },
                { "id": "part-2", "type": "tool", "tool": "bash" }
            ]
        }),
    );

    assert_eq!(events[0].event_kind, ObservedEventKind::Item);
    assert_eq!(events[1].event_kind, ObservedEventKind::Tool);
    assert_eq!(events[1].item_id.as_deref(), Some("part-2"));
}
```

- [ ] **Step 2: 跑快照归一化测试**

Run: `cargo test -p prismtrace-host session_snapshot_maps_to_thread_event -- --nocapture`

Expected: FAIL，提示 `normalize_session_event` 尚不存在，或返回结构与断言不一致。

Run: `cargo test -p prismtrace-host message_part_maps_tool_parts_to_tool_events -- --nocapture`

Expected: FAIL，提示 `normalize_message_parts` 尚不存在，或工具 part 没有映射到 `Tool`。

- [ ] **Step 3: 把 `/session`、`/session/:id/export`、`/session/:id/message` 的解析收敛成显式 helper**

```rust
fn normalize_session_event(session: &Value) -> ObservedEvent {
    let session_id = session
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("unknown-session")
        .to_string();

    ObservedEvent {
        channel_kind: ObserverChannelKind::OpencodeServer,
        event_kind: ObservedEventKind::Thread,
        summary: session_summary(session),
        method: Some("GET /session".into()),
        thread_id: Some(session_id),
        turn_id: None,
        item_id: None,
        timestamp: session
            .get("updated")
            .and_then(Value::as_i64)
            .map(|value| value.to_string()),
        raw_json: session.clone(),
    }
}

fn normalize_message_parts(session_id: &str, message: &Value) -> Vec<ObservedEvent> {
    let info = message.get("info").cloned().unwrap_or(Value::Null);
    let role = info.get("role").and_then(Value::as_str).unwrap_or("unknown");
    let turn_id = info.get("id").and_then(Value::as_str).map(str::to_string);
    let timestamp = info
        .get("time")
        .and_then(|time| time.get("created"))
        .and_then(Value::as_i64)
        .map(|value| value.to_string());

    message
        .get("parts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|part| {
            let part_type = part.get("type").and_then(Value::as_str).unwrap_or("unknown");
            let event_kind = if part_type == "tool" {
                ObservedEventKind::Tool
            } else {
                ObservedEventKind::Item
            };
            let summary = match part_type {
                "text" | "reasoning" => format!(
                    "{role} {part_type}: {}",
                    truncate(part.get("text").and_then(Value::as_str).unwrap_or(""), 120)
                ),
                "tool" => format!(
                    "{role} tool: {}",
                    part.get("tool").and_then(Value::as_str).unwrap_or("unknown tool")
                ),
                other => format!("{role} {other}"),
            };

            ObservedEvent {
                channel_kind: ObserverChannelKind::OpencodeServer,
                event_kind,
                summary,
                method: Some("GET /session/:id/message".into()),
                thread_id: Some(session_id.to_string()),
                turn_id: turn_id.clone(),
                item_id: part.get("id").and_then(Value::as_str).map(str::to_string),
                timestamp: timestamp.clone(),
                raw_json: json!({ "info": info, "part": part }),
            }
        })
        .collect()
}
```

- [ ] **Step 4: 重跑快照归一化与 observer 测试**

Run: `cargo test -p prismtrace-host opencode_observer -- --nocapture`

Expected: PASS，session/message/tool 映射测试和现有测试一起通过。

- [ ] **Step 5: Commit**

```bash
git add crates/prismtrace-host/src/opencode_observer.rs
git commit -m "feat: normalize opencode snapshot events"
```

### Task 4: `global/event` 实时事件与未知事件回退

**Files:**
- Modify: `crates/prismtrace-host/src/opencode_observer.rs`
- Test: `crates/prismtrace-host/src/opencode_observer.rs` inline tests

- [x] **Step 1: 先写实时事件映射测试**

```rust
#[test]
fn global_event_maps_permission_to_approval() {
    let event = normalize_global_event(&json!({
        "type": "permission.updated",
        "sessionID": "session-1",
        "message": "waiting for approval",
        "time": 1714000002
    }));

    assert_eq!(event.event_kind, ObservedEventKind::Approval);
    assert_eq!(event.thread_id.as_deref(), Some("session-1"));
}

#[test]
fn global_event_falls_back_to_unknown() {
    let event = normalize_global_event(&json!({
        "type": "mystery.event",
        "sessionID": "session-2"
    }));

    assert_eq!(event.event_kind, ObservedEventKind::Unknown);
    assert!(event.summary.contains("mystery.event"));
}
```

- [x] **Step 2: 跑实时事件映射测试**

Run: `cargo test -p prismtrace-host global_event_ -- --nocapture`

Expected: FAIL，提示 `normalize_global_event` 尚不存在，或未知事件没有回退到 `Unknown`。

- [x] **Step 3: 实现 `global/event` 拉取与保守归一化**

```rust
fn normalize_global_event(raw: &Value) -> ObservedEvent {
    let event_type = raw.get("type").and_then(Value::as_str).unwrap_or("unknown");
    let event_kind = if event_type.contains("permission") || event_type.contains("approval") {
        ObservedEventKind::Approval
    } else if event_type.contains("agent") {
        ObservedEventKind::Agent
    } else if event_type.contains("mcp") {
        ObservedEventKind::Mcp
    } else if event_type.contains("provider") {
        ObservedEventKind::Provider
    } else if event_type.contains("plugin") {
        ObservedEventKind::Plugin
    } else if event_type.contains("command") {
        ObservedEventKind::Command
    } else if event_type.contains("app") {
        ObservedEventKind::App
    } else if event_type.contains("tool") {
        ObservedEventKind::Tool
    } else {
        ObservedEventKind::Unknown
    };

    ObservedEvent {
        channel_kind: ObserverChannelKind::OpencodeServer,
        event_kind,
        summary: raw
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("opencode event: {event_type}")),
        method: Some("GET /global/event".into()),
        thread_id: raw.get("sessionID").and_then(Value::as_str).map(str::to_string),
        turn_id: raw.get("messageID").and_then(Value::as_str).map(str::to_string),
        item_id: raw.get("partID").and_then(Value::as_str).map(str::to_string),
        timestamp: raw.get("time").and_then(Value::as_i64).map(|value| value.to_string()),
        raw_json: raw.clone(),
    }
}

fn next_event(&mut self, _timeout: Duration) -> io::Result<Option<ObservedEvent>> {
    if let Some(event) = self.pending.pop_front() {
        return Ok(Some(event));
    }

    let payload = self.get_json("/global/event")?;
    for raw in payload.as_array().into_iter().flatten() {
        self.pending.push_back(normalize_global_event(raw));
    }

    Ok(self.pending.pop_front())
}
```

- [x] **Step 4: 重跑 `opencode_observer` 测试**

Run: `cargo test -p prismtrace-host opencode_observer -- --nocapture`

Expected: PASS，`global/event` 映射测试、未知事件回退测试和已有测试全部通过。

- [ ] **Step 5: Commit**

```bash
git add crates/prismtrace-host/src/opencode_observer.rs
git commit -m "feat: collect opencode global events"
```

### Task 5: OpenSpec 任务同步与本地基线验证

**Files:**
- Modify: `openspec/changes/add-opencode-server-observer/tasks.md`
- Modify: `docs/stories/add-opencode-server-observer/plan.md`
- Test: workspace verification commands

- [x] **Step 1: 把 OpenSpec tasks 同步到本轮实施顺序**

```md
## 1. CLI and source plumbing

- [ ] 1.1 新增 `--opencode-observe` / `--opencode-url`
- [ ] 1.2 让 host session 通过 storage 驱动 `opencode` observer

## 2. Snapshot and artifact pipeline

- [ ] 2.1 为握手与事件补 `observer_events/opencode` artifact 落盘
- [ ] 2.2 归一化 session / export / message 快照

## 3. Realtime events and verification

- [ ] 3.1 补 `global/event` 的保守映射与 `unknown` 回退
- [ ] 3.2 为协议层与 CLI 层补聚焦测试
- [ ] 3.3 跑本地 CI 基线并做 live `opencode` 验证
```

- [x] **Step 2: 运行格式与静态检查**

Run: `cargo fmt --check`

Expected: PASS，无格式差异输出。

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS，无 warning。

- [x] **Step 3: 运行测试与 discover 基线**

Run: `cargo test --workspace`

Expected: PASS，workspace 全量测试通过。

Run: `cargo run -p prismtrace-host -- --discover`

Expected: PASS，输出 startup summary 和 discovered targets。

- [ ] **Step 4: live `opencode` 手工验证**

当前状态：本机 `127.0.0.1:4096` 没有 `opencode` server 监听，因此这一项尚未执行通过。

Run: `cargo run -p prismtrace-host -- --opencode-observe --opencode-url http://127.0.0.1:4096`

Expected: 输出 startup summary、一条 `opencode_observer_handshake`、至少一批 `opencode_observer_event`，并在 `.prismtrace/artifacts/observer_events/opencode/` 生成 `.jsonl` 文件。

- [ ] **Step 5: Commit**

```bash
git add openspec/changes/add-opencode-server-observer/tasks.md docs/stories/add-opencode-server-observer/plan.md
git commit -m "docs: finalize opencode observer implementation plan"
```
