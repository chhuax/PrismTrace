use crate::BootstrapResult;
use crate::discovery::{ProcessSampleSource, discover_targets};
use prismtrace_api::{
    ApiFilterContext, render_empty_capability_projection_payload,
    render_empty_session_diagnostics_payload,
};
use prismtrace_core::ProcessTarget;
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

mod api;
pub(crate) mod model;
mod observer;
mod page;
mod payload;
mod server;

pub(crate) use self::api::{
    load_read_model_event_detail_payload, load_read_model_request_detail_payload,
    load_read_model_request_summaries, load_read_model_session_detail_payload,
    load_read_model_session_events_payload, load_read_model_session_summaries,
    load_session_capabilities_payload, load_session_diagnostics_payload,
    render_read_model_event_not_found_payload,
};
pub use self::model::{
    ConsoleActivityItem, ConsoleFilterContext, ConsoleHeaderDetail, ConsoleKnownErrorActivity,
    ConsoleRecentRequestActivity, ConsoleRequestDetail, ConsoleRequestSummary,
    ConsoleResponseDetail, ConsoleSessionDetail, ConsoleSessionSummary, ConsoleSessionTimelineItem,
    ConsoleSnapshot, ConsoleTargetFilterConfig, ConsoleTargetSummary, ConsoleToolSummary,
    ConsoleToolVisibilityDetail,
};
pub(crate) use self::observer::{load_observer_activity_items, load_observer_target_summaries};
pub(crate) use self::page::render_console_homepage;
#[cfg(test)]
pub(crate) use self::payload::render_health_payload;
pub(crate) use self::payload::{
    append_filter_context_fields, render_activity_payload_from_items,
    render_health_payload_with_state_root, render_request_detail_payload, render_requests_payload,
    render_session_detail_payload, render_session_events_payload,
    render_sessions_payload_with_pagination, render_targets_payload_from_summaries,
};
pub(crate) use self::server::collect_console_snapshot_for_bind_addr;
pub use self::server::{
    ConsoleServer, collect_console_snapshot, console_startup_report, run_console_server,
    run_console_server_with_target_filters, start_console_server,
    start_console_server_on_bind_addr, start_console_server_with_target_filters,
};

const SESSION_WINDOW_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone)]
struct RequestArtifactRecord {
    request_id: String,
    exchange_id: Option<String>,
    pid: Option<u32>,
    captured_at_ms: u64,
    provider: String,
    model: Option<String>,
    target_display_name: String,
    hook_name: String,
    method: String,
    url: String,
    headers: Vec<ConsoleHeaderDetail>,
    body_text: Option<String>,
    body_size_bytes: usize,
    truncated: bool,
    artifact_path: PathBuf,
}

#[derive(Debug, Clone)]
struct ResponseArtifactRecord {
    exchange_id: String,
    status_code: u16,
    headers: Vec<ConsoleHeaderDetail>,
    body_text: Option<String>,
    body_size_bytes: usize,
    truncated: bool,
    started_at_ms: u64,
    completed_at_ms: u64,
    duration_ms: u64,
    artifact_path: PathBuf,
}

#[derive(Debug, Clone)]
struct ToolVisibilityArtifactRecord {
    request_id: String,
    exchange_id: Option<String>,
    captured_at_ms: u64,
    visibility_stage: String,
    tool_choice: Option<String>,
    tool_count_final: usize,
    final_tools: Vec<ConsoleToolSummary>,
    final_tools_json: String,
    artifact_path: PathBuf,
}

#[derive(Debug, Clone)]
struct ExchangeRecord {
    request_id: String,
    exchange_id: Option<String>,
    pid: u32,
    target_display_name: String,
    provider: String,
    model: Option<String>,
    started_at_ms: u64,
    completed_at_ms: u64,
    duration_ms: u64,
    request_summary: String,
    response_status: Option<u16>,
    tool_count_final: usize,
    has_response: bool,
    has_tool_visibility: bool,
}

pub struct ConsoleActivitySource<'a> {
    pub recent_requests: &'a [ConsoleRecentRequestActivity],
    pub known_errors: &'a [ConsoleKnownErrorActivity],
}

#[cfg(test)]
fn write_console_response(stream: &mut TcpStream, snapshot: &ConsoleSnapshot) -> io::Result<()> {
    write_console_response_with_storage(stream, snapshot, None, None)
}

#[cfg(test)]
fn write_console_response_with_storage(
    stream: &mut TcpStream,
    snapshot: &ConsoleSnapshot,
    storage: Option<&StorageLayout>,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<()> {
    let request_path = read_request_path(stream)?;
    write_console_response_for_path(request_path, stream, snapshot, storage, filter)
}

fn render_console_static_asset(path: &str) -> Option<(&'static str, Vec<u8>)> {
    match path {
        "/assets/console.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../../assets/console.css").to_vec(),
        )),
        "/assets/console.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../../assets/console.js").to_vec(),
        )),
        "/assets/console-utilities.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../../assets/console-utilities.css").to_vec(),
        )),
        "/assets/console-base.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../../assets/console-base.css").to_vec(),
        )),
        "/assets/console-theme-dark.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../../assets/console-theme-dark.css").to_vec(),
        )),
        "/assets/console-theme-light.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../../assets/console-theme-light.css").to_vec(),
        )),
        "/assets/i18n/en-US.json" => Some((
            "application/json; charset=utf-8",
            include_bytes!("../../assets/i18n/en-US.json").to_vec(),
        )),
        "/assets/i18n/zh-CN.json" => Some((
            "application/json; charset=utf-8",
            include_bytes!("../../assets/i18n/zh-CN.json").to_vec(),
        )),
        "/assets/prismtrace-logo.png" => Some((
            "image/png",
            include_bytes!("../../assets/prismtrace-logo.png").to_vec(),
        )),
        _ => None,
    }
}

