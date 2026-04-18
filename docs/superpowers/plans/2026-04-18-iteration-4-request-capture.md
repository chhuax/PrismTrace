# 迭代 4：请求采集实现计划

> **给执行代理的要求：** 实施本计划时，必须使用 `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans`。以下步骤使用复选框 `- [ ]` 语法进行跟踪。

**目标：** 构建第一版前台请求采集循环，在 attach 到目标进程后观测高置信度模型 HTTP 请求、打印单行 CLI 摘要，并把原始请求 payload 写入 `.prismtrace/state/artifacts/requests/`。

**架构：** 保持 probe 轻量、让 host 承担策略判断。probe 通过 `IpcMessage::HttpRequestObserved` 发出原始 HTTP 请求事实，由 Rust host 判断它是否属于类模型请求、写入 artifact，并渲染摘要。当前 `NodeInstrumentationRuntime` 仍然是线上运行时落地的阻塞点，所以这份计划先用单元测试和 `LiveAttachBackend<ScriptedInstrumentationRuntime>` 验证新链路，而不是在这里引入新的注入后端设计。

**技术栈：** Rust workspace（`prismtrace-core`、`prismtrace-host`、`prismtrace-storage`）、`prismtrace-core` 中已有的 `serde` IPC 枚举、Node.js probe 脚本与 `node:test` 单元测试。

---

## 文件结构

- 修改：`crates/prismtrace-core/src/lib.rs`
  新增 `HttpHeader` 和 `IpcMessage::HttpRequestObserved`，并补充协议往返测试。
- 修改：`crates/prismtrace-host/probe/bootstrap.js`
  把现有 no-op 包装器改成透明请求观察器，在不改变请求行为的前提下发出按行分隔的 JSON。
- 修改：`crates/prismtrace-host/probe/bootstrap.test.js`
  覆盖请求观察消息发送、hook 安装幂等性和非文本 body 的安全性。
- 新建：`crates/prismtrace-host/src/request_capture.rs`
  负责 host 侧的 provider 识别、artifact 落盘、CLI 摘要渲染和前台 IPC 消费循环。
- 修改：`crates/prismtrace-host/Cargo.toml`
  为 host crate 增加 `serde_json`，用于 artifact 序列化。
- 修改：`crates/prismtrace-host/src/lib.rs`
  导出新的 `request_capture` 模块。
- 修改：`crates/prismtrace-host/src/attach.rs`
  增加 bootstrap 后继续消费 probe IPC 的能力，而不是在 attach 成功后立即停止。
- 修改：`crates/prismtrace-host/src/main.rs`
  把一次性的脚本化 `--attach` 演示入口改成前台采集路径；测试中使用 `LiveAttachBackend<ScriptedInstrumentationRuntime>`，生产接线中使用 `LiveAttachBackend<NodeInstrumentationRuntime>`。

## 任务 1：扩展“已观察 HTTP 请求”的 IPC 协议

**涉及文件：**
- 修改：`crates/prismtrace-core/src/lib.rs`
- 测试：`crates/prismtrace-core/src/lib.rs`

- [ ] **步骤 1：先写新 IPC 消息的失败测试**

把下面这些测试加到 `crates/prismtrace-core/src/lib.rs` 里现有 `IpcMessage` 测试附近：

```rust
    #[test]
    fn ipc_message_http_request_observed_round_trip() {
        let msg = IpcMessage::HttpRequestObserved {
            hook_name: "fetch".into(),
            method: "POST".into(),
            url: "https://api.openai.com/v1/responses".into(),
            headers: vec![
                HttpHeader {
                    name: "authorization".into(),
                    value: "Bearer sk-test".into(),
                },
                HttpHeader {
                    name: "content-type".into(),
                    value: "application/json".into(),
                },
            ],
            body_text: Some(r#"{"model":"gpt-4.1","input":"hello"}"#.into()),
            timestamp_ms: 1_714_000_003_000,
        };

        let line = msg.to_json_line();
        let parsed = IpcMessage::from_json_line(&line).expect("should parse request event");

        assert_eq!(parsed, msg);
    }

    #[test]
    fn ipc_message_http_request_observed_parses_without_body() {
        let line = r#"{"type":"http_request_observed","hook_name":"http","method":"GET","url":"https://openrouter.ai/api/v1/chat/completions","headers":[],"body_text":null,"timestamp_ms":9}"#;

        let parsed = IpcMessage::from_json_line(line).expect("should parse request without body");

        assert_eq!(
            parsed,
            IpcMessage::HttpRequestObserved {
                hook_name: "http".into(),
                method: "GET".into(),
                url: "https://openrouter.ai/api/v1/chat/completions".into(),
                headers: vec![],
                body_text: None,
                timestamp_ms: 9,
            }
        );
    }
```

