use super::{ConsoleActivityItem, ConsoleTargetFilterConfig, ConsoleTargetSummary};
use prismtrace_storage::StorageLayout;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead};
use std::path::Path;

#[derive(Debug, Clone)]
struct ObserverArtifactEventRecord {
    event_id: String,
    event_kind: String,
    summary: String,
    occurred_at_ms: u64,
    source_label: String,
}

#[derive(Debug, Clone)]
struct ObserverArtifactSessionRecord {
    session_id: String,
    channel: String,
    source_label: String,
    completed_at_ms: u64,
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
    let mut server_label = None;
    let mut started_at_ms = 0_u64;
    let mut completed_at_ms = 0_u64;
    let mut events = Vec::new();

    for (index, line) in reader.lines().enumerate() {
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
                let thread_id = value
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let turn_id = value
                    .get("turn_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let item_id = value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
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
                        thread_id,
                        turn_id,
                        item_id,
                    ],
                ) {
                    continue;
                }

                events.push(ObserverArtifactEventRecord {
                    event_id: format!("{session_id}:{index}"),
                    event_kind,
                    summary,
                    occurred_at_ms,
                    source_label: current_source_label,
                });
            }
            _ => {}
        }
    }

    if events.is_empty() {
        return None;
    }

    if completed_at_ms == 0 {
        completed_at_ms = started_at_ms.max(
            events
                .iter()
                .map(|event| event.occurred_at_ms)
                .max()
                .unwrap_or_default(),
        );
    }
    let source_label = observer_source_display_name(&channel, server_label.as_deref());

    Some(ObserverArtifactSessionRecord {
        session_id,
        channel,
        source_label,
        completed_at_ms,
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
        .map(
            |(display_name, (channel, session_count, event_count, last_seen_at_ms))| {
                ConsoleTargetSummary {
                    pid: 0,
                    display_name,
                    runtime_kind: "observer".into(),
                    source_state: "active".into(),
                    source_summary: format!(
                        "official observer · channel: {channel} · sessions: {session_count} · events: {event_count} · last seen: {last_seen_at_ms}"
                    ),
                }
            },
        )
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
