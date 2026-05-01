use serde_json::Value;
use std::io;
use std::time::Duration;

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

#[cfg(test)]
mod tests {
    use super::{ObservedEventKind, ObserverChannelKind};

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
}