- [ ] **步骤 2：运行 core 测试，确认它们先失败**

运行：`cargo test -p prismtrace-core ipc_message_http_request_observed_round_trip -- --exact`

预期：因为 `HttpHeader` 和 `IpcMessage::HttpRequestObserved` 还不存在，测试会以编译错误失败。

- [ ] **步骤 3：补上最小协议类型和枚举变体**

在 `crates/prismtrace-core/src/lib.rs` 中加入这些内容：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcMessage {
    Heartbeat {
        timestamp_ms: u64,
    },
    BootstrapReport {
        installed_hooks: Vec<String>,
        failed_hooks: Vec<String>,
        timestamp_ms: u64,
    },
    HttpRequestObserved {
        hook_name: String,
        method: String,
        url: String,
        headers: Vec<HttpHeader>,
        body_text: Option<String>,
        timestamp_ms: u64,
    },
    DetachAck {
        timestamp_ms: u64,
    },
}
```

同时更新测试模块中的 `use super::{ ... }` 列表，把 `HttpHeader` 也导入进来。

- [ ] **步骤 4：运行聚焦的 core 测试，确认它们通过**

运行：`cargo test -p prismtrace-core ipc_message_http_request_observed_round_trip ipc_message_http_request_observed_parses_without_body`

预期：两个新测试都通过。

- [ ] **步骤 5：提交协议改动**

```bash
git add crates/prismtrace-core/src/lib.rs
git commit -m "feat: add observed http request ipc message"
```

## 任务 2：让 probe hook 发出请求观察事件

**涉及文件：**
- 修改：`crates/prismtrace-host/probe/bootstrap.js`
- 修改：`crates/prismtrace-host/probe/bootstrap.test.js`
- 测试：`crates/prismtrace-host/probe/bootstrap.test.js`

- [ ] **步骤 1：先写请求观察消息发送的失败测试**

把下面这些测试追加到 `crates/prismtrace-host/probe/bootstrap.test.js`：

```javascript
test('fetch hook emits http_request_observed for JSON request bodies', async function () {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const originalFetch = globalThis.fetch;
  globalThis.fetch = async function fakeFetch() {
    return { ok: true, status: 200 };
  };

  const { installHooks, dispose } = freshModule();

  try {
    installHooks(['fetch']);

    await globalThis.fetch('https://api.openai.com/v1/responses', {
      method: 'POST',
      headers: { authorization: 'Bearer sk-test', 'content-type': 'application/json' },
      body: '{"model":"gpt-4.1","input":"hello"}',
    });

    const observed = writes.find((chunk) => chunk.includes('"type":"http_request_observed"'));
    assert.ok(observed, 'expected one emitted request event');
    assert.match(observed, /"hook_name":"fetch"/);
    assert.match(observed, /"url":"https:\\/\\/api.openai.com\\/v1\\/responses"/);
    assert.match(observed, /"method":"POST"/);
  } finally {
    process.stdout.write = originalWrite;
    globalThis.fetch = originalFetch;
    dispose();
  }
});

