use prismtrace_sources::{
    ObservedEvent, ObservedEventKind, ObserverArtifactSource, ObserverArtifactWriter,
    ObserverChannelKind, ObserverHandshake, ObserverSession, ObserverSource, ObserverSourceFactory,
};
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

const DEFAULT_OPENCODE_URL: &str = "http://127.0.0.1:4096";
const DEFAULT_SESSION_LIMIT: usize = 3;
const DEFAULT_MESSAGE_LIMIT: usize = 6;
const DEFAULT_GLOBAL_EVENT_LIMIT: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpencodeObserverOptions {
    pub base_url: String,
    pub session_limit: usize,
    pub message_limit: usize,
}

impl Default for OpencodeObserverOptions {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_OPENCODE_URL.into(),
            session_limit: DEFAULT_SESSION_LIMIT,
            message_limit: DEFAULT_MESSAGE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpencodeObserverSource {
    base_url: String,
}

impl ObserverSource for OpencodeObserverSource {
    fn channel_kind(&self) -> ObserverChannelKind {
        ObserverChannelKind::OpencodeServer
    }

    fn transport_label(&self) -> String {
        self.base_url.clone()
    }

    fn connect(&self) -> io::Result<Box<dyn ObserverSession>> {
        Ok(Box::new(OpencodeObserverSession::new(
            self.base_url.clone(),
        )))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OpencodeObserverFactory;

impl ObserverSourceFactory<OpencodeObserverOptions> for OpencodeObserverFactory {
    fn build_sources(
        &self,
        request: &OpencodeObserverOptions,
    ) -> io::Result<Vec<Box<dyn ObserverSource>>> {
        Ok(vec![Box::new(OpencodeObserverSource {
            base_url: request.base_url.clone(),
        })])
    }
}

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
    let artifact_writer =
        ObserverArtifactWriter::create(storage, ObserverArtifactSource::Opencode, &handshake)?;
    writeln!(
        output,
        "{}",
        serde_json::to_string(&json!({
            "type": "opencode_observer_handshake",
            "channel": handshake.channel_kind.label(),
            "transport": handshake.transport_label,
            "server_label": handshake.server_label,
            "raw": handshake.raw_json,
        }))?
    )?;

    let mut events = session.collect_capability_events()?;
    if events.len() > options.session_limit + options.message_limit {
        events.truncate(options.session_limit + options.message_limit);
    }

    let mut emitted_messages = 0usize;
    for event in events {
        if event.event_kind == ObservedEventKind::Item {
            if emitted_messages >= options.message_limit {
                continue;
            }
            emitted_messages += 1;
        }

        artifact_writer.append_event(&event)?;
        writeln!(output, "{}", serde_json::to_string(&event_as_json(&event))?)?;
    }

    for _ in 0..DEFAULT_GLOBAL_EVENT_LIMIT {
        let Some(event) = session.next_event(Duration::from_millis(0))? else {
            break;
        };
        artifact_writer.append_event(&event)?;
        writeln!(output, "{}", serde_json::to_string(&event_as_json(&event))?)?;
    }

    Ok(())
}

fn event_as_json(event: &ObservedEvent) -> Value {
    json!({
        "type": "opencode_observer_event",
        "channel": event.channel_kind.label(),
        "event_kind": event.event_kind.label(),
        "summary": event.summary,
        "method": event.method,
        "thread_id": event.thread_id,
        "turn_id": event.turn_id,
        "item_id": event.item_id,
        "timestamp": event.timestamp,
        "raw": event.raw_json,
    })
}

struct OpencodeObserverSession {
    base_url: String,
    pending: VecDeque<ObservedEvent>,
}

impl OpencodeObserverSession {
    fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            pending: VecDeque::new(),
        }
    }

