use super::{
    ConsoleActivityItem, ConsoleFilterContext, ConsoleRequestSummary, ConsoleSessionSummary,
    ConsoleTargetFilterConfig, ConsoleTargetSummary, append_filter_context_fields,
};
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::io::BufRead;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct ObserverArtifactEventRecord {
    event_id: String,
    session_id: String,
    channel: String,
    source_label: String,
    event_kind: String,
    summary: String,
    method: Option<String>,
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    timestamp: Option<String>,
    occurred_at_ms: u64,
    artifact_path: PathBuf,
    raw_json: Value,
}

#[derive(Debug, Clone)]
struct ObserverArtifactSessionRecord {
    session_id: String,
    channel: String,
    source_label: String,
    transport: Option<String>,
    server_label: Option<String>,
    started_at_ms: u64,
    completed_at_ms: u64,
    artifact_path: PathBuf,
    events: Vec<ObserverArtifactEventRecord>,
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

fn observer_filter_matches(filter: Option<&ConsoleTargetFilterConfig>, fields: &[&str]) -> bool {
    let Some(filter) = filter else {
        return true;
    };

    if !filter.is_enabled() {
        return true;
    }

    let normalized = fields
        .iter()
        .map(|field| field.to_ascii_lowercase())
        .collect::<Vec<_>>();
    filter
        .terms
        .iter()
        .any(|term| normalized.iter().any(|field| field.contains(term)))
}

fn load_observer_sessions(
    storage: &StorageLayout,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ObserverArtifactSessionRecord> {
    let observer_root = storage.artifacts_dir.join("observer_events");
    if !observer_root.exists() {
        return Vec::new();
    }

    let mut sessions = Vec::new();
    let Ok(channel_entries) = fs::read_dir(&observer_root) else {
        return Vec::new();
    };

    for channel_entry in channel_entries.flatten() {
        let channel_path = channel_entry.path();
        if !channel_path.is_dir() {
            continue;
        }

        let channel = channel_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("observer")
            .to_string();
        let Ok(entries) = fs::read_dir(&channel_path) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }

            if let Some(session) = read_observer_session(&path, &channel, filter) {
                sessions.push(session);
            }
        }
    }