test('http hook ignores non-text request bodies without throwing', function () {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const http = require('http');
  const originalRequest = http.request;
  http.request = function fakeRequest() {
    return {
      on() {},
      once() {},
      write() {},
      end() {},
    };
  };

  const { installHooks, dispose } = freshModule();

  try {
    installHooks(['http']);
    http.request('https://api.anthropic.com/v1/messages', {
      method: 'POST',
      headers: { 'x-api-key': 'test' },
    });

    const observed = writes.find((chunk) => chunk.includes('"type":"http_request_observed"'));
    assert.ok(observed, 'expected one emitted request event');
    assert.match(observed, /"hook_name":"http"/);
  } finally {
    process.stdout.write = originalWrite;
    http.request = originalRequest;
    dispose();
  }
});
```

- [ ] **步骤 2：运行 probe 测试，确认它们先失败**

运行：`node --test crates/prismtrace-host/probe/bootstrap.test.js`

预期：当前包装器只会调用原始请求函数，不会发出 `http_request_observed`，所以测试失败。

- [ ] **步骤 3：在 probe 中实现最小请求观察辅助函数**

在 `crates/prismtrace-host/probe/bootstrap.js` 中加入这些辅助函数和包装器调用：

```javascript
  var BODY_TEXT_LIMIT_BYTES = 64 * 1024;

  function normalizeHeaders(headers) {
    if (!headers) return [];
    if (Array.isArray(headers)) {
      return headers.map(function (entry) {
        return { name: String(entry[0]).toLowerCase(), value: String(entry[1]) };
      });
    }
    return Object.keys(headers).map(function (name) {
      return { name: String(name).toLowerCase(), value: String(headers[name]) };
    });
  }

  function toBodyText(body) {
    if (typeof body === 'string') {
      return body.slice(0, BODY_TEXT_LIMIT_BYTES);
    }
    if (body && typeof body === 'object' && typeof body.toString === 'function') {
      var text = body.toString();
      if (text !== '[object Object]') {
        return text.slice(0, BODY_TEXT_LIMIT_BYTES);
      }
    }
    return null;
  }

  function emitObservedRequest(observed) {
    sendMessage({
      type: 'http_request_observed',
      hook_name: observed.hookName,
      method: observed.method,
      url: observed.url,
      headers: observed.headers,
      body_text: observed.bodyText,
      timestamp_ms: Date.now(),
    });
  }
```

然后把包装器改成“先发出观察消息，再调用原始函数”：

```javascript
            globalThis.fetch = function patchedFetch(input, init) {
              var method = (init && init.method) || 'GET';
              var headers = normalizeHeaders((init && init.headers) || {});
              var bodyText = init ? toBodyText(init.body) : null;
              var url = typeof input === 'string' ? input : String(input.url || input);

              emitObservedRequest({
                hookName: 'fetch',
                method: String(method).toUpperCase(),
                url: url,
                headers: headers,
                bodyText: bodyText,
              });

              return originalFetch.apply(this, arguments);
            };
