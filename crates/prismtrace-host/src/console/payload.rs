use super::{
    ConsoleActivityItem, ConsoleFilterContext, ConsoleRequestDetail, ConsoleRequestSummary,
    ConsoleSessionDetail, ConsoleSessionSummary, ConsoleTargetFilterConfig, ConsoleTargetSummary,
};
use prismtrace_core::ProcessTarget;
use serde_json::{Value, json};
use std::path::PathBuf;

pub(crate) fn render_health_payload(
    targets: &[ConsoleTargetSummary],
    activity_items: &[ConsoleActivityItem],
    filter: Option<&ConsoleTargetFilterConfig>,
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let filtered_targets = if let Some(filter) = filter {
        if filter.is_enabled() {
            targets
                .iter()
                .filter(|target| {
                    let candidate = ProcessTarget {
                        pid: target.pid,
                        app_name: target.display_name.clone(),
                        executable_path: PathBuf::from(&target.display_name),
                        command_line: None,
                        runtime_kind: prismtrace_core::RuntimeKind::Unknown,
                    };
                    filter.matches_target(&candidate)
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            targets.to_vec()
        }
    } else {
        targets.to_vec()
    };

    let source_summary = filtered_targets
        .iter()
        .find(|target| target.source_state == "active")
        .map(|target| target.source_summary.clone())
        .or_else(|| {
            filtered_targets
                .first()
                .map(|target| target.source_summary.clone())
        });

    let errors = activity_items
        .iter()
        .filter(|item| item.activity_type == "error")
        .filter(|item| match item.related_pid {
            Some(pid) => filtered_targets.iter().any(|target| target.pid == pid),
            None => true,
        })
        .map(|item| {
            json!({
                "title": item.title,
                "subtitle": item.subtitle,
                "related_pid": item.related_pid,
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "source_summary": source_summary,
        "errors": errors,
        "empty_state": if source_summary.is_none() && errors.is_empty() { Some("尚未发现 source 健康或错误提示") } else { None::<&str> }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

fn filtered_empty_state_message(
    filter_context: Option<&ConsoleFilterContext>,
    default: &str,
) -> String {
    match filter_context {
        Some(filter_context) if filter_context.is_filtered_view => match default {
            "尚无可观测目标" => "当前过滤条件下没有匹配目标".to_string(),
            "尚无可观测 source" => "当前过滤条件下没有匹配 source".to_string(),
            "尚无观测活动" => "当前过滤条件下没有匹配活动".to_string(),
            "尚无请求记录" => "当前过滤条件下没有匹配请求".to_string(),
            "尚无会话记录" => "当前过滤条件下没有匹配会话".to_string(),
            _ => default.to_string(),
        },
        _ => default.to_string(),
    }
}

pub(crate) fn render_targets_payload_from_summaries(
    targets: &[ConsoleTargetSummary],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let targets = targets
        .iter()
        .map(|target| {
            json!({
                "pid": target.pid,
                "display_name": target.display_name,
                "runtime_kind": target.runtime_kind,
                "source_state": target.source_state,
                "source_summary": target.source_summary,
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "targets": targets,
        "empty_state": if targets.is_empty() { Some(filtered_empty_state_message(filter_context, "尚无可观测 source")) } else { None::<String> }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub(crate) fn render_activity_payload_from_items(
    items: &[ConsoleActivityItem],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let activity = items
        .iter()
        .map(|item| {
            json!({
                "activity_id": item.activity_id,
                "activity_type": item.activity_type,
                "occurred_at_ms": item.occurred_at_ms,
                "title": item.title,
                "subtitle": item.subtitle,
                "related_pid": item.related_pid,
                "related_request_id": item.related_request_id,
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "activity": activity,
        "empty_state": if items.is_empty() { Some(filtered_empty_state_message(filter_context, "尚无观测活动")) } else { None::<String> }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub(crate) fn render_requests_payload(
    requests: &[ConsoleRequestSummary],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let requests = requests
        .iter()
        .map(|request| {
            json!({
                "request_id": request.request_id,
                "captured_at_ms": request.captured_at_ms,
                "provider": request.provider,
                "model": request.model,
                "target_display_name": request.target_display_name,
                "summary_text": request.summary_text,
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "requests": requests,
        "empty_state": if requests.is_empty() { Some(filtered_empty_state_message(filter_context, "尚无请求记录")) } else { None::<String> }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub(crate) fn render_sessions_payload(
    sessions: &[ConsoleSessionSummary],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let sessions = sessions
        .iter()
        .map(|session| {
            json!({
                "session_id": session.session_id,
                "pid": session.pid,
                "target_display_name": session.target_display_name,
                "started_at_ms": session.started_at_ms,
                "completed_at_ms": session.completed_at_ms,
                "exchange_count": session.exchange_count,
                "request_count": session.request_count,
                "response_count": session.response_count,
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "sessions": sessions,
        "empty_state": if sessions.is_empty() { Some(filtered_empty_state_message(filter_context, "尚无会话记录")) } else { None::<String> }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub(crate) fn render_request_detail_payload(
    request_id: &str,
    detail: Option<ConsoleRequestDetail>,
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let mut payload = match detail {
        Some(detail) => {
            let headers = detail
                .headers
                .iter()
                .map(|header| {
                    json!({
                        "name": &header.name,
                        "value": &header.value,
                    })
                })
                .collect::<Vec<_>>();
            let response_payload = detail.response.as_ref().map(|response| {
                json!({
                    "artifact_path": &response.artifact_path,
                    "status_code": response.status_code,
                    "headers": response.headers.iter().map(|header| {
                        json!({
                            "name": &header.name,
                            "value": &header.value,
                        })
                    }).collect::<Vec<_>>(),
                    "body_text": &response.body_text,
                    "body_size_bytes": response.body_size_bytes,
                    "truncated": response.truncated,
                    "started_at_ms": response.started_at_ms,
                    "completed_at_ms": response.completed_at_ms,
                    "duration_ms": response.duration_ms,
                })
            });
            let tool_visibility_payload = detail.tool_visibility.as_ref().map(|visibility| {
                json!({
                    "artifact_path": &visibility.artifact_path,
                    "visibility_stage": &visibility.visibility_stage,
                    "tool_choice": &visibility.tool_choice,
                    "tool_count_final": visibility.tool_count_final,
                    "final_tools": visibility.final_tools.iter().map(|tool| {
                        json!({
                            "name": &tool.name,
                            "tool_type": &tool.tool_type,
                        })
                    }).collect::<Vec<_>>(),
                    "final_tools_json": &visibility.final_tools_json,
                })
            });

            json!({
                "request": {
                    "request_id": detail.request_id,
                    "exchange_id": detail.exchange_id,
                    "captured_at_ms": detail.captured_at_ms,
                    "provider": detail.provider,
                    "model": detail.model,
                    "target_display_name": detail.target_display_name,
                    "artifact_path": detail.artifact_path,
                    "request_summary": detail.request_summary,
                    "hook_name": detail.hook_name,
                    "method": detail.method,
                    "url": detail.url,
                    "headers": headers,
                    "body_text": detail.body_text,
                    "body_size_bytes": detail.body_size_bytes,
                    "truncated": detail.truncated,
                    "probe_context": detail.probe_context,
                    "tool_visibility": tool_visibility_payload,
                    "response": response_payload,
                }
            })
        }
        None => json!({
            "request": {
                "request_id": request_id,
                "status": "not_found",
                "detail": "request detail is not available yet"
            }
        }),
    };
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub(crate) fn render_session_detail_payload(
    session_id: &str,
    detail: Option<ConsoleSessionDetail>,
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    let mut payload = match detail {
        Some(detail) => {
            let timeline_items = detail
                .timeline_items
                .iter()
                .map(|item| {
                    json!({
                        "request_id": item.request_id,
                        "exchange_id": item.exchange_id,
                        "pid": item.pid,
                        "target_display_name": item.target_display_name,
                        "provider": item.provider,
                        "model": item.model,
                        "started_at_ms": item.started_at_ms,
                        "completed_at_ms": item.completed_at_ms,
                        "duration_ms": item.duration_ms,
                        "request_summary": item.request_summary,
                        "response_status": item.response_status,
                        "tool_count_final": item.tool_count_final,
                        "has_response": item.has_response,
                        "has_tool_visibility": item.has_tool_visibility,
                    })
                })
                .collect::<Vec<_>>();

            json!({
                "session": {
                    "session_id": detail.session_id,
                    "pid": detail.pid,
                    "target_display_name": detail.target_display_name,
                    "started_at_ms": detail.started_at_ms,
                    "completed_at_ms": detail.completed_at_ms,
                    "exchange_count": detail.exchange_count,
                    "timeline_items": timeline_items,
                }
            })
        }
        None => json!({
            "session": {
                "session_id": session_id,
                "status": "not_found",
                "detail": "session detail is not available yet"
            }
        }),
    };
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub(crate) fn append_filter_context_fields(
    payload: &mut Value,
    filter_context: Option<&ConsoleFilterContext>,
) {
    let Some(filter_context) = filter_context else {
        return;
    };

    payload["active_filters"] = json!(filter_context.active_filters);
    payload["is_filtered_view"] = json!(filter_context.is_filtered_view);
}
