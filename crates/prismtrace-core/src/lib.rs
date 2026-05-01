use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Node,
    Electron,
    Unknown,
}

impl RuntimeKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Electron => "electron",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTarget {
    pub pid: u32,
    pub app_name: String,
    pub executable_path: PathBuf,
    pub command_line: Option<String>,
    pub runtime_kind: RuntimeKind,
}

impl ProcessTarget {
    pub fn display_name(&self) -> &str {
        if !self.app_name.is_empty() {
            return &self.app_name;
        }

        self.executable_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSample {
    pub pid: u32,
    pub process_name: String,
    pub executable_path: PathBuf,
    pub command_line: Option<String>,
}

impl ProcessSample {
    fn is_packaged_node_cli(&self) -> bool {
        let process_name = self.process_name.to_ascii_lowercase();
        let executable_path = self.executable_path.to_string_lossy().to_ascii_lowercase();
        let command_line = self
            .command_line
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();

        (process_name == "opencode"
            && (executable_path == "opencode"
                || executable_path.ends_with("/.opencode/bin/opencode")))
            || (executable_path.ends_with("/applications/codex.app/contents/resources/codex")
                && command_line.contains(" app-server"))
    }

    fn is_packaged_electron_app(&self) -> bool {
        let executable_path = self.executable_path.to_string_lossy().to_ascii_lowercase();
        let command_line = self
            .command_line
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();

        executable_path.contains(".app/contents/macos/")
            && (command_line.contains(".app/contents/resources/app.asar")
                || executable_path.ends_with("/applications/codex.app/contents/macos/codex"))
    }

    fn first_script_argument<'a, I>(parts: &mut I) -> Option<&'a str>
    where
        I: Iterator<Item = &'a str>,
    {
        while let Some(part) = parts.next() {
            if part == "--" {
                return parts.next();
            }

            if matches!(
                part,
                "-r" | "--require" | "--loader" | "--import" | "-e" | "--eval" | "-p" | "--print"
            ) {
                let _ = parts.next();
                continue;
            }

            if part.starts_with("--require=")
                || part.starts_with("--loader=")
                || part.starts_with("--import=")
                || part.starts_with("--eval=")
                || part.starts_with("--print=")
            {
                continue;
            }

            if part.starts_with('-') {
                continue;
            }

            return Some(part);
        }

        None
    }

    fn script_name_from_command_line(&self) -> Option<String> {
        let command_line = self.command_line.as_deref()?;
        let mut parts = command_line.split_whitespace();
        let _runtime = parts.next()?;

        let script = Self::first_script_argument(&mut parts)?;
        let script_name = std::path::Path::new(script)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(script);

        Some(
            script_name
                .trim_end_matches(".js")
                .trim_end_matches(".mjs")
                .trim_end_matches(".cjs")
                .to_string(),
        )
    }

    pub fn runtime_kind(&self) -> RuntimeKind {
        let process_name = self.process_name.to_ascii_lowercase();
        let executable_name = self
            .executable_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if process_name == "node" || executable_name == "node" || self.is_packaged_node_cli() {
            RuntimeKind::Node
        } else if process_name == "electron"
            || executable_name == "electron"
            || self.is_packaged_electron_app()
            || self
                .executable_path
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("electron")
        {
            RuntimeKind::Electron
        } else {
            RuntimeKind::Unknown
        }
    }

    pub fn normalized_app_name(&self) -> String {
        let is_generic_runtime_name = matches!(
            (
                self.runtime_kind(),
                self.process_name.to_ascii_lowercase().as_str()
            ),
            (RuntimeKind::Node, "node") | (RuntimeKind::Electron, "electron")
        );

        if !self.process_name.trim().is_empty() && !is_generic_runtime_name {
            return self.process_name.clone();
        }

        if let Some(script_name) = self.script_name_from_command_line()
            && !script_name.trim().is_empty()
        {
            return script_name;
        }

        self.executable_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn into_target(&self) -> ProcessTarget {
        ProcessTarget {
            pid: self.pid,
            app_name: self.normalized_app_name(),
            executable_path: self.executable_path.clone(),
            command_line: self.command_line.clone(),
            runtime_kind: self.runtime_kind(),
        }
    }
}

/// Structured error returned when an IPC line cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcParseError {
    pub kind: IpcParseErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcParseErrorKind {
    /// The input was not valid JSON.
    InvalidJson,
    /// The JSON was valid but did not match any known message variant.
    UnknownVariant,
}

impl std::fmt::Display for IpcParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.message)
    }
}

impl std::error::Error for IpcParseError {}