fn sort_target_summaries(targets: &mut [ConsoleTargetSummary]) {
    targets.sort_by(|left, right| {
        left.pid
            .cmp(&right.pid)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
}

fn sort_activity_items(items: &mut [ConsoleActivityItem]) {
    items.sort_by(|left, right| {
        right
            .occurred_at_ms
            .cmp(&left.occurred_at_ms)
            .then_with(|| left.activity_id.cmp(&right.activity_id))
    });
}

fn sort_request_summaries(requests: &mut [ConsoleRequestSummary]) {
    requests.sort_by(|left, right| {
        right
            .captured_at_ms
            .cmp(&left.captured_at_ms)
            .then_with(|| left.request_id.cmp(&right.request_id))
    });
}

fn sort_session_summaries(sessions: &mut [ConsoleSessionSummary]) {
    sessions.sort_by(|left, right| {
        right
            .completed_at_ms
            .cmp(&left.completed_at_ms)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
}

fn dedup_target_summaries(targets: &mut Vec<ConsoleTargetSummary>) {
    let mut seen = HashSet::new();
    targets.retain(|target| {
        seen.insert(format!(
            "{}:{}:{}",
            target.pid, target.display_name, target.source_state
        ))
    });
}

fn dedup_activity_items(items: &mut Vec<ConsoleActivityItem>) {
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.activity_id.clone()));
}

fn dedup_request_summaries(requests: &mut Vec<ConsoleRequestSummary>) {
    let mut seen = HashSet::new();
    requests.retain(|request| seen.insert(request.request_id.clone()));
}

fn dedup_session_summaries(sessions: &mut Vec<ConsoleSessionSummary>) {
    let mut seen = HashSet::new();
    sessions.retain(|session| seen.insert(session.session_id.clone()));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConsoleRouteResponse {
    status_line: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

fn write_console_response_for_path(
    request_path: Option<String>,
    stream: &mut TcpStream,
    snapshot: &ConsoleSnapshot,
    storage: Option<&StorageLayout>,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<()> {
    let response =
        render_console_route_response(request_path.as_deref(), snapshot, storage, filter);
    write_console_route_response(stream, &response)
}

fn render_console_route_response(
    request_path: Option<&str>,
    snapshot: &ConsoleSnapshot,
    storage: Option<&StorageLayout>,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> ConsoleRouteResponse {
    if let Some((content_type, body)) = request_path
        .map(route_path)
        .and_then(render_console_static_asset)
    {
        return ConsoleRouteResponse {
            status_line: "HTTP/1.1 200 OK",
            content_type,
            body,
        };
    }

    let request_target = request_path;
    let route = request_target.map(route_path);
    let (status_line, content_type, body) = match route {
        Some("/") => (
            "HTTP/1.1 200 OK",
            "text/html; charset=utf-8",
            render_console_homepage(snapshot),
        ),
        Some("/favicon.ico") => ("HTTP/1.1 200 OK", "image/x-icon", String::new()),
        Some("/api/targets") => ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
            let mut targets = snapshot.target_summaries.clone();
            if let Some(storage) = storage {
                targets.extend(load_observer_target_summaries(storage, filter));
                dedup_target_summaries(&mut targets);
                sort_target_summaries(&mut targets);
            }
            render_targets_payload_from_summaries(&targets, snapshot.filter_context.as_ref())
        }),
        Some("/api/activity") => ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
            let mut activity_items = snapshot.activity_items.clone();
            if let Some(storage) = storage {
                activity_items.extend(load_observer_activity_items(storage, filter));
                dedup_activity_items(&mut activity_items);
                sort_activity_items(&mut activity_items);
            }
            render_activity_payload_from_items(&activity_items, snapshot.filter_context.as_ref())
        }),
        Some("/api/requests") => ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
            let mut requests = snapshot.request_summaries.clone();
            if let Some(storage) = storage {
                requests.extend(load_read_model_request_summaries(storage, filter));
                dedup_request_summaries(&mut requests);
                sort_request_summaries(&mut requests);
            }
            render_requests_payload(&requests, snapshot.filter_context.as_ref())
        }),
        Some("/api/sessions") => ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
            let mut sessions = snapshot.session_summaries.clone();
            if let Some(storage) = storage {
                sessions.extend(load_read_model_session_summaries(storage, filter));
                dedup_session_summaries(&mut sessions);
                sort_session_summaries(&mut sessions);
            }
            render_sessions_payload_with_pagination(
                &sessions,
                snapshot.filter_context.as_ref(),
                request_target.and_then(session_pagination_from_request_target),
            )
        }),
        Some("/api/health") => ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
            let mut targets = snapshot.target_summaries.clone();
            let mut activity_items = snapshot.activity_items.clone();
            if let Some(storage) = storage {
                targets.extend(load_observer_target_summaries(storage, filter));
                activity_items.extend(load_observer_activity_items(storage, filter));
                dedup_target_summaries(&mut targets);
                dedup_activity_items(&mut activity_items);
                sort_target_summaries(&mut targets);
                sort_activity_items(&mut activity_items);
            }
            render_health_payload_with_state_root(
                &targets,
                &activity_items,
                None,
                snapshot.filter_context.as_ref(),
                state_root_from_summary(&snapshot.summary).as_deref(),
            )
        }),
        Some(path) if path.starts_with("/api/events/") => {
            ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
                let event_id = path.trim_start_matches("/api/events/");
                match storage {
                    Some(storage) => load_read_model_event_detail_payload(
                        storage,
                        event_id,
                        filter,
                        snapshot.filter_context.as_ref(),
                    )
                    .unwrap_or_else(|| {
                        render_read_model_event_not_found_payload(
                            event_id,
                            snapshot.filter_context.as_ref(),
                        )
                    }),
                    None => render_read_model_event_not_found_payload(
                        event_id,
                        snapshot.filter_context.as_ref(),
                    ),
                }
            })
        }
        Some(path) if path.starts_with("/api/requests/") => {
            ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
                let request_id = path.trim_start_matches("/api/requests/");
                let detail = match storage {
                    Some(storage) => load_request_detail(storage, request_id).ok().flatten(),
                    None => load_request_detail_from_snapshot(snapshot, request_id),
                }
                .filter(|detail| request_detail_matches_filter(detail, filter));
                match (storage, detail) {
                    (_, Some(detail)) => render_request_detail_payload(
                        request_id,
                        Some(detail),
                        snapshot.filter_context.as_ref(),
                    ),
                    (Some(storage), None) => load_read_model_request_detail_payload(
                        storage,
                        request_id,
                        filter,
                        snapshot.filter_context.as_ref(),
                    )
                    .unwrap_or_else(|| {
                        render_request_detail_payload(
                            request_id,
                            None,
                            snapshot.filter_context.as_ref(),
                        )
                    }),
                    (None, None) => render_request_detail_payload(
                        request_id,
                        None,
                        snapshot.filter_context.as_ref(),
                    ),
                }
            })
        }
        Some(path) if session_capabilities_route_session_id(path).is_some() => {
            ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
                let session_id =
                    session_capabilities_route_session_id(path).expect("route guard should match");
                match storage {
                    Some(storage) => load_session_capabilities_payload(
                        storage,
                        session_id,
                        filter,
                        snapshot.filter_context.as_ref(),
                    )
                    .unwrap_or_else(|| {
                        let api_filter_context =
                            api_filter_context(snapshot.filter_context.as_ref());
                        render_empty_capability_projection_payload(
                            session_id,
                            api_filter_context.as_ref(),
                        )
                    }),
                    None => {
                        let api_filter_context =
                            api_filter_context(snapshot.filter_context.as_ref());
                        render_empty_capability_projection_payload(
                            session_id,
                            api_filter_context.as_ref(),
                        )
                    }
                }
            })
        }
        Some(path) if session_diagnostics_route_session_id(path).is_some() => {
            ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
                let session_id =
                    session_diagnostics_route_session_id(path).expect("route guard should match");
                match storage {
                    Some(storage) => load_session_diagnostics_payload(
                        storage,
                        session_id,
                        filter,
                        snapshot.filter_context.as_ref(),
                    )
                    .unwrap_or_else(|| {
                        let api_filter_context =
                            api_filter_context(snapshot.filter_context.as_ref());
                        render_empty_session_diagnostics_payload(
                            session_id,
                            api_filter_context.as_ref(),
                        )
                    }),
                    None => {
                        let api_filter_context =
                            api_filter_context(snapshot.filter_context.as_ref());
                        render_empty_session_diagnostics_payload(
                            session_id,
                            api_filter_context.as_ref(),
                        )
                    }
                }
            })
        }
        Some(path) if session_events_route_session_id(path).is_some() => {
            ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
                let session_id =
                    session_events_route_session_id(path).expect("route guard should match");
                let detail = match storage {
                    Some(storage) => load_session_detail(storage, session_id).ok().flatten(),
                    None => load_session_detail_from_snapshot(snapshot, session_id),
                }
                .filter(|detail| session_detail_matches_filter(detail, filter));
                match detail {
                    Some(detail) => render_session_events_payload(
                        session_id,
                        &detail.timeline_items,
                        snapshot.filter_context.as_ref(),
                        request_target.and_then(session_pagination_from_request_target),
                    ),
                    None if storage.is_some() => load_read_model_session_events_payload(
                        storage.expect("storage checked above"),
                        session_id,
                        filter,
                        snapshot.filter_context.as_ref(),
                        request_target.and_then(session_pagination_from_request_target),
                    )
                    .unwrap_or_else(|| {
                        render_session_events_payload(
                            session_id,
                            &[],
                            snapshot.filter_context.as_ref(),
                            request_target.and_then(session_pagination_from_request_target),
                        )
                    }),
                    None => render_session_events_payload(
                        session_id,
                        &[],
                        snapshot.filter_context.as_ref(),
                        request_target.and_then(session_pagination_from_request_target),
                    ),
                }
            })
        }
        Some(path) if path.starts_with("/api/sessions/") => {
            ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
                let session_id = path.trim_start_matches("/api/sessions/");
                let detail = match storage {
                    Some(storage) => load_session_detail(storage, session_id).ok().flatten(),
                    None => load_session_detail_from_snapshot(snapshot, session_id),
                }
                .filter(|detail| session_detail_matches_filter(detail, filter));
                match (storage, detail) {
                    (_, Some(detail)) => render_session_detail_payload(
                        session_id,
                        Some(detail),
                        snapshot.filter_context.as_ref(),
                    ),
                    (Some(storage), None) => load_read_model_session_detail_payload(
                        storage,
                        session_id,
                        filter,
                        snapshot.filter_context.as_ref(),
                    )
                    .unwrap_or_else(|| {
                        render_session_detail_payload(
                            session_id,
                            None,
                            snapshot.filter_context.as_ref(),
                        )
                    }),
                    (None, None) => render_session_detail_payload(
                        session_id,
                        None,
                        snapshot.filter_context.as_ref(),
                    ),
                }
            })
        }
        Some(path) if path.starts_with("/api/") => (
            "HTTP/1.1 404 Not Found",
            "application/json; charset=utf-8",
            render_json_error_payload("not_found", &format!("unknown API route: {path}")),
        ),
        Some(_) => (
            "HTTP/1.1 404 Not Found",
            "text/html; charset=utf-8",
            render_not_found_page(),
        ),
        None => (
            "HTTP/1.1 400 Bad Request",
            "text/plain; charset=utf-8",
            "bad request\n".to_string(),
        ),
    };

    ConsoleRouteResponse {
        status_line,
        content_type,
        body: body.into_bytes(),
    }
}

