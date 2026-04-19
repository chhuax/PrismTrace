# Iteration 4 Node CLI Real Attach Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `--attach <pid>` 收口为真实前台持续采集入口，并用真实 Node inspector 后端替换 `NodeInstrumentationRuntime` 占位实现，让纯 Node CLI 目标可以在不重启的前提下被 attach 并捕获至少一条真实请求。

状态：已完成（已合并）

**Architecture:** Host 仍然由 Rust 驱动 attach、inspector 建连、后台 WebSocket worker、消息桥接和前台消费循环；目标进程内继续执行现有 `bootstrap.js` probe，只把 emitter 抽象成可在 `stdout` 与 inspector bridge 间切换，并暴露一个可被 inspector 触发的 detach helper。`attach.rs` 和 `request_capture.rs` 尽量保持职责不变，只替换“怎么把 probe 放进去”和“attach 后是否持续消费事件”。

**Tech Stack:** Rust workspace、Node.js bootstrap probe、blocking inspector HTTP/WebSocket client、`node --test`、`cargo test`

---

## 文件结构

- 修改：`crates/prismtrace-host/Cargo.toml`
  - 为真实 inspector runtime 增加最小依赖。
- 修改：`crates/prismtrace-host/probe/bootstrap.js`
  - 抽象 probe emitter，兼容 `stdout` 和 inspector bridge。
- 修改：`crates/prismtrace-host/probe/bootstrap.test.js`
  - 验证 emitter fallback 与 bridge 模式消息格式不变。
- 修改：`crates/prismtrace-host/src/runtime.rs`
  - 让 `NodeInstrumentationRuntime` 成为真实实现，并保留 `ScriptedInstrumentationRuntime` 测试替身。
- 修改：`crates/prismtrace-host/src/attach.rs`
  - 暴露 attach 成功后的 listener 消费路径，保证前台持续采集可复用现有状态机。
- 修改：`crates/prismtrace-host/src/lib.rs`
  - 补一个复用 discovery/readiness/attach 的前台 attach 入口辅助函数。
- 修改：`crates/prismtrace-host/src/main.rs`
  - 把 `--attach` 从脚本化快照输出切到真实前台持续采集入口。
- 修改：`crates/prismtrace-host/src/request_capture.rs`
  - 只做必要的小改动，让前台循环和 Ctrl+C 清理路径更容易组合。
- 新建：`crates/prismtrace-host/tests/node_cli_attach.rs`
  - 用真实 Node CLI 目标做黑盒验收测试。

### Task 1: 给 bootstrap probe 增加 bridge emitter

**Files:**
- Modify: `crates/prismtrace-host/probe/bootstrap.js`
- Modify: `crates/prismtrace-host/probe/bootstrap.test.js`

- [ ] **Step 1: 先写 bridge emitter 的失败测试**

在 `crates/prismtrace-host/probe/bootstrap.test.js` 追加两组测试，先固定“有 bridge 时优先 bridge、没有 bridge 时退回 stdout”以及“detach helper 可直接复用 dispose 逻辑”的行为：

```javascript
test('bootstrap prefers bridge emitter when available', function () {
  const emitted = [];
  const originalBridge = globalThis.__prismtraceEmit;
  const originalWrite = process.stdout.write;
  globalThis.__prismtraceEmit = function (line) {
    emitted.push(String(line));
  };
  process.stdout.write = function () {
    throw new Error('stdout should not be used when bridge emitter exists');
  };

  const { sendMessage, dispose } = freshModule();

  try {
    sendMessage({ type: 'heartbeat', timestamp_ms: 1 });
    assert.equal(emitted.length, 1);
    assert.match(emitted[0], /"type":"heartbeat"/);
  } finally {
    globalThis.__prismtraceEmit = originalBridge;
    process.stdout.write = originalWrite;
    dispose();
  }
});

test('bootstrap falls back to stdout when bridge emitter is absent', function () {
  const writes = [];
  const originalBridge = globalThis.__prismtraceEmit;
  const originalWrite = process.stdout.write;
  delete globalThis.__prismtraceEmit;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const { sendMessage, dispose } = freshModule();

  try {
    sendMessage({ type: 'heartbeat', timestamp_ms: 2 });
    assert.equal(writes.length, 1);
    assert.match(writes[0], /"type":"heartbeat"/);
  } finally {
    globalThis.__prismtraceEmit = originalBridge;
    process.stdout.write = originalWrite;
    dispose();
  }
});

test('bootstrap detach helper emits detach_ack and disposes hooks', function () {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const { installHooks, dispose } = freshModule();

  try {
    installHooks(['fetch']);
    assert.equal(typeof globalThis.__prismtraceDetach, 'function');
    globalThis.__prismtraceDetach();
    const observed = writes.find((chunk) => chunk.includes('"type":"detach_ack"'));
    assert.ok(observed, 'expected detach ack to be emitted');
  } finally {
    process.stdout.write = originalWrite;
    dispose();
    delete globalThis.__prismtraceDetach;
  }
});
```

