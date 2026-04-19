use crate::request_capture::{detect_provider_hint, path_only, sanitized_headers};
use prismtrace_core::{IpcMessage, ProcessTarget};
use prismtrace_storage::StorageLayout;
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedResponseEvent {
    pub event_id: String,
    pub exchange_id: String,
    pub pid: u32,
    pub target_display_name: String,
    pub provider_hint: String,
    pub hook_name: String,
    pub method: String,
    pub url: String,
    pub status_code: u16,
    pub completed_at_ms: u64,
    pub duration_ms: u64,
    pub artifact_path: PathBuf,
    pub body_size_bytes: usize,
    pub summary: String,
}

pub fn capture_observed_response(
    storage: &StorageLayout,
    target: &ProcessTarget,
    message: &IpcMessage,
    sequence: u64,
) -> io::Result<Option<CapturedResponseEvent>> {
    capture_observed_response_with_hint(storage, target, message, sequence, None)
}

pub fn capture_observed_response_with_hint(
    storage: &StorageLayout,
    target: &ProcessTarget,
    message: &IpcMessage,
    sequence: u64,
    provider_hint_override: Option<&str>,
) -> io::Result<Option<CapturedResponseEvent>> {
    let IpcMessage::HttpResponseObserved {
        exchange_id,
        hook_name,
        method,
        url,
        status_code,
        headers,
        body_text,
        body_truncated,
        started_at_ms,
        completed_at_ms,
    } = message
    else {
        return Ok(None);
    };

    let provider_hint = provider_hint_override
        .map(str::to_string)
        .or_else(|| detect_provider_hint(url, headers, body_text.as_deref()).map(str::to_string));
    let Some(provider_hint) = provider_hint else {
        return Ok(None);
    };

    let responses_dir = storage.artifacts_dir.join("responses");
    fs::create_dir_all(&responses_dir)?;

    let event_id = format!("{}-{}-{sequence}", target.pid, completed_at_ms);
    let artifact_path =
        responses_dir.join(format!("{completed_at_ms}-{}-{sequence}.json", target.pid));
    let duration_ms = completed_at_ms.saturating_sub(*started_at_ms);
    let body_size_bytes = body_text.as_deref().map(str::len).unwrap_or(0);
    let path_label = artifact_path.display().to_string();
    let safe_headers = sanitized_headers(headers);

    fs::write(
        &artifact_path,
        serde_json::json!({
            "event_id": event_id,
            "exchange_id": exchange_id,
            "pid": target.pid,
            "target_display_name": target.display_name(),
            "provider_hint": provider_hint,
            "hook_name": hook_name,
            "method": method,
            "url": url,
            "status_code": status_code,
            "headers": safe_headers,
            "body_text": body_text,
            "body_size_bytes": body_size_bytes,
            "truncated": body_truncated,
            "started_at_ms": started_at_ms,
            "completed_at_ms": completed_at_ms,
            "duration_ms": duration_ms,
        })
        .to_string(),
    )?;

    Ok(Some(CapturedResponseEvent {
        event_id,
        exchange_id: exchange_id.clone(),
        pid: target.pid,
        target_display_name: target.display_name().to_string(),
        provider_hint: provider_hint.clone(),
        hook_name: hook_name.clone(),
        method: method.clone(),
        url: url.clone(),
        status_code: *status_code,
        completed_at_ms: *completed_at_ms,
        duration_ms,
        artifact_path,
        body_size_bytes,
        summary: format!(
            "[response] {} {} {} {} {}ms artifact={}",
            provider_hint,
            status_code,
            method,
            path_only(url),
            duration_ms,
            path_label
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::{capture_observed_response, capture_observed_response_with_hint};
    use prismtrace_core::{HttpHeader, IpcMessage, ProcessTarget, RuntimeKind};
    use prismtrace_storage::StorageLayout;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn capture_observed_response_persists_openai_response() {
        let root = temp_root("openai-response");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = ProcessTarget {
            pid: 123,
            app_name: "Example".into(),
            executable_path: PathBuf::from("/usr/local/bin/example"),
            runtime_kind: RuntimeKind::Node,
        };
        let msg = IpcMessage::HttpResponseObserved {
            exchange_id: "ex-1".into(),
            hook_name: "fetch".into(),
            method: "POST".into(),
            url: "https://api.openai.com/v1/responses".into(),
            status_code: 200,
            headers: vec![HttpHeader {
                name: "content-type".into(),
                value: "application/json".into(),
            }],
            body_text: Some(r#"{"output":[{"type":"message"}]}"#.into()),
            body_truncated: false,
            started_at_ms: 100,
            completed_at_ms: 220,
        };

        let event = capture_observed_response(&storage, &target, &msg, 1)
            .expect("capture should succeed")
            .expect("should capture response");

        assert_eq!(event.exchange_id, "ex-1");
        assert_eq!(event.status_code, 200);
        assert!(
            event
                .summary
                .contains("[response] openai 200 POST /v1/responses 120ms")
        );
        assert!(event.artifact_path.is_file());
        let artifact =
            fs::read_to_string(event.artifact_path).expect("artifact should be readable");
        assert!(artifact.contains(r#""exchange_id":"ex-1""#));

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    #[test]
    fn capture_observed_response_redacts_cookie_headers() {
        let root = temp_root("cookie-response");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");
        let target = ProcessTarget {
            pid: 124,
            app_name: "Example".into(),
            executable_path: PathBuf::from("/usr/local/bin/example"),
            runtime_kind: RuntimeKind::Node,
        };
        let msg = IpcMessage::HttpResponseObserved {
            exchange_id: "ex-cookie".into(),
            hook_name: "fetch".into(),
            method: "POST".into(),
            url: "https://example.invalid/v1/fake-llm".into(),
            status_code: 200,
            headers: vec![
                HttpHeader {
                    name: "set-cookie".into(),
                    value: "session=secret".into(),
                },
                HttpHeader {
                    name: "content-type".into(),
                    value: "application/json".into(),
                },
            ],
            body_text: None,
            body_truncated: false,
            started_at_ms: 10,
            completed_at_ms: 20,
        };

        let event =
            capture_observed_response_with_hint(&storage, &target, &msg, 1, Some("generic-llm"))
                .expect("capture should succeed")
                .expect("should capture response");
        let artifact =
            fs::read_to_string(event.artifact_path).expect("artifact should be readable");

        assert!(!artifact.contains("session=secret"));
        assert!(artifact.contains("[redacted]"));

        fs::remove_dir_all(root).expect("temp root cleanup should succeed");
    }

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "prismtrace-response-capture-{label}-{}-{nanos}",
            process::id()
        ))
    }
}