```

对 `undici.request`、`http.request` 和 `https.request` 也采用同样的处理方式，直接使用各自 API 已经暴露的参数，不要额外发明新的 probe 抽象。

- [ ] **步骤 4：运行完整 probe 测试文件，确认通过**

运行：`node --test crates/prismtrace-host/probe/bootstrap.test.js`

预期：通过，包括两个新测试和已有的 bootstrap hook 测试。

- [ ] **步骤 5：提交 probe 观察器改动**

```bash
git add crates/prismtrace-host/probe/bootstrap.js crates/prismtrace-host/probe/bootstrap.test.js
git commit -m "feat: emit observed request events from probe hooks"
```

## 任务 3：增加 host 侧请求过滤、artifact 落盘与摘要渲染

**涉及文件：**
- 新建：`crates/prismtrace-host/src/request_capture.rs`
- 修改：`crates/prismtrace-host/Cargo.toml`
- 修改：`crates/prismtrace-host/src/lib.rs`
- 测试：`crates/prismtrace-host/src/request_capture.rs`

- [ ] **步骤 1：先写过滤和 artifact 写入的失败测试**

先创建 `crates/prismtrace-host/src/request_capture.rs`，并放入下面这个测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::capture_observed_request;
    use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget, RuntimeKind};
    use prismtrace_storage::StorageLayout;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn capture_observed_request_persists_openai_request() {
        let root = temp_root("openai");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = ProcessTarget {
            pid: 42,
            app_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            runtime_kind: RuntimeKind::Electron,
        };
        let msg = IpcMessage::HttpRequestObserved {
            hook_name: "fetch".into(),
            method: "POST".into(),
            url: "https://api.openai.com/v1/responses".into(),
            headers: vec![HttpHeader {
                name: "authorization".into(),
                value: "Bearer sk-test".into(),
            }],
            body_text: Some(r#"{"model":"gpt-4.1","input":"hello"}"#.into()),
            timestamp_ms: 1_714_000_004_000,
        };

        let event = capture_observed_request(&storage, &target, &msg, 1)
            .expect("capture should succeed")
            .expect("request should match provider filters");

        assert_eq!(event.provider_hint, "openai");
        assert!(event.summary.contains("[captured] openai POST /v1/responses"));
        assert!(storage
            .artifacts_dir
            .join("requests")
            .join("1714000004000-42-1.json")
            .exists());

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn capture_observed_request_ignores_non_llm_http_requests() {
        let root = temp_root("ignored");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = ProcessTarget {
            pid: 7,
            app_name: "Example".into(),
            executable_path: PathBuf::from("/tmp/example"),
            runtime_kind: RuntimeKind::Node,
        };
        let msg = IpcMessage::HttpRequestObserved {
            hook_name: "http".into(),
            method: "GET".into(),
            url: "https://example.com/healthz".into(),
            headers: vec![],
            body_text: None,
            timestamp_ms: 11,
        };

        let event = capture_observed_request(&storage, &target, &msg, 1)
            .expect("capture should not error");

        assert!(event.is_none(), "non-LLM requests should be ignored");

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "prismtrace-request-capture-{label}-{}-{nanos}",
            process::id()
        ))
    }
}
```

- [ ] **步骤 2：运行 host 测试，确认它们先失败**

运行：`cargo test -p prismtrace-host capture_observed_request_persists_openai_request -- --exact`

预期：因为 `request_capture.rs`、`capture_observed_request` 和 `CapturedRequestEvent` 还不存在，测试失败。

- [ ] **步骤 3：实现最小 host 请求采集模块**

围绕下面这些定义构建 `crates/prismtrace-host/src/request_capture.rs`：

```rust
use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget};
use prismtrace_storage::StorageLayout;
use std::fs;
use std::io;
use std::path::PathBuf;

fn path_only(url: &str) -> &str {
    url.split_once("://")
        .and_then(|(_, rest)| rest.find('/').map(|index| &rest[index..]))
        .unwrap_or(url)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedRequestEvent {
    pub event_id: String,
    pub pid: u32,
    pub target_display_name: String,
    pub provider_hint: String,
    pub hook_name: String,
    pub method: String,
    pub url: String,
    pub captured_at_ms: u64,
    pub artifact_path: PathBuf,
    pub body_size_bytes: usize,
    pub summary: String,
}

pub fn capture_observed_request(
    storage: &StorageLayout,
    target: &ProcessTarget,
    message: &IpcMessage,
    sequence: u64,
) -> io::Result<Option<CapturedRequestEvent>> {
    let IpcMessage::HttpRequestObserved {
        hook_name,
        method,
        url,
        headers,
        body_text,
        timestamp_ms,
    } = message
    else {
        return Ok(None);
    };

    let provider_hint = match detect_provider_hint(url, headers, body_text.as_deref()) {
        Some(provider) => provider,
        None => return Ok(None),
    };

    let requests_dir = storage.artifacts_dir.join("requests");
    fs::create_dir_all(&requests_dir)?;
    let artifact_path = requests_dir.join(format!("{timestamp_ms}-{}-{sequence}.json", target.pid));
    let body_size_bytes = body_text.as_deref().map(str::len).unwrap_or(0);
    let path_label = artifact_path.display().to_string();

    fs::write(
        &artifact_path,
        serde_json::json!({
            "event_id": format!("{}-{}-{sequence}", target.pid, timestamp_ms),
            "pid": target.pid,
            "target_display_name": target.display_name(),
            "provider_hint": provider_hint,
            "hook_name": hook_name,
            "method": method,
            "url": url,
            "headers": headers,
            "body_text": body_text,
            "body_size_bytes": body_size_bytes,
            "truncated": false,
            "captured_at_ms": timestamp_ms,
        })
        .to_string(),
    )?;

    Ok(Some(CapturedRequestEvent {
        event_id: format!("{}-{}-{sequence}", target.pid, timestamp_ms),
        pid: target.pid,
        target_display_name: target.display_name(),
        provider_hint: provider_hint.to_string(),
        hook_name: hook_name.clone(),
        method: method.clone(),
        url: url.clone(),
        captured_at_ms: *timestamp_ms,
        artifact_path,
        body_size_bytes,
        summary: format!(
            "[captured] {} {} {} artifact={}",
            provider_hint,
            method,
            path_only(url),
            path_label
        ),
    }))
}
```