- [ ] **Step 2: 运行 probe 测试，确认新测试先失败**

Run: `node --test crates/prismtrace-host/probe/bootstrap.test.js`

Expected: 至少新增的 bridge emitter 测试失败，因为当前 `sendMessage` 只会写 `process.stdout`。

- [ ] **Step 3: 在 probe 中实现 emitter 抽象**

把 `sendMessage` 拆成“构造 JSON 行”与“发送 JSON 行”两层，保留默认 stdout 行为，同时允许 inspector 运行时注入 bridge：

```javascript
  function emitLine(line) {
    if (typeof globalThis.__prismtraceEmit === 'function') {
      globalThis.__prismtraceEmit(line);
      return;
    }

    if (
      typeof process !== 'undefined' &&
      process.stdout &&
      typeof process.stdout.write === 'function'
    ) {
      process.stdout.write(line);
    }
  }

  function sendMessage(msg) {
    emitLine(JSON.stringify(msg) + '\n');
  }

  function triggerDetach() {
    sendMessage({ type: 'detach_ack', timestamp_ms: Date.now() });
    dispose();
  }
```

同时把测试辅助里导出的对象补成：

```javascript
  return {
    detectRuntimes,
    installHooks,
    dispose,
    sendMessage,
    triggerDetach,
  };
```

并把 autorun 路径里的 stdin detach 分支改成：

```javascript
  globalThis.__prismtraceDetach = triggerDetach;

  if (msg && msg.type === 'detach') {
    triggerDetach();
  }
```

- [ ] **Step 4: 重新运行 probe 测试，确认 stdout 模式和 bridge 模式都通过**

Run: `node --test crates/prismtrace-host/probe/bootstrap.test.js`

Expected: PASS，已有 hook 测试、bridge emitter 测试和 detach helper 测试同时通过。

- [ ] **Step 5: 提交 probe emitter 改动**

```bash
git add crates/prismtrace-host/probe/bootstrap.js crates/prismtrace-host/probe/bootstrap.test.js
git commit -m "feat: add bridge emitter support for bootstrap probe"
```

### Task 2: 落地真实 `NodeInstrumentationRuntime`

**Files:**
- Modify: `crates/prismtrace-host/Cargo.toml`
- Modify: `crates/prismtrace-host/src/runtime.rs`

- [ ] **Step 1: 先为端点发现和错误映射写失败测试**

在 `crates/prismtrace-host/src/runtime.rs` 的测试模块旁边追加纯函数级测试，先把最关键的解析与错误边界固定住：

```rust
#[test]
fn parse_lsof_listener_port_extracts_localhost_port() {
    let output = "\
COMMAND   PID   USER   FD   TYPE DEVICE SIZE/OFF NODE NAME\n\
node    80309 test   14u  IPv4 0x0      0t0  TCP 127.0.0.1:9229 (LISTEN)\n";

    assert_eq!(parse_listener_port(output), Some(9229));
}

#[test]
fn parse_websocket_debugger_url_reads_json_list_payload() {
    let payload = r#"[{"webSocketDebuggerUrl":"ws://127.0.0.1:9229/uuid"}]"#;

    assert_eq!(
        parse_websocket_debugger_url(payload).as_deref(),
        Some("ws://127.0.0.1:9229/uuid")
    );
}

#[test]
fn inspector_bridge_reader_yields_complete_lines() {
    let (mut writer, reader) = InspectorBridge::new();
    writer.push_line(r#"{"type":"heartbeat","timestamp_ms":1}"#);
    writer.push_line(r#"{"type":"detach_ack","timestamp_ms":2}"#);

    let mut reader = reader;
    let mut buf = String::new();
    assert!(reader.read_line(&mut buf).unwrap() > 0);
    assert!(buf.contains("\"heartbeat\""));
}
```

- [ ] **Step 2: 运行聚焦测试，确认它们先失败**

Run: `cargo test -p prismtrace-host parse_lsof_listener_port_extracts_localhost_port -- --exact`

Expected: FAIL，原因是 `parse_listener_port`、`parse_websocket_debugger_url` 和 `InspectorBridge` 还不存在。

- [ ] **Step 3: 增加 inspector runtime 所需依赖**

在 `crates/prismtrace-host/Cargo.toml` 加入最小同步依赖：