    sessions.sort_by(|left, right| {
        right
            .completed_at_ms
            .cmp(&left.completed_at_ms)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    sessions
}

fn read_observer_session(
    path: &Path,
    channel_dir: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Option<ObserverArtifactSessionRecord> {
    let file = fs::File::open(path).ok()?;
    let reader = io::BufReader::new(file);
    let session_stem = path.file_stem()?.to_string_lossy();
    let session_id = format!("observer:{channel_dir}:{session_stem}");

    let mut channel = channel_dir.to_string();
    let mut transport = None;
    let mut server_label = None;
    let mut started_at_ms = 0_u64;
    let mut completed_at_ms = 0_u64;
    let mut events = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(&line).ok()?;
        match value.get("record_type").and_then(Value::as_str) {
            Some("handshake") => {
                channel = value
                    .get("channel")
                    .and_then(Value::as_str)
                    .unwrap_or(channel_dir)
                    .to_string();
                transport = value
                    .get("transport")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                server_label = value
                    .get("server_label")
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
                let method = value
                    .get("method")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let thread_id = value
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let turn_id = value
                    .get("turn_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let item_id = value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let timestamp = value
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                let occurred_at_ms = value
                    .get("recorded_at_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(|| started_at_ms.max(1));
                completed_at_ms = completed_at_ms.max(occurred_at_ms);
                let current_source_label =
                    observer_source_display_name(&event_channel, server_label.as_deref());

                if !observer_filter_matches(
                    filter,
                    &[
                        &current_source_label,
                        &event_channel,
                        &event_kind,
                        &summary,
                        thread_id.as_deref().unwrap_or_default(),
                        turn_id.as_deref().unwrap_or_default(),
                        item_id.as_deref().unwrap_or_default(),
                    ],
                ) {
                    continue;
                }

                events.push(ObserverArtifactEventRecord {
                    event_id: format!("{session_id}:{index}"),
                    session_id: session_id.clone(),
                    channel: event_channel,
                    source_label: current_source_label,
                    event_kind,
                    summary,
                    method,
                    thread_id,
                    turn_id,
                    item_id,
                    timestamp,
                    occurred_at_ms,
                    artifact_path: path.to_path_buf(),
                    raw_json: value.get("raw_json").cloned().unwrap_or(Value::Null),
                });
            }
            _ => {}
        }
    }

    if events.is_empty() {
        return None;
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
    let source_label = observer_source_display_name(&channel, server_label.as_deref());

    Some(ObserverArtifactSessionRecord {
        session_id,
        channel,
        source_label,
        transport,
        server_label,
        started_at_ms,
        completed_at_ms,
        artifact_path: path.to_path_buf(),
        events,
    })
}

pub(crate) fn load_observer_target_summaries(
    storage: &StorageLayout,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ConsoleTargetSummary> {
    let sessions = load_observer_sessions(storage, filter);
    let mut grouped = BTreeMap::<String, (String, usize, usize, u64)>::new();

    for session in sessions {
        let entry = grouped
            .entry(session.source_label.clone())
            .or_insert_with(|| (session.channel.clone(), 0, 0, 0));
        entry.1 += 1;
        entry.2 += session.events.len();
        entry.3 = entry.3.max(session.completed_at_ms);
    }

    grouped
        .into_iter()
        .map(|(display_name, (channel, session_count, event_count, last_seen_at_ms))| {
            ConsoleTargetSummary {
                pid: 0,
                display_name,
                runtime_kind: "observer".into(),
                source_state: "active".into(),
                source_summary: format!(
                    "official observer · channel: {channel} · sessions: {session_count} · events: {event_count} · last seen: {last_seen_at_ms}"
                ),
            }
        })
        .collect()
}

pub(crate) fn load_observer_activity_items(
    storage: &StorageLayout,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ConsoleActivityItem> {
    load_observer_sessions(storage, filter)
        .into_iter()
        .flat_map(|session| {
            session
                .events
                .into_iter()
                .map(move |event| ConsoleActivityItem {
                    activity_id: format!("activity-{}", event.event_id),
                    activity_type: format!("observer:{}", event.event_kind),
                    occurred_at_ms: event.occurred_at_ms,
                    title: format!("{} · {}", event.source_label, event.summary),
                    subtitle: format!(
                        "{} · session {}",
                        event.event_kind,
                        session.session_id.trim_start_matches("observer:")
                    ),
                    related_pid: None,
                    related_request_id: Some(event.event_id),
                })
        })
        .collect()
}

pub(crate) fn load_observer_request_summaries(
    storage: &StorageLayout,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ConsoleRequestSummary> {
    let mut requests = load_observer_sessions(storage, filter)
        .into_iter()
        .flat_map(|session| session.events.into_iter())
        .map(|event| ConsoleRequestSummary {
            request_id: event.event_id,
            captured_at_ms: event.occurred_at_ms,
            provider: event.channel,
            model: Some(event.event_kind),
            target_display_name: event.source_label,
            summary_text: event.summary,
        })
        .collect::<Vec<_>>();
    super::sort_request_summaries(&mut requests);
    requests
}

pub(crate) fn load_observer_session_summaries(
    storage: &StorageLayout,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ConsoleSessionSummary> {
    let mut sessions = load_observer_sessions(storage, filter)
        .into_iter()
        .map(|session| {
            let tool_events = session
                .events
                .iter()
                .filter(|event| event.event_kind == "tool")
                .count();
            ConsoleSessionSummary {
                session_id: session.session_id,
                pid: 0,
                target_display_name: session.source_label,
                started_at_ms: session.started_at_ms,
                completed_at_ms: session.completed_at_ms,
                exchange_count: session.events.len(),
                request_count: session.events.len(),
                response_count: tool_events,
            }
        })
        .collect::<Vec<_>>();
    super::sort_session_summaries(&mut sessions);
    sessions
}

pub(crate) fn load_observer_request_detail_payload(
    storage: &StorageLayout,
    request_id: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
) -> Option<String> {
    let event = load_observer_sessions(storage, filter)
        .into_iter()
        .flat_map(|session| session.events.into_iter())
        .find(|event| event.event_id == request_id)?;

    let raw_json_pretty = serde_json::to_string_pretty(&event.raw_json).ok()?;
    let probe_context = format!(
        "thread={} · turn={} · item={} · timestamp={}",
        event.thread_id.as_deref().unwrap_or("n/a"),
        event.turn_id.as_deref().unwrap_or("n/a"),
        event.item_id.as_deref().unwrap_or("n/a"),
        event.timestamp.as_deref().unwrap_or("n/a")
    );
    let mut payload = json!({
        "request": {
            "detail_kind": "observer_event",
            "request_id": event.event_id,
            "exchange_id": event.turn_id,
            "captured_at_ms": event.occurred_at_ms,
            "provider": event.channel,
            "model": event.event_kind,
            "target_display_name": event.source_label,
            "artifact_path": event.artifact_path.display().to_string(),
            "request_summary": event.summary,
            "hook_name": event.session_id,
            "method": event.method.unwrap_or_else(|| "observer-event".into()),
            "url": format!("observer://{}", event.channel),
            "headers": [],
            "body_text": raw_json_pretty,
            "body_size_bytes": raw_json_pretty.len(),
            "truncated": false,
            "probe_context": probe_context,
            "thread_id": event.thread_id,
            "turn_id": event.turn_id,
            "item_id": event.item_id,
            "timestamp": event.timestamp,
            "tool_visibility": Value::Null,
            "response": Value::Null,
        }
    });
    append_filter_context_fields(&mut payload, filter_context);
    Some(payload.to_string())
}

pub(crate) fn load_observer_session_detail_payload(
    storage: &StorageLayout,
    session_id: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
) -> Option<String> {
    let session = load_observer_sessions(storage, filter)
        .into_iter()
        .find(|session| session.session_id == session_id)?;
    let timeline_items = session
        .events
        .iter()
        .map(|event| {
            json!({
                "request_id": event.event_id,
                "exchange_id": event.turn_id,
                "pid": 0,
                "target_display_name": event.source_label,
                "provider": event.channel,
                "model": event.event_kind,
                "started_at_ms": event.occurred_at_ms,
                "completed_at_ms": event.occurred_at_ms,
                "duration_ms": 0,
                "request_summary": event.summary,
                "response_status": "observed",
                "tool_count_final": if event.event_kind == "tool" { 1 } else { 0 },
                "has_response": false,
                "has_tool_visibility": event.event_kind == "tool",
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "session": {
            "detail_kind": "observer_session",
            "session_id": session.session_id,
            "pid": 0,
            "target_display_name": session.source_label,
            "started_at_ms": session.started_at_ms,
            "completed_at_ms": session.completed_at_ms,
            "exchange_count": timeline_items.len(),
            "timeline_items": timeline_items,
            "artifact_path": session.artifact_path.display().to_string(),
            "transport": session.transport,
            "server_label": session.server_label,
        }
    });
    append_filter_context_fields(&mut payload, filter_context);
    Some(payload.to_string())
}
