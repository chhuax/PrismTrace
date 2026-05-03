use crate::index::ObservabilityIndexStore;
use prismtrace_analysis::{
    CapabilityProjection, CapabilityRawRef, EventCapabilityInput, project_event_capabilities,
};
use prismtrace_index::{
    ArtifactRef as StorageArtifactRef, EventIndexEntry as StorageEventIndexEntry,
    ObservabilityIndex, SessionIndexEntry as StorageSessionIndexEntry,
};
use prismtrace_storage::StorageLayout;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::Command;

const CODEX_ROLLOUT_EVENT_LIMIT: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    pub kind: String,
    pub display_name: String,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
    pub path: PathBuf,
    pub line_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventSummary {
    pub event_id: String,
    pub session_id: String,
    pub source: SourceRef,
    pub artifact: ArtifactRef,
    pub event_kind: String,
    pub summary: String,
    pub occurred_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventDetail {
    pub event_id: String,
    pub session_id: String,
    pub source: SourceRef,
    pub artifact: ArtifactRef,
    pub event_kind: String,
    pub summary: String,
    pub occurred_at_ms: u64,
    pub raw_json: Value,
    pub detail_json: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub session_id: String,
    pub source: SourceRef,
    pub title: String,
    pub subtitle: String,
    pub cwd: Option<String>,
    pub artifact: ArtifactRef,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub event_count: usize,
    pub response_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionDetail {
    pub summary: SessionSummary,
    pub events: Vec<EventSummary>,
}

#[derive(Debug, Clone, Default)]
pub struct ObservabilityReadModel {
    sessions: Vec<SessionDetail>,
    event_index: HashMap<String, EventDetail>,
    capabilities: Vec<CapabilityProjection>,
    artifact_index: ObservabilityIndex,
}

impl ObservabilityReadModel {
    pub fn build(storage: &StorageLayout) -> io::Result<Self> {
        let codex_home = env::var_os("HOME").map(PathBuf::from);
        Self::build_with_codex_home_and_workspace(storage, codex_home.as_deref(), None)
    }

    pub fn build_with_codex_home(
        storage: &StorageLayout,
        codex_home: Option<&Path>,
    ) -> io::Result<Self> {
        Self::build_with_codex_home_and_workspace(storage, codex_home, None)
    }

    fn build_with_codex_home_and_workspace(
        storage: &StorageLayout,
        codex_home: Option<&Path>,
        workspace_root: Option<&Path>,
    ) -> io::Result<Self> {
        let mut sessions = Vec::new();
        sessions.extend(read_observer_artifact_sessions(storage)?);
        if let Some(codex_home) = codex_home {
            sessions.extend(read_codex_rollout_sessions(codex_home, workspace_root)?);
        }
        let index_write_plan = ObservabilityIndexStore::prepare_write(storage, &sessions)?;

        sessions.sort_by(|left, right| {
            right
                .summary
                .completed_at_ms
                .cmp(&left.summary.completed_at_ms)
                .then_with(|| left.summary.session_id.cmp(&right.summary.session_id))
        });

        let mut event_index = HashMap::new();
        let mut capabilities = Vec::new();
        let mut artifact_index = ObservabilityIndex::new();
        for session in &sessions {
            artifact_index.insert_session(StorageSessionIndexEntry {
                session_id: session.summary.session_id.clone(),
                source_kind: session.summary.source.kind.clone(),
                updated_at_ms: session.summary.completed_at_ms,
                artifact: StorageArtifactRef {
                    path: session.summary.artifact.path.clone(),
                    line_index: session.summary.artifact.line_index,
                },
            });
            for event in &session.events {
                artifact_index.insert_event(StorageEventIndexEntry {
                    event_id: event.event_id.clone(),
                    session_id: session.summary.session_id.clone(),
                    source_kind: event.source.kind.clone(),
                    occurred_at_ms: event.occurred_at_ms,
                    artifact: StorageArtifactRef {
                        path: event.artifact.path.clone(),
                        line_index: event.artifact.line_index,
                    },
                });
                if let Some(detail) = load_event_detail(session, event) {
                    let event_capabilities = project_event_capabilities(EventCapabilityInput {
                        session_id: &detail.session_id,
                        event_id: &detail.event_id,
                        source_kind: &detail.source.kind,
                        event_kind: &detail.event_kind,
                        summary: &detail.summary,
                        observed_at_ms: detail.occurred_at_ms,
                        raw_ref: CapabilityRawRef {
                            path: detail.artifact.path.clone(),
                            line_index: detail.artifact.line_index,
                        },
                        raw_json: &detail.raw_json,
                        detail_json: &detail.detail_json,
                    });
                    for capability in &event_capabilities {
                        artifact_index.insert_capability(
                            ObservabilityIndexStore::capability_index_entry(capability),
                        );
                    }
                    capabilities.extend(event_capabilities);
                    event_index.insert(event.event_id.clone(), detail);
                }
            }
        }
        ObservabilityIndexStore::persist_changed_projection(
            storage,
            &sessions,
            &capabilities,
            &index_write_plan,
        )?;

        Ok(Self {
            sessions,
            event_index,
            capabilities,
            artifact_index,
        })
    }

    pub fn session_summaries(&self, limit: usize) -> Vec<SessionSummary> {
        self.sessions
            .iter()
            .take(limit)
            .map(|session| session.summary.clone())
            .collect()
    }

    pub fn event_summaries(&self, limit: usize) -> Vec<EventSummary> {
        let mut events = self
            .sessions
            .iter()
            .flat_map(|session| session.events.iter().cloned())
            .collect::<Vec<_>>();
        events.sort_by(|left, right| {
            right
                .occurred_at_ms
                .cmp(&left.occurred_at_ms)
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        events.truncate(limit);
        events
    }

    pub fn session_detail(&self, session_id: &str) -> Option<SessionDetail> {
        self.sessions
            .iter()
            .find(|session| session.summary.session_id == session_id)
            .cloned()
    }

    pub fn event_detail(&self, event_id: &str) -> Option<EventDetail> {
        self.event_index.get(event_id).cloned()
    }

    pub fn session_capabilities(&self, session_id: &str) -> Vec<CapabilityProjection> {
        let mut capabilities = self
            .capabilities
            .iter()
            .filter(|capability| capability.session_id == session_id)
            .cloned()
            .collect::<Vec<_>>();
        capabilities.sort_by(|left, right| {
            left.observed_at_ms
                .cmp(&right.observed_at_ms)
                .then_with(|| left.capability_type.cmp(&right.capability_type))
                .then_with(|| left.capability_name.cmp(&right.capability_name))
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        capabilities
    }

    pub fn event_reference(&self, event_id: &str) -> Option<StorageEventIndexEntry> {
        self.artifact_index.event_detail(event_id)
    }
}

pub(crate) fn load_codex_rollout_session_detail_from_state_db(
    _storage: &StorageLayout,
    session_id: &str,
) -> io::Result<Option<SessionDetail>> {
    let Some(thread_id) = session_id.strip_prefix("codex-thread:") else {
        return Ok(None);
    };
    if !is_codex_thread_id(thread_id) {
        return Ok(None);
    }

    let Some(codex_home) = env::var_os("HOME").map(PathBuf::from) else {
        return Ok(None);
    };
    let db_path = codex_home.join(".codex").join("state_5.sqlite");
    if !db_path.exists() {
        return Ok(None);
    }

    let query = format!(
        "select rollout_path from threads \
         where archived = 0 and source in ('cli', 'vscode') and id = {} \
         limit 1;",
        sqlite_string_literal(thread_id)
    );
    let output = Command::new("sqlite3")
        .arg("-batch")
        .arg("-noheader")
        .arg(db_path)
        .arg(query)
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let Some(path) = stdout.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return Ok(None);
    };
    let Some(session) = read_codex_rollout_session(Path::new(path), None)? else {
        return Ok(None);
    };

    Ok((session.summary.session_id == session_id).then_some(session))
}

fn read_observer_artifact_sessions(storage: &StorageLayout) -> io::Result<Vec<SessionDetail>> {
    let observer_root = storage.artifacts_dir.join("observer_events");
    if !observer_root.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for channel_entry in fs::read_dir(observer_root)?.flatten() {
        let channel_path = channel_entry.path();
        if !channel_path.is_dir() {
            continue;
        }
        let channel = channel_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("observer")
            .to_string();

        for entry in fs::read_dir(channel_path)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(session) = read_observer_artifact_session(&path, &channel)? {
                sessions.push(session);
            }
        }
    }

    Ok(sessions)
}

pub(crate) fn read_observer_artifact_session(
    path: &Path,
    channel_dir: &str,
) -> io::Result<Option<SessionDetail>> {
    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let session_stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("session");
    let session_id = format!("observer:{channel_dir}:{session_stem}");
    let mut channel = channel_dir.to_string();
    let mut server_label = None;
    let mut transport = None;
    let mut started_at_ms = 0_u64;
    let mut completed_at_ms = 0_u64;
    let mut events = Vec::new();

    for (line_index, line) in reader.lines().enumerate() {
        let Ok(line) = line else {
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        match value.get("record_type").and_then(Value::as_str) {
            Some("handshake") => {
                channel = value
                    .get("channel")
                    .and_then(Value::as_str)
                    .unwrap_or(channel_dir)
                    .to_string();
                server_label = value
                    .get("server_label")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                transport = value
                    .get("transport")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                started_at_ms = value
                    .get("recorded_at_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                completed_at_ms = completed_at_ms.max(started_at_ms);
            }
            Some("event") => {
                let event_channel = value
                    .get("channel")
                    .and_then(Value::as_str)
                    .unwrap_or(&channel)
                    .to_string();
                let occurred_at_ms = value
                    .get("recorded_at_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(|| started_at_ms.max(1));
                completed_at_ms = completed_at_ms.max(occurred_at_ms);
                let event_kind = value
                    .get("event_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let summary = value
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("observer event")
                    .to_string();
                let source = SourceRef {
                    kind: "observer_event".into(),
                    display_name: observer_source_display_name(
                        &event_channel,
                        server_label.as_deref(),
                    ),
                    channel: Some(event_channel),
                };
                events.push(EventSummary {
                    event_id: format!("{session_id}:{line_index}"),
                    session_id: session_id.clone(),
                    source,
                    artifact: ArtifactRef {
                        path: path.to_path_buf(),
                        line_index: Some(line_index),
                    },
                    event_kind,
                    summary,
                    occurred_at_ms,
                });
            }
            _ => {}
        }
    }

    if events.is_empty() {
        return Ok(None);
    }
    if started_at_ms == 0 {
        started_at_ms = events
            .iter()
            .map(|event| event.occurred_at_ms)
            .min()
            .unwrap_or_default();
    }
    if completed_at_ms == 0 {
        completed_at_ms = started_at_ms;
    }

    let source = SourceRef {
        kind: "observer_event".into(),
        display_name: observer_source_display_name(&channel, server_label.as_deref()),
        channel: Some(channel),
    };
    let (title, subtitle) = observer_session_display_copy(&events, &source.display_name);
    let response_count = events
        .iter()
        .filter(|event| event.event_kind == "tool")
        .count();
    let summary = SessionSummary {
        session_id: session_id.clone(),
        source,
        title,
        subtitle: transport
            .map(|transport| format!("{subtitle} · {transport}"))
            .unwrap_or(subtitle),
        cwd: None,
        artifact: ArtifactRef {
            path: path.to_path_buf(),
            line_index: None,
        },
        started_at_ms,
        completed_at_ms,
        event_count: events.len(),
        response_count,
    };

    Ok(Some(SessionDetail { summary, events }))
}

fn observer_source_display_name(channel: &str, server_label: Option<&str>) -> String {
    server_label
        .filter(|label| !label.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| match channel {
            "codex-app-server" | "codex" => "Codex App Server".to_string(),
            "opencode-server" | "opencode" => "Opencode Observer".to_string(),
            other => other.to_string(),
        })
}

fn observer_session_display_copy(events: &[EventSummary], source_label: &str) -> (String, String) {
    let primary = events
        .iter()
        .find(|event| {
            !matches!(
                event.event_kind.as_str(),
                "agent" | "app" | "mcp" | "plugin" | "provider" | "skill"
            )
        })
        .or_else(|| events.first());
    let title = primary
        .map(|event| event.summary.trim())
        .filter(|summary| !summary.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| source_label.to_string());
    let subtitle = primary
        .map(|event| format!("{} session", event.event_kind))
        .unwrap_or_else(|| source_label.to_string());

    (title, subtitle)
}

fn read_codex_rollout_sessions(
    codex_home: &Path,
    workspace_root: Option<&Path>,
) -> io::Result<Vec<SessionDetail>> {
    let mut paths = Vec::new();
    collect_jsonl_files(&codex_home.join(".codex").join("sessions"), &mut paths);
    let unarchived_thread_ids =
        load_unarchived_interactive_codex_thread_ids(codex_home, workspace_root);
    let mut sessions = Vec::new();
    for path in paths {
        if let Some(thread_ids) = unarchived_thread_ids.as_ref()
            && !codex_rollout_path_thread_id(&path)
                .is_some_and(|thread_id| thread_ids.contains(thread_id))
        {
            continue;
        }
        if let Some(session) = read_codex_rollout_session(&path, workspace_root)? {
            if let Some(thread_ids) = unarchived_thread_ids.as_ref() {
                let thread_id = session
                    .summary
                    .session_id
                    .trim_start_matches("codex-thread:");
                if !thread_ids.contains(thread_id) {
                    continue;
                }
            }
            sessions.push(session);
        }
    }
    Ok(sessions)
}

fn codex_rollout_path_thread_id(path: &Path) -> Option<&str> {
    let file_stem = path.file_stem()?.to_str()?;
    let thread_id = file_stem.rsplit('-').take(5).collect::<Vec<_>>();
    if thread_id.len() != 5 {
        return None;
    }
    let start = file_stem.len().checked_sub(36)?;
    let candidate = file_stem.get(start..)?;
    is_codex_thread_id(candidate).then_some(candidate)
}

fn load_unarchived_interactive_codex_thread_ids(
    codex_home: &Path,
    workspace_root: Option<&Path>,
) -> Option<HashSet<String>> {
    let db_path = codex_home.join(".codex").join("state_5.sqlite");
    if !db_path.exists() {
        return None;
    }

    let cwd_clause = workspace_root
        .map(|root| {
            format!(
                " and cwd = {}",
                sqlite_string_literal(&root.display().to_string())
            )
        })
        .unwrap_or_default();
    let query = format!(
        "select id from threads where archived = 0 and source in ('cli', 'vscode'){cwd_clause};"
    );
    let output = Command::new("sqlite3")
        .arg("-batch")
        .arg("-noheader")
        .arg(&db_path)
        .arg(query)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(
        stdout
            .lines()
            .map(str::trim)
            .filter(|line| is_codex_thread_id(line))
            .map(ToString::to_string)
            .collect(),
    )
}

fn sqlite_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn is_codex_thread_id(value: &str) -> bool {
    value.len() == 36
        && value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        && value.chars().filter(|ch| *ch == '-').count() == 4
}

fn collect_jsonl_files(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if is_archived_artifact_path(&path) {
            continue;
        }
        if path.is_dir() {
            collect_jsonl_files(&path, paths);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            paths.push(path);
        }
    }
}

fn is_archived_artifact_path(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name == "archived_sessions")
    })
}

pub(crate) fn read_codex_rollout_session(
    path: &Path,
    workspace_root: Option<&Path>,
) -> io::Result<Option<SessionDetail>> {
    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut thread_id = None;
    let mut cwd = None;
    let mut started_at_ms = 0_u64;
    let mut completed_at_ms = 0_u64;
    let mut events = Vec::new();

    for (line_index, line) in reader.lines().enumerate() {
        let Ok(line) = line else {
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let occurred_at_ms = value
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_timestamp_ms)
            .unwrap_or_default();
        if occurred_at_ms > 0 {
            if started_at_ms == 0 {
                started_at_ms = occurred_at_ms;
            }
            completed_at_ms = completed_at_ms.max(occurred_at_ms);
        }

        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                let payload = value.get("payload").unwrap_or(&Value::Null);
                thread_id = payload
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                cwd = payload
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            }
            Some("response_item") => {
                let current_thread_id = thread_id.as_deref().unwrap_or("unknown");
                if let Some(event) = normalize_codex_rollout_event(
                    current_thread_id,
                    line_index,
                    occurred_at_ms,
                    path,
                    value.get("payload").unwrap_or(&Value::Null),
                ) {
                    events.push(event);
                    if events.len() > CODEX_ROLLOUT_EVENT_LIMIT {
                        events.remove(0);
                    }
                }
            }
            _ => {}
        }
    }

    let Some(thread_id) = thread_id else {
        return Ok(None);
    };
    if events.is_empty() {
        return Ok(None);
    }
    if let Some(root) = workspace_root {
        let session_cwd = cwd.as_deref().map(Path::new);
        if session_cwd != Some(root) {
            return Ok(None);
        }
    }
    let title = events
        .iter()
        .find(|event| event.event_kind == "message")
        .map(|event| {
            event
                .summary
                .trim_start_matches("用户: ")
                .trim()
                .to_string()
        })
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| thread_id.clone());
    let started_at_ms = started_at_ms.max(
        events
            .first()
            .map(|event| event.occurred_at_ms)
            .unwrap_or_default(),
    );
    let completed_at_ms = completed_at_ms.max(
        events
            .last()
            .map(|event| event.occurred_at_ms)
            .unwrap_or(started_at_ms),
    );
    let session_id = format!("codex-thread:{thread_id}");
    let source = SourceRef {
        kind: "codex_rollout".into(),
        display_name: "Codex Desktop".into(),
        channel: Some("codex-thread".into()),
    };
    let response_count = events
        .iter()
        .filter(|event| matches!(event.event_kind.as_str(), "tool" | "tool_result"))
        .count();
    let summary = SessionSummary {
        session_id: session_id.clone(),
        source,
        title,
        subtitle: format!("thread {thread_id}"),
        cwd,
        artifact: ArtifactRef {
            path: path.to_path_buf(),
            line_index: None,
        },
        started_at_ms,
        completed_at_ms,
        event_count: events.len(),
        response_count,
    };

    Ok(Some(SessionDetail { summary, events }))
}

fn normalize_codex_rollout_event(
    thread_id: &str,
    line_index: usize,
    occurred_at_ms: u64,
    artifact_path: &Path,
    payload: &Value,
) -> Option<EventSummary> {
    let payload_type = payload.get("type").and_then(Value::as_str)?;
    let (event_kind, summary) = match payload_type {
        "message" => {
            let role = payload
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let text = extract_codex_message_text(payload)?;
            let label = match role {
                "user" => "用户",
                "assistant" => "助手",
                "developer" => "开发者指令",
                other => other,
            };
            (
                "message".to_string(),
                format!("{label}: {}", truncate_text(&text, 180)),
            )
        }
        "function_call" => {
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown tool");
            ("tool".to_string(), format!("工具调用: {name}"))
        }
        "function_call_output" => {
            let output = payload
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or("tool output");
            (
                "tool_result".to_string(),
                format!("工具结果: {}", truncate_text(output, 180)),
            )
        }
        _ => return None,
    };

    Some(EventSummary {
        event_id: format!("codex-thread:{thread_id}:{line_index}"),
        session_id: format!("codex-thread:{thread_id}"),
        source: SourceRef {
            kind: "codex_rollout".into(),
            display_name: "Codex Desktop".into(),
            channel: Some("codex-thread".into()),
        },
        artifact: ArtifactRef {
            path: artifact_path.to_path_buf(),
            line_index: Some(line_index),
        },
        event_kind,
        summary,
        occurred_at_ms,
    })
}

fn extract_codex_message_text(payload: &Value) -> Option<String> {
    let parts = payload.get("content")?.as_array()?;
    let text = parts
        .iter()
        .filter_map(|part| {
            part.get("text")
                .or_else(|| part.get("input_text"))
                .or_else(|| part.get("output_text"))
                .and_then(Value::as_str)
        })
        .collect::<Vec<_>>()
        .join("\n");

    (!text.trim().is_empty()).then(|| text.trim().to_string())
}

pub(crate) fn load_event_detail(
    session: &SessionDetail,
    event: &EventSummary,
) -> Option<EventDetail> {
    let line_index = event.artifact.line_index?;
    let raw_json = read_jsonl_value_at_line(&event.artifact.path, line_index)?;
    let detail_json = if event.source.kind == "codex_rollout" {
        codex_rollout_event_detail(&raw_json, &event.event_kind, &event.summary)
    } else {
        raw_json.clone()
    };
    let event_raw_json = if event.source.kind == "codex_rollout" {
        raw_json.clone()
    } else {
        raw_json.get("raw_json").cloned().unwrap_or(Value::Null)
    };

    Some(EventDetail {
        event_id: event.event_id.clone(),
        session_id: session.summary.session_id.clone(),
        source: event.source.clone(),
        artifact: event.artifact.clone(),
        event_kind: event.event_kind.clone(),
        summary: event.summary.clone(),
        occurred_at_ms: event.occurred_at_ms,
        raw_json: event_raw_json,
        detail_json,
    })
}

fn read_jsonl_value_at_line(path: &Path, target_line_index: usize) -> Option<Value> {
    let file = fs::File::open(path).ok()?;
    for (line_index, line) in io::BufReader::new(file).lines().enumerate() {
        if line_index != target_line_index {
            continue;
        }
        return serde_json::from_str(&line.ok()?).ok();
    }
    None
}

fn codex_rollout_event_detail(raw_json: &Value, event_kind: &str, summary: &str) -> Value {
    let payload = raw_json.get("payload").unwrap_or(&Value::Null);
    match payload.get("type").and_then(Value::as_str) {
        Some("message") => serde_json::json!({
            "kind": "message",
            "role": payload.get("role").and_then(Value::as_str).unwrap_or("unknown"),
            "title": summary,
            "full_text": extract_codex_message_text(payload).unwrap_or_default(),
        }),
        Some("function_call") => serde_json::json!({
            "kind": "tool_call",
            "title": "工具调用",
            "tool_name": payload.get("name").and_then(Value::as_str).unwrap_or("unknown"),
            "arguments_text": payload.get("arguments").and_then(Value::as_str).unwrap_or(""),
        }),
        Some("function_call_output") => serde_json::json!({
            "kind": "tool_result",
            "title": "工具结果",
            "output_text": payload.get("output").and_then(Value::as_str).unwrap_or(""),
        }),
        _ => serde_json::json!({
            "kind": event_kind,
            "title": summary,
        }),
    }
}

fn truncate_text(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn parse_rfc3339_timestamp_ms(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return None;
    }
    let year = value.get(0..4)?.parse::<i32>().ok()?;
    let month = value.get(5..7)?.parse::<u32>().ok()?;
    let day = value.get(8..10)?.parse::<u32>().ok()?;
    let hour = value.get(11..13)?.parse::<u32>().ok()?;
    let minute = value.get(14..16)?.parse::<u32>().ok()?;
    let second = value.get(17..19)?.parse::<u32>().ok()?;
    let mut millis = 0_u32;
    if let Some(dot_index) = value.find('.') {
        let fraction = value.get(dot_index + 1..)?.trim_end_matches('Z');
        let digits = fraction
            .chars()
            .take_while(|char| char.is_ascii_digit())
            .take(3)
            .collect::<String>();
        if !digits.is_empty() {
            let mut padded = digits;
            while padded.len() < 3 {
                padded.push('0');
            }
            millis = padded.parse::<u32>().ok()?;
        }
    }
    let days = days_from_civil(year, month, day)?;
    days.checked_mul(86_400_000)?
        .checked_add(u64::from(hour) * 3_600_000)?
        .checked_add(u64::from(minute) * 60_000)?
        .checked_add(u64::from(second) * 1_000)?
        .checked_add(u64::from(millis))
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<u64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let adjusted_year = year - i32::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year / 400
    } else {
        (adjusted_year - 399) / 400
    };
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month as i32 + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era as i64 * 146_097 + day_of_era as i64 - 719_468;

    (days >= 0).then_some(days as u64)
}