```toml
[dependencies]
prismtrace-core = { path = "../prismtrace-core" }
prismtrace-storage = { path = "../prismtrace-storage" }
serde_json = "1"
libc = "0.2"
tungstenite = "0.24"
ureq = "2"
```

- [ ] **Step 4: 先实现纯函数与 bridge reader**

先在 `crates/prismtrace-host/src/runtime.rs` 加上纯函数与 bridge reader，让单元测试先可通过，再接真实 inspector：

```rust
fn parse_listener_port(output: &str) -> Option<u16> {
    output
        .lines()
        .find_map(|line| line.split("127.0.0.1:").nth(1))
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|port| port.parse::<u16>().ok())
}

fn parse_websocket_debugger_url(payload: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    value
        .as_array()?
        .iter()
        .find_map(|entry| entry.get("webSocketDebuggerUrl")?.as_str().map(str::to_string))
}

struct InspectorBridgeWriter {
    tx: std::sync::mpsc::Sender<String>,
}

impl InspectorBridgeWriter {
    fn push_line(&mut self, line: impl Into<String>) {
        let _ = self.tx.send(format!("{}\n", line.into()));
    }
}

struct InspectorBridgeReader {
    rx: std::sync::mpsc::Receiver<String>,
    pending: std::io::Cursor<Vec<u8>>,
}
```

- [ ] **Step 5: 实现真实 `NodeInstrumentationRuntime::inject_probe`**

保持 trait 入口不变，但让 `NodeInstrumentationRuntime` 真正做这条同步链路，并在内部保存一个按 pid 建立的 inspector control map：

```rust
impl InstrumentationRuntime for NodeInstrumentationRuntime {
    fn inject_probe(
        &self,
        pid: u32,
        probe_script: &str,
    ) -> Result<Box<dyn BufRead + Send>, InstrumentationError> {
        send_sigusr1(pid)?;
        let port = discover_inspector_port(pid)?;
        let ws_url = fetch_websocket_debugger_url(port)?;
        let mut session = InspectorSession::connect(&ws_url)?;
        session.assert_pid(pid)?;

        let (mut bridge_writer, bridge_reader) = InspectorBridge::new();
        session.install_bridge()?;
        session.eval_bootstrap(probe_script)?;
        let control = session.spawn_bridge_worker(bridge_writer)?;
        self.controls.lock().unwrap().insert(pid, control);

        Ok(Box::new(bridge_reader))
    }

    fn send_detach_signal(&self, pid: u32) -> Result<(), InstrumentationError> {
        let control = self.controls.lock().unwrap().remove(&pid);
        match control {
            Some(control) => control.request_detach(),
            None => Err(InstrumentationError {
                kind: InstrumentationErrorKind::DetachFailed,
                message: format!("no active inspector control for pid {pid}"),
            }),
        }
    }
}
```

实现时注意：

- `send_sigusr1` 用 `libc::kill`
- `discover_inspector_port` 用 `lsof -nP -a -p <pid> -iTCP -sTCP:LISTEN`
- `fetch_websocket_debugger_url` 调 `http://127.0.0.1:<port>/json/list`
- inspector 会话建立后先 `Runtime.evaluate("process.pid")`
- bootstrap 装载前先注册 `globalThis.__prismtraceEmit = (line) => console.log("__PRISMTRACE__" + line)`
- 同一段装载逻辑里也注册 `globalThis.__prismtraceDetach`，这样后续 detach 可以通过 `Runtime.evaluate("__prismtraceDetach()")` 触发
- bridge 只转发带 `__PRISMTRACE__` 前缀的 console 输出
- WebSocket 读循环放到后台线程里持续跑，`inject_probe` 自己只返回 `BufRead` bridge，不要在前台阻塞到会话结束

- [ ] **Step 6: 运行 runtime 聚焦测试，再跑 host 全量单测**

Run: `cargo test -p prismtrace-host parse_lsof_listener_port_extracts_localhost_port parse_websocket_debugger_url_reads_json_list_payload inspector_bridge_reader_yields_complete_lines`

Expected: PASS

Run: `cargo test -p prismtrace-host`

Expected: PASS，现有 `ScriptedInstrumentationRuntime` 和 `LiveAttachBackend` 状态机测试不回退。

- [ ] **Step 7: 提交真实 runtime 改动**

```bash
git add crates/prismtrace-host/Cargo.toml crates/prismtrace-host/src/runtime.rs
git commit -m "feat: implement node inspector instrumentation runtime"
```

### Task 3: 把 `--attach` 切到前台持续采集模式