    fn get_json(&self, path: &str) -> io::Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let response = ureq::get(&url).call().map_err(|error| {
            io::Error::other(format!(
                "request to opencode observer endpoint failed: {error}"
            ))
        })?;
        let payload = response
            .into_string()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        serde_json::from_str::<Value>(&payload)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn collect_message_events(&self, session_id: &str) -> io::Result<Vec<ObservedEvent>> {
        let payload = self.get_json(&format!("/session/{session_id}/message"))?;
        Ok(payload
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|message| normalize_message_parts(session_id, message))
            .collect())
    }

    fn collect_source_capability_events(&self) -> Vec<ObservedEvent> {
        [
            ("GET /agent", "/agent", ObservedEventKind::Agent),
            ("GET /mcp", "/mcp", ObservedEventKind::Mcp),
            ("GET /provider", "/provider", ObservedEventKind::Provider),
            (
                "GET /experimental/tool/ids",
                "/experimental/tool/ids",
                ObservedEventKind::Tool,
            ),
        ]
        .into_iter()
        .map(|(method, path, event_kind)| match self.get_json(path) {
            Ok(payload) => normalize_capability_snapshot_event(method, event_kind, &payload),
            Err(error) => ObservedEvent {
                channel_kind: ObserverChannelKind::OpencodeServer,
                event_kind: ObservedEventKind::Unknown,
                summary: format!("{method} unavailable: {error}"),
                method: Some(method.to_string()),
                thread_id: None,
                turn_id: None,
                item_id: None,
                timestamp: None,
                raw_json: json!({
                    "method": method,
                    "error": error.to_string(),
                }),
            },
        })
        .collect()
    }
}

impl ObserverSession for OpencodeObserverSession {
    fn initialize(&mut self) -> io::Result<ObserverHandshake> {
        let health = self.get_json("/global/health")?;
        let version = health
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown");

        Ok(ObserverHandshake {
            channel_kind: ObserverChannelKind::OpencodeServer,
            transport_label: self.base_url.clone(),
            server_label: format!("opencode {version}"),
            raw_json: health,
        })
    }

    fn collect_capability_events(&mut self) -> io::Result<Vec<ObservedEvent>> {
        let sessions = self.get_json("/session")?;
        let mut events = self.collect_source_capability_events();

        for session in sessions.as_array().into_iter().flatten() {
            let session_event = normalize_session_event(session);
            let session_id = session_event
                .thread_id
                .clone()
                .unwrap_or_else(|| "unknown-session".to_string());
            events.push(session_event);

            match self.collect_message_events(&session_id) {
                Ok(message_events) => events.extend(message_events),
                Err(error) => events.push(ObservedEvent {
                    channel_kind: ObserverChannelKind::OpencodeServer,
                    event_kind: ObservedEventKind::Unknown,
                    summary: format!("message fetch failed for {session_id}: {error}"),
                    method: Some("GET /session/:id/message".into()),
                    thread_id: Some(session_id),
                    turn_id: None,
                    item_id: None,
                    timestamp: None,
                    raw_json: json!({ "error": error.to_string() }),
                }),
            }
        }

        Ok(events)
    }

    fn next_event(&mut self, _timeout: Duration) -> io::Result<Option<ObservedEvent>> {
        if let Some(event) = self.pending.pop_front() {
            return Ok(Some(event));
        }

        let payload = match self.get_json("/global/event") {
            Ok(payload) => payload,
            Err(error) => {
                return Ok(Some(ObservedEvent {
                    channel_kind: ObserverChannelKind::OpencodeServer,
                    event_kind: ObservedEventKind::Unknown,
                    summary: format!("global event fetch failed: {error}"),
                    method: Some("GET /global/event".into()),
                    thread_id: None,
                    turn_id: None,
                    item_id: None,
                    timestamp: None,
                    raw_json: json!({ "error": error.to_string() }),
                }));
            }
        };

        for raw in global_event_values(&payload) {
            self.pending.push_back(normalize_global_event(raw));
        }

        Ok(self.pending.pop_front())
    }
}

fn session_summary(session: &Value) -> String {
    let title = session
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("untitled session");
    let directory = session
        .get("directory")
        .and_then(Value::as_str)
        .unwrap_or("unknown directory");
    format!("{title} @ {directory}")
}

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
    let role = info
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
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
            let part_type = part
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
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
                    part.get("tool")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown tool")
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

fn global_event_values(payload: &Value) -> Vec<&Value> {
    if let Some(events) = payload.as_array() {
        return events.iter().collect();
    }
    if let Some(events) = payload.get("events").and_then(Value::as_array) {
        return events.iter().collect();
    }
    if payload.get("type").is_some() || payload.get("event").is_some() {
        return vec![payload];
    }
    Vec::new()
}

