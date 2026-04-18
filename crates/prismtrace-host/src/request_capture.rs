use crate::ipc::{IpcEvent, IpcListener};
use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget};
use prismtrace_storage::StorageLayout;
use std::fs;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

fn path_only(url: &str) -> &str {
    url.split_once("://")
        .and_then(|(_, rest)| rest.find('/').map(|index| &rest[index..]))
        .unwrap_or(url)
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
) -> io::Result<()> {
    let mut sequence = 1_u64;
    let timeout = listener.heartbeat_timeout();
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut listener = listener;
        loop {
            let event = listener.next_event();
            let terminal = matches!(
                event,
                IpcEvent::Message(IpcMessage::DetachAck { .. })
                    | IpcEvent::ChannelDisconnected { .. }
            );
            if tx.send(event).is_err() {
                break;
            }
            if terminal {
                break;
            }
        }
    });

    loop {
        match rx.recv_timeout(timeout) {
            Ok(IpcEvent::Message(message @ IpcMessage::HttpRequestObserved { .. })) => {
                if let Some(event) = capture_observed_request(storage, target, &message, sequence)?
                {
                    writeln!(output, "{}", event.summary)?;
                    sequence += 1;
                }
            }
            Ok(IpcEvent::Message(IpcMessage::DetachAck { .. })) => return Ok(()),
            Ok(IpcEvent::ChannelDisconnected { .. }) => return Ok(()),
            Ok(IpcEvent::HeartbeatTimeout { elapsed_ms }) => {
                writeln!(output, "[probe-timeout] {} ms since heartbeat", elapsed_ms)?;
                return Ok(());
            }
            Ok(IpcEvent::Message(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {
                writeln!(
                    output,
                    "[probe-timeout] {} ms since heartbeat",
                    timeout.as_millis()
                )?;
                return Ok(());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{capture_observed_request, consume_probe_events};
    use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget, RuntimeKind};
    use prismtrace_storage::StorageLayout;
    use std::fs;
    use std::io::{BufRead, Cursor, Read};
    use std::path::PathBuf;
    use std::process;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

        consume_probe_events(&storage, &target, listener, &mut output)
            .expect("loop should succeed");

        let text = String::from_utf8(output).expect("stdout should be utf8");
        assert!(text.contains("[captured] openai POST /v1/responses"));
        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn consume_probe_events_reports_timeout_when_reader_blocks_past_heartbeat_deadline() {
        let root = temp_root("timeout");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = sample_target();
        let mut output = Vec::new();
        let reader = Box::new(SlowReader::new(Duration::from_millis(20)));
        let listener = crate::ipc::IpcListener::new(reader, Duration::from_millis(1));

        consume_probe_events(&storage, &target, listener, &mut output).expect("loop should finish");

        let text = String::from_utf8(output).expect("stdout should be utf8");
        assert!(text.contains("[probe-timeout]"));
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

    struct SlowReader {
        delay: Duration,
    }

    impl SlowReader {
        fn new(delay: Duration) -> Self {
            Self { delay }
        }
    }

    impl Read for SlowReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            thread::sleep(self.delay);
            Ok(0)
        }
    }

    impl BufRead for SlowReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            thread::sleep(self.delay);
            Ok(&[])
        }

        fn consume(&mut self, _amt: usize) {}
    }
}