**Files:**
- Modify: `crates/prismtrace-host/src/lib.rs`
- Modify: `crates/prismtrace-host/src/attach.rs`
- Modify: `crates/prismtrace-host/src/request_capture.rs`
- Modify: `crates/prismtrace-host/src/main.rs`

- [ ] **Step 1: 先写 CLI attach 行为的失败测试**

在 `crates/prismtrace-host/src/main.rs` 的测试模块里追加一条参数解析测试，并在 `crates/prismtrace-host/src/lib.rs` 里给前台入口补一个最小单测：

```rust
#[test]
fn attach_pid_arg_still_parses_pid_for_foreground_mode() {
    let args = vec!["--attach".to_string(), "321".to_string()];
    assert_eq!(attach_pid_arg(&args).unwrap(), Some(321));
}

#[test]
fn foreground_attach_rejects_missing_target_pid() {
    let result = run_foreground_attach_session(
        &bootstrap(std::env::temp_dir()).unwrap(),
        &FakeProcessSampleSource::empty(),
        NodeInstrumentationRuntime,
        999_999,
        &mut Vec::new(),
    );

    assert!(result.is_err());
}
```

- [ ] **Step 2: 运行聚焦测试，确认前台入口函数先不存在**

Run: `cargo test -p prismtrace-host foreground_attach_rejects_missing_target_pid -- --exact`

Expected: FAIL，因为 `run_foreground_attach_session` 还不存在。

- [ ] **Step 3: 在 `lib.rs` 增加前台 attach 组合入口**

把现有 discovery/readiness/attach 组合成一个真正可复用的前台入口：

```rust
pub fn run_foreground_attach_session(
    result: &BootstrapResult,
    source: &impl ProcessSampleSource,
    runtime: impl crate::runtime::InstrumentationRuntime,
    pid: u32,
    output: &mut impl std::io::Write,
) -> io::Result<()> {
    let discovered_targets = discover_targets(source)?;
    let readiness_results = evaluate_targets(&discovered_targets);
    let readiness = readiness_results
        .iter()
        .find(|readiness| readiness.target.pid == pid)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("pid {pid} not found")))?;

    let mut controller = AttachController::new(crate::attach::LiveAttachBackend::new(runtime));
    let session = controller
        .attach(readiness)
        .map_err(|failure| io::Error::other(failure.summary()))?;

    writeln!(output, "{}", session.summary())?;
    let listener = controller
        .backend_listener_mut()
        .ok_or_else(|| io::Error::other("attach succeeded without ipc listener"))?;

    crate::request_capture::consume_probe_events(
        &result.storage,
        &session.target,
        listener,
        output,
    )
}
```

实现时如果 `AttachController` 当前拿不到 backend，可在 `attach.rs` 为 `AttachController<LiveAttachBackend<_>>` 补一个最小的 `backend_listener_mut()` 专用帮助函数，但不要把 request capture 逻辑塞回 `attach.rs`。

- [ ] **Step 4: 让 `main.rs` 的 `--attach` 切到真实运行时**

把当前脚本化分支：

```rust
let snapshot = prismtrace_host::collect_attach_snapshot(
    &result,
    &prismtrace_host::discovery::PsProcessSampleSource,
    prismtrace_host::attach::ScriptedAttachBackend::ready(),
    pid,
)?;
println!("{}", prismtrace_host::attach_snapshot_report(&snapshot));
```

改成：

```rust
prismtrace_host::run_foreground_attach_session(
    &result,
    &prismtrace_host::discovery::PsProcessSampleSource,
    prismtrace_host::runtime::NodeInstrumentationRuntime,
    pid,
    &mut std::io::stdout(),
)?;
```

同时保留 `--discover`、`--readiness`、`--detach` 和 `--attach-status` 现有入口，不顺手重构别的命令面。

- [ ] **Step 5: 跑 host 单测与一个本地手工 smoke**

Run: `cargo test -p prismtrace-host`

Expected: PASS

Run: `cargo run -p prismtrace-host -- --attach 999999`

Expected: 以结构化错误退出，信息类似 `no discovered target with pid 999999`，而不是 panic。

- [ ] **Step 6: 提交前台 attach 入口改动**

```bash
git add crates/prismtrace-host/src/lib.rs crates/prismtrace-host/src/attach.rs crates/prismtrace-host/src/request_capture.rs crates/prismtrace-host/src/main.rs
git commit -m "feat: run attach as foreground capture session"
```

### Task 4: 用真实 Node CLI 目标做黑盒验收

**Files:**
- Create: `crates/prismtrace-host/tests/node_cli_attach.rs`

- [ ] **Step 1: 先写真实 Node CLI attach 的失败测试**