/// IPC messages exchanged between the host and the injected probe.
///
/// Wire format: one JSON object per line, `type` field as discriminant.
///
/// ```json
/// {"type":"heartbeat","timestamp_ms":1714000000000}
/// {"type":"bootstrap_report","installed_hooks":["fetch"],"failed_hooks":[],"timestamp_ms":1714000001000}
/// {"type":"detach_ack","timestamp_ms":1714000002000}
/// ```
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
        exchange_id: String,
        hook_name: String,
        method: String,
        url: String,
        headers: Vec<HttpHeader>,
        body_text: Option<String>,
        body_truncated: bool,
        timestamp_ms: u64,
    },
    HttpResponseObserved {
        exchange_id: String,
        hook_name: String,
        method: String,
        url: String,
        status_code: u16,
        headers: Vec<HttpHeader>,
        body_text: Option<String>,
        body_truncated: bool,
        started_at_ms: u64,
        completed_at_ms: u64,
    },
    DetachAck {
        timestamp_ms: u64,
    },
}

impl IpcMessage {
    /// Serialize to a newline-terminated JSON string.
    pub fn to_json_line(&self) -> String {
        let mut s = serde_json::to_string(self).expect("IpcMessage serialization is infallible");
        s.push('\n');
        s
    }

    /// Deserialize from a single JSON line (trailing newline is ignored).
    pub fn from_json_line(s: &str) -> Result<Self, IpcParseError> {
        serde_json::from_str(s.trim_end_matches('\n')).map_err(|e| {
            // Distinguish "unknown variant / missing field" from "invalid JSON"
            let kind = if e.is_data() {
                IpcParseErrorKind::UnknownVariant
            } else {
                IpcParseErrorKind::InvalidJson
            };
            IpcParseError {
                kind,
                message: e.to_string(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HttpHeader, IpcMessage, IpcParseErrorKind, ProcessSample, ProcessTarget, RuntimeKind,
    };
    use std::path::PathBuf;

    #[test]
    fn runtime_kind_labels_are_stable() {
        assert_eq!(RuntimeKind::Node.label(), "node");
        assert_eq!(RuntimeKind::Electron.label(), "electron");
        assert_eq!(RuntimeKind::Unknown.label(), "unknown");
    }

    #[test]
    fn process_target_display_name_falls_back_to_executable_name() {
        let target = ProcessTarget {
            pid: 42,
            app_name: String::new(),
            executable_path: PathBuf::from("/Applications/Example.app/Contents/MacOS/Example"),
            command_line: None,
            runtime_kind: RuntimeKind::Electron,
        };

        assert_eq!(target.display_name(), "Example");
    }

    #[test]
    fn process_sample_classifies_node_processes() {
        let sample = ProcessSample {
            pid: 7,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            command_line: None,
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Node);
    }

    #[test]
    fn process_sample_classifies_electron_processes() {
        let sample = ProcessSample {
            pid: 8,
            process_name: "Electron".into(),
            executable_path: PathBuf::from("/Applications/Electron.app/Contents/MacOS/Electron"),
            command_line: None,
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Electron);
    }

    #[test]
    fn process_sample_classifies_packaged_opencode_binary_as_node() {
        let sample = ProcessSample {
            pid: 8,
            process_name: "opencode".into(),
            executable_path: PathBuf::from("/Users/test/.opencode/bin/opencode"),
            command_line: Some("/Users/test/.opencode/bin/opencode".into()),
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Node);
    }

    #[test]
    fn process_sample_classifies_bare_opencode_process_name_as_node() {
        let sample = ProcessSample {
            pid: 8,
            process_name: "opencode".into(),
            executable_path: PathBuf::from("opencode"),
            command_line: Some("opencode".into()),
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Node);
    }

    #[test]
    fn process_sample_classifies_codex_main_app_as_electron() {
        let sample = ProcessSample {
            pid: 8,
            process_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            command_line: Some("/Applications/Codex.app/Contents/MacOS/Codex".into()),
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Electron);
    }

    #[test]
    fn process_sample_classifies_codex_app_server_as_node() {
        let sample = ProcessSample {
            pid: 8,
            process_name: "codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
            command_line: Some(
                "/Applications/Codex.app/Contents/Resources/codex app-server --analytics-default-enabled"
                    .into(),
            ),
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Node);
    }

    #[test]
    fn process_sample_keeps_unknown_when_no_runtime_matches() {
        let sample = ProcessSample {
            pid: 9,
            process_name: "python3".into(),
            executable_path: PathBuf::from("/usr/bin/python3"),
            command_line: None,
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Unknown);
    }

    #[test]
    fn process_sample_normalizes_generic_runtime_names_to_executable_name() {
        let sample = ProcessSample {
            pid: 10,
            process_name: "node".into(),
            executable_path: PathBuf::from(
                "/Applications/Claude Code.app/Contents/MacOS/Claude Code",
            ),
            command_line: None,
        };

        assert_eq!(sample.normalized_app_name(), "Claude Code");
    }

    #[test]
    fn process_sample_normalizes_generic_runtime_names_to_script_name_from_command_line() {
        let sample = ProcessSample {
            pid: 10,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            command_line: Some(
                "node /Users/test/.cache/opencode/packages/yaml-language-server/node_modules/.bin/yaml-language-server --stdio".into(),
            ),
        };

        assert_eq!(sample.normalized_app_name(), "yaml-language-server");
    }

    #[test]
    fn process_sample_skips_common_node_option_value_pairs_when_finding_script_name() {
        let sample = ProcessSample {
            pid: 12,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            command_line: Some("node -r ts-node/register /tmp/app.js".into()),
        };

        assert_eq!(sample.normalized_app_name(), "app");
    }

    #[test]
    fn process_sample_converts_to_structured_target() {
        let sample = ProcessSample {
            pid: 11,
            process_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            command_line: Some("/Applications/Codex.app/Contents/MacOS/Codex".into()),
        };

        let target = sample.into_target();

        assert_eq!(target.pid, 11);
        assert_eq!(target.app_name, "Codex");
        assert_eq!(
            target.command_line,
            Some("/Applications/Codex.app/Contents/MacOS/Codex".into())
        );
        assert_eq!(target.runtime_kind, RuntimeKind::Electron);
    }

    // --- IpcMessage tests ---
    #[test]
    fn ipc_message_heartbeat_round_trip() {
        let msg = IpcMessage::Heartbeat {
            timestamp_ms: 1714000000000,
        };
        let line = msg.to_json_line();
        let parsed = IpcMessage::from_json_line(&line).expect("should parse heartbeat");
        assert_eq!(parsed, msg);
    }

    #[test]
    fn ipc_message_bootstrap_report_round_trip() {
        let msg = IpcMessage::BootstrapReport {
            installed_hooks: vec!["fetch".into(), "undici".into()],
            failed_hooks: vec![],
            timestamp_ms: 1714000001000,
        };
        let line = msg.to_json_line();
        let parsed = IpcMessage::from_json_line(&line).expect("should parse bootstrap_report");
        assert_eq!(parsed, msg);
    }

    #[test]
    fn ipc_message_http_request_observed_round_trip_with_exchange_id() {
        let msg = IpcMessage::HttpRequestObserved {
            exchange_id: "ex-1".into(),
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
            body_truncated: false,
            timestamp_ms: 1_714_000_003_000,
        };
        let line = msg.to_json_line();
        let parsed = IpcMessage::from_json_line(&line).expect("should parse request event");
        assert_eq!(parsed, msg);
    }

    #[test]
    fn ipc_message_http_response_observed_round_trip() {
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
            body_text: Some(
                r#"{"output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]}"#
                    .into(),
            ),
            body_truncated: false,
            started_at_ms: 100,
            completed_at_ms: 180,
        };
        let line = msg.to_json_line();
        let parsed = IpcMessage::from_json_line(&line).expect("should parse response event");
        assert_eq!(parsed, msg);
    }

    #[test]
    fn ipc_message_http_request_observed_parses_without_body() {
        let line = r#"{"type":"http_request_observed","exchange_id":"ex-2","hook_name":"http","method":"GET","url":"https://openrouter.ai/api/v1/chat/completions","headers":[],"body_text":null,"body_truncated":false,"timestamp_ms":9}"#;
        let parsed = IpcMessage::from_json_line(line).expect("should parse request without body");
        assert_eq!(
            parsed,
            IpcMessage::HttpRequestObserved {
                exchange_id: "ex-2".into(),
                hook_name: "http".into(),
                method: "GET".into(),
                url: "https://openrouter.ai/api/v1/chat/completions".into(),
                headers: vec![],
                body_text: None,
                body_truncated: false,
                timestamp_ms: 9,
            }
        );
    }

    #[test]
    fn ipc_message_detach_ack_round_trip() {
        let msg = IpcMessage::DetachAck {
            timestamp_ms: 1714000002000,
        };
        let line = msg.to_json_line();
        let parsed = IpcMessage::from_json_line(&line).expect("should parse detach_ack");
        assert_eq!(parsed, msg);
    }

    #[test]
    fn ipc_message_malformed_input_returns_invalid_json_error() {
        let result = IpcMessage::from_json_line("not json at all");
        let err = result.expect_err("should fail on malformed input");
        assert_eq!(err.kind, IpcParseErrorKind::InvalidJson);
    }

    #[test]
    fn ipc_message_unknown_variant_returns_unknown_variant_error() {
        let result = IpcMessage::from_json_line(r#"{"type":"unknown_type","timestamp_ms":0}"#);
        let err = result.expect_err("should fail on unknown variant");
        assert_eq!(err.kind, IpcParseErrorKind::UnknownVariant);
    }

    #[test]
    fn ipc_message_trailing_newline_is_handled_correctly() {
        let msg = IpcMessage::Heartbeat { timestamp_ms: 42 };
        let without_newline = r#"{"type":"heartbeat","timestamp_ms":42}"#;
        let with_newline = format!("{}\n", without_newline);
        let parsed_plain = IpcMessage::from_json_line(without_newline).expect("plain should parse");
        let parsed_newline =
            IpcMessage::from_json_line(&with_newline).expect("with newline should parse");
        assert_eq!(parsed_plain, msg);
        assert_eq!(parsed_newline, msg);
    }
}
