use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverChannelKind {
    CodexAppServer,
    ClaudeCodeTranscript,
    OpencodeServer,
}

impl ObserverChannelKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::CodexAppServer => "codex-app-server",
            Self::ClaudeCodeTranscript => "claude-code",
            Self::OpencodeServer => "opencode-server",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedEventKind {
    Thread,
    Turn,
    Item,
    Tool,
    Approval,
    Hook,
    Agent,
    Command,
    Mcp,
    Provider,
    Plugin,
    Skill,
    App,
    Unknown,
}

impl ObservedEventKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Thread => "thread",
            Self::Turn => "turn",
            Self::Item => "item",
            Self::Tool => "tool",
            Self::Approval => "approval",
            Self::Hook => "hook",
            Self::Agent => "agent",
            Self::Command => "command",
            Self::Mcp => "mcp",
            Self::Provider => "provider",
            Self::Plugin => "plugin",
            Self::Skill => "skill",
            Self::App => "app",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObserverHandshake {
    pub channel_kind: ObserverChannelKind,
    pub transport_label: String,
    pub server_label: String,
    pub raw_json: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservedEvent {
    pub channel_kind: ObserverChannelKind,
    pub event_kind: ObservedEventKind,
    pub summary: String,
    pub method: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub item_id: Option<String>,
    pub timestamp: Option<String>,
    pub raw_json: Value,
}

pub trait ObserverSession {
    fn initialize(&mut self) -> io::Result<ObserverHandshake>;

    fn collect_capability_events(&mut self) -> io::Result<Vec<ObservedEvent>>;

    fn next_event(&mut self, timeout: Duration) -> io::Result<Option<ObservedEvent>>;
}

pub trait ObserverSource {
    fn channel_kind(&self) -> ObserverChannelKind;

    fn transport_label(&self) -> String;

    fn connect(&self) -> io::Result<Box<dyn ObserverSession>>;
}

pub trait ObserverSourceFactory<Request> {
    fn build_sources(&self, request: &Request) -> io::Result<Vec<Box<dyn ObserverSource>>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverArtifactSource {
    Codex,
    ClaudeCode,
    Opencode,
}

impl ObserverArtifactSource {
    pub const fn directory_name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::Opencode => "opencode",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserverArtifactWriter {
    artifact_path: PathBuf,
}

impl ObserverArtifactWriter {
    pub fn create(
        storage: &StorageLayout,
        source: ObserverArtifactSource,
        handshake: &ObserverHandshake,
    ) -> io::Result<Self> {
        let observer_dir = storage
            .artifacts_dir
            .join("observer_events")
            .join(source.directory_name());
        fs::create_dir_all(&observer_dir)?;

        let started_at_ms = current_time_ms()?;
        let artifact_path =
            observer_dir.join(format!("{started_at_ms}-{}.jsonl", std::process::id()));
        let writer = Self { artifact_path };
        writer.append_json_line(&handshake_record(handshake, started_at_ms))?;

        Ok(writer)
    }

    pub fn append_event(&self, event: &ObservedEvent) -> io::Result<()> {
        self.append_json_line(&event_record(event, current_time_ms()?))
    }

    pub fn artifact_path(&self) -> &Path {
        &self.artifact_path
    }

    fn append_json_line(&self, value: &Value) -> io::Result<()> {
        let mut artifact = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.artifact_path)?;
        serde_json::to_writer(&mut artifact, value)?;
        artifact.write_all(b"\n")?;
        artifact.flush()?;
        Ok(())
    }
}

fn handshake_record(handshake: &ObserverHandshake, recorded_at_ms: u64) -> Value {
    json!({
        "record_type": "handshake",
        "channel": handshake.channel_kind.label(),
        "transport": handshake.transport_label,
        "server_label": handshake.server_label,
        "recorded_at_ms": recorded_at_ms,
        "raw_json": handshake.raw_json,
    })
}

fn event_record(event: &ObservedEvent, recorded_at_ms: u64) -> Value {
    json!({
        "record_type": "event",
        "channel": event.channel_kind.label(),
        "event_kind": event.event_kind.label(),
        "summary": event.summary,
        "method": event.method,
        "thread_id": event.thread_id,
        "turn_id": event.turn_id,
        "item_id": event.item_id,
        "timestamp": event.timestamp,
        "recorded_at_ms": recorded_at_ms,
        "raw_json": event.raw_json,
    })
}

fn current_time_ms() -> io::Result<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    Ok(duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::{
        ObservedEvent, ObservedEventKind, ObserverArtifactSource, ObserverArtifactWriter,
        ObserverChannelKind, ObserverHandshake,
    };
    use prismtrace_storage::StorageLayout;
    use serde_json::json;
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn observer_channel_kind_label_is_stable() {
        assert_eq!(
            ObserverChannelKind::CodexAppServer.label(),
            "codex-app-server"
        );
        assert_eq!(
            ObserverChannelKind::ClaudeCodeTranscript.label(),
            "claude-code"
        );
        assert_eq!(
            ObserverChannelKind::OpencodeServer.label(),
            "opencode-server"
        );
    }

    #[test]
    fn observed_event_kind_labels_cover_minimal_surface() {
        assert_eq!(ObservedEventKind::Thread.label(), "thread");
        assert_eq!(ObservedEventKind::Turn.label(), "turn");
        assert_eq!(ObservedEventKind::Item.label(), "item");
        assert_eq!(ObservedEventKind::Tool.label(), "tool");
        assert_eq!(ObservedEventKind::Approval.label(), "approval");
        assert_eq!(ObservedEventKind::Hook.label(), "hook");
        assert_eq!(ObservedEventKind::Agent.label(), "agent");
        assert_eq!(ObservedEventKind::Command.label(), "command");
        assert_eq!(ObservedEventKind::Mcp.label(), "mcp");
        assert_eq!(ObservedEventKind::Provider.label(), "provider");
        assert_eq!(ObservedEventKind::Plugin.label(), "plugin");
        assert_eq!(ObservedEventKind::Skill.label(), "skill");
        assert_eq!(ObservedEventKind::App.label(), "app");
        assert_eq!(ObservedEventKind::Unknown.label(), "unknown");
    }

    #[test]
    fn observer_artifact_writer_persists_handshake_and_event_under_source_directory()
    -> io::Result<()> {
        let root = unique_temp_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;

        let writer = ObserverArtifactWriter::create(
            &storage,
            ObserverArtifactSource::Codex,
            &ObserverHandshake {
                channel_kind: ObserverChannelKind::CodexAppServer,
                transport_label: "proxy-socket (/tmp/codex.sock)".into(),
                server_label: "Codex Test".into(),
                raw_json: json!({ "version": "test" }),
            },
        )?;
        writer.append_event(&ObservedEvent {
            channel_kind: ObserverChannelKind::CodexAppServer,
            event_kind: ObservedEventKind::Thread,
            summary: "thread/started".into(),
            method: Some("thread/started".into()),
            thread_id: Some("thread-1".into()),
            turn_id: Some("turn-1".into()),
            item_id: None,
            timestamp: Some("1714000004000".into()),
            raw_json: json!({ "method": "thread/started" }),
        })?;

        let artifact = fs::read_to_string(writer.artifact_path())?;
        assert!(artifact.contains(r#""record_type":"handshake""#));
        assert!(artifact.contains(r#""record_type":"event""#));
        assert!(artifact.contains(r#""event_kind":"thread""#));
        assert!(artifact.contains(r#""thread_id":"thread-1""#));
        assert!(
            writer
                .artifact_path()
                .to_string_lossy()
                .contains("/observer_events/codex/")
        );

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn observer_artifact_source_directory_names_are_backwards_compatible() {
        assert_eq!(ObserverArtifactSource::Codex.directory_name(), "codex");
        assert_eq!(
            ObserverArtifactSource::ClaudeCode.directory_name(),
            "claude-code"
        );
        assert_eq!(
            ObserverArtifactSource::Opencode.directory_name(),
            "opencode"
        );
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        PathBuf::from("/tmp").join(format!(
            "prismtrace-sources-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