fn normalize_global_event(raw: &Value) -> ObservedEvent {
    let event_type = string_field(raw, &["type", "event"]).unwrap_or_else(|| "unknown".into());
    let normalized_type = event_type.to_ascii_lowercase();
    let event_kind =
        if normalized_type.contains("permission") || normalized_type.contains("approval") {
            ObservedEventKind::Approval
        } else if normalized_type.contains("agent") {
            ObservedEventKind::Agent
        } else if normalized_type.contains("mcp") {
            ObservedEventKind::Mcp
        } else if normalized_type.contains("provider") {
            ObservedEventKind::Provider
        } else if normalized_type.contains("plugin") {
            ObservedEventKind::Plugin
        } else if normalized_type.contains("command") {
            ObservedEventKind::Command
        } else if normalized_type.contains("app") {
            ObservedEventKind::App
        } else if normalized_type.contains("tool") {
            ObservedEventKind::Tool
        } else if normalized_type.contains("session") || normalized_type.contains("thread") {
            ObservedEventKind::Thread
        } else if normalized_type.contains("message")
            || normalized_type.contains("text")
            || normalized_type.contains("reasoning")
            || normalized_type.contains("part")
        {
            ObservedEventKind::Item
        } else {
            ObservedEventKind::Unknown
        };

    ObservedEvent {
        channel_kind: ObserverChannelKind::OpencodeServer,
        event_kind,
        summary: string_field(raw, &["message", "summary", "title"])
            .unwrap_or_else(|| format!("opencode event: {event_type}")),
        method: Some("GET /global/event".into()),
        thread_id: string_field(raw, &["sessionID", "sessionId", "session_id", "thread_id"]),
        turn_id: string_field(raw, &["messageID", "messageId", "message_id", "turn_id"]),
        item_id: string_field(raw, &["partID", "partId", "part_id", "item_id"]),
        timestamp: timestamp_field(raw, &["time", "timestamp", "created", "updated"]),
        raw_json: raw.clone(),
    }
}

fn normalize_capability_snapshot_event(
    method: &str,
    event_kind: ObservedEventKind,
    payload: &Value,
) -> ObservedEvent {
    let names = match event_kind {
        ObservedEventKind::Agent => {
            capability_names_from_value(payload, &["agents", "data", "all"])
        }
        ObservedEventKind::Mcp => {
            capability_names_from_value(payload, &["mcp", "servers", "data", "all"])
        }
        ObservedEventKind::Provider => {
            capability_names_from_value(payload, &["providers", "all", "data", "connected"])
        }
        ObservedEventKind::Tool => {
            capability_names_from_value(payload, &["tools", "ids", "data", "all"])
        }
        _ => Vec::new(),
    };
    let raw_json = match event_kind {
        ObservedEventKind::Agent => json!({
            "method": method,
            "agent_count": names.len(),
            "agent_names_preview": names,
            "source": payload,
        }),
        ObservedEventKind::Mcp => json!({
            "method": method,
            "mcp_server_count": names.len(),
            "mcp_server_names_preview": names,
            "source": payload,
        }),
        ObservedEventKind::Provider => json!({
            "method": method,
            "provider_count": names.len(),
            "provider_names_preview": names,
            "source": payload,
        }),
        ObservedEventKind::Tool => json!({
            "method": method,
            "tool_count": names.len(),
            "tools": names,
            "source": payload,
        }),
        _ => json!({
            "method": method,
            "source": payload,
        }),
    };
    let count = names_len_from_capability_raw(&raw_json, event_kind);

    ObservedEvent {
        channel_kind: ObserverChannelKind::OpencodeServer,
        event_kind,
        summary: format!("{method} returned {count} entries"),
        method: Some(method.to_string()),
        thread_id: None,
        turn_id: None,
        item_id: None,
        timestamp: None,
        raw_json,
    }
}