同时把缺失的依赖加到 `crates/prismtrace-host/Cargo.toml`：

```toml
[dependencies]
prismtrace-core = { path = "../prismtrace-core" }
prismtrace-storage = { path = "../prismtrace-storage" }
serde_json = "1"
```

同时加入一个满足当前 spec 的最小 host 侧 provider 匹配器：

```rust
fn detect_provider_hint(url: &str, headers: &[HttpHeader], body_text: Option<&str>) -> Option<&'static str> {
    let lower = url.to_ascii_lowercase();
    if lower.contains("api.openai.com/v1/responses") || lower.contains("api.openai.com/v1/chat/completions") {
        return Some("openai");
    }
    if lower.contains("api.anthropic.com/v1/messages") {
        return Some("anthropic");
    }
    if lower.contains("generativelanguage.googleapis.com/") && lower.contains(":generatecontent") {
        return Some("gemini");
    }
    if lower.contains("openrouter.ai/") {
        return Some("openrouter");
    }

    let has_auth = headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case("authorization")
            || header.name.eq_ignore_ascii_case("x-api-key")
            || header.name.eq_ignore_ascii_case("anthropic-version")
    });
    let body = body_text.unwrap_or_default();
    if has_auth && (body.contains("\"model\"") || body.contains("\"messages\"") || body.contains("\"input\"") || body.contains("\"contents\"")) {
        return Some("generic-llm");
    }

    None
}
```

最后，在 `crates/prismtrace-host/src/lib.rs` 中导出这个模块：

```rust
pub mod request_capture;
```

- [ ] **步骤 4：运行聚焦的 host 测试，确认通过**

运行：`cargo test -p prismtrace-host capture_observed_request_persists_openai_request capture_observed_request_ignores_non_llm_http_requests`

预期：两个新测试都通过。

- [ ] **步骤 5：提交 host 采集模块**

```bash
git add crates/prismtrace-host/Cargo.toml crates/prismtrace-host/src/lib.rs crates/prismtrace-host/src/request_capture.rs
git commit -m "feat: add host-side request capture filtering and artifacts"
```

## 任务 4：在 attach 后保持 probe IPC 流存活，并在前台 CLI 中消费它

**涉及文件：**
- 修改：`crates/prismtrace-host/src/attach.rs`
- 修改：`crates/prismtrace-host/src/main.rs`
- 修改：`crates/prismtrace-host/src/request_capture.rs`
- 测试：`crates/prismtrace-host/src/attach.rs`
- 测试：`crates/prismtrace-host/src/request_capture.rs`

- [ ] **步骤 1：先写前台采集消费的失败测试**

把下面这个 attach/backend 测试加到 `crates/prismtrace-host/src/attach.rs`：

```rust
    #[test]
    fn live_backend_next_event_returns_observed_request_after_bootstrap() {
        let runtime = ScriptedInstrumentationRuntime::success_with_messages(vec![
            bootstrap_report_line(),
            IpcMessage::HttpRequestObserved {
                hook_name: "fetch".into(),
                method: "POST".into(),
                url: "https://api.openai.com/v1/responses".into(),
                headers: vec![],
                body_text: Some("{}".into()),
                timestamp_ms: 3,
            }
            .to_json_line(),
        ]);
        let mut backend = LiveAttachBackend::new(runtime);
        let target = sample_target();

        backend.attach(&target).expect("attach should succeed");
        let event = backend
            .listener_mut()
            .expect("listener should still be available")
            .next_event();

        match event {
            IpcEvent::Message(IpcMessage::HttpRequestObserved { url, .. }) => {
                assert_eq!(url, "https://api.openai.com/v1/responses");
            }
            _ => panic!("expected observed request event"),
        }
    }
```

