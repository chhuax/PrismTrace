use crate::observer::{
    ObservedEvent, ObservedEventKind, ObserverChannelKind, ObserverHandshake, ObserverSession,
    ObserverSource, ObserverSourceFactory,
};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

const DEFAULT_OPENCODE_URL: &str = "http://127.0.0.1:4096";
const DEFAULT_SESSION_LIMIT: usize = 3;
const DEFAULT_MESSAGE_LIMIT: usize = 6;

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

    fn collect_message_events(&self, session_id: &str) -> io::Result<Vec<ObservedEvent>> {
        let payload = self.get_json(&format!("/session/{session_id}/message"))?;
        let mut events = Vec::new();

        for message in payload.as_array().into_iter().flatten() {
            let info = message.get("info").cloned().unwrap_or(Value::Null);
            let parts = message
                .get("parts")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let role = info
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let message_id = info.get("id").and_then(Value::as_str).map(str::to_string);
            let created = info
                .get("time")
                .and_then(|time| time.get("created"))
                .and_then(Value::as_i64)
                .map(|value| value.to_string());

            for part in parts {
                let part_type = part
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let summary = match part_type {
                    "text" | "reasoning" => {
                        let text = part.get("text").and_then(Value::as_str).unwrap_or("");
                        format!("{role} {part_type}: {}", truncate(text, 120))
                    }
                    "tool" => {
                        let tool_name = part
                            .get("tool")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown tool");
                        format!("{role} tool: {tool_name}")
                    }
                    other => format!("{role} {other}"),
                };

                events.push(ObservedEvent {
                    channel_kind: ObserverChannelKind::OpencodeServer,
                    event_kind: if part_type == "tool" {
                        ObservedEventKind::Tool
                    } else {
                        ObservedEventKind::Item
                    },
                    summary,
                    method: Some("GET /session/:id/message".into()),
                    thread_id: Some(session_id.to_string()),
                    turn_id: message_id.clone(),
                    item_id: part.get("id").and_then(Value::as_str).map(str::to_string),
                    timestamp: created.clone(),
                    raw_json: json!({
                        "info": info,
                        "part": part,
                    }),
                });
            }
        }

        Ok(events)
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
        let mut events = Vec::new();

        for session in sessions.as_array().into_iter().flatten() {
            let session_id = session
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("unknown-session")
                .to_string();

            events.push(ObservedEvent {
                channel_kind: ObserverChannelKind::OpencodeServer,
                event_kind: ObservedEventKind::Thread,
                summary: Self::session_summary(session),
                method: Some("GET /session".into()),
                thread_id: Some(session_id.clone()),
                turn_id: None,
                item_id: None,
                timestamp: session
                    .get("updated")
                    .and_then(Value::as_i64)
                    .map(|value| value.to_string()),
                raw_json: session.clone(),
            });

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
        Ok(self.pending.pop_front())
    }
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
mod tests {
    use super::{OpencodeObserverOptions, truncate};

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
}
