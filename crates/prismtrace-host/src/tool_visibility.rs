use prismtrace_core::ProcessTarget;
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedToolVisibilityEvent {
    pub request_id: String,
    pub exchange_id: String,
    pub pid: u32,
    pub target_display_name: String,
    pub provider_hint: String,
    pub visibility_stage: String,
    pub tool_count_final: usize,
    pub tool_choice: Option<String>,
    pub artifact_path: PathBuf,
    pub summary: String,
}

#[derive(Debug, Clone, Copy)]
pub struct RequestToolVisibilityCapture<'a> {
    pub request_id: &'a str,
    pub exchange_id: &'a str,
    pub provider_hint: &'a str,
    pub captured_at_ms: u64,
    pub body_text: Option<&'a str>,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedToolVisibility {
    final_tools_json: Value,
    tool_count_final: usize,
    tool_choice: Option<String>,
}

pub fn capture_request_embedded_tool_visibility(
    storage: &StorageLayout,
    target: &ProcessTarget,
    capture: RequestToolVisibilityCapture<'_>,
) -> io::Result<Option<CapturedToolVisibilityEvent>> {
    let Some(body_text) = capture.body_text else {
        return Ok(None);
    };
    let Some(parsed) = extract_request_embedded_tool_visibility(body_text) else {
        return Ok(None);
    };

    let visibility_dir = storage.artifacts_dir.join("tool_visibility");
    fs::create_dir_all(&visibility_dir)?;

    let artifact_path = visibility_dir.join(format!(
        "{}-{}-{}.json",
        capture.captured_at_ms, target.pid, capture.sequence
    ));
    let path_label = artifact_path.display().to_string();
    let visibility_stage = "request-embedded".to_string();

    fs::write(
        &artifact_path,
        json!({
            "request_id": capture.request_id,
            "exchange_id": capture.exchange_id,
            "pid": target.pid,
            "target_display_name": target.display_name(),
            "provider_hint": capture.provider_hint,
            "captured_at_ms": capture.captured_at_ms,
            "visibility_stage": &visibility_stage,
            "tool_choice": &parsed.tool_choice,
            "final_tools_json": &parsed.final_tools_json,
            "tool_count_final": parsed.tool_count_final,
        })
        .to_string(),
    )?;

    let tool_choice_summary = parsed
        .tool_choice
        .as_deref()
        .map(|choice| format!(" choice={choice}"))
        .unwrap_or_default();

    Ok(Some(CapturedToolVisibilityEvent {
        request_id: capture.request_id.to_string(),
        exchange_id: capture.exchange_id.to_string(),
        pid: target.pid,
        target_display_name: target.display_name().to_string(),
        provider_hint: capture.provider_hint.to_string(),
        visibility_stage,
        tool_count_final: parsed.tool_count_final,
        tool_choice: parsed.tool_choice,
        artifact_path,
        summary: format!(
            "[tools] {} request-embedded {} final tool(s){} artifact={}",
            capture.provider_hint, parsed.tool_count_final, tool_choice_summary, path_label
        ),
    }))
}

fn extract_request_embedded_tool_visibility(body_text: &str) -> Option<ParsedToolVisibility> {
    let payload: Value = serde_json::from_str(body_text).ok()?;
    let final_tools_json = payload
        .get("tools")
        .filter(|value| value.is_array())
        .cloned()
        .or_else(|| {
            payload
                .get("functions")
                .filter(|value| value.is_array())
                .cloned()
        });
    let tool_choice = stringify_optional_json_value(payload.get("tool_choice"));

    if final_tools_json.is_none() && tool_choice.is_none() {
        return None;
    }

    let final_tools_json = final_tools_json.unwrap_or_else(|| Value::Array(Vec::new()));
    let tool_count_final = final_tools_json
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();

    Some(ParsedToolVisibility {
        final_tools_json,
        tool_count_final,
        tool_choice,
    })
}

fn stringify_optional_json_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        other => serde_json::to_string(other).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::{RequestToolVisibilityCapture, capture_request_embedded_tool_visibility};
    use prismtrace_core::{ProcessTarget, RuntimeKind};
    use prismtrace_storage::StorageLayout;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn capture_request_embedded_tool_visibility_persists_tools_array() {
        let root = temp_root("tools-array");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");

        let event = capture_request_embedded_tool_visibility(
            &storage,
            &sample_target(),
            RequestToolVisibilityCapture {
                request_id: "req-1",
                exchange_id: "ex-1",
                provider_hint: "openai",
                captured_at_ms: 1714000004000,
                body_text: Some(
                    r#"{"model":"gpt-4.1","tool_choice":"auto","tools":[{"type":"function","function":{"name":"list_files"}}]}"#,
                ),
                sequence: 1,
            },
        )
        .expect("capture should succeed")
        .expect("visibility should be captured");

        assert_eq!(event.tool_count_final, 1);
        assert_eq!(event.tool_choice.as_deref(), Some("auto"));
        assert!(
            event
                .summary
                .contains("[tools] openai request-embedded 1 final tool(s)")
        );

        let artifact = fs::read_to_string(event.artifact_path).expect("artifact should exist");
        let payload: Value = serde_json::from_str(&artifact).expect("artifact should be json");
        assert_eq!(payload["request_id"], "req-1");
        assert_eq!(payload["tool_count_final"], 1);
        assert_eq!(payload["visibility_stage"], "request-embedded");

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn capture_request_embedded_tool_visibility_falls_back_to_functions_array() {
        let root = temp_root("functions-array");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");

        let event = capture_request_embedded_tool_visibility(
            &storage,
            &sample_target(),
            RequestToolVisibilityCapture {
                request_id: "req-2",
                exchange_id: "ex-2",
                provider_hint: "openai",
                captured_at_ms: 1714000005000,
                body_text: Some(
                    r#"{"functions":[{"name":"run_command"}],"tool_choice":{"type":"function","name":"run_command"}}"#,
                ),
                sequence: 2,
            },
        )
        .expect("capture should succeed")
        .expect("visibility should be captured");

        let artifact = fs::read_to_string(event.artifact_path).expect("artifact should exist");
        let payload: Value = serde_json::from_str(&artifact).expect("artifact should be json");
        assert_eq!(payload["tool_count_final"], 1);
        assert_eq!(
            serde_json::from_str::<Value>(
                payload["tool_choice"]
                    .as_str()
                    .expect("tool choice should be serialized as text")
            )
            .expect("tool choice text should be json"),
            serde_json::json!({
                "type": "function",
                "name": "run_command",
            })
        );

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn capture_request_embedded_tool_visibility_skips_requests_without_tools() {
        let root = temp_root("no-tools");
        let storage = StorageLayout::new(&root);
        storage.initialize().expect("storage should initialize");

        let event = capture_request_embedded_tool_visibility(
            &storage,
            &sample_target(),
            RequestToolVisibilityCapture {
                request_id: "req-3",
                exchange_id: "ex-3",
                provider_hint: "openai",
                captured_at_ms: 1714000006000,
                body_text: Some(r#"{"model":"gpt-4.1","input":"hello"}"#),
                sequence: 3,
            },
        )
        .expect("capture should succeed");

        assert!(event.is_none(), "requests without tools should be skipped");
        assert!(
            !storage.artifacts_dir.join("tool_visibility").exists(),
            "artifact dir should not be created when nothing is captured"
        );

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "prismtrace-tool-visibility-{label}-{}-{nanos}",
            process::id()
        ))
    }

    fn sample_target() -> ProcessTarget {
        ProcessTarget {
            pid: 42,
            app_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            command_line: None,
            runtime_kind: RuntimeKind::Electron,
        }
    }
}