把下面这个前台循环测试加到 `crates/prismtrace-host/src/request_capture.rs`：

```rust
    #[test]
    fn consume_probe_events_writes_summary_for_observed_requests() {
        let root = temp_root("loop");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let mut output = Vec::new();
        let reader = Box::new(std::io::Cursor::new(
            format!(
                "{}{}",
                IpcMessage::HttpRequestObserved {
                    hook_name: "fetch".into(),
                    method: "POST".into(),
                    url: "https://api.openai.com/v1/responses".into(),
                    headers: vec![],
                    body_text: Some("{}".into()),
                    timestamp_ms: 10,
                }
                .to_json_line(),
                IpcMessage::DetachAck { timestamp_ms: 11 }.to_json_line(),
            )
            .into_bytes(),
        ));
        let mut listener = crate::ipc::IpcListener::new(reader, std::time::Duration::from_secs(15));

        consume_probe_events(&storage, &target, &mut listener, &mut output).expect("loop should succeed");

        let text = String::from_utf8(output).expect("stdout should be utf8");
        assert!(text.contains("[captured] openai POST /v1/responses"));
        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    fn sample_target() -> ProcessTarget {
        ProcessTarget {
            pid: 42,
            app_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            runtime_kind: RuntimeKind::Electron,
        }
    }
```

- [ ] **步骤 2：运行聚焦的 host 测试，确认它们先失败**

运行：`cargo test -p prismtrace-host live_backend_next_event_returns_observed_request_after_bootstrap consume_probe_events_writes_summary_for_observed_requests`

预期：因为 `listener_mut`、`into_parts` 和 `consume_probe_events` 还不存在，测试失败。

- [ ] **步骤 3：实现最小前台采集循环**

在 `crates/prismtrace-host/src/attach.rs` 中，暴露 bootstrap 之后保留下来的 listener：

```rust
impl<R: InstrumentationRuntime> LiveAttachBackend<R> {
    pub fn listener_mut(&mut self) -> Option<&mut IpcListener> {
        self.ipc_listener.as_mut()
    }
}

impl<B> AttachController<B> {
    pub fn into_parts(self) -> (B, Option<AttachSession>) {
        (self.backend, self.active_session)
    }
}
```

在 `crates/prismtrace-host/src/request_capture.rs` 中，加入一个会一直读取到 detach 或断连为止的循环：

```rust
use crate::ipc::{IpcEvent, IpcListener};
use std::io::Write;

pub fn consume_probe_events(
    storage: &StorageLayout,
    target: &ProcessTarget,
    listener: &mut IpcListener,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut sequence = 1_u64;

    loop {
        match listener.next_event() {
            IpcEvent::Message(message @ IpcMessage::HttpRequestObserved { .. }) => {
                if let Some(event) = capture_observed_request(storage, target, &message, sequence)? {
                    writeln!(output, "{}", event.summary)?;
                    sequence += 1;
                }
            }
            IpcEvent::Message(IpcMessage::DetachAck { .. }) => return Ok(()),
            IpcEvent::ChannelDisconnected { .. } => return Ok(()),
            IpcEvent::HeartbeatTimeout { elapsed_ms } => {
                writeln!(output, "[probe-timeout] {} ms since heartbeat", elapsed_ms)?;
                return Ok(());
            }
            IpcEvent::Message(_) => {}
        }
    }
}
```

然后把 `crates/prismtrace-host/src/main.rs` 接到真正的前台采集形态，而不是一次性的脚本化 demo：

