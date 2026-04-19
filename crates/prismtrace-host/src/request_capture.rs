use crate::ipc::{IpcEvent, IpcListener};
use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget};
use prismtrace_storage::StorageLayout;
use std::fs;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn path_only(url: &str) -> &str {
    parse_host_and_path(url)
        .map(|(_, path)| path)
        .unwrap_or_else(|| url.split(['?', '#']).next().unwrap_or(url))
}

fn parse_host_and_path(url: &str) -> Option<(&str, &str)> {
    let (_, rest) = url.split_once("://")?;
    let authority_end = rest
        .find(|c| ['/', '?', '#'].contains(&c))
        .unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_with_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_with_port
        .split(':')
        .next()
        .filter(|value| !value.is_empty())?;
    let tail = if authority_end < rest.len() {
        &rest[authority_end..]
    } else {
        "/"
    };
    let path_end = tail.find(|c| ['?', '#'].contains(&c)).unwrap_or(tail.len());
    let path = &tail[..path_end];

    Some((host, path))
}

fn is_sensitive_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("authorization")
        || name.eq_ignore_ascii_case("x-api-key")
        || name.eq_ignore_ascii_case("proxy-authorization")
}

fn sanitized_headers(headers: &[HttpHeader]) -> Vec<HttpHeader> {
    headers
        .iter()
        .map(|header| HttpHeader {
            name: header.name.clone(),
            value: if is_sensitive_header(&header.name) {
                "[redacted]".to_string()
            } else {
                header.value.clone()
            },
        })
        .collect()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeConsumeExit {
    DetachAck,
    ChannelDisconnected { reason: String },
    HeartbeatTimeout { elapsed_ms: u64 },
}

pub struct ProbeConsumeOutcome {
    pub exit: ProbeConsumeExit,
    pub listener: Option<IpcListener>,
}

fn detect_provider_hint(
    url: &str,
    headers: &[HttpHeader],
    body_text: Option<&str>,
) -> Option<&'static str> {
    if let Some((host, path)) = parse_host_and_path(url) {
        if host.eq_ignore_ascii_case("api.openai.com")
            && (path == "/v1/responses" || path == "/v1/chat/completions")
        {
            return Some("openai");
        }
        if host.eq_ignore_ascii_case("api.anthropic.com") && path == "/v1/messages" {
            return Some("anthropic");
        }
        if host.eq_ignore_ascii_case("generativelanguage.googleapis.com")
            && path.contains(":generateContent")
        {
            return Some("gemini");
        }
        if host.eq_ignore_ascii_case("openrouter.ai") && path.starts_with('/') {
            return Some("openrouter");
        }
    }

    let has_auth = headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case("authorization")
            || header.name.eq_ignore_ascii_case("x-api-key")
            || header.name.eq_ignore_ascii_case("anthropic-version")
    });
    let body = body_text.unwrap_or_default();
    if has_auth
        && (body.contains("\"model\"")
            || body.contains("\"messages\"")
            || body.contains("\"input\"")
            || body.contains("\"contents\""))
    {
        return Some("generic-llm");
    }

    None
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
        body_truncated,
        timestamp_ms,
    } = message
    else {
        return Ok(None);
    };

    let Some(provider_hint) = detect_provider_hint(url, headers, body_text.as_deref()) else {
        return Ok(None);
    };

    let requests_dir = storage.artifacts_dir.join("requests");
    fs::create_dir_all(&requests_dir)?;

    let event_id = format!("{}-{}-{sequence}", target.pid, timestamp_ms);
    let artifact_path = requests_dir.join(format!("{timestamp_ms}-{}-{sequence}.json", target.pid));
    let body_size_bytes = body_text.as_deref().map(str::len).unwrap_or(0);
    let path_label = artifact_path.display().to_string();
    let safe_headers = sanitized_headers(headers);

    fs::write(
        &artifact_path,
        serde_json::json!({
            "event_id": event_id,
            "pid": target.pid,
            "target_display_name": target.display_name(),
            "provider_hint": provider_hint,
            "hook_name": hook_name,
            "method": method,
            "url": url,
            "headers": safe_headers,
            "body_text": body_text,
            "body_size_bytes": body_size_bytes,
            "truncated": body_truncated,
            "captured_at_ms": timestamp_ms,
        })
        .to_string(),
    )?;

    Ok(Some(CapturedRequestEvent {
        event_id,
        pid: target.pid,
        target_display_name: target.display_name().to_string(),
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

pub fn consume_probe_events(
    storage: &StorageLayout,
    target: &ProcessTarget,
    listener: IpcListener,
    output: &mut impl Write,
) -> io::Result<ProbeConsumeOutcome> {
    let mut sequence = 1_u64;
    let timeout = listener.heartbeat_timeout();
    // Give the worker enough room to surface a heartbeat timeout event even when
    // the underlying bridge reader wakes on a coarser timeout cadence.
    let worker_timeout_slack = Duration::from_millis(300);
    let wait_poll_step = Duration::from_millis(100);
    let mut wait_deadline = Instant::now() + timeout.saturating_add(worker_timeout_slack);
    let shutdown = listener.shutdown_handle();
    let (tx, rx) = mpsc::channel();

    let mut worker = Some(thread::spawn(move || {
        let mut listener = listener;
        loop {
            let event = listener.next_event();
            let terminal = matches!(
                event,
                IpcEvent::Message(IpcMessage::DetachAck { .. })
                    | IpcEvent::ChannelDisconnected { .. }
                    | IpcEvent::HeartbeatTimeout { .. }
            );
            if tx.send(event).is_err() {
                break;
            }
            if terminal {
                break;
            }
        }
        listener
    }));

    let cleanup_worker = |worker: &mut Option<thread::JoinHandle<IpcListener>>,
                          request_shutdown: bool|
     -> Option<IpcListener> {
        if request_shutdown && let Some(handle) = shutdown.as_ref() {
            handle.shutdown();
        }

        let can_join = match worker.as_ref() {
            None => false,
            Some(join_handle) => {
                !request_shutdown || shutdown.is_some() || join_handle.is_finished()
            }
        };
        if !can_join {
            return None;
        }

        worker
            .take()
            .and_then(|join_handle| join_handle.join().ok())
    };
    let refresh_wait_deadline = |deadline: &mut Instant| {
        *deadline = Instant::now() + timeout.saturating_add(worker_timeout_slack);
    };

    loop {
        let now = Instant::now();
        if now >= wait_deadline {
            let elapsed_ms = timeout.as_millis() as u64;
            let listener = cleanup_worker(&mut worker, true);
            writeln!(output, "[probe-timeout] {} ms since heartbeat", elapsed_ms)?;
            return Ok(ProbeConsumeOutcome {
                exit: ProbeConsumeExit::HeartbeatTimeout { elapsed_ms },
                listener,
            });
        }

        let remaining = wait_deadline.saturating_duration_since(now);
        let recv_window = if remaining > wait_poll_step {
            wait_poll_step
        } else {
            remaining
        };

        match rx.recv_timeout(recv_window) {
            Ok(IpcEvent::Message(message @ IpcMessage::HttpRequestObserved { .. })) => {
                refresh_wait_deadline(&mut wait_deadline);
                if let Some(event) = capture_observed_request(storage, target, &message, sequence)?
                {
                    writeln!(output, "{}", event.summary)?;
                    sequence += 1;
                }
            }
            Ok(IpcEvent::Message(IpcMessage::DetachAck { .. })) => {
                let listener = cleanup_worker(&mut worker, false);
                return Ok(ProbeConsumeOutcome {
                    exit: ProbeConsumeExit::DetachAck,
                    listener,
                });
            }
            Ok(IpcEvent::ChannelDisconnected { reason }) => {
                let listener = cleanup_worker(&mut worker, false);
                return Ok(ProbeConsumeOutcome {
                    exit: ProbeConsumeExit::ChannelDisconnected { reason },
                    listener,
                });
            }
            Ok(IpcEvent::HeartbeatTimeout { elapsed_ms }) => {
                let listener = cleanup_worker(&mut worker, false);
                writeln!(output, "[probe-timeout] {} ms since heartbeat", elapsed_ms)?;
                return Ok(ProbeConsumeOutcome {
                    exit: ProbeConsumeExit::HeartbeatTimeout { elapsed_ms },
                    listener,
                });
            }
            Ok(IpcEvent::Message(_)) => {
                refresh_wait_deadline(&mut wait_deadline);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let listener = cleanup_worker(&mut worker, false);
                return Ok(ProbeConsumeOutcome {
                    exit: ProbeConsumeExit::ChannelDisconnected {
                        reason: "probe event worker channel disconnected".into(),
                    },
                    listener,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProbeConsumeExit, capture_observed_request, consume_probe_events};
    use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget, RuntimeKind};
    use prismtrace_storage::StorageLayout;
    use std::fs;
    use std::io::{BufRead, Cursor, Read};
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
            body_truncated: false,
            timestamp_ms: 1_714_000_004_000,
        };

        let event = capture_observed_request(&storage, &target, &msg, 1)
            .expect("capture should succeed")
            .expect("request should match provider filters");

        assert_eq!(event.provider_hint, "openai");
        assert!(
            event
                .summary
                .contains("[captured] openai POST /v1/responses")
        );
        assert!(
            storage
                .artifacts_dir
                .join("requests")
                .join("1714000004000-42-1.json")
                .exists()
        );

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn capture_observed_request_redacts_sensitive_headers_in_artifact() {
        let root = temp_root("redacted");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let msg = IpcMessage::HttpRequestObserved {
            hook_name: "fetch".into(),
            method: "POST".into(),
            url: "https://api.openai.com/v1/responses".into(),
            headers: vec![
                HttpHeader {
                    name: "authorization".into(),
                    value: "Bearer sk-secret".into(),
                },
                HttpHeader {
                    name: "content-type".into(),
                    value: "application/json".into(),
                },
            ],
            body_text: Some(r#"{"model":"gpt-4.1","input":"hello"}"#.into()),
            body_truncated: false,
            timestamp_ms: 100,
        };

        let event = capture_observed_request(&storage, &target, &msg, 1)
            .expect("capture should succeed")
            .expect("request should be captured");
        let artifact =
            fs::read_to_string(event.artifact_path).expect("artifact should be readable");

        assert!(!artifact.contains("Bearer sk-secret"));
        assert!(artifact.contains("[redacted]"));

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
            body_truncated: false,
            timestamp_ms: 11,
        };

        let event =
            capture_observed_request(&storage, &target, &msg, 1).expect("capture should not error");

        assert!(event.is_none(), "non-LLM requests should be ignored");

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn capture_observed_request_does_not_match_provider_name_in_query_string() {
        let root = temp_root("query-string");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let msg = IpcMessage::HttpRequestObserved {
            hook_name: "fetch".into(),
            method: "POST".into(),
            url: "https://example.com/collect?next=https://api.openai.com/v1/responses".into(),
            headers: vec![],
            body_text: Some(r#"{"model":"gpt-4.1"}"#.into()),
            body_truncated: false,
            timestamp_ms: 12,
        };

        let event =
            capture_observed_request(&storage, &target, &msg, 1).expect("capture should not error");

        assert!(
            event.is_none(),
            "provider query string should not count as a match"
        );

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn capture_observed_request_persists_truncated_flag_from_probe() {
        let root = temp_root("truncated");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let msg = IpcMessage::HttpRequestObserved {
            hook_name: "fetch".into(),
            method: "POST".into(),
            url: "https://api.openai.com/v1/responses".into(),
            headers: vec![],
            body_text: Some("truncated body".into()),
            body_truncated: true,
            timestamp_ms: 13,
        };

        let event = capture_observed_request(&storage, &target, &msg, 1)
            .expect("capture should succeed")
            .expect("request should be captured");
        let artifact =
            fs::read_to_string(event.artifact_path).expect("artifact should be readable");

        assert!(artifact.contains(r#""truncated":true"#));

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn capture_observed_request_summary_omits_query_string() {
        let root = temp_root("summary-query");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let msg = IpcMessage::HttpRequestObserved {
            hook_name: "fetch".into(),
            method: "POST".into(),
            url: "https://api.openai.com/v1/responses?api-version=1&sig=secret".into(),
            headers: vec![],
            body_text: Some(r#"{"model":"gpt-4.1"}"#.into()),
            body_truncated: false,
            timestamp_ms: 14,
        };

        let event = capture_observed_request(&storage, &target, &msg, 1)
            .expect("capture should succeed")
            .expect("request should be captured");

        assert!(
            event
                .summary
                .contains("[captured] openai POST /v1/responses")
        );
        assert!(!event.summary.contains("sig=secret"));
        assert!(!event.summary.contains("api-version=1"));

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn consume_probe_events_writes_summary_for_observed_requests() {
        let root = temp_root("loop");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let mut output = Vec::new();
        let reader = Box::new(Cursor::new(
            format!(
                "{}{}",
                IpcMessage::HttpRequestObserved {
                    hook_name: "fetch".into(),
                    method: "POST".into(),
                    url: "https://api.openai.com/v1/responses".into(),
                    headers: vec![],
                    body_text: Some("{}".into()),
                    body_truncated: false,
                    timestamp_ms: 10,
                }
                .to_json_line(),
                IpcMessage::DetachAck { timestamp_ms: 11 }.to_json_line(),
            )
            .into_bytes(),
        ));
        let listener = crate::ipc::IpcListener::new(reader, Duration::from_secs(15));

        let outcome = consume_probe_events(&storage, &target, listener, &mut output)
            .expect("loop should succeed");

        let text = String::from_utf8(output).expect("stdout should be utf8");
        assert!(text.contains("[captured] openai POST /v1/responses"));
        assert_eq!(outcome.exit, ProbeConsumeExit::DetachAck);
        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn consume_probe_events_requests_shutdown_when_reader_blocks_past_heartbeat_deadline() {
        let root = temp_root("timeout");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let mut output = Vec::new();
        let state = Arc::new(BlockingState::default());
        let reader = Box::new(BlockingReader::new(Arc::clone(&state)));
        let listener = crate::ipc::IpcListener::new_with_shutdown(
            reader,
            Duration::from_millis(1),
            Arc::new(TestShutdown::new(Arc::clone(&state))),
        );

        let outcome = consume_probe_events(&storage, &target, listener, &mut output)
            .expect("loop should finish");

        let text = String::from_utf8(output).expect("stdout should be utf8");
        assert!(text.contains("[probe-timeout]"));
        assert!(matches!(
            outcome.exit,
            ProbeConsumeExit::HeartbeatTimeout { .. }
        ));
        assert!(
            state.interrupted.load(Ordering::SeqCst),
            "timeout should request reader shutdown"
        );
        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn consume_probe_events_reclaims_listener_on_timeout_without_shutdown_handle() {
        let root = temp_root("timeout-no-shutdown");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let mut output = Vec::new();
        let listener = crate::ipc::IpcListener::new(
            Box::new(AlwaysWouldBlockReader),
            Duration::from_millis(5),
        );

        let outcome = consume_probe_events(&storage, &target, listener, &mut output)
            .expect("loop should finish");

        let text = String::from_utf8(output).expect("stdout should be utf8");
        assert!(text.contains("[probe-timeout]"));
        assert!(
            matches!(outcome.exit, ProbeConsumeExit::HeartbeatTimeout { .. }),
            "expected heartbeat timeout exit"
        );
        assert!(
            outcome.listener.is_some(),
            "listener should be reclaimed even without explicit shutdown handle"
        );
        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn consume_probe_events_waits_for_worker_timeout_event_before_fallback_timeout() {
        let root = temp_root("timeout-worker-late");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let mut output = Vec::new();
        let listener = crate::ipc::IpcListener::new(
            Box::new(SlowWouldBlockReader::new(Duration::from_millis(180))),
            Duration::from_millis(10),
        );

        let started = Instant::now();
        let outcome = consume_probe_events(&storage, &target, listener, &mut output)
            .expect("loop should finish");
        let elapsed = started.elapsed();

        assert!(
            elapsed >= Duration::from_millis(150),
            "main loop should wait for worker timeout event, elapsed={elapsed:?}"
        );
        assert!(matches!(
            outcome.exit,
            ProbeConsumeExit::HeartbeatTimeout { .. }
        ));
        assert!(
            outcome.listener.is_some(),
            "listener should be reclaimed from worker timeout path"
        );

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn consume_probe_events_refreshes_fallback_deadline_after_each_message() {
        let root = temp_root("deadline-refresh");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let mut output = Vec::new();
        let listener = crate::ipc::IpcListener::new(
            Box::new(ScriptedLineReader::new(vec![
                (
                    Duration::from_millis(90),
                    IpcMessage::Heartbeat { timestamp_ms: 1 }.to_json_line(),
                ),
                (
                    Duration::from_millis(90),
                    IpcMessage::Heartbeat { timestamp_ms: 2 }.to_json_line(),
                ),
                (
                    Duration::from_millis(90),
                    IpcMessage::Heartbeat { timestamp_ms: 3 }.to_json_line(),
                ),
                (
                    Duration::from_millis(90),
                    IpcMessage::Heartbeat { timestamp_ms: 4 }.to_json_line(),
                ),
                (
                    Duration::from_millis(90),
                    IpcMessage::DetachAck { timestamp_ms: 5 }.to_json_line(),
                ),
            ])),
            Duration::from_millis(100),
        );

        let started = Instant::now();
        let outcome = consume_probe_events(&storage, &target, listener, &mut output)
            .expect("loop should finish after detach ack");
        let elapsed = started.elapsed();

        assert!(
            elapsed >= Duration::from_millis(430),
            "session should outlive the original fixed fallback deadline, elapsed={elapsed:?}"
        );
        assert_eq!(outcome.exit, ProbeConsumeExit::DetachAck);
        assert!(
            !String::from_utf8(output)
                .expect("stdout should be utf8")
                .contains("[probe-timeout]"),
            "healthy heartbeats should not trip fallback timeout"
        );

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

    fn sample_target() -> ProcessTarget {
        ProcessTarget {
            pid: 42,
            app_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            runtime_kind: RuntimeKind::Electron,
        }
    }

    #[derive(Default)]
    struct BlockingState {
        released: Mutex<bool>,
        wake: Condvar,
        interrupted: AtomicBool,
    }

    struct BlockingReader {
        state: Arc<BlockingState>,
    }

    impl BlockingReader {
        fn new(state: Arc<BlockingState>) -> Self {
            Self { state }
        }

        fn wait_until_released(&self) {
            let mut released = self
                .state
                .released
                .lock()
                .expect("blocking state lock should succeed");
            while !*released {
                released = self
                    .state
                    .wake
                    .wait(released)
                    .expect("blocking state wait should succeed");
            }
        }
    }

    impl Read for BlockingReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            self.wait_until_released();
            Ok(0)
        }
    }

    impl BufRead for BlockingReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            self.wait_until_released();
            Ok(&[])
        }

        fn consume(&mut self, _amt: usize) {}
    }

    struct TestShutdown {
        state: Arc<BlockingState>,
    }

    impl TestShutdown {
        fn new(state: Arc<BlockingState>) -> Self {
            Self { state }
        }
    }

    impl crate::ipc::ReaderShutdown for TestShutdown {
        fn shutdown(&self) {
            self.state.interrupted.store(true, Ordering::SeqCst);
            let mut released = self
                .state
                .released
                .lock()
                .expect("blocking state lock should succeed");
            *released = true;
            self.state.wake.notify_all();
        }
    }

    struct AlwaysWouldBlockReader;

    impl Read for AlwaysWouldBlockReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            thread::sleep(Duration::from_millis(1));
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "synthetic timeout",
            ))
        }
    }

    impl BufRead for AlwaysWouldBlockReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            thread::sleep(Duration::from_millis(1));
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "synthetic timeout",
            ))
        }

        fn consume(&mut self, _amt: usize) {}
    }

    struct SlowWouldBlockReader {
        delay: Duration,
    }

    impl SlowWouldBlockReader {
        fn new(delay: Duration) -> Self {
            Self { delay }
        }
    }

    impl Read for SlowWouldBlockReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            thread::sleep(self.delay);
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "synthetic delayed timeout",
            ))
        }
    }

    impl BufRead for SlowWouldBlockReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            thread::sleep(self.delay);
            Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "synthetic delayed timeout",
            ))
        }

        fn consume(&mut self, _amt: usize) {}
    }

    struct ScriptedLineReader {
        steps: Vec<(Duration, Vec<u8>)>,
        next_step: usize,
        current: Vec<u8>,
        position: usize,
    }

    impl ScriptedLineReader {
        fn new(steps: Vec<(Duration, String)>) -> Self {
            Self {
                steps: steps
                    .into_iter()
                    .map(|(delay, line)| (delay, line.into_bytes()))
                    .collect(),
                next_step: 0,
                current: Vec::new(),
                position: 0,
            }
        }
    }

    impl Read for ScriptedLineReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let available = self.fill_buf()?;
            let amount = available.len().min(buf.len());
            buf[..amount].copy_from_slice(&available[..amount]);
            self.consume(amount);
            Ok(amount)
        }
    }

    impl BufRead for ScriptedLineReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            loop {
                if self.position < self.current.len() {
                    return Ok(&self.current[self.position..]);
                }

                if self.next_step >= self.steps.len() {
                    return Ok(&[]);
                }

                let (delay, bytes) = &self.steps[self.next_step];
                thread::sleep(*delay);
                self.current = bytes.clone();
                self.position = 0;
                self.next_step += 1;
            }
        }

        fn consume(&mut self, amount: usize) {
            self.position = self.position.saturating_add(amount);
        }
    }
}
