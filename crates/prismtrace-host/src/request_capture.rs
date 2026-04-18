use crate::ipc::{IpcEvent, IpcListener};
use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget};
use prismtrace_storage::StorageLayout;
use std::fs;
use std::io;
use std::io::Write;
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

fn detect_provider_hint(
    url: &str,
    headers: &[HttpHeader],
    body_text: Option<&str>,
) -> Option<&'static str> {
    let lower = url.to_ascii_lowercase();
    if lower.contains("api.openai.com/v1/responses")
        || lower.contains("api.openai.com/v1/chat/completions")
    {
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
            "headers": headers,
            "body_text": body_text,
            "body_size_bytes": body_size_bytes,
            "truncated": false,
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
    listener: &mut IpcListener,
    output: &mut impl Write,
) -> io::Result<()> {
    let mut sequence = 1_u64;

    loop {
        match listener.next_event() {
            IpcEvent::Message(message @ IpcMessage::HttpRequestObserved { .. }) => {
                if let Some(event) = capture_observed_request(storage, target, &message, sequence)?
                {
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

#[cfg(test)]
mod tests {
    use super::{capture_observed_request, consume_probe_events};
    use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget, RuntimeKind};
    use prismtrace_storage::StorageLayout;
    use std::fs;
    use std::io::Cursor;
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

        let event =
            capture_observed_request(&storage, &target, &msg, 1).expect("capture should not error");

        assert!(event.is_none(), "non-LLM requests should be ignored");

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
                    timestamp_ms: 10,
                }
                .to_json_line(),
                IpcMessage::DetachAck { timestamp_ms: 11 }.to_json_line(),
            )
            .into_bytes(),
        ));
        let mut listener = crate::ipc::IpcListener::new(reader, std::time::Duration::from_secs(15));

        consume_probe_events(&storage, &target, &mut listener, &mut output)
            .expect("loop should succeed");

        let text = String::from_utf8(output).expect("stdout should be utf8");
        assert!(text.contains("[captured] openai POST /v1/responses"));
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
}
