use super::{
    ConsoleFilterContext, ConsoleRequestSummary, ConsoleSessionSummary, ConsoleSessionTimelineItem,
    ConsoleTargetFilterConfig, append_filter_context_fields, load_request_detail,
    load_session_detail, render_session_events_payload, session_detail_matches_filter,
};
use crate::index::ObservabilityIndexStore;
use crate::observability_read_model::{
    EventDetail, EventSummary, ObservabilityReadModel, SessionDetail as ReadModelSessionDetail,
    SessionSummary as ReadModelSessionSummary, load_codex_rollout_session_detail_from_state_db,
};
use prismtrace_analysis::{
    CapabilityProjection, CapabilityRawRef, PromptEventInput, ToolVisibilityCapabilityInput,
    project_tool_visibility_capabilities,
};
use prismtrace_api::{
    ApiFilterContext, render_capability_projection_payload, render_session_diagnostics_payload,
};
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn load_read_model_request_summaries(
    storage: &StorageLayout,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ConsoleRequestSummary> {
    if let Ok(store) = ObservabilityIndexStore::load_read_store(storage, None) {
        return store
            .event_summaries(usize::MAX)
            .map(|events| {
                events
                    .into_iter()
                    .filter(|event| read_model_event_matches_filter(event, filter))
                    .map(console_request_summary_from_read_model_event)
                    .collect()
            })
            .unwrap_or_default();
    }

    ObservabilityReadModel::build(storage)
        .map(|model| {
            model
                .event_summaries(usize::MAX)
                .into_iter()
                .filter(|event| read_model_event_matches_filter(event, filter))
                .map(console_request_summary_from_read_model_event)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn load_read_model_session_summaries(
    storage: &StorageLayout,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ConsoleSessionSummary> {
    if let Some(sessions) = load_codex_thread_session_summaries_from_state_db(storage, filter) {
        return sessions;
    }

    if let Ok(store) = ObservabilityIndexStore::load_read_store(storage, None) {
        return store
            .session_summaries(usize::MAX)
            .map(|sessions| {
                sessions
                    .into_iter()
                    .filter(|session| session.source.kind == "codex_rollout")
                    .filter(|session| read_model_session_matches_filter(session, filter))
                    .map(console_session_summary_from_read_model)
                    .collect()
            })
            .unwrap_or_default();
    }

    ObservabilityReadModel::build(storage)
        .map(|model| {
            model
                .session_summaries(usize::MAX)
                .into_iter()
                .filter(|session| session.source.kind == "codex_rollout")
                .filter(|session| read_model_session_matches_filter(session, filter))
                .map(console_session_summary_from_read_model)
                .collect()
        })
        .unwrap_or_default()
}

fn load_codex_thread_session_summaries_from_state_db(
    _storage: &StorageLayout,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Option<Vec<ConsoleSessionSummary>> {
    let codex_home = env::var_os("HOME")?;
    let db_path = Path::new(&codex_home).join(".codex/state_5.sqlite");
    if !db_path.exists() {
        return None;
    }

    let query = "select id, rollout_path, title, first_user_message, cwd, created_at_ms, updated_at_ms \
         from threads \
         where archived = 0 and source in ('cli', 'vscode') \
         order by updated_at_ms desc;";
    let output = Command::new("sqlite3")
        .arg("-batch")
        .arg("-noheader")
        .arg("-separator")
        .arg("\u{1f}")
        .arg(db_path)
        .arg(query)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let sessions = stdout
        .lines()
        .filter_map(console_session_summary_from_state_db_row)
        .filter(|session| console_session_matches_filter(session, filter))
        .collect();
    Some(sessions)
}

fn console_session_summary_from_state_db_row(row: &str) -> Option<ConsoleSessionSummary> {
    let mut fields = row.split('\u{1f}');
    let thread_id = fields.next()?.to_string();
    if !is_codex_thread_id(&thread_id) {
        return None;
    }
    let rollout_path = fields.next()?.to_string();
    let title = fields.next().unwrap_or_default().trim().to_string();
    let first_user_message = fields.next().unwrap_or_default().trim().to_string();
    let cwd = fields.next().map(ToString::to_string);
    let started_at_ms = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default();
    let completed_at_ms = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(started_at_ms);
    let title = if title.is_empty() {
        first_user_message
    } else {
        title
    };

    Some(ConsoleSessionSummary {
        session_id: format!("codex-thread:{thread_id}"),
        title,
        subtitle: format!("thread {thread_id}"),
        cwd,
        artifact_path: Some(rollout_path),
        pid: 0,
        target_display_name: "Codex Desktop".into(),
        started_at_ms,
        completed_at_ms,
        exchange_count: 0,
        request_count: 0,
        response_count: 0,
    })
}

fn is_codex_thread_id(value: &str) -> bool {
    value.len() == 36
        && value.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        && value.chars().filter(|ch| *ch == '-').count() == 4
}

pub(crate) fn load_read_model_request_detail_payload(
    storage: &StorageLayout,
    request_id: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
) -> Option<String> {
    if let Ok(store) = ObservabilityIndexStore::load_read_store(storage, None)
        && let Ok(Some(event)) = store.event_detail(request_id)
    {
        if !read_model_event_detail_matches_filter(&event, filter) {
            return None;
        }
        return Some(render_read_model_event_detail_payload(
            &event,
            filter_context,
        ));
    }

    let model = ObservabilityReadModel::build(storage).ok()?;
    let event = model.event_detail(request_id)?;
    if !read_model_event_detail_matches_filter(&event, filter) {
        return None;
    }

    Some(render_read_model_event_detail_payload(
        &event,
        filter_context,
    ))
}

pub(crate) fn load_read_model_event_detail_payload(
    storage: &StorageLayout,
    event_id: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
) -> Option<String> {
    if let Ok(store) = ObservabilityIndexStore::load_read_store(storage, None)
        && let Ok(Some(event)) = store.event_detail(event_id)
    {
        if !read_model_event_detail_matches_filter(&event, filter) {
            return None;
        }
        return Some(render_read_model_event_api_payload(&event, filter_context));
    }

    let model = ObservabilityReadModel::build(storage).ok()?;
    let event = model.event_detail(event_id)?;
    if !read_model_event_detail_matches_filter(&event, filter) {
        return None;
    }

    Some(render_read_model_event_api_payload(&event, filter_context))
}

pub(crate) fn render_read_model_event_not_found_payload(
    event_id: &str,
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let mut payload = json!({
        "event": {
            "event_id": event_id,
            "status": "not_found",
            "detail": "event detail is not available yet",
        }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub(crate) fn load_read_model_session_detail_payload(
    storage: &StorageLayout,
    session_id: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
) -> Option<String> {
    if let Ok(Some(session)) = load_codex_rollout_session_detail_from_state_db(storage, session_id)
    {
        if !read_model_session_matches_filter(&session.summary, filter) {
            return None;
        }
        return Some(render_read_model_session_detail_payload(
            &session,
            filter_context,
        ));
    }

    if let Ok(store) = ObservabilityIndexStore::load_read_store(storage, None)
        && let Ok(Some(session)) = store.session_detail(session_id)
    {
        if !read_model_session_matches_filter(&session.summary, filter) {
            return None;
        }
        return Some(render_read_model_session_detail_payload(
            &session,
            filter_context,
        ));
    }

    let model = ObservabilityReadModel::build(storage).ok()?;
    let session = model.session_detail(session_id)?;
    if !read_model_session_matches_filter(&session.summary, filter) {
        return None;
    }

    Some(render_read_model_session_detail_payload(
        &session,
        filter_context,
    ))
}

pub(crate) fn load_read_model_session_events_payload(
    storage: &StorageLayout,
    session_id: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
    pagination: Option<(usize, usize)>,
) -> Option<String> {
    if let Ok(Some(session)) = load_codex_rollout_session_detail_from_state_db(storage, session_id)
    {
        if !read_model_session_matches_filter(&session.summary, filter) {
            return None;
        }
        let timeline_items = console_timeline_items_from_read_model_session(&session);
        return Some(render_session_events_payload(
            session_id,
            &timeline_items,
            filter_context,
            pagination,
        ));
    }

    if let Ok(store) = ObservabilityIndexStore::load_read_store(storage, None)
        && let Ok(Some(session)) = store.session_detail(session_id)
    {
        if !read_model_session_matches_filter(&session.summary, filter) {
            return None;
        }
        let timeline_items = console_timeline_items_from_read_model_session(&session);
        return Some(render_session_events_payload(
            session_id,
            &timeline_items,
            filter_context,
            pagination,
        ));
    }

    let model = ObservabilityReadModel::build(storage).ok()?;
    let session = model.session_detail(session_id)?;
    if !read_model_session_matches_filter(&session.summary, filter) {
        return None;
    }
    let timeline_items = console_timeline_items_from_read_model_session(&session);
    Some(render_session_events_payload(
        session_id,
        &timeline_items,
        filter_context,
        pagination,
    ))
}

pub(crate) fn load_session_capabilities_payload(
    storage: &StorageLayout,
    session_id: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
) -> Option<String> {
    if let Some(session) = load_session_detail(storage, session_id).ok().flatten() {
        if !session_detail_matches_filter(&session, filter) {
            return None;
        }
        let capabilities =
            legacy_session_tool_visibility_capabilities(storage, session_id, &session);
        let api_filter_context = api_filter_context(filter_context);
        return Some(render_capability_projection_payload(
            session_id,
            &capabilities,
            api_filter_context.as_ref(),
        ));
    }

    if let Ok(store) = ObservabilityIndexStore::load_read_store(storage, None)
        && let Ok(Some(session)) = store.session_detail(session_id)
    {
        if !read_model_session_matches_filter(&session.summary, filter) {
            return None;
        }
        let capabilities = store.session_capabilities(session_id);
        let api_filter_context = api_filter_context(filter_context);
        return Some(render_capability_projection_payload(
            session_id,
            &capabilities,
            api_filter_context.as_ref(),
        ));
    }

    let model = ObservabilityReadModel::build(storage).ok()?;
    let session = model.session_detail(session_id)?;
    if !read_model_session_matches_filter(&session.summary, filter) {
        return None;
    }
    let capabilities = model.session_capabilities(session_id);
    let api_filter_context = api_filter_context(filter_context);
    Some(render_capability_projection_payload(
        session_id,
        &capabilities,
        api_filter_context.as_ref(),
    ))
}

pub(crate) fn load_session_diagnostics_payload(
    storage: &StorageLayout,
    session_id: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
) -> Option<String> {
    if let Some(session) = load_session_detail(storage, session_id).ok().flatten() {
        if !session_detail_matches_filter(&session, filter) {
            return None;
        }
        let capabilities =
            legacy_session_tool_visibility_capabilities(storage, session_id, &session);
        return Some(render_read_model_session_diagnostics_payload(
            session_id,
            &[],
            &capabilities,
            filter_context,
        ));
    }

    if let Ok(store) = ObservabilityIndexStore::load_read_store(storage, None)
        && let Ok(Some(session)) = store.session_detail(session_id)
    {
        if !read_model_session_matches_filter(&session.summary, filter) {
            return None;
        }
        let events = session
            .events
            .iter()
            .filter_map(|event| store.event_detail(&event.event_id).ok().flatten())
            .collect::<Vec<_>>();
        let capabilities = store.session_capabilities(session_id);
        return Some(render_read_model_session_diagnostics_payload(
            session_id,
            &events,
            &capabilities,
            filter_context,
        ));
    }

    let model = ObservabilityReadModel::build(storage).ok()?;
    let session = model.session_detail(session_id)?;
    if !read_model_session_matches_filter(&session.summary, filter) {
        return None;
    }
    let events = session
        .events
        .iter()
        .filter_map(|event| model.event_detail(&event.event_id))
        .collect::<Vec<_>>();
    let capabilities = model.session_capabilities(session_id);
    Some(render_read_model_session_diagnostics_payload(
        session_id,
        &events,
        &capabilities,
        filter_context,
    ))
}

fn console_request_summary_from_read_model_event(event: EventSummary) -> ConsoleRequestSummary {
    ConsoleRequestSummary {
        request_id: event.event_id,
        captured_at_ms: event.occurred_at_ms,
        provider: event
            .source
            .channel
            .clone()
            .unwrap_or_else(|| event.source.kind.clone()),
        model: Some(event.event_kind),
        target_display_name: event.source.display_name,
        summary_text: event.summary,
    }
}

fn console_session_summary_from_read_model(
    session: ReadModelSessionSummary,
) -> ConsoleSessionSummary {
    ConsoleSessionSummary {
        session_id: session.session_id,
        title: session.title,
        subtitle: session.subtitle,
        cwd: session.cwd,
        artifact_path: Some(session.artifact.path.display().to_string()),
        pid: 0,
        target_display_name: session.source.display_name,
        started_at_ms: session.started_at_ms,
        completed_at_ms: session.completed_at_ms,
        exchange_count: session.event_count,
        request_count: session.event_count,
        response_count: session.response_count,
    }
}

fn console_timeline_items_from_read_model_session(
    session: &ReadModelSessionDetail,
) -> Vec<ConsoleSessionTimelineItem> {
    session
        .events
        .iter()
        .map(|event| {
            let provider = event
                .source
                .channel
                .as_deref()
                .unwrap_or(event.source.kind.as_str());
            ConsoleSessionTimelineItem {
                request_id: event.event_id.clone(),
                exchange_id: if event.source.kind == "codex_rollout" {
                    session
                        .summary
                        .session_id
                        .strip_prefix("codex-thread:")
                        .map(ToString::to_string)
                } else {
                    None
                },
                pid: 0,
                target_display_name: event.source.display_name.clone(),
                provider: if event.source.kind == "codex_rollout" {
                    "codex-rollout".into()
                } else {
                    provider.to_string()
                },
                model: Some(event.event_kind.clone()),
                started_at_ms: event.occurred_at_ms,
                completed_at_ms: event.occurred_at_ms,
                duration_ms: 0,
                request_summary: event.summary.clone(),
                response_status: None,
                tool_count_final: usize::from(event.event_kind == "tool"),
                has_response: false,
                has_tool_visibility: event.event_kind == "tool",
            }
        })
        .collect()
}

fn render_read_model_event_detail_payload(
    event: &EventDetail,
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let raw_json_pretty =
        serde_json::to_string_pretty(&event.raw_json).unwrap_or_else(|_| "null".into());
    let source_json = &event.detail_json;
    let provider = event
        .source
        .channel
        .as_deref()
        .unwrap_or(event.source.kind.as_str());

    let request = if event.source.kind == "codex_rollout" {
        json!({
            "detail_kind": "codex_rollout_event",
            "request_id": event.event_id,
            "exchange_id": Value::Null,
            "captured_at_ms": event.occurred_at_ms,
            "provider": "codex-rollout",
            "model": event.event_kind,
            "target_display_name": event.source.display_name,
            "artifact_path": event.artifact.path.display().to_string(),
            "request_summary": event.summary,
            "hook_name": "codex rollout",
            "method": "local-jsonl",
            "url": "codex://rollout",
            "headers": [],
            "codex_rollout": event.detail_json,
            "body_text": raw_json_pretty,
            "body_size_bytes": raw_json_pretty.len(),
            "truncated": false,
            "probe_context": "source=codex rollout jsonl",
            "tool_visibility": Value::Null,
            "response": Value::Null,
        })
    } else {
        let thread_id = source_json
            .get("thread_id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let turn_id = source_json
            .get("turn_id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let item_id = source_json
            .get("item_id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let timestamp = source_json
            .get("timestamp")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        let probe_context = format!(
            "thread={} · turn={} · item={} · timestamp={}",
            thread_id.as_deref().unwrap_or("n/a"),
            turn_id.as_deref().unwrap_or("n/a"),
            item_id.as_deref().unwrap_or("n/a"),
            timestamp.as_deref().unwrap_or("n/a")
        );

        json!({
            "detail_kind": "observer_event",
            "request_id": event.event_id,
            "exchange_id": turn_id,
            "captured_at_ms": event.occurred_at_ms,
            "provider": provider,
            "model": event.event_kind,
            "target_display_name": event.source.display_name,
            "artifact_path": event.artifact.path.display().to_string(),
            "request_summary": event.summary,
            "hook_name": event.session_id,
            "method": source_json
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("observer-event"),
            "url": format!("observer://{provider}"),
            "headers": [],
            "body_text": raw_json_pretty,
            "body_size_bytes": raw_json_pretty.len(),
            "truncated": false,
            "probe_context": probe_context,
            "thread_id": thread_id,
            "turn_id": source_json.get("turn_id").and_then(Value::as_str),
            "item_id": item_id,
            "timestamp": timestamp,
            "tool_visibility": Value::Null,
            "response": Value::Null,
        })
    };

    let mut payload = json!({ "request": request });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

fn render_read_model_event_api_payload(
    event: &EventDetail,
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let mut payload = json!({
        "event": {
            "event_id": &event.event_id,
            "session_id": &event.session_id,
            "source": {
                "kind": &event.source.kind,
                "display_name": &event.source.display_name,
                "channel": &event.source.channel,
            },
            "artifact": {
                "path": event.artifact.path.display().to_string(),
                "line_index": event.artifact.line_index,
            },
            "event_kind": &event.event_kind,
            "summary": &event.summary,
            "occurred_at_ms": event.occurred_at_ms,
            "raw_json": &event.raw_json,
            "detail": &event.detail_json,
        }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

fn render_read_model_session_detail_payload(
    session: &ReadModelSessionDetail,
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let timeline_items = session
        .events
        .iter()
        .map(|event| {
            let provider = event
                .source
                .channel
                .as_deref()
                .unwrap_or(event.source.kind.as_str());
            json!({
                "request_id": event.event_id,
                "exchange_id": if event.source.kind == "codex_rollout" {
                    session.summary.session_id.strip_prefix("codex-thread:").map(ToString::to_string)
                } else {
                    None::<String>
                },
                "pid": 0,
                "target_display_name": event.source.display_name,
                "provider": if event.source.kind == "codex_rollout" { "codex-rollout" } else { provider },
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
    let detail_kind = if session.summary.source.kind == "codex_rollout" {
        "codex_thread"
    } else {
        "observer_session"
    };
    let mut session_payload = json!({
        "detail_kind": detail_kind,
        "session_id": session.summary.session_id,
        "pid": 0,
        "target_display_name": session.summary.source.display_name,
        "started_at_ms": session.summary.started_at_ms,
        "completed_at_ms": session.summary.completed_at_ms,
        "exchange_count": timeline_items.len(),
        "timeline_items": timeline_items,
        "artifact_path": session.summary.artifact.path.display().to_string(),
        "cwd": session.summary.cwd,
    });
    if session.summary.source.kind == "codex_rollout" {
        session_payload["thread_id"] = json!(
            session
                .summary
                .session_id
                .trim_start_matches("codex-thread:")
        );
        session_payload["title"] = json!(session.summary.title);
    }

    let mut payload = json!({ "session": session_payload });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

fn legacy_session_tool_visibility_capabilities(
    storage: &StorageLayout,
    session_id: &str,
    session: &super::ConsoleSessionDetail,
) -> Vec<CapabilityProjection> {
    let mut capabilities = Vec::new();
    for item in &session.timeline_items {
        if !item.has_tool_visibility {
            continue;
        }
        let Ok(Some(detail)) = load_request_detail(storage, &item.request_id) else {
            continue;
        };
        let Some(tool_visibility) = detail.tool_visibility else {
            continue;
        };
        let final_tools_json = serde_json::from_str::<Value>(&tool_visibility.final_tools_json)
            .unwrap_or_else(|_| Value::Array(Vec::new()));

        capabilities.extend(project_tool_visibility_capabilities(
            ToolVisibilityCapabilityInput {
                session_id,
                event_id: &detail.request_id,
                source_kind: "tool_visibility",
                observed_at_ms: detail.captured_at_ms,
                visibility_stage: &tool_visibility.visibility_stage,
                raw_ref: CapabilityRawRef {
                    path: PathBuf::from(&tool_visibility.artifact_path),
                    line_index: None,
                },
                final_tools_json: &final_tools_json,
            },
        ));
    }

    capabilities.sort_by(|left, right| {
        left.observed_at_ms
            .cmp(&right.observed_at_ms)
            .then_with(|| left.capability_type.cmp(&right.capability_type))
            .then_with(|| left.capability_name.cmp(&right.capability_name))
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    capabilities.dedup_by(|left, right| {
        left.session_id == right.session_id
            && left.event_id == right.event_id
            && left.capability_type == right.capability_type
            && left.capability_name == right.capability_name
    });
    capabilities
}

fn render_read_model_session_diagnostics_payload(
    session_id: &str,
    events: &[EventDetail],
    capabilities: &[CapabilityProjection],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let prompt_events = events
        .iter()
        .map(prompt_event_input_from_read_model_event)
        .collect::<Vec<_>>();
    let api_filter_context = api_filter_context(filter_context);
    render_session_diagnostics_payload(
        session_id,
        &prompt_events,
        capabilities,
        api_filter_context.as_ref(),
    )
}

fn prompt_event_input_from_read_model_event(event: &EventDetail) -> PromptEventInput<'_> {
    PromptEventInput {
        session_id: &event.session_id,
        event_id: &event.event_id,
        event_kind: &event.event_kind,
        summary: &event.summary,
        occurred_at_ms: event.occurred_at_ms,
        raw_ref: CapabilityRawRef {
            path: event.artifact.path.clone(),
            line_index: event.artifact.line_index,
        },
        raw_json: &event.raw_json,
        detail_json: &event.detail_json,
    }
}

fn api_filter_context(filter_context: Option<&ConsoleFilterContext>) -> Option<ApiFilterContext> {
    filter_context.map(|filter_context| ApiFilterContext {
        active_filters: filter_context.active_filters.clone(),
        is_filtered_view: filter_context.is_filtered_view,
    })
}

fn read_model_event_matches_filter(
    event: &EventSummary,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> bool {
    read_model_filter_matches(
        filter,
        &[
            &event.source.display_name,
            event.source.channel.as_deref().unwrap_or_default(),
            &event.event_kind,
            &event.summary,
            &event.session_id,
        ],
    )
}

fn read_model_event_detail_matches_filter(
    event: &EventDetail,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> bool {
    read_model_filter_matches(
        filter,
        &[
            &event.source.display_name,
            event.source.channel.as_deref().unwrap_or_default(),
            &event.event_kind,
            &event.summary,
            &event.session_id,
        ],
    )
}

fn read_model_session_matches_filter(
    session: &ReadModelSessionSummary,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> bool {
    read_model_filter_matches(
        filter,
        &[
            &session.source.display_name,
            session.source.channel.as_deref().unwrap_or_default(),
            &session.title,
            &session.subtitle,
            session.cwd.as_deref().unwrap_or_default(),
            &session.session_id,
        ],
    )
}

fn console_session_matches_filter(
    session: &ConsoleSessionSummary,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> bool {
    read_model_filter_matches(
        filter,
        &[
            &session.target_display_name,
            &session.title,
            &session.subtitle,
            session.cwd.as_deref().unwrap_or_default(),
            &session.session_id,
        ],
    )
}

fn read_model_filter_matches(filter: Option<&ConsoleTargetFilterConfig>, fields: &[&str]) -> bool {
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