创建 `crates/prismtrace-host/tests/node_cli_attach.rs`，用临时 Node 脚本和本地 HTTP 服务器做黑盒验收：

```rust
#[test]
fn attach_to_running_node_cli_captures_one_request_artifact() {
    let fixture = spawn_node_fixture(
        r#"
        setInterval(async () => {
          await fetch(process.env.PRISMTRACE_TARGET_URL, {
            method: 'POST',
            headers: {
              authorization: 'Bearer sk-test',
              'content-type': 'application/json',
            },
            body: JSON.stringify({ model: 'gpt-4.1', input: 'hello' }),
          });
        }, 250);
        "#,
    );

    let app = prismtrace_host::bootstrap(fixture.workspace_root()).unwrap();
    let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let output_for_thread = output.clone();

    let join = std::thread::spawn(move || {
        let mut guard = output_for_thread.lock().unwrap();
        prismtrace_host::run_foreground_attach_session(
            &app,
            &fixture.process_source(),
            prismtrace_host::runtime::NodeInstrumentationRuntime,
            fixture.pid(),
            &mut *guard,
        )
    });

    fixture.wait_for_first_artifact();
    fixture.stop_target();
    join.join().unwrap().unwrap();

    let stdout = String::from_utf8(output.lock().unwrap().clone()).unwrap();
    assert!(stdout.contains("[attached]"));
    assert!(stdout.contains("[captured]"));
    assert!(fixture.requests_dir().read_dir().unwrap().next().is_some());
}
```

- [ ] **Step 2: 跑单条集成测试，确认它先失败**

Run: `cargo test -p prismtrace-host --test node_cli_attach -- --nocapture`

Expected: FAIL，原因可能是 `run_foreground_attach_session` 还未真正连上 inspector，或真实 runtime 还未把 probe 消息桥回 host。

- [ ] **Step 3: 补齐测试夹具和退出控制**

在同一个测试文件里补足最小夹具：

```rust
fn spawn_node_fixture(script_body: &str) -> NodeFixture {
    let temp = make_temp_fixture_dir();
    let server = MockLlmServer::spawn().unwrap();
    let script_path = temp.path().join("target.js");
    std::fs::write(&script_path, script_body).unwrap();

    let child = std::process::Command::new("node")
        .arg(&script_path)
        .env("PRISMTRACE_TARGET_URL", server.url())
        .spawn()
        .unwrap();

    NodeFixture::new(temp, child, server)
}
```

实现时注意：

- 用本地 `TcpListener` 模拟一个最小 LLM HTTP 端点
- 在测试结束时显式 kill child，避免残留进程
- `make_temp_fixture_dir()` 用 `std::env::temp_dir()` + 时间戳/进程号拼出唯一目录，避免额外引入 `tempfile`
- 只断言“至少出现一个 artifact”和“CLI 出现 `[captured]`”，不要把 provider 细节断得过死

- [ ] **Step 4: 跑集成测试与全量 workspace 测试**

Run: `cargo test -p prismtrace-host --test node_cli_attach -- --nocapture`

Expected: PASS，真实 Node CLI 目标可以在已启动状态下被 attach，并产出至少一条 request artifact。

Run: `cargo test`

Expected: PASS，全 workspace 测试无回退。

- [ ] **Step 5: 做一次手工黑盒验收**

Run:

```bash
tmpdir=$(mktemp -d)
cat > "$tmpdir/target.js" <<'EOF'
setInterval(async () => {
  await fetch(process.env.PRISMTRACE_TARGET_URL, {
    method: 'POST',
    headers: {
      authorization: 'Bearer sk-test',
      'content-type': 'application/json',
    },
    body: JSON.stringify({ model: 'gpt-4.1', input: 'hello' }),
  });
}, 1000);
EOF
node "$tmpdir/target.js" &
target_pid=$!
cargo run -p prismtrace-host -- --attach "$target_pid"
```

Expected:

- 终端先打印 `[attached] ...`
- 随后打印至少一条 `[captured] ...`
- `.prismtrace/state/artifacts/requests/` 下新增 JSON artifact

- [ ] **Step 6: 提交黑盒验收测试**

```bash
git add crates/prismtrace-host/tests/node_cli_attach.rs
git commit -m "test: verify real node cli attach capture flow"
```

## 自检清单

- 设计要求里的两项主目标都有任务覆盖：
  - 前台持续采集模式：Task 3、Task 4
  - 真实 `NodeInstrumentationRuntime`：Task 2、Task 4
- 无 `TBD`、`TODO` 或“后面再补”式占位描述
- 已明确 Rust host / JS probe 分工，避免执行时误把 host 改造成 Node 进程