fn write_console_route_response(
    stream: &mut TcpStream,
    response: &ConsoleRouteResponse,
) -> io::Result<()> {
    let header = format!(
        "{}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status_line,
        response.content_type,
        response.body.len()
    );

    stream.write_all(header.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

pub(crate) fn write_console_json_error_response(
    stream: &mut TcpStream,
    status_line: &'static str,
    code: &str,
    message: &str,
) -> io::Result<()> {
    let response = ConsoleRouteResponse {
        status_line,
        content_type: "application/json; charset=utf-8",
        body: render_json_error_payload(code, message).into_bytes(),
    };
    write_console_route_response(stream, &response)
}

fn render_json_error_payload(code: &str, message: &str) -> String {
    json!({
        "error": {
            "code": code,
            "message": message,
        }
    })
    .to_string()
}

fn route_path(request_target: &str) -> &str {
    request_target
        .split_once('?')
        .map(|(path, _)| path)
        .unwrap_or(request_target)
}

fn session_pagination_from_request_target(request_target: &str) -> Option<(usize, usize)> {
    let query = request_target.split_once('?')?.1;
    let limit = query_param(query, "limit")?
        .parse::<usize>()
        .ok()
        .filter(|limit| *limit > 0)?;
    let cursor = query_param(query, "cursor")
        .and_then(|cursor| cursor.parse::<usize>().ok())
        .unwrap_or_default();
    Some((cursor, limit))
}

fn session_events_route_session_id(path: &str) -> Option<&str> {
    path.strip_prefix("/api/sessions/")?.strip_suffix("/events")
}

fn session_capabilities_route_session_id(path: &str) -> Option<&str> {
    path.strip_prefix("/api/sessions/")?
        .strip_suffix("/capabilities")
}

fn session_diagnostics_route_session_id(path: &str) -> Option<&str> {
    path.strip_prefix("/api/sessions/")?
        .strip_suffix("/diagnostics")
}

fn api_filter_context(filter_context: Option<&ConsoleFilterContext>) -> Option<ApiFilterContext> {
    filter_context.map(|filter_context| ApiFilterContext {
        active_filters: filter_context.active_filters.clone(),
        is_filtered_view: filter_context.is_filtered_view,
    })
}

fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

fn write_live_console_response(
    stream: &mut TcpStream,
    result: &BootstrapResult,
    bind_addr: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<()> {
    let request_path = read_request_path(stream)?;
    let request_route = request_path.as_deref().map(route_path);
    if let Some((content_type, body)) = request_path
        .as_deref()
        .map(route_path)
        .and_then(render_console_static_asset)
    {
        return write_console_bytes_response(stream, content_type, &body);
    }

    if request_route == Some("/") || request_route == Some("/favicon.ico") {
        let snapshot = lightweight_console_snapshot(bind_addr, filter);
        return write_console_response_for_path(
            request_path,
            stream,
            &snapshot,
            Some(&result.storage),
            filter,
        );
    }

    let include_sessions = request_route
        .map(|path| path == "/" || path == "/api/sessions" || path.starts_with("/api/sessions/"))
        .unwrap_or(false);
    let snapshot = if include_sessions {
        session_console_snapshot(result, bind_addr, filter)
    } else {
        collect_console_snapshot_for_bind_addr(result, bind_addr, filter, false)
    };
    write_console_response_for_path(
        request_path,
        stream,
        &snapshot,
        Some(&result.storage),
        filter,
    )
}

fn write_console_bytes_response(
    stream: &mut TcpStream,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    stream.write_all(response.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn lightweight_console_snapshot(
    bind_addr: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> ConsoleSnapshot {
    ConsoleSnapshot {
        summary: "PrismTrace console".into(),
        bind_addr: format!("http://{bind_addr}"),
        filter_context: model::console_filter_context(filter),
        target_summaries: Vec::new(),
        activity_items: Vec::new(),
        request_summaries: Vec::new(),
        session_summaries: Vec::new(),
        request_details: Vec::new(),
        session_details: Vec::new(),
    }
}

fn session_console_snapshot(
    result: &BootstrapResult,
    bind_addr: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> ConsoleSnapshot {
    let mut session_summaries = model::filter_session_summaries(
        &load_session_summaries(&result.storage).unwrap_or_else(|_| Vec::new()),
        filter,
    );
    session_summaries.extend(load_read_model_session_summaries(&result.storage, filter));
    dedup_session_summaries(&mut session_summaries);
    sort_session_summaries(&mut session_summaries);

    ConsoleSnapshot {
        session_summaries,
        ..lightweight_console_snapshot(bind_addr, filter)
    }
}

fn read_request_path(stream: &mut TcpStream) -> io::Result<Option<String>> {
    read_request_path_from_reader(stream)
}

fn render_not_found_page() -> String {
    "<!doctype html><html lang=\"en\"><body><h1>404</h1><p>Not Found</p></body></html>".to_string()
}

fn request_path_only(url: &str) -> &str {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let path_start = without_scheme.find('/').unwrap_or(without_scheme.len());
    let path_and_more = &without_scheme[path_start..];
    path_and_more
        .split(['?', '#'])
        .next()
        .filter(|path| !path.is_empty())
        .unwrap_or("/")
}

fn state_root_from_summary(summary: &str) -> Option<String> {
    summary.lines().find_map(|line| {
        line.strip_prefix("state root: ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

pub fn collect_target_summaries(
    source: &impl ProcessSampleSource,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<Vec<ConsoleTargetSummary>> {
    let (_, _, summaries) = collect_target_partition_and_summaries(source, filter)?;

    Ok(summaries)
}

fn collect_target_partition_and_summaries(
    source: &impl ProcessSampleSource,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<(
    Vec<ProcessTarget>,
    Vec<ProcessTarget>,
    Vec<ConsoleTargetSummary>,
)> {
    let discovered_targets = discover_targets(source)?;
    let (matched_targets, unmatched_targets) = partition_targets(discovered_targets, filter);
    let summaries = matched_targets.iter().map(summarize_target).collect();

    Ok((matched_targets, unmatched_targets, summaries))
}

fn partition_targets(
    discovered_targets: Vec<ProcessTarget>,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> (Vec<ProcessTarget>, Vec<ProcessTarget>) {
    let Some(filter) = filter else {
        return (discovered_targets, Vec::new());
    };

    if !filter.is_enabled() {
        return (discovered_targets, Vec::new());
    }

    discovered_targets
        .into_iter()
        .partition(|target| filter.matches_target(target))
}

fn summarize_target(target: &ProcessTarget) -> ConsoleTargetSummary {
    ConsoleTargetSummary {
        pid: target.pid,
        display_name: target.display_name().to_string(),
        runtime_kind: target.runtime_kind.label().to_string(),
        source_state: "discoverable".to_string(),
        source_summary: format!(
            "local process target · {} runtime",
            target.runtime_kind.label()
        ),
    }
}

pub fn collect_activity_items(source: ConsoleActivitySource<'_>) -> Vec<ConsoleActivityItem> {
    let mut items = Vec::new();

    items.extend(
        source
            .recent_requests
            .iter()
            .map(|request| ConsoleActivityItem {
                activity_id: format!("request-{}", request.request_id),
                activity_type: "request".into(),
                occurred_at_ms: request.captured_at_ms,
                title: request.title.clone(),
                subtitle: request.subtitle.clone(),
                related_pid: request.related_pid,
                related_request_id: Some(request.request_id.clone()),
            }),
    );

    items.extend(source.known_errors.iter().map(|error| ConsoleActivityItem {
        activity_id: error.activity_id.clone(),
        activity_type: "error".into(),
        occurred_at_ms: error.occurred_at_ms,
        title: error.title.clone(),
        subtitle: error.subtitle.clone(),
        related_pid: error.related_pid,
        related_request_id: None,
    }));

    items.sort_by(|left, right| {
        right
            .occurred_at_ms
            .cmp(&left.occurred_at_ms)
            .then_with(|| left.activity_id.cmp(&right.activity_id))
    });

    items
}

fn collect_activity_items_filtered(
    source: ConsoleActivitySource<'_>,
    filter: Option<&ConsoleTargetFilterConfig>,
    unmatched_targets: &[ProcessTarget],
) -> Vec<ConsoleActivityItem> {
    let items = collect_activity_items(source);
    let Some(filter) = filter else {
        return items;
    };

    if !filter.is_enabled() {
        return items;
    }

    let unmatched_pids = unmatched_targets
        .iter()
        .map(|target| target.pid)
        .collect::<Vec<_>>();

    items
        .into_iter()
        .filter(|item| match item.related_pid {
            Some(pid) => !unmatched_pids.contains(&pid),
            None => true,
        })
        .collect()
}

pub fn load_request_summaries(storage: &StorageLayout) -> io::Result<Vec<ConsoleRequestSummary>> {
    let mut summaries = load_request_records(storage)?
        .into_iter()
        .map(|record| {
            let summary_text = format!(
                "{} {} {}",
                record.provider,
                record.method,
                request_path_only(&record.url)
            );

            ConsoleRequestSummary {
                request_id: record.request_id,
                captured_at_ms: record.captured_at_ms,
                provider: record.provider,
                model: record.model,
                target_display_name: record.target_display_name,
                summary_text,
            }
        })
        .collect::<Vec<_>>();

    summaries.sort_by(|left, right| {
        right
            .captured_at_ms
            .cmp(&left.captured_at_ms)
            .then_with(|| left.request_id.cmp(&right.request_id))
    });

    Ok(summaries)
}

fn load_recent_request_activity(storage: &StorageLayout) -> Vec<ConsoleRecentRequestActivity> {
    load_request_records(storage)
        .unwrap_or_default()
        .into_iter()
        .map(|record| ConsoleRecentRequestActivity {
            request_id: record.request_id,
            captured_at_ms: record.captured_at_ms,
            title: format!("Captured {} request", record.provider),
            subtitle: format!(
                "{} {} {}",
                record.provider,
                record.method,
                request_path_only(&record.url)
            ),
            related_pid: record.pid,
        })
        .collect()
}

pub fn load_request_detail(
    storage: &StorageLayout,
    request_id: &str,
) -> io::Result<Option<ConsoleRequestDetail>> {
    let requests_dir = storage.artifacts_dir.join("requests");
    if !requests_dir.exists() {
        return Ok(None);
    }

    for entry in fs::read_dir(&requests_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let Some(record) = read_request_record(&path)? else {
            continue;
        };

        if record.request_id != request_id {
            continue;
        }

        let request_summary = format!(
            "{} {} {}",
            record.provider,
            record.method,
            request_path_only(&record.url)
        );
        let response = record
            .exchange_id
            .as_deref()
            .map(|exchange_id| load_matching_response_detail(storage, exchange_id))
            .transpose()?
            .flatten();
        let tool_visibility = load_matching_tool_visibility_detail(
            storage,
            &record.request_id,
            record.exchange_id.as_deref(),
        )?;

        return Ok(Some(ConsoleRequestDetail {
            request_id: record.request_id,
            exchange_id: record.exchange_id,
            captured_at_ms: record.captured_at_ms,
            provider: record.provider,
            model: record.model,
            target_display_name: record.target_display_name,
            artifact_path: record.artifact_path.display().to_string(),
            request_summary,
            hook_name: record.hook_name,
            method: record.method,
            url: record.url,
            headers: record.headers,
            body_text: record.body_text,
            body_size_bytes: record.body_size_bytes,
            truncated: record.truncated,
            probe_context: None,
            tool_visibility,
            response,
        }));
    }

    Ok(None)
}

pub fn load_session_summaries(storage: &StorageLayout) -> io::Result<Vec<ConsoleSessionSummary>> {
    let sessions = load_session_details(storage)?;

    Ok(sessions
        .into_iter()
        .map(|detail| {
            let title = derive_session_title(&detail);
            let subtitle = derive_session_subtitle(&detail);
            let response_count = detail
                .timeline_items
                .iter()
                .filter(|item| item.has_response)
                .count();

            ConsoleSessionSummary {
                session_id: detail.session_id,
                title,
                subtitle,
                cwd: None,
                artifact_path: None,
                pid: detail.pid,
                target_display_name: detail.target_display_name,
                started_at_ms: detail.started_at_ms,
                completed_at_ms: detail.completed_at_ms,
                exchange_count: detail.exchange_count,
                request_count: detail.exchange_count,
                response_count,
            }
        })
        .collect())
}

fn derive_session_title(detail: &ConsoleSessionDetail) -> String {
    detail
        .timeline_items
        .iter()
        .find_map(|item| {
            let summary = item.request_summary.trim();
            (!summary.is_empty()).then(|| summary.to_string())
        })
        .unwrap_or_else(|| detail.target_display_name.clone())
}

fn derive_session_subtitle(detail: &ConsoleSessionDetail) -> String {
    detail
        .timeline_items
        .first()
        .map(|item| match item.model.as_deref() {
            Some(model) if !model.trim().is_empty() => format!("{} · {}", item.provider, model),
            _ => item.provider.clone(),
        })
        .unwrap_or_else(|| format!("session {}", detail.session_id))
}

pub fn load_session_detail(
    storage: &StorageLayout,
    session_id: &str,
) -> io::Result<Option<ConsoleSessionDetail>> {
    Ok(load_session_details(storage)?
        .into_iter()
        .find(|detail| detail.session_id == session_id))
}

fn load_session_details(storage: &StorageLayout) -> io::Result<Vec<ConsoleSessionDetail>> {
    let exchanges = load_exchange_records(storage)?;
    Ok(build_session_details(exchanges))
}

fn load_exchange_records(storage: &StorageLayout) -> io::Result<Vec<ExchangeRecord>> {
    let response_index = build_response_detail_index(storage)?;
    let tool_visibility_index = build_tool_visibility_detail_index(storage)?;
    let mut exchanges = load_request_records(storage)?
        .into_iter()
        .filter_map(|record| {
            let pid = record.pid?;
            let request_summary = format!(
                "{} {} {}",
                record.provider,
                record.method,
                request_path_only(&record.url)
            );
            let response = record
                .exchange_id
                .as_deref()
                .and_then(|exchange_id| response_index.get(exchange_id))
                .cloned();
            let tool_visibility = select_tool_visibility_detail(
                &tool_visibility_index,
                &record.request_id,
                record.exchange_id.as_deref(),
            );

            let completed_at_ms = response
                .as_ref()
                .map(|detail| detail.completed_at_ms)
                .unwrap_or(record.captured_at_ms);
            let duration_ms = response
                .as_ref()
                .map(|detail| detail.duration_ms)
                .unwrap_or_default();
            let response_status = response.as_ref().map(|detail| detail.status_code);
            let tool_count_final = tool_visibility
                .as_ref()
                .map(|detail| detail.tool_count_final)
                .unwrap_or_default();

            Some(ExchangeRecord {
                request_id: record.request_id,
                exchange_id: record.exchange_id,
                pid,
                target_display_name: record.target_display_name,
                provider: record.provider,
                model: record.model,
                started_at_ms: record.captured_at_ms,
                completed_at_ms,
                duration_ms,
                request_summary,
                response_status,
                tool_count_final,
                has_response: response.is_some(),
                has_tool_visibility: tool_visibility.is_some(),
            })
        })
        .collect::<Vec<_>>();

    exchanges.sort_by(|left, right| {
        left.started_at_ms
            .cmp(&right.started_at_ms)
            .then_with(|| left.request_id.cmp(&right.request_id))
    });

    Ok(exchanges)
}

fn build_session_details(exchanges: Vec<ExchangeRecord>) -> Vec<ConsoleSessionDetail> {
    let mut sessions = Vec::new();
    let mut exchanges_by_pid = BTreeMap::<u32, Vec<ExchangeRecord>>::new();

    for exchange in exchanges {
        exchanges_by_pid
            .entry(exchange.pid)
            .or_default()
            .push(exchange);
    }

    for (pid, mut pid_exchanges) in exchanges_by_pid {
        pid_exchanges.sort_by(|left, right| {
            left.started_at_ms
                .cmp(&right.started_at_ms)
                .then_with(|| left.request_id.cmp(&right.request_id))
        });

        let mut current: Option<ConsoleSessionDetail> = None;
        let mut ordinal = 0_usize;

        for exchange in pid_exchanges {
            let should_start_new = current.as_ref().is_none_or(|session| {
                exchange
                    .started_at_ms
                    .saturating_sub(session.last_exchange_started_at_ms)
                    > SESSION_WINDOW_MS
            });

            if should_start_new {
                if let Some(session) = current.take() {
                    sessions.push(session);
                }

                ordinal += 1;
                current = Some(ConsoleSessionDetail {
                    session_id: format!("{pid}-{}-{ordinal}", exchange.started_at_ms),
                    pid,
                    target_display_name: exchange.target_display_name.clone(),
                    started_at_ms: exchange.started_at_ms,
                    completed_at_ms: exchange.completed_at_ms,
                    last_exchange_started_at_ms: exchange.started_at_ms,
                    exchange_count: 0,
                    timeline_items: Vec::new(),
                });
            }

            if let Some(session) = current.as_mut() {
                session.completed_at_ms = session.completed_at_ms.max(exchange.completed_at_ms);
                session.last_exchange_started_at_ms = exchange.started_at_ms;
                session.exchange_count += 1;
                session.timeline_items.push(ConsoleSessionTimelineItem {
                    request_id: exchange.request_id,
                    exchange_id: exchange.exchange_id,
                    pid,
                    target_display_name: exchange.target_display_name,
                    provider: exchange.provider,
                    model: exchange.model,
                    started_at_ms: exchange.started_at_ms,
                    completed_at_ms: exchange.completed_at_ms,
                    duration_ms: exchange.duration_ms,
                    request_summary: exchange.request_summary,
                    response_status: exchange.response_status,
                    tool_count_final: exchange.tool_count_final,
                    has_response: exchange.has_response,
                    has_tool_visibility: exchange.has_tool_visibility,
                });
            }
        }

        if let Some(session) = current {
            sessions.push(session);
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

fn load_request_records(storage: &StorageLayout) -> io::Result<Vec<RequestArtifactRecord>> {
    let requests_dir = storage.artifacts_dir.join("requests");
    if !requests_dir.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in fs::read_dir(&requests_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        match read_request_record(&path) {
            Ok(Some(record)) => records.push(record),
            Ok(None) => {}
            Err(_) => continue,
        }
    }

    Ok(records)
}

fn read_request_record(path: &Path) -> io::Result<Option<RequestArtifactRecord>> {
    let raw = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw).map_err(io::Error::other)?;

    let Some(request_id) = value.get("event_id").and_then(Value::as_str) else {
        return Ok(None);
    };

    let captured_at_ms = value
        .get("captured_at_ms")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let exchange_id = value
        .get("exchange_id")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let pid = value
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok());
    let provider = value
        .get("provider_hint")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let target_display_name = value
        .get("target_display_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let hook_name = value
        .get("hook_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_string();
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("/")
        .to_string();
    let headers = parse_header_details(value.get("headers"));
    let body_text = value
        .get("body_text")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let body_size_bytes = value
        .get("body_size_bytes")
        .and_then(Value::as_u64)
        .and_then(|size| usize::try_from(size).ok())
        .unwrap_or_default();
    let truncated = value
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let model = value
        .get("body_text")
        .and_then(Value::as_str)
        .and_then(extract_model_from_body_text);

    Ok(Some(RequestArtifactRecord {
        request_id: request_id.to_string(),
        exchange_id,
        pid,
        captured_at_ms,
        provider,
        model,
        target_display_name,
        hook_name,
        method,
        url,
        headers,
        body_text,
        body_size_bytes,
        truncated,
        artifact_path: path.to_path_buf(),
    }))
}

fn parse_header_details(value: Option<&Value>) -> Vec<ConsoleHeaderDetail> {
    value
        .and_then(Value::as_array)
        .map(|headers| {
            headers
                .iter()
                .filter_map(|header| {
                    Some(ConsoleHeaderDetail {
                        name: header.get("name")?.as_str()?.to_string(),
                        value: header.get("value")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn load_matching_response_detail(
    storage: &StorageLayout,
    exchange_id: &str,
) -> io::Result<Option<ConsoleResponseDetail>> {
    Ok(build_response_detail_index(storage)?
        .get(exchange_id)
        .cloned())
}

fn load_matching_tool_visibility_detail(
    storage: &StorageLayout,
    request_id: &str,
    exchange_id: Option<&str>,
) -> io::Result<Option<ConsoleToolVisibilityDetail>> {
    Ok(select_tool_visibility_detail(
        &build_tool_visibility_detail_index(storage)?,
        request_id,
        exchange_id,
    ))
}

fn read_response_record(path: &Path) -> io::Result<Option<ResponseArtifactRecord>> {
    let raw = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw).map_err(io::Error::other)?;

    let Some(exchange_id) = value.get("exchange_id").and_then(Value::as_str) else {
        return Ok(None);
    };

    let status_code = value
        .get("status_code")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .unwrap_or_default();
    let headers = parse_header_details(value.get("headers"));
    let body_text = value
        .get("body_text")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let body_size_bytes = value
        .get("body_size_bytes")
        .and_then(Value::as_u64)
        .and_then(|size| usize::try_from(size).ok())
        .unwrap_or_default();
    let truncated = value
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let started_at_ms = value
        .get("started_at_ms")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let completed_at_ms = value
        .get("completed_at_ms")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let duration_ms = value
        .get("duration_ms")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| completed_at_ms.saturating_sub(started_at_ms));

    Ok(Some(ResponseArtifactRecord {
        exchange_id: exchange_id.to_string(),
        status_code,
        headers,
        body_text,
        body_size_bytes,
        truncated,
        started_at_ms,
        completed_at_ms,
        duration_ms,
        artifact_path: path.to_path_buf(),
    }))
}

fn build_response_detail_index(
    storage: &StorageLayout,
) -> io::Result<HashMap<String, ConsoleResponseDetail>> {
    let responses_dir = storage.artifacts_dir.join("responses");
    if !responses_dir.exists() {
        return Ok(HashMap::new());
    }

    let mut index = HashMap::new();
    let mut completed_at_index = HashMap::<String, u64>::new();
    for entry in fs::read_dir(&responses_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let Some(record) = read_response_record(&path)? else {
            continue;
        };

        let should_replace = completed_at_index
            .get(&record.exchange_id)
            .map(|current| record.completed_at_ms >= *current)
            .unwrap_or(true);
        if should_replace {
            completed_at_index.insert(record.exchange_id.clone(), record.completed_at_ms);
            index.insert(
                record.exchange_id.clone(),
                ConsoleResponseDetail {
                    artifact_path: record.artifact_path.display().to_string(),
                    status_code: record.status_code,
                    headers: record.headers,
                    body_text: record.body_text,
                    body_size_bytes: record.body_size_bytes,
                    truncated: record.truncated,
                    started_at_ms: record.started_at_ms,
                    completed_at_ms: record.completed_at_ms,
                    duration_ms: record.duration_ms,
                },
            );
        }
    }

    Ok(index)
}

fn read_tool_visibility_record(path: &Path) -> io::Result<Option<ToolVisibilityArtifactRecord>> {
    let raw = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw).map_err(io::Error::other)?;

    let Some(request_id) = value.get("request_id").and_then(Value::as_str) else {
        return Ok(None);
    };

    let exchange_id = value
        .get("exchange_id")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let captured_at_ms = value
        .get("captured_at_ms")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let visibility_stage = value
        .get("visibility_stage")
        .and_then(Value::as_str)
        .unwrap_or("request-embedded")
        .to_string();
    let tool_choice = value
        .get("tool_choice")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let tool_count_final = value
        .get("tool_count_final")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or_default();
    let final_tools_value = value
        .get("final_tools_json")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let final_tools = parse_tool_summaries(&final_tools_value);
    let final_tools_json =
        serde_json::to_string_pretty(&final_tools_value).map_err(io::Error::other)?;

    Ok(Some(ToolVisibilityArtifactRecord {
        request_id: request_id.to_string(),
        exchange_id,
        captured_at_ms,
        visibility_stage,
        tool_choice,
        tool_count_final,
        final_tools,
        final_tools_json,
        artifact_path: path.to_path_buf(),
    }))
}

#[derive(Default)]
struct ToolVisibilityDetailIndex {
    by_request_id: HashMap<String, (u64, ConsoleToolVisibilityDetail)>,
    by_exchange_id: HashMap<String, (u64, ConsoleToolVisibilityDetail)>,
}

fn build_tool_visibility_detail_index(
    storage: &StorageLayout,
) -> io::Result<ToolVisibilityDetailIndex> {
    let visibility_dir = storage.artifacts_dir.join("tool_visibility");
    if !visibility_dir.exists() {
        return Ok(ToolVisibilityDetailIndex::default());
    }

    let mut index = ToolVisibilityDetailIndex::default();
    for entry in fs::read_dir(&visibility_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let Some(record) = read_tool_visibility_record(&path)? else {
            continue;
        };

        let detail = ConsoleToolVisibilityDetail {
            artifact_path: record.artifact_path.display().to_string(),
            visibility_stage: record.visibility_stage.clone(),
            tool_choice: record.tool_choice.clone(),
            tool_count_final: record.tool_count_final,
            final_tools: record.final_tools.clone(),
            final_tools_json: record.final_tools_json.clone(),
        };

        let request_should_replace = index
            .by_request_id
            .get(&record.request_id)
            .map(|(captured_at_ms, _)| record.captured_at_ms >= *captured_at_ms)
            .unwrap_or(true);
        if request_should_replace {
            index.by_request_id.insert(
                record.request_id.clone(),
                (record.captured_at_ms, detail.clone()),
            );
        }

        if let Some(exchange_id) = &record.exchange_id {
            let exchange_should_replace = index
                .by_exchange_id
                .get(exchange_id)
                .map(|(captured_at_ms, _)| record.captured_at_ms >= *captured_at_ms)
                .unwrap_or(true);
            if exchange_should_replace {
                index
                    .by_exchange_id
                    .insert(exchange_id.clone(), (record.captured_at_ms, detail));
            }
        }
    }

    Ok(index)
}

fn select_tool_visibility_detail(
    index: &ToolVisibilityDetailIndex,
    request_id: &str,
    exchange_id: Option<&str>,
) -> Option<ConsoleToolVisibilityDetail> {
    index
        .by_request_id
        .get(request_id)
        .map(|(_, detail)| detail.clone())
        .or_else(|| {
            exchange_id
                .and_then(|exchange_id| index.by_exchange_id.get(exchange_id))
                .map(|(_, detail)| detail.clone())
        })
}

fn parse_tool_summaries(value: &Value) -> Vec<ConsoleToolSummary> {
    value
        .as_array()
        .map(|tools| tools.iter().map(parse_tool_summary).collect())
        .unwrap_or_default()
}

fn parse_tool_summary(tool: &Value) -> ConsoleToolSummary {
    let fallback_name = serde_json::to_string(tool).unwrap_or_else(|_| "unknown".to_string());

    if let Some(function) = tool.get("function") {
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed tool");
        let tool_type = tool
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("function");
        return ConsoleToolSummary {
            name: name.to_string(),
            tool_type: tool_type.to_string(),
        };
    }

    if let Some(name) = tool.get("name").and_then(Value::as_str) {
        let tool_type = tool.get("type").and_then(Value::as_str).unwrap_or("tool");
        return ConsoleToolSummary {
            name: name.to_string(),
            tool_type: tool_type.to_string(),
        };
    }

    if let Some(name) = tool.as_str() {
        return ConsoleToolSummary {
            name: name.to_string(),
            tool_type: "unknown".to_string(),
        };
    }

    ConsoleToolSummary {
        name: fallback_name,
        tool_type: tool
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
    }
}

fn request_detail_matches_filter(
    detail: &ConsoleRequestDetail,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };

    let target = ProcessTarget {
        pid: 0,
        app_name: detail.target_display_name.clone(),
        executable_path: PathBuf::from(&detail.target_display_name),
        command_line: None,
        runtime_kind: prismtrace_core::RuntimeKind::Unknown,
    };

    filter.matches_target(&target)
}

fn session_detail_matches_filter(
    detail: &ConsoleSessionDetail,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };

    let target = ProcessTarget {
        pid: detail.pid,
        app_name: detail.target_display_name.clone(),
        executable_path: PathBuf::from(&detail.target_display_name),
        command_line: None,
        runtime_kind: prismtrace_core::RuntimeKind::Unknown,
    };

    filter.matches_target(&target)
}

fn extract_model_from_body_text(body_text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body_text).ok()?;
    value
        .get("model")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn load_request_detail_from_snapshot(
    snapshot: &ConsoleSnapshot,
    request_id: &str,
) -> Option<ConsoleRequestDetail> {
    if let Some(detail) = snapshot
        .request_details
        .iter()
        .find(|detail| detail.request_id == request_id)
    {
        return Some(detail.clone());
    }

    None
}

fn load_session_detail_from_snapshot(
    snapshot: &ConsoleSnapshot,
    session_id: &str,
) -> Option<ConsoleSessionDetail> {
    snapshot
        .session_details
        .iter()
        .find(|detail| detail.session_id == session_id)
        .cloned()
}

fn read_request_path_from_reader(reader: &mut impl Read) -> io::Result<Option<String>> {
    let mut buffer = [0_u8; 2048];
    let bytes_read = reader.read(&mut buffer)?;
    if bytes_read == 0 {
        return Ok(None);
    }

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let Some(line) = request.lines().next() else {
        return Ok(None);
    };

    let mut parts = line.split_whitespace();
    let method = parts.next();
    let path = parts.next();

    match (method, path) {
        (Some("GET"), Some(path)) => Ok(Some(path.to_string())),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests;