fn names_len_from_capability_raw(raw_json: &Value, event_kind: ObservedEventKind) -> usize {
    let key = match event_kind {
        ObservedEventKind::Agent => "agent_names_preview",
        ObservedEventKind::Skill => "skill_names_preview",
        ObservedEventKind::Mcp => "mcp_server_names_preview",
        ObservedEventKind::Provider => "provider_names_preview",
        ObservedEventKind::Plugin => "plugin_names_preview",
        ObservedEventKind::App => "app_names_preview",
        ObservedEventKind::Tool => "tools",
        _ => return 0,
    };
    raw_json
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn capability_names_from_value(value: &Value, collection_keys: &[&str]) -> Vec<String> {
    let mut names = Vec::new();
    collect_capability_names(value, collection_keys, &mut names);
    names.sort();
    names.dedup();
    names
}

fn collect_capability_names(value: &Value, collection_keys: &[&str], names: &mut Vec<String>) {
    match value {
        Value::String(name) => push_capability_name(names, name),
        Value::Array(items) => {
            for item in items {
                collect_capability_names(item, collection_keys, names);
            }
        }
        Value::Object(object) => {
            if let Some(name) = ["name", "id", "title", "label", "slug"]
                .into_iter()
                .find_map(|key| object.get(key).and_then(Value::as_str))
            {
                push_capability_name(names, name);
                return;
            }

            let mut used_collection_key = false;
            for key in collection_keys {
                if let Some(child) = object.get(*key) {
                    used_collection_key = true;
                    collect_capability_names(child, collection_keys, names);
                }
            }

            if !used_collection_key {
                for (key, child) in object {
                    if child.is_object() || child.is_array() || child.is_boolean() {
                        push_capability_name(names, key);
                    }
                }
            }
        }
        _ => {}
    }
}

fn push_capability_name(names: &mut Vec<String>, name: &str) {
    let name = name.trim();
    if !name.is_empty() {
        names.push(name.to_string());
    }
}

fn string_field(raw: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| raw.get(*key).and_then(Value::as_str))
        .map(str::to_string)
}

fn timestamp_field(raw: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let value = raw.get(*key)?;
        value
            .as_str()
            .map(str::to_string)
            .or_else(|| value.as_i64().map(|value| value.to_string()))
            .or_else(|| value.as_u64().map(|value| value.to_string()))
    })
}