```rust
    if let Some(pid) = attach_pid_arg(&args)? {
        let source = prismtrace_host::discovery::PsProcessSampleSource;
        let targets = prismtrace_host::discovery::discover_targets(&source)?;
        let readiness = prismtrace_host::readiness::evaluate_targets(&targets);
        let target = readiness
            .into_iter()
            .find(|item| item.target.pid == pid && item.status == prismtrace_core::AttachReadinessStatus::Supported)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, format!("pid {pid} is not attach-ready")))?;

        let mut controller = prismtrace_host::attach::AttachController::new(
            prismtrace_host::attach::LiveAttachBackend::new(
                prismtrace_host::runtime::NodeInstrumentationRuntime,
            ),
        );
        let session = controller
            .attach(&target)
            .map_err(|failure| std::io::Error::new(std::io::ErrorKind::Other, failure.summary()))?;

        println!("[attached] {} (pid {})", session.target.display_name(), session.target.pid);
        let (mut backend, active_session) = controller.into_parts();
        let session = active_session.expect("active session should remain after successful attach");
        let listener = backend
            .listener_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "probe listener missing after attach"))?;
        let mut stdout = std::io::stdout().lock();
        prismtrace_host::request_capture::consume_probe_events(&result.storage, &session.target, listener, &mut stdout)?;
        return Ok(());
    }
```

- [ ] **步骤 4：先运行聚焦的前台测试，再运行完整 host 测试套件**

运行：`cargo test -p prismtrace-host live_backend_next_event_returns_observed_request_after_bootstrap consume_probe_events_writes_summary_for_observed_requests`

预期：两个新测试都通过。

然后运行：`cargo test -p prismtrace-host`

预期：整个 `prismtrace-host` crate 的测试全部通过。

- [ ] **步骤 5：提交前台采集循环**

```bash
git add crates/prismtrace-host/src/attach.rs crates/prismtrace-host/src/main.rs crates/prismtrace-host/src/request_capture.rs
git commit -m "feat: stream captured request events in attach foreground mode"
```

## 任务 5：对 core 和 host 做最终验证

**涉及文件：**
- No code changes expected
- 测试：`crates/prismtrace-core/src/lib.rs`
- 测试：`crates/prismtrace-host/src/*.rs`
- 测试：`crates/prismtrace-host/probe/bootstrap.test.js`

- [ ] **步骤 1：运行 core crate 测试**

运行：`cargo test -p prismtrace-core`

预期：通过。

- [ ] **步骤 2：运行 host crate 测试**

运行：`cargo test -p prismtrace-host`

预期：通过。

- [ ] **步骤 3：运行 probe 单元测试**

运行：`node --test crates/prismtrace-host/probe/bootstrap.test.js`

预期：通过。

- [ ] **步骤 4：收尾前检查工作区状态**

运行：`git status --short`

预期：只有预期内的实现文件被修改。

- [ ] **步骤 5：如果需要，提交最后的清理改动**

```bash
git add crates/prismtrace-core/src/lib.rs crates/prismtrace-host/src/lib.rs crates/prismtrace-host/src/attach.rs crates/prismtrace-host/src/main.rs crates/prismtrace-host/src/request_capture.rs crates/prismtrace-host/probe/bootstrap.js crates/prismtrace-host/probe/bootstrap.test.js
git commit -m "chore: verify iteration 4 request capture implementation"
```

## 自检

规格覆盖检查：

- 前台 attach 循环：由任务 4 覆盖。
- 新的 IPC 请求事件：由任务 1 覆盖。
- probe 侧对 `fetch`、`undici`、`http`、`https` 的请求观察：由任务 2 覆盖。
- host 侧的类模型请求过滤、artifact 落盘和 CLI 摘要：由任务 3 覆盖。
- Rust 和 Node 测试的确定性验证：由任务 5 覆盖。

占位项检查：

- 任务步骤中没有残留 `TODO`、`TBD` 或 “implement later” 之类的占位符。
- 每个改代码的步骤都给出了具体片段。
- 每个验证步骤都给出了明确命令和预期结果。

类型一致性检查：

- `HttpHeader` 和 `IpcMessage::HttpRequestObserved` 在任务 1 中引入，并在任务 2 到任务 4 中保持一致复用。
- `CapturedRequestEvent` 和 `capture_observed_request` 在任务 3 中定义，并在任务 4 中保持一致复用。
- `consume_probe_events` 在任务 4 中定义，只在该任务及其专属测试中被引用。