#[cfg(test)]
mod tests {
    use super::ObservabilityReadModel;
    use crate::index::ObservabilityIndexStore;
    use prismtrace_index::{ObservabilityIndex, ObservabilityIndexManifest};
    use prismtrace_storage::StorageLayout;
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::process::{self, Command};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static UNIQUE_TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn indexes_observer_artifacts_as_sessions_and_events() -> io::Result<()> {
        let root = unique_test_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;
        let observer_dir = storage
            .artifacts_dir
            .join("observer_events/codex-app-server");
        fs::create_dir_all(&observer_dir)?;
        fs::write(
            observer_dir.join("session-1.jsonl"),
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"socket\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1000}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Thread started\",\"thread_id\":\"thread-1\",\"recorded_at_ms\":1010,\"raw_json\":{\"hello\":\"world\"}}\n"
            ),
        )?;

        let model = ObservabilityReadModel::build_with_codex_home(&storage, None)?;

        let sessions = model.session_summaries(10);
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].session_id,
            "observer:codex-app-server:session-1"
        );
        assert_eq!(sessions[0].source.display_name, "Codex App Server");
        assert_eq!(sessions[0].event_count, 1);

        let event = model
            .event_detail("observer:codex-app-server:session-1:1")
            .expect("event should be indexed");
        assert_eq!(event.session_id, "observer:codex-app-server:session-1");
        assert_eq!(event.source.kind, "observer_event");
        assert_eq!(event.raw_json["hello"], "world");

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn projects_capabilities_from_observer_snapshots_and_tool_events() -> io::Result<()> {
        let root = unique_test_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;
        let observer_dir = storage.artifacts_dir.join("observer_events/codex");
        fs::create_dir_all(&observer_dir)?;
        fs::write(
            observer_dir.join("session-1.jsonl"),
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"recorded_at_ms\":1000}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"skill\",\"summary\":\"skills/list returned 2 entries\",\"method\":\"skills/list\",\"recorded_at_ms\":1010,\"raw_json\":{\"method\":\"skills/list\",\"skill_names_preview\":[\"review\",\"test\"]}}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"mcp\",\"summary\":\"mcpServer/listStatus returned 1 entries\",\"method\":\"mcpServer/listStatus\",\"recorded_at_ms\":1020,\"raw_json\":{\"method\":\"mcpServer/listStatus\",\"mcp_server_names_preview\":[\"github\"]}}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"plugin\",\"summary\":\"plugin/list returned 1 entries\",\"method\":\"plugin/list\",\"recorded_at_ms\":1030,\"raw_json\":{\"method\":\"plugin/list\",\"marketplace_names_preview\":[\"github\"]}}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"app\",\"summary\":\"app/list returned 1 entries\",\"method\":\"app/list\",\"recorded_at_ms\":1040,\"raw_json\":{\"method\":\"app/list\",\"app_names_preview\":[\"todoist\"]}}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"tool\",\"summary\":\"Ran shell command\",\"method\":\"shell.exec\",\"recorded_at_ms\":1040,\"raw_json\":{\"tool\":\"exec_command\"}}\n"
            ),
        )?;

        let model = ObservabilityReadModel::build_with_codex_home(&storage, None)?;

        let capabilities = model.session_capabilities("observer:codex:session-1");
        assert_eq!(capabilities.len(), 6);
        assert!(capabilities.iter().any(|capability| {
            capability.capability_type == "skill" && capability.capability_name == "review"
        }));
        assert!(capabilities.iter().any(|capability| {
            capability.capability_type == "mcp" && capability.capability_name == "github"
        }));
        assert!(capabilities.iter().any(|capability| {
            capability.capability_type == "plugin" && capability.capability_name == "github"
        }));
        assert!(capabilities.iter().any(|capability| {
            capability.capability_type == "app" && capability.capability_name == "todoist"
        }));
        assert!(capabilities.iter().any(|capability| {
            capability.capability_type == "tool"
                && capability.capability_name == "exec_command"
                && capability.visibility_stage == "observed"
        }));
        assert!(capabilities.iter().all(|capability| {
            capability.raw_ref.path.ends_with("session-1.jsonl")
                && capability.raw_ref.line_index.is_some()
        }));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn projects_opencode_capabilities_without_codex_domain_aliases() -> io::Result<()> {
        let root = unique_test_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;
        let observer_dir = storage.artifacts_dir.join("observer_events/opencode");
        fs::create_dir_all(&observer_dir)?;
        fs::write(
            observer_dir.join("session-1.jsonl"),
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"opencode-server\",\"recorded_at_ms\":1000}\n",
                "{\"record_type\":\"event\",\"channel\":\"opencode-server\",\"event_kind\":\"agent\",\"summary\":\"GET /agent returned 1 entries\",\"method\":\"GET /agent\",\"recorded_at_ms\":1010,\"raw_json\":{\"method\":\"GET /agent\",\"agent_names_preview\":[\"build\"]}}\n",
                "{\"record_type\":\"event\",\"channel\":\"opencode-server\",\"event_kind\":\"mcp\",\"summary\":\"GET /mcp returned 1 entries\",\"method\":\"GET /mcp\",\"recorded_at_ms\":1020,\"raw_json\":{\"method\":\"GET /mcp\",\"mcp_server_names_preview\":[\"github\"]}}\n",
                "{\"record_type\":\"event\",\"channel\":\"opencode-server\",\"event_kind\":\"provider\",\"summary\":\"GET /provider returned 1 entries\",\"method\":\"GET /provider\",\"recorded_at_ms\":1030,\"raw_json\":{\"method\":\"GET /provider\",\"provider_names_preview\":[\"anthropic\"]}}\n"
            ),
        )?;

        let model = ObservabilityReadModel::build_with_codex_home(&storage, None)?;

        let capabilities = model.session_capabilities("observer:opencode:session-1");
        assert!(capabilities.iter().any(|capability| {
            capability.capability_type == "agent" && capability.capability_name == "build"
        }));
        assert!(capabilities.iter().any(|capability| {
            capability.capability_type == "mcp" && capability.capability_name == "github"
        }));
        assert!(capabilities.iter().any(|capability| {
            capability.capability_type == "provider" && capability.capability_name == "anthropic"
        }));
        assert!(!capabilities.iter().any(|capability| {
            capability.capability_type == "skill" && capability.capability_name == "build"
        }));
        assert!(!capabilities.iter().any(|capability| {
            capability.capability_type == "plugin" && capability.capability_name == "github"
        }));
        assert!(!capabilities.iter().any(|capability| {
            capability.capability_type == "app" && capability.capability_name == "anthropic"
        }));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn writes_index_manifest_for_parsed_source_files() -> io::Result<()> {
        let root = unique_test_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;
        let observer_dir = storage
            .artifacts_dir
            .join("observer_events/codex-app-server");
        let source_path = observer_dir.join("session-1.jsonl");
        fs::create_dir_all(&observer_dir)?;
        fs::write(
            &source_path,
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"recorded_at_ms\":1000}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Thread started\",\"recorded_at_ms\":1010}\n"
            ),
        )?;

        ObservabilityReadModel::build_with_codex_home(&storage, None)?;

        let manifest = ObservabilityIndexManifest::load(&storage.index_manifest_path)?;
        let source = manifest
            .reusable_source(&source_path, "observer_event")?
            .expect("source should be reusable after indexing");
        assert_eq!(source.source_path, source_path);
        assert_eq!(source.source_kind, "observer_event");
        assert!(source.indexed_at_ms > 0);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn writes_session_event_and_capability_index_projection_files() -> io::Result<()> {
        let root = unique_test_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;
        let observer_dir = storage
            .artifacts_dir
            .join("observer_events/codex-app-server");
        fs::create_dir_all(&observer_dir)?;
        fs::write(
            observer_dir.join("session-1.jsonl"),
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"recorded_at_ms\":1000}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Thread started\",\"recorded_at_ms\":1010}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"mcp\",\"summary\":\"mcpServer/listStatus returned 1 entries\",\"method\":\"mcpServer/listStatus\",\"recorded_at_ms\":1020,\"raw_json\":{\"method\":\"mcpServer/listStatus\",\"mcp_server_names_preview\":[\"github\"]}}\n"
            ),
        )?;

        ObservabilityReadModel::build_with_codex_home(&storage, None)?;

        let index = ObservabilityIndex::load_jsonl(
            &storage.sessions_index_path,
            &storage.events_index_path,
            &storage.capabilities_index_path,
        )?;
        assert_eq!(
            index.session_summaries(10)[0].session_id,
            "observer:codex-app-server:session-1"
        );
        assert!(
            index
                .event_detail("observer:codex-app-server:session-1:1")
                .is_some()
        );
        assert!(
            index
                .session_capabilities("observer:codex-app-server:session-1")
                .iter()
                .any(|capability| {
                    capability.capability_type == "mcp" && capability.capability_name == "github"
                })
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn index_read_store_uses_persisted_index_without_rescanning_artifact_dirs() -> io::Result<()> {
        let root = unique_test_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;
        let observer_dir = storage
            .artifacts_dir
            .join("observer_events/codex-app-server");
        fs::create_dir_all(&observer_dir)?;
        fs::write(
            observer_dir.join("session-1.jsonl"),
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"recorded_at_ms\":1000}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Indexed session\",\"recorded_at_ms\":1010}\n"
            ),
        )?;

        ObservabilityReadModel::build_with_codex_home(&storage, None)?;
        fs::write(
            observer_dir.join("session-2.jsonl"),
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"recorded_at_ms\":2000}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Unindexed session\",\"recorded_at_ms\":2010}\n"
            ),
        )?;

        let store = ObservabilityIndexStore::load_read_store(&storage, None)?;

        assert!(
            store
                .session_detail("observer:codex-app-server:session-1")?
                .is_some()
        );
        assert!(
            store
                .session_detail("observer:codex-app-server:session-2")?
                .is_none()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn index_read_store_lives_outside_observability_read_model_module() {
        let read_model_module = include_str!("observability_read_model.rs");
        let index_module = include_str!("index/read_store.rs");
        let store_definition = ["pub(crate) struct ", "IndexReadStore"].concat();

        assert!(
            !read_model_module.contains(&store_definition),
            "read model should not own index query store"
        );
        assert!(
            index_module.contains(&store_definition),
            "index module should own index query store"
        );
    }

    #[test]
    fn index_write_store_lives_outside_observability_read_model_module() {
        let read_model_module = include_str!("observability_read_model.rs");
        let write_store_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("index")
            .join("write_store.rs");
        let index_module =
            fs::read_to_string(write_store_path).expect("index write store module should exist");
        let projection_writer = ["fn ", "persist_changed_index_projection"].concat();
        let store_definition = ["pub(crate) struct ", "IndexWriteStore"].concat();

        assert!(
            !read_model_module.contains(&projection_writer),
            "read model should not own index projection persistence"
        );
        assert!(
            index_module.contains(&store_definition),
            "index module should own index projection persistence"
        );
    }

    #[test]
    fn preserves_manifest_entry_for_unchanged_source_when_new_source_is_added() -> io::Result<()> {
        let root = unique_test_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;
        let observer_dir = storage
            .artifacts_dir
            .join("observer_events/codex-app-server");
        let first_source = observer_dir.join("session-1.jsonl");
        let second_source = observer_dir.join("session-2.jsonl");
        fs::create_dir_all(&observer_dir)?;
        fs::write(
            &first_source,
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"recorded_at_ms\":1000}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"First\",\"recorded_at_ms\":1010}\n"
            ),
        )?;

        ObservabilityReadModel::build_with_codex_home(&storage, None)?;

        let mut manifest = ObservabilityIndexManifest::load(&storage.index_manifest_path)?;
        let first_entry = manifest
            .sources
            .iter_mut()
            .find(|source| source.source_path == first_source)
            .expect("first source should be indexed");
        first_entry.indexed_at_ms = 1;
        manifest.save(&storage.index_manifest_path)?;
        fs::write(
            &second_source,
            concat!(
                "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"recorded_at_ms\":2000}\n",
                "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Second\",\"recorded_at_ms\":2010}\n"
            ),
        )?;

        ObservabilityReadModel::build_with_codex_home(&storage, None)?;

        let manifest = ObservabilityIndexManifest::load(&storage.index_manifest_path)?;
        let first_entry = manifest
            .sources
            .iter()
            .find(|source| source.source_path == first_source)
            .expect("first source should still be indexed");
        let second_entry = manifest
            .sources
            .iter()
            .find(|source| source.source_path == second_source)
            .expect("second source should be indexed");
        assert_eq!(first_entry.indexed_at_ms, 1);
        assert!(second_entry.indexed_at_ms > 1);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn indexes_codex_transcripts_and_excludes_archived_sessions() -> io::Result<()> {
        let root = unique_test_dir();
        let codex_home = root.join("home");
        let active_dir = codex_home.join(".codex/sessions/2026/04");
        let archived_dir = codex_home.join(".codex/sessions/archived_sessions/2026/04");
        fs::create_dir_all(&active_dir)?;
        fs::create_dir_all(&archived_dir)?;
        fs::write(
            active_dir.join("active.jsonl"),
            codex_session_jsonl("active-thread"),
        )?;
        fs::write(
            archived_dir.join("archived.jsonl"),
            codex_session_jsonl("archived-thread"),
        )?;

        let storage = StorageLayout::new(root.join("workspace/.prismtrace"));
        let model = ObservabilityReadModel::build_with_codex_home(&storage, Some(&codex_home))?;

        let sessions = model.session_summaries(10);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "codex-thread:active-thread");
        assert_eq!(sessions[0].source.display_name, "Codex Desktop");

        let event = model
            .event_detail("codex-thread:active-thread:1")
            .expect("active event should be indexed");
        assert_eq!(event.summary, "用户: hello from active-thread");
        assert!(
            model
                .event_detail("codex-thread:archived-thread:1")
                .is_none()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn indexes_only_unarchived_interactive_codex_threads_from_state_db() -> io::Result<()> {
        let root = unique_test_dir();
        let codex_home = root.join("home");
        let active_dir = codex_home.join(".codex/sessions/2026/04");
        fs::create_dir_all(&active_dir)?;
        fs::write(
            active_dir
                .join("rollout-2026-04-30T00-00-00-019dd9fb-86a3-73b1-931a-2cb1b64bff98.jsonl"),
            codex_session_jsonl("019dd9fb-86a3-73b1-931a-2cb1b64bff98"),
        )?;
        fs::write(
            active_dir
                .join("rollout-2026-04-30T00-00-00-019dc5d0-7c22-7a03-84e9-e2671c6ffe03.jsonl"),
            codex_session_jsonl("019dc5d0-7c22-7a03-84e9-e2671c6ffe03"),
        )?;
        fs::write(
            active_dir
                .join("rollout-2026-04-30T00-00-00-019dc84a-f535-70e1-b226-630a2067b547.jsonl"),
            codex_session_jsonl("019dc84a-f535-70e1-b226-630a2067b547"),
        )?;

        let db_path = codex_home.join(".codex/state_5.sqlite");
        let workspace = PathBuf::from("/tmp/workspace");
        let setup_sql = concat!(
            "create table threads (id text, archived integer, source text, cwd text);",
            "insert into threads values ('019dd9fb-86a3-73b1-931a-2cb1b64bff98', 0, 'vscode', '/tmp/workspace');",
            "insert into threads values ('019dc5d0-7c22-7a03-84e9-e2671c6ffe03', 1, 'vscode', '/tmp/workspace');",
            "insert into threads values ('019dc84a-f535-70e1-b226-630a2067b547', 0, '{\"type\":\"agent\"}', '/tmp/workspace');"
        );
        let Ok(status) = Command::new("sqlite3")
            .arg(&db_path)
            .arg(setup_sql)
            .status()
        else {
            fs::remove_dir_all(root)?;
            return Ok(());
        };
        if !status.success() {
            fs::remove_dir_all(root)?;
            return Ok(());
        }

        let storage = StorageLayout::new(root.join("workspace/.prismtrace"));
        let model = ObservabilityReadModel::build_with_codex_home_and_workspace(
            &storage,
            Some(&codex_home),
            Some(&workspace),
        )?;

        let sessions = model.session_summaries(10);
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].session_id,
            "codex-thread:019dd9fb-86a3-73b1-931a-2cb1b64bff98"
        );
        assert!(
            model
                .event_detail("codex-thread:019dc5d0-7c22-7a03-84e9-e2671c6ffe03:1")
                .is_none()
        );
        assert!(
            model
                .event_detail("codex-thread:019dc84a-f535-70e1-b226-630a2067b547:1")
                .is_none()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn filters_unarchived_codex_threads_by_workspace_root() -> io::Result<()> {
        let root = unique_test_dir();
        let codex_home = root.join("home");
        let active_dir = codex_home.join(".codex/sessions/2026/04");
        fs::create_dir_all(&active_dir)?;
        fs::write(
            active_dir
                .join("rollout-2026-04-30T00-00-00-019dd9fb-86a3-73b1-931a-2cb1b64bff98.jsonl"),
            codex_session_jsonl("019dd9fb-86a3-73b1-931a-2cb1b64bff98"),
        )?;
        fs::write(
            active_dir
                .join("rollout-2026-04-30T00-00-00-019dc5d0-7c22-7a03-84e9-e2671c6ffe03.jsonl"),
            codex_session_jsonl_with_cwd(
                "019dc5d0-7c22-7a03-84e9-e2671c6ffe03",
                "/tmp/other-workspace",
            ),
        )?;

        let db_path = codex_home.join(".codex/state_5.sqlite");
        let setup_sql = concat!(
            "create table threads (id text, archived integer, source text, cwd text);",
            "insert into threads values ('019dd9fb-86a3-73b1-931a-2cb1b64bff98', 0, 'vscode', '/tmp/workspace');",
            "insert into threads values ('019dc5d0-7c22-7a03-84e9-e2671c6ffe03', 0, 'vscode', '/tmp/other-workspace');"
        );
        let Ok(status) = Command::new("sqlite3")
            .arg(&db_path)
            .arg(setup_sql)
            .status()
        else {
            fs::remove_dir_all(root)?;
            return Ok(());
        };
        if !status.success() {
            fs::remove_dir_all(root)?;
            return Ok(());
        }

        let storage = StorageLayout::new(root.join("workspace/.prismtrace"));
        let workspace = PathBuf::from("/tmp/workspace");
        let model = ObservabilityReadModel::build_with_codex_home_and_workspace(
            &storage,
            Some(&codex_home),
            Some(&workspace),
        )?;

        let sessions = model.session_summaries(10);
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].session_id,
            "codex-thread:019dd9fb-86a3-73b1-931a-2cb1b64bff98"
        );
        assert!(
            model
                .event_detail("codex-thread:019dc5d0-7c22-7a03-84e9-e2671c6ffe03:1")
                .is_none()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn default_codex_indexing_keeps_cwd_as_metadata_not_boundary() -> io::Result<()> {
        let root = unique_test_dir();
        let codex_home = root.join("home");
        let active_dir = codex_home.join(".codex/sessions/2026/04");
        fs::create_dir_all(&active_dir)?;
        fs::write(
            active_dir
                .join("rollout-2026-04-30T00-00-00-019dd9fb-86a3-73b1-931a-2cb1b64bff98.jsonl"),
            codex_session_jsonl_with_cwd("019dd9fb-86a3-73b1-931a-2cb1b64bff98", "/tmp/workspace"),
        )?;
        fs::write(
            active_dir
                .join("rollout-2026-04-30T00-00-00-019dc5d0-7c22-7a03-84e9-e2671c6ffe03.jsonl"),
            codex_session_jsonl_with_cwd(
                "019dc5d0-7c22-7a03-84e9-e2671c6ffe03",
                "/tmp/other-workspace",
            ),
        )?;

        let db_path = codex_home.join(".codex/state_5.sqlite");
        let setup_sql = concat!(
            "create table threads (id text, archived integer, source text, cwd text);",
            "insert into threads values ('019dd9fb-86a3-73b1-931a-2cb1b64bff98', 0, 'vscode', '/tmp/workspace');",
            "insert into threads values ('019dc5d0-7c22-7a03-84e9-e2671c6ffe03', 0, 'vscode', '/tmp/other-workspace');"
        );
        let Ok(status) = Command::new("sqlite3")
            .arg(&db_path)
            .arg(setup_sql)
            .status()
        else {
            fs::remove_dir_all(root)?;
            return Ok(());
        };
        if !status.success() {
            fs::remove_dir_all(root)?;
            return Ok(());
        }

        let storage = StorageLayout::new(root.join("global-state"));
        let model = ObservabilityReadModel::build_with_codex_home_and_workspace(
            &storage,
            Some(&codex_home),
            None,
        )?;

        let sessions = model.session_summaries(10);
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().any(|session| {
            session.session_id == "codex-thread:019dd9fb-86a3-73b1-931a-2cb1b64bff98"
                && session.cwd.as_deref() == Some("/tmp/workspace")
        }));
        assert!(sessions.iter().any(|session| {
            session.session_id == "codex-thread:019dc5d0-7c22-7a03-84e9-e2671c6ffe03"
                && session.cwd.as_deref() == Some("/tmp/other-workspace")
        }));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn codex_session_jsonl(thread_id: &str) -> String {
        codex_session_jsonl_with_cwd(thread_id, "/tmp/workspace")
    }

    fn codex_session_jsonl_with_cwd(thread_id: &str, cwd: &str) -> String {
        format!(
            "{{\"timestamp\":\"2026-04-30T00:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{thread_id}\",\"cwd\":\"{cwd}\"}}}}\n\
             {{\"timestamp\":\"2026-04-30T00:00:01.000Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"hello from {thread_id}\"}}]}}}}\n"
        )
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let counter = UNIQUE_TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);

        std::env::temp_dir().join(format!(
            "prismtrace-read-model-test-{}-{}-{}",
            process::id(),
            nanos,
            counter
        ))
    }
}