fn truncate(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
pub(crate) struct TestOpencodeServer {
    base_url: String,
    _server_thread: std::thread::JoinHandle<io::Result<()>>,
}

#[cfg(test)]
impl TestOpencodeServer {
    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
pub(crate) fn spawn_test_opencode_server() -> io::Result<TestOpencodeServer> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};

    fn response_body(path: &str) -> Option<&'static str> {
        match path {
            "/global/health" => Some(r#"{"version":"test"}"#),
            "/session" => {
                Some(r#"[{"id":"session-1","title":"demo","directory":"/tmp/demo","updated":1}]"#)
            }
            "/agent" => Some(r#"[{"name":"build"},{"id":"review"}]"#),
            "/mcp" => Some(r#"{"github":{"status":"connected"}}"#),
            "/provider" => Some(r#"{"all":[{"id":"anthropic"},{"name":"openai"}]}"#),
            "/experimental/tool/ids" => Some(r#"["bash","edit"]"#),
            "/session/session-1/message" => Some(
                r#"[{"info":{"role":"user","id":"turn-1","time":{"created":1}},"parts":[{"id":"part-1","type":"text","text":"hello"}]}]"#,
            ),
            "/global/event" => Some(
                r#"[{"type":"permission.updated","sessionID":"session-1","message":"waiting for approval","time":2}]"#,
            ),
            _ => None,
        }
    }

    fn write_response(mut stream: TcpStream, status: &str, body: &str) -> io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )?;
        stream.flush()
    }

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base_url = format!("http://{}", listener.local_addr()?);
    let server_thread = std::thread::spawn(move || -> io::Result<()> {
        for _ in 0..8 {
            let (stream, _) = listener.accept()?;
            let mut reader = BufReader::new(stream.try_clone()?);
            let mut request_line = String::new();
            reader.read_line(&mut request_line)?;

            let path = request_line.split_whitespace().nth(1).unwrap_or("/");

            loop {
                let mut header_line = String::new();
                reader.read_line(&mut header_line)?;
                if header_line == "\r\n" || header_line.is_empty() {
                    break;
                }
            }

            if let Some(body) = response_body(path) {
                write_response(stream, "200 OK", body)?;
            } else {
                write_response(stream, "404 Not Found", r#"{"error":"not found"}"#)?;
            }
        }

        Ok(())
    });

    Ok(TestOpencodeServer {
        base_url,
        _server_thread: server_thread,
    })
}

#[cfg(test)]
mod tests {
    use super::{OpencodeObserverOptions, spawn_test_opencode_server, truncate};
    use prismtrace_sources::{
        ObservedEvent, ObservedEventKind, ObserverArtifactSource, ObserverArtifactWriter,
        ObserverChannelKind, ObserverHandshake,
    };
    use serde_json::json;
    use std::io;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_opencode_observer_options_are_stable() {
        let options = OpencodeObserverOptions::default();
        assert_eq!(options.base_url, "http://127.0.0.1:4096");
        assert_eq!(options.session_limit, 3);
        assert_eq!(options.message_limit, 6);
    }

    #[test]
    fn truncate_adds_suffix_when_text_exceeds_limit() {
        assert_eq!(truncate("abcdef", 3), "abc...");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn session_snapshot_maps_to_thread_event() {
        let event = super::normalize_session_event(&json!({
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
        let events = super::normalize_message_parts(
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

    #[test]
    fn global_event_maps_permission_to_approval() {
        let event = super::normalize_global_event(&json!({
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
        let event = super::normalize_global_event(&json!({
            "type": "mystery.event",
            "sessionID": "session-2"
        }));

        assert_eq!(event.event_kind, ObservedEventKind::Unknown);
        assert!(event.summary.contains("mystery.event"));
    }

    #[test]
    fn capability_snapshot_keeps_opencode_domains_distinct() {
        let agent_event = super::normalize_capability_snapshot_event(
            "GET /agent",
            ObservedEventKind::Agent,
            &json!([{ "name": "build" }, { "id": "review" }]),
        );
        let mcp_event = super::normalize_capability_snapshot_event(
            "GET /mcp",
            ObservedEventKind::Mcp,
            &json!({ "github": { "status": "connected" } }),
        );
        let provider_event = super::normalize_capability_snapshot_event(
            "GET /provider",
            ObservedEventKind::Provider,
            &json!({ "all": [{ "id": "anthropic" }, { "name": "openai" }] }),
        );
        let tool_event = super::normalize_capability_snapshot_event(
            "GET /experimental/tool/ids",
            ObservedEventKind::Tool,
            &json!(["bash", "edit"]),
        );

        assert_eq!(agent_event.event_kind, ObservedEventKind::Agent);
        assert_eq!(
            agent_event.raw_json["agent_names_preview"],
            json!(["build", "review"])
        );
        assert_eq!(mcp_event.event_kind, ObservedEventKind::Mcp);
        assert_eq!(
            mcp_event.raw_json["mcp_server_names_preview"],
            json!(["github"])
        );
        assert_eq!(provider_event.event_kind, ObservedEventKind::Provider);
        assert_eq!(
            provider_event.raw_json["provider_names_preview"],
            json!(["anthropic", "openai"])
        );
        assert_eq!(tool_event.raw_json["tools"], json!(["bash", "edit"]));
        assert!(agent_event.summary.contains("2 entries"));
    }

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
        let writer = ObserverArtifactWriter::create(
            &result.storage,
            ObserverArtifactSource::Opencode,
            &handshake,
        )?;
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
        assert!(
            writer
                .artifact_path()
                .to_string_lossy()
                .contains(".prismtrace/state/artifacts/observer_events/opencode/")
        );
        std::fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn run_opencode_observer_writes_artifact_records() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = crate::bootstrap(&workspace_root)?;
        let server = spawn_test_opencode_server()?;

        let mut output = Vec::new();
        super::run_opencode_observer(
            &result.storage,
            &mut output,
            OpencodeObserverOptions {
                base_url: server.base_url().into(),
                session_limit: 8,
                message_limit: 8,
            },
        )?;

        let observer_dir = result
            .storage
            .artifacts_dir
            .join("observer_events")
            .join("opencode");
        assert!(observer_dir.is_dir());
        let artifact_path = std::fs::read_dir(&observer_dir)?
            .find_map(|entry| entry.ok().map(|entry| entry.path()))
            .expect("artifact should exist");
        let artifact = std::fs::read_to_string(artifact_path)?;
        assert!(artifact.contains("\"record_type\":\"handshake\""));
        assert!(artifact.contains("\"record_type\":\"event\""));
        assert!(String::from_utf8_lossy(&output).contains("opencode_observer_event"));
        std::fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("prismtrace-host-test-{}-{}", process::id(), nanos))
    }
}
