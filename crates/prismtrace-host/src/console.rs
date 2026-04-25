use crate::BootstrapResult;
use crate::discovery::{ProcessSampleSource, PsProcessSampleSource, discover_targets};
use crate::probe_health::ProbeHealthStore;
use prismtrace_core::{AttachSession, ProbeHealth, ProcessTarget};
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

const SESSION_WINDOW_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleSnapshot {
    pub summary: String,
    pub bind_addr: String,
    pub filter_context: Option<ConsoleFilterContext>,
    pub target_summaries: Vec<ConsoleTargetSummary>,
    pub activity_items: Vec<ConsoleActivityItem>,
    pub request_summaries: Vec<ConsoleRequestSummary>,
    pub session_summaries: Vec<ConsoleSessionSummary>,
    pub request_details: Vec<ConsoleRequestDetail>,
    pub session_details: Vec<ConsoleSessionDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleFilterContext {
    pub active_filters: Vec<String>,
    pub is_filtered_view: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleTargetSummary {
    pub pid: u32,
    pub display_name: String,
    pub runtime_kind: String,
    pub attach_state: String,
    pub probe_state_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleTargetFilterConfig {
    terms: Vec<String>,
}

impl ConsoleTargetFilterConfig {
    pub fn new(terms: Vec<String>) -> Self {
        Self {
            terms: terms
                .into_iter()
                .map(|term| term.trim().to_ascii_lowercase())
                .filter(|term| !term.is_empty())
                .collect(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        !self.terms.is_empty()
    }

    pub fn matches_target(&self, target: &ProcessTarget) -> bool {
        if !self.is_enabled() {
            return true;
        }

        let display_name = target.display_name().to_ascii_lowercase();
        let executable_path = target
            .executable_path
            .to_string_lossy()
            .to_ascii_lowercase();
        let command_identity = target
            .command_line
            .as_deref()
            .and_then(command_line_identity)
            .unwrap_or_default();

        self.terms.iter().any(|term| {
            display_name.contains(term)
                || executable_path.contains(term)
                || command_identity.contains(term)
        })
    }
}

fn command_line_identity(command_line: &str) -> Option<String> {
    let mut parts = command_line.split_whitespace();
    let _process = parts.next()?;
    let command = loop {
        let part = parts.next()?;

        if matches!(part, "--target") {
            let _ = parts.next();
            continue;
        }

        if part.starts_with('-') {
            continue;
        }

        break part;
    };

    let identity = std::path::Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .trim_end_matches(".js")
        .trim_end_matches(".mjs")
        .trim_end_matches(".cjs")
        .to_ascii_lowercase();

    (!identity.is_empty()).then_some(identity)
}

fn console_filter_context(
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Option<ConsoleFilterContext> {
    filter
        .filter(|filter| filter.is_enabled())
        .map(|filter| ConsoleFilterContext {
            active_filters: filter.terms.clone(),
            is_filtered_view: true,
        })
}

fn filter_request_summaries(
    requests: &[ConsoleRequestSummary],
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ConsoleRequestSummary> {
    let Some(filter) = filter else {
        return requests.to_vec();
    };

    if !filter.is_enabled() {
        return requests.to_vec();
    }

    requests
        .iter()
        .filter(|request| {
            let target = ProcessTarget {
                pid: 0,
                app_name: request.target_display_name.clone(),
                executable_path: PathBuf::from(&request.target_display_name),
                command_line: None,
                runtime_kind: prismtrace_core::RuntimeKind::Unknown,
            };

            filter.matches_target(&target)
        })
        .cloned()
        .collect()
}

fn filter_session_summaries(
    sessions: &[ConsoleSessionSummary],
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Vec<ConsoleSessionSummary> {
    let Some(filter) = filter else {
        return sessions.to_vec();
    };

    if !filter.is_enabled() {
        return sessions.to_vec();
    }

    sessions
        .iter()
        .filter(|session| {
            let target = ProcessTarget {
                pid: session.pid,
                app_name: session.target_display_name.clone(),
                executable_path: PathBuf::from(&session.target_display_name),
                command_line: None,
                runtime_kind: prismtrace_core::RuntimeKind::Unknown,
            };

            filter.matches_target(&target)
        })
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleActivityItem {
    pub activity_id: String,
    pub activity_type: String,
    pub occurred_at_ms: u64,
    pub title: String,
    pub subtitle: String,
    pub related_pid: Option<u32>,
    pub related_request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleRecentRequestActivity {
    pub request_id: String,
    pub captured_at_ms: u64,
    pub title: String,
    pub subtitle: String,
    pub related_pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleKnownErrorActivity {
    pub activity_id: String,
    pub occurred_at_ms: u64,
    pub title: String,
    pub subtitle: String,
    pub related_pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleRequestSummary {
    pub request_id: String,
    pub captured_at_ms: u64,
    pub provider: String,
    pub model: Option<String>,
    pub target_display_name: String,
    pub summary_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleSessionSummary {
    pub session_id: String,
    pub pid: u32,
    pub target_display_name: String,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub exchange_count: usize,
    pub request_count: usize,
    pub response_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleSessionTimelineItem {
    pub request_id: String,
    pub exchange_id: Option<String>,
    pub pid: u32,
    pub target_display_name: String,
    pub provider: String,
    pub model: Option<String>,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub duration_ms: u64,
    pub request_summary: String,
    pub response_status: Option<u16>,
    pub tool_count_final: usize,
    pub has_response: bool,
    pub has_tool_visibility: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleSessionDetail {
    pub session_id: String,
    pub pid: u32,
    pub target_display_name: String,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub last_exchange_started_at_ms: u64,
    pub exchange_count: usize,
    pub timeline_items: Vec<ConsoleSessionTimelineItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleHeaderDetail {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleResponseDetail {
    pub artifact_path: String,
    pub status_code: u16,
    pub headers: Vec<ConsoleHeaderDetail>,
    pub body_text: Option<String>,
    pub body_size_bytes: usize,
    pub truncated: bool,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleToolSummary {
    pub name: String,
    pub tool_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleToolVisibilityDetail {
    pub artifact_path: String,
    pub visibility_stage: String,
    pub tool_choice: Option<String>,
    pub tool_count_final: usize,
    pub final_tools: Vec<ConsoleToolSummary>,
    pub final_tools_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleRequestDetail {
    pub request_id: String,
    pub exchange_id: Option<String>,
    pub captured_at_ms: u64,
    pub provider: String,
    pub model: Option<String>,
    pub target_display_name: String,
    pub artifact_path: String,
    pub request_summary: String,
    pub hook_name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<ConsoleHeaderDetail>,
    pub body_text: Option<String>,
    pub body_size_bytes: usize,
    pub truncated: bool,
    pub probe_context: Option<String>,
    pub tool_visibility: Option<ConsoleToolVisibilityDetail>,
    pub response: Option<ConsoleResponseDetail>,
}

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
    pub attach_session: Option<&'a AttachSession>,
    pub attach_occurred_at_ms: Option<u64>,
    pub probe_health: Option<&'a ProbeHealth>,
    pub probe_occurred_at_ms: Option<u64>,
    pub recent_requests: &'a [ConsoleRecentRequestActivity],
    pub known_errors: &'a [ConsoleKnownErrorActivity],
}

#[derive(Debug)]
pub struct ConsoleServer {
    listener: TcpListener,
    snapshot: ConsoleSnapshot,
    result: BootstrapResult,
    bind_addr: String,
    filter: Option<ConsoleTargetFilterConfig>,
}

impl ConsoleServer {
    pub fn snapshot(&self) -> &ConsoleSnapshot {
        &self.snapshot
    }

    pub fn local_url(&self) -> io::Result<String> {
        Ok(format!("http://{}", self.listener.local_addr()?))
    }

    pub fn serve_once(&self) -> io::Result<()> {
        let (mut stream, _) = self.listener.accept()?;
        write_live_console_response(
            &mut stream,
            &self.result,
            &self.bind_addr,
            self.filter.as_ref(),
        )
    }

    pub fn serve_forever(&self) -> io::Result<()> {
        loop {
            let (mut stream, _) = self.listener.accept()?;
            write_live_console_response(
                &mut stream,
                &self.result,
                &self.bind_addr,
                self.filter.as_ref(),
            )?;
        }
    }
}

pub fn collect_console_snapshot(
    result: &BootstrapResult,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> ConsoleSnapshot {
    collect_console_snapshot_for_bind_addr(result, &result.config.bind_addr, filter, false)
}

fn collect_console_snapshot_for_bind_addr(
    result: &BootstrapResult,
    bind_addr: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    include_sessions: bool,
) -> ConsoleSnapshot {
    let (_, unmatched_targets, target_summaries) =
        collect_target_partition_and_summaries(&PsProcessSampleSource, filter, None, None)
            .unwrap_or_else(|_| (Vec::new(), Vec::new(), Vec::new()));
    let request_summaries = filter_request_summaries(
        &load_request_summaries(&result.storage).unwrap_or_else(|_| Vec::new()),
        filter,
    );
    let session_summaries = if include_sessions {
        filter_session_summaries(
            &load_session_summaries(&result.storage).unwrap_or_else(|_| Vec::new()),
            filter,
        )
    } else {
        Vec::new()
    };
    let recent_requests = load_recent_request_activity(&result.storage);

    ConsoleSnapshot {
        summary: crate::startup_summary(result),
        bind_addr: format!("http://{bind_addr}"),
        filter_context: console_filter_context(filter),
        target_summaries,
        activity_items: collect_activity_items_filtered(
            ConsoleActivitySource {
                attach_session: None,
                attach_occurred_at_ms: None,
                probe_health: None,
                probe_occurred_at_ms: None,
                recent_requests: &recent_requests,
                known_errors: &[],
            },
            filter,
            &unmatched_targets,
        ),
        request_summaries,
        session_summaries,
        request_details: Vec::new(),
        session_details: Vec::new(),
    }
}

pub fn console_startup_report(snapshot: &ConsoleSnapshot) -> String {
    format!(
        "{}
PrismTrace Local Console
open: {}",
        snapshot.summary, snapshot.bind_addr
    )
}

pub fn start_console_server(result: &BootstrapResult) -> io::Result<ConsoleServer> {
    start_console_server_with_target_filters(result, None)
}

pub fn run_console_server(result: &BootstrapResult, output: &mut impl Write) -> io::Result<()> {
    run_console_server_with_target_filters(result, None, output)
}

pub fn run_console_server_with_target_filters(
    result: &BootstrapResult,
    target_filters: Option<&[String]>,
    output: &mut impl Write,
) -> io::Result<()> {
    let server = start_console_server_with_target_filters(result, target_filters)?;
    writeln!(output, "{}", console_startup_report(server.snapshot()))?;
    server.serve_forever()
}

pub fn start_console_server_with_target_filters(
    result: &BootstrapResult,
    target_filters: Option<&[String]>,
) -> io::Result<ConsoleServer> {
    let filter = target_filters.map(|terms| ConsoleTargetFilterConfig::new(terms.to_vec()));
    start_console_server_on_bind_addr(result, &result.config.bind_addr, filter.as_ref())
}

pub fn start_console_server_on_bind_addr(
    result: &BootstrapResult,
    bind_addr: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<ConsoleServer> {
    let listener = TcpListener::bind(bind_addr)?;
    let local_addr = listener.local_addr()?;
    let (_, unmatched_targets, target_summaries) =
        collect_target_partition_and_summaries(&PsProcessSampleSource, filter, None, None)
            .unwrap_or_else(|_| (Vec::new(), Vec::new(), Vec::new()));

    Ok(ConsoleServer {
        listener,
        snapshot: ConsoleSnapshot {
            summary: crate::startup_summary(result),
            bind_addr: format!("http://{local_addr}"),
            filter_context: console_filter_context(filter),
            target_summaries,
            activity_items: collect_activity_items_filtered(
                ConsoleActivitySource {
                    attach_session: None,
                    attach_occurred_at_ms: None,
                    probe_health: None,
                    probe_occurred_at_ms: None,
                    recent_requests: &[],
                    known_errors: &[],
                },
                filter,
                &unmatched_targets,
            ),
            request_summaries: filter_request_summaries(
                &load_request_summaries(&result.storage).unwrap_or_else(|_| Vec::new()),
                filter,
            ),
            session_summaries: Vec::new(),
            request_details: Vec::new(),
            session_details: Vec::new(),
        },
        result: result.clone(),
        bind_addr: local_addr.to_string(),
        filter: filter.cloned(),
    })
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

fn write_console_response_for_path(
    request_path: Option<String>,
    stream: &mut TcpStream,
    snapshot: &ConsoleSnapshot,
    storage: Option<&StorageLayout>,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<()> {
    let (status_line, content_type, body) = match request_path.as_deref() {
        Some("/") => (
            "HTTP/1.1 200 OK",
            "text/html; charset=utf-8",
            render_console_homepage(snapshot),
        ),
        Some("/favicon.ico") => ("HTTP/1.1 200 OK", "image/x-icon", String::new()),
        Some("/api/targets") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_targets_payload_from_summaries(
                &snapshot.target_summaries,
                snapshot.filter_context.as_ref(),
            ),
        ),
        Some("/api/activity") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_activity_payload_from_items(
                &snapshot.activity_items,
                snapshot.filter_context.as_ref(),
            ),
        ),
        Some("/api/requests") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_requests_payload(
                &snapshot.request_summaries,
                snapshot.filter_context.as_ref(),
            ),
        ),
        Some("/api/sessions") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_sessions_payload(
                &snapshot.session_summaries,
                snapshot.filter_context.as_ref(),
            ),
        ),
        Some("/api/health") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_health_payload(
                &snapshot.target_summaries,
                &snapshot.activity_items,
                None,
                snapshot.filter_context.as_ref(),
            ),
        ),
        Some(path) if path.starts_with("/api/requests/") => {
            ("HTTP/1.1 200 OK", "application/json; charset=utf-8", {
                let request_id = path.trim_start_matches("/api/requests/");
                let detail = match storage {
                    Some(storage) => load_request_detail(storage, request_id).ok().flatten(),
                    None => load_request_detail_from_snapshot(snapshot, request_id),
                }
                .filter(|detail| request_detail_matches_filter(detail, filter));
                render_request_detail_payload(request_id, detail, snapshot.filter_context.as_ref())
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
                render_session_detail_payload(session_id, detail, snapshot.filter_context.as_ref())
            })
        }
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

    let response = format!(
        "{status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn write_live_console_response(
    stream: &mut TcpStream,
    result: &BootstrapResult,
    bind_addr: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<()> {
    let request_path = read_request_path(stream)?;
    let include_sessions = request_path
        .as_deref()
        .map(|path| path == "/" || path == "/api/sessions" || path.starts_with("/api/sessions/"))
        .unwrap_or(false);
    let snapshot =
        collect_console_snapshot_for_bind_addr(result, bind_addr, filter, include_sessions);
    write_console_response_for_path(
        request_path,
        stream,
        &snapshot,
        Some(&result.storage),
        filter,
    )
}

fn read_request_path(stream: &mut TcpStream) -> io::Result<Option<String>> {
    read_request_path_from_reader(stream)
}

fn render_console_homepage(snapshot: &ConsoleSnapshot) -> String {
    let filter_context_html = render_filter_context_banner(snapshot.filter_context.as_ref());
    let targets_html =
        render_targets_panel_items(&snapshot.target_summaries, snapshot.filter_context.as_ref());
    let activity_html =
        render_activity_panel_items(&snapshot.activity_items, snapshot.filter_context.as_ref());
    let requests_html = render_requests_panel_items(
        &snapshot.request_summaries,
        snapshot.filter_context.as_ref(),
    );
    let sessions_html = render_sessions_panel_items(
        &snapshot.session_summaries,
        snapshot.filter_context.as_ref(),
    );
    let session_timeline_html = render_session_detail_panel(snapshot.session_details.first());
    let request_detail_html = render_request_detail_panel(snapshot.request_details.first());
    let health_html = render_health_panel(&snapshot.target_summaries, &snapshot.activity_items);
    let script = render_console_homepage_script(
        snapshot.session_summaries.first(),
        snapshot.request_summaries.first(),
    );

    format!(
        "<!doctype html>
<html lang=\"en\">
  <head>
    <meta charset=\"utf-8\" />
    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
    <title>PrismTrace Local Console</title>
    <style>
      :root {{
        color-scheme: dark;
        font-family: -apple-system, BlinkMacSystemFont, sans-serif;
        background: #08111f;
        color: #e6edf7;
      }}
      * {{ box-sizing: border-box; }}
      body.console-shell {{
        margin: 0;
        min-height: 100vh;
        background: radial-gradient(circle at top, #13233f 0%, #08111f 52%, #050914 100%);
        color: #e6edf7;
      }}
      .console-frame {{
        max-width: 1440px;
        margin: 0 auto;
        padding: 32px 24px 40px;
      }}
      .console-header {{
        display: grid;
        gap: 16px;
        padding: 24px;
        border: 1px solid #223455;
        border-radius: 20px;
        background: rgba(10, 18, 34, 0.88);
        box-shadow: 0 20px 60px rgba(0, 0, 0, 0.28);
      }}
      .console-eyebrow {{
        margin: 0;
        color: #8dd0ff;
        font-size: 13px;
        font-weight: 600;
        letter-spacing: 0.08em;
        text-transform: uppercase;
      }}
      .console-header-main {{
        display: flex;
        flex-wrap: wrap;
        align-items: end;
        justify-content: space-between;
        gap: 16px;
      }}
      .console-title-group {{ display: grid; gap: 10px; }}
      .console-summary {{
        margin: 0;
        white-space: pre-line;
        color: #9cb0d1;
        line-height: 1.5;
      }}
      .console-entrypoint {{
        display: inline-flex;
        flex-direction: column;
        gap: 6px;
        padding: 12px 14px;
        border: 1px solid #23385c;
        border-radius: 14px;
        background: rgba(7, 13, 24, 0.9);
      }}
      .console-entrypoint-label {{
        margin: 0;
        color: #9cb0d1;
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .console-layout {{
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 16px;
        margin-top: 20px;
      }}
      .console-secondary-layout {{
        display: grid;
        grid-template-columns: minmax(0, 1.4fr) minmax(320px, 0.8fr);
        gap: 16px;
        margin-top: 16px;
      }}
      .console-panel {{
        display: grid;
        grid-template-rows: auto 1fr;
        min-height: 320px;
        border: 1px solid #223455;
        border-radius: 20px;
        background: rgba(10, 18, 34, 0.84);
        box-shadow: 0 14px 36px rgba(0, 0, 0, 0.2);
        overflow: hidden;
      }}
      .console-panel-header {{
        padding: 18px 18px 14px;
        border-bottom: 1px solid #1f2f4d;
      }}
      .console-panel-body {{
        display: grid;
        align-content: start;
        gap: 12px;
        padding: 18px;
      }}
      .console-list {{
        display: grid;
        gap: 12px;
      }}
      .console-list-item {{
        padding: 14px;
        border: 1px solid #23385c;
        border-radius: 14px;
        background: rgba(9, 15, 28, 0.82);
      }}
      .console-list-title {{
        margin: 0;
        font-size: 15px;
        font-weight: 600;
        color: #f4f8ff;
      }}
      .console-list-subtitle {{
        margin: 6px 0 0;
        color: #9cb0d1;
      }}
      .console-list-meta {{
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        margin-top: 10px;
      }}
      .console-detail-grid {{ display: grid; gap: 12px; }}
      .console-detail-row {{
        display: grid;
        gap: 6px;
        padding: 12px 14px;
        border: 1px solid #23385c;
        border-radius: 14px;
        background: rgba(9, 15, 28, 0.82);
      }}
      .console-detail-label {{
        color: #9cb0d1;
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .console-detail-section {{
        display: grid;
        gap: 12px;
        padding: 14px;
        border: 1px solid #23385c;
        border-radius: 16px;
        background: rgba(9, 15, 28, 0.82);
      }}
      .console-detail-section-title {{
        margin: 0;
        font-size: 14px;
        font-weight: 700;
        color: #f4f8ff;
      }}
      .console-code-block {{
        margin: 0;
        padding: 12px 14px;
        border: 1px solid #1f2f4d;
        border-radius: 12px;
        background: rgba(6, 11, 20, 0.92);
        color: #d9e7ff;
        font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
        font-size: 12px;
        line-height: 1.55;
        overflow: auto;
        white-space: pre-wrap;
        word-break: break-word;
      }}
      .console-health-stack {{ display: grid; gap: 12px; }}
      .console-health-card {{
        padding: 14px;
        border-radius: 14px;
        border: 1px solid #23385c;
        background: rgba(9, 15, 28, 0.82);
      }}
      .console-health-card.is-error {{
        border-color: #7d3344;
        background: rgba(49, 15, 24, 0.35);
      }}
      .console-pill {{
        display: inline-flex;
        align-items: center;
        min-height: 24px;
        padding: 0 10px;
        border-radius: 999px;
        border: 1px solid #31507c;
        background: rgba(19, 35, 63, 0.72);
        color: #cfe1ff;
        font-size: 12px;
        cursor: pointer;
      }}
      .console-placeholder {{
        margin: 0;
        padding: 14px;
        border: 1px dashed #31507c;
        border-radius: 14px;
        background: rgba(9, 15, 28, 0.82);
      }}
      h1, h2 {{ margin: 0; }}
      h1 {{ font-size: 32px; line-height: 1.1; }}
      h2 {{ font-size: 20px; line-height: 1.2; }}
      p {{ margin: 0; line-height: 1.5; }}
      .muted {{ color: #9cb0d1; }}
      code {{ color: #8dd0ff; word-break: break-all; }}
      @media (max-width: 1080px) {{
        .console-layout {{ grid-template-columns: 1fr; }}
        .console-secondary-layout {{ grid-template-columns: 1fr; }}
      }}
    </style>
  </head>
  <body class=\"console-shell\">
    <div class=\"console-frame\">
      <header class=\"console-header\">
        <p class=\"console-eyebrow\">Local-first observability</p>
        <div class=\"console-header-main\">
          <div class=\"console-title-group\">
            <h1>PrismTrace Local Console</h1>
            <p class=\"console-summary\">{}</p>
          </div>
          <div class=\"console-entrypoint\">
            <p class=\"console-entrypoint-label\">Browser entrypoint</p>
            <p><code>{}</code></p>
          </div>
        </div>
        {}
      </header>
      <main class=\"console-layout\">
        <section class=\"console-panel\" aria-labelledby=\"targets-heading\">
          <div class=\"console-panel-header\">
            <h2 id=\"targets-heading\">Targets</h2>
          </div>
          <div class=\"console-panel-body\" id=\"targets-region\">{}</div>
        </section>
        <section class=\"console-panel\" aria-labelledby=\"activity-heading\">
          <div class=\"console-panel-header\">
            <h2 id=\"activity-heading\">Activity</h2>
          </div>
          <div class=\"console-panel-body\" id=\"activity-region\">{}</div>
        </section>
        <section class=\"console-panel\" aria-labelledby=\"requests-heading\">
          <div class=\"console-panel-header\">
            <h2 id=\"requests-heading\">Requests</h2>
          </div>
          <div class=\"console-panel-body\" id=\"requests-region\">{}</div>
        </section>
        <section class=\"console-panel\" aria-labelledby=\"sessions-heading\">
          <div class=\"console-panel-header\">
            <h2 id=\"sessions-heading\">Sessions</h2>
          </div>
          <div class=\"console-panel-body\" id=\"sessions-region\">{}</div>
        </section>
      </main>
      <section class=\"console-secondary-layout\">
        <section class=\"console-panel\" aria-labelledby=\"session-detail-heading\">
          <div class=\"console-panel-header\">
            <h2 id=\"session-detail-heading\">Session Timeline</h2>
          </div>
          <div class=\"console-panel-body\" id=\"session-detail-region\">{}</div>
        </section>
        <section class=\"console-panel\" aria-labelledby=\"request-detail-heading\">
          <div class=\"console-panel-header\">
            <h2 id=\"request-detail-heading\">Request Detail</h2>
          </div>
          <div class=\"console-panel-body\" id=\"request-detail-region\">{}</div>
        </section>
        <section class=\"console-panel\" aria-labelledby=\"health-heading\">
          <div class=\"console-panel-header\">
            <h2 id=\"health-heading\">Observability Health</h2>
          </div>
          <div class=\"console-panel-body\" id=\"health-region\">{}</div>
        </section>
      </section>
    </div>
    <script>{}</script>
  </body>
</html>",
        snapshot.summary,
        snapshot.bind_addr,
        filter_context_html,
        targets_html,
        activity_html,
        requests_html,
        sessions_html,
        session_timeline_html,
        request_detail_html,
        health_html,
        script
    )
}

fn render_filter_context_banner(filter_context: Option<&ConsoleFilterContext>) -> String {
    let Some(filter_context) = filter_context else {
        return String::new();
    };

    let filters = filter_context
        .active_filters
        .iter()
        .map(|filter| {
            format!(
                "<span class=\"console-pill\">{}</span>",
                escape_html(filter)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<div class=\"console-entrypoint\"><p class=\"console-entrypoint-label\">Filtered monitor scope</p><div class=\"console-list-meta\">{}</div></div>",
        filters
    )
}

fn render_console_homepage_script(
    initial_session: Option<&ConsoleSessionSummary>,
    initial_request: Option<&ConsoleRequestSummary>,
) -> String {
    let mut script = r#"
      const escapeHtml = (value) => String(value ?? '')
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;');

      const renderEmptyState = (text) => `<p class="muted console-placeholder">${escapeHtml(text)}</p>`;

      const renderTargets = (payload) => {
        if (!payload.targets?.length) return renderEmptyState(payload.empty_state || '尚无可观测目标');
        return `<div class="console-list">${payload.targets.map((target) => `
          <article class="console-list-item">
            <p class="console-list-title">${escapeHtml(target.display_name)}</p>
            <p class="console-list-subtitle">PID ${escapeHtml(target.pid)} · ${escapeHtml(target.runtime_kind)}</p>
            <div class="console-list-meta">
              <span class="console-pill">attach: ${escapeHtml(target.attach_state)}</span>
              <span class="console-pill">${escapeHtml(target.probe_state_summary)}</span>
            </div>
          </article>`).join('')}</div>`;
      };

      const renderActivity = (payload) => {
        if (!payload.activity?.length) return renderEmptyState(payload.empty_state || '尚无观测活动');
        return `<div class="console-list">${payload.activity.map((item) => `
          <article class="console-list-item">
            <p class="console-list-title">${escapeHtml(item.title)}</p>
            <p class="console-list-subtitle">${escapeHtml(item.subtitle)}</p>
            <div class="console-list-meta">
              <span class="console-pill">${escapeHtml(item.activity_type)}</span>
              <span class="console-pill">ts: ${escapeHtml(item.occurred_at_ms)}</span>
            </div>
          </article>`).join('')}</div>`;
      };

      const renderRequests = (payload) => {
        if (!payload.requests?.length) return renderEmptyState(payload.empty_state || '尚无请求记录');
        return `<div class="console-list">${payload.requests.map((request) => `
          <article class="console-list-item" data-request-id="${escapeHtml(request.request_id)}">
            <p class="console-list-title">${escapeHtml(request.summary_text)}</p>
            <p class="console-list-subtitle">${escapeHtml(request.target_display_name)}</p>
            <div class="console-list-meta">
              <span class="console-pill">provider: ${escapeHtml(request.provider)}</span>
              <span class="console-pill">model: ${escapeHtml(request.model || 'unknown')}</span>
              <button type="button" class="console-pill" data-request-detail-trigger="${escapeHtml(request.request_id)}">view detail</button>
            </div>
          </article>`).join('')}</div>`;
      };

      const renderSessions = (payload) => {
        if (!payload.sessions?.length) return renderEmptyState(payload.empty_state || '尚无会话记录');
        return `<div class="console-list">${payload.sessions.map((session) => `
          <article class="console-list-item" data-session-id="${escapeHtml(session.session_id)}">
            <p class="console-list-title">${escapeHtml(session.target_display_name)}</p>
            <p class="console-list-subtitle">PID ${escapeHtml(session.pid)} · ${escapeHtml(session.started_at_ms)} → ${escapeHtml(session.completed_at_ms)}</p>
            <div class="console-list-meta">
              <span class="console-pill">exchanges: ${escapeHtml(session.exchange_count)}</span>
              <span class="console-pill">responses: ${escapeHtml(session.response_count)}</span>
              <button type="button" class="console-pill" data-session-detail-trigger="${escapeHtml(session.session_id)}">view timeline</button>
            </div>
          </article>`).join('')}</div>`;
      };

      const renderHeaderList = (headers, emptyText) => {
        if (!headers?.length) return renderEmptyState(emptyText);
        return `<div class="console-list">${headers.map((header) => `
          <article class="console-list-item">
            <p class="console-list-title">${escapeHtml(header.name)}</p>
            <p class="console-list-subtitle"><code>${escapeHtml(header.value)}</code></p>
          </article>`).join('')}</div>`;
      };

      const renderBodyBlock = (bodyText, truncated, emptyText) => {
        if (!bodyText) return renderEmptyState(emptyText);
        const truncatedLabel = truncated
          ? '<p class="console-detail-label">captured body is truncated</p>'
          : '';
        return `${truncatedLabel}<pre class="console-code-block">${escapeHtml(bodyText)}</pre>`;
      };

      const renderToolList = (tools, emptyText) => {
        if (!tools?.length) return renderEmptyState(emptyText);
        return `<div class="console-list">${tools.map((tool) => `
          <article class="console-list-item">
            <p class="console-list-title">${escapeHtml(tool.name)}</p>
            <p class="console-list-subtitle">type: ${escapeHtml(tool.tool_type)}</p>
          </article>`).join('')}</div>`;
      };

      const renderRequestDetail = (payload) => {
        const request = payload.request;
        if (!request || request.status === 'not_found') {
          return renderEmptyState(request?.detail || 'request detail is not available yet');
        }

        const response = request.response;
        const toolVisibility = request.tool_visibility;
        return `<div class="console-detail-grid">
          <section class="console-detail-section">
            <p class="console-detail-section-title">Request Overview</p>
            <div class="console-detail-row">
              <p class="console-detail-label">Request Summary</p>
              <p class="console-list-title">${escapeHtml(request.request_summary)}</p>
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Target</p>
              <p>${escapeHtml(request.target_display_name)}</p>
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Provider / Model</p>
              <p>${escapeHtml(request.provider)} · ${escapeHtml(request.model || 'unknown')}</p>
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Request Route</p>
              <p><code>${escapeHtml(request.method)} ${escapeHtml(request.url)}</code></p>
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Exchange / Hook</p>
              <p>${escapeHtml(request.exchange_id || 'unknown')} · ${escapeHtml(request.hook_name || 'unknown')}</p>
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Artifact Path</p>
              <p><code>${escapeHtml(request.artifact_path)}</code></p>
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Probe Context</p>
              <p>${escapeHtml(request.probe_context || '暂无 probe context')}</p>
            </div>
          </section>
          <section class="console-detail-section">
            <p class="console-detail-section-title">Request Payload</p>
            <div class="console-detail-row">
              <p class="console-detail-label">Headers</p>
              ${renderHeaderList(request.headers, '未记录 request headers')}
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Body (${escapeHtml(request.body_size_bytes || 0)} bytes)</p>
              ${renderBodyBlock(request.body_text, request.truncated, '未记录 request body')}
            </div>
          </section>
          <section class="console-detail-section">
            <p class="console-detail-section-title">Tool Visibility</p>
            ${toolVisibility ? `
              <div class="console-detail-row">
                <p class="console-detail-label">Stage / Count</p>
                <p class="console-list-title">${escapeHtml(toolVisibility.visibility_stage)} · ${escapeHtml(toolVisibility.tool_count_final)} tool(s)</p>
              </div>
              <div class="console-detail-row">
                <p class="console-detail-label">Tool Choice</p>
                <p><code>${escapeHtml(toolVisibility.tool_choice || '未记录 tool choice')}</code></p>
              </div>
              <div class="console-detail-row">
                <p class="console-detail-label">Final Tools</p>
                ${renderToolList(toolVisibility.final_tools, 'final tools array is empty')}
              </div>
              <div class="console-detail-row">
                <p class="console-detail-label">Visibility Artifact</p>
                <p><code>${escapeHtml(toolVisibility.artifact_path)}</code></p>
              </div>
              <div class="console-detail-row">
                <p class="console-detail-label">Final Tools JSON</p>
                ${renderBodyBlock(toolVisibility.final_tools_json, false, '未记录 final tools json')}
              </div>
            ` : renderEmptyState('尚未关联到 tool visibility artifact')}
          </section>
          <section class="console-detail-section">
            <p class="console-detail-section-title">Response Detail</p>
            ${response ? `
              <div class="console-detail-row">
                <p class="console-detail-label">Status / Duration</p>
                <p class="console-list-title">${escapeHtml(response.status_code)} · ${escapeHtml(response.duration_ms)}ms</p>
              </div>
              <div class="console-detail-row">
                <p class="console-detail-label">Response Timing</p>
                <p>${escapeHtml(response.started_at_ms)} → ${escapeHtml(response.completed_at_ms)}</p>
              </div>
              <div class="console-detail-row">
                <p class="console-detail-label">Response Artifact</p>
                <p><code>${escapeHtml(response.artifact_path)}</code></p>
              </div>
              <div class="console-detail-row">
                <p class="console-detail-label">Headers</p>
                ${renderHeaderList(response.headers, '未记录 response headers')}
              </div>
              <div class="console-detail-row">
                <p class="console-detail-label">Body (${escapeHtml(response.body_size_bytes || 0)} bytes)</p>
                ${renderBodyBlock(response.body_text, response.truncated, '尚未记录 response body')}
              </div>
            ` : renderEmptyState('尚未关联到 response artifact')}
          </section>
        </div>`;
      };

      const renderSessionDetail = (payload) => {
        const session = payload.session;
        if (!session || session.status === 'not_found') {
          return renderEmptyState(session?.detail || 'session detail is not available yet');
        }

        if (!session.timeline_items?.length) {
          return renderEmptyState('当前 session 尚无 timeline item');
        }

        return `<div class="console-detail-grid">
          <section class="console-detail-section">
            <p class="console-detail-section-title">Session Overview</p>
            <div class="console-detail-row">
              <p class="console-detail-label">Session</p>
              <p class="console-list-title">${escapeHtml(session.target_display_name)} · PID ${escapeHtml(session.pid)}</p>
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Window</p>
              <p>${escapeHtml(session.started_at_ms)} → ${escapeHtml(session.completed_at_ms)}</p>
            </div>
            <div class="console-detail-row">
              <p class="console-detail-label">Exchange Count</p>
              <p>${escapeHtml(session.exchange_count)}</p>
            </div>
          </section>
          <section class="console-detail-section">
            <p class="console-detail-section-title">Timeline</p>
            <div class="console-list">${session.timeline_items.map((item) => `
              <article class="console-list-item">
                <p class="console-list-title">${escapeHtml(item.request_summary)}</p>
                <p class="console-list-subtitle">${escapeHtml(item.started_at_ms)} → ${escapeHtml(item.completed_at_ms)} · ${escapeHtml(item.target_display_name)}</p>
                <div class="console-list-meta">
                  <span class="console-pill">provider: ${escapeHtml(item.provider)}</span>
                  <span class="console-pill">model: ${escapeHtml(item.model || 'unknown')}</span>
                  <span class="console-pill">status: ${escapeHtml(item.response_status ?? 'pending')}</span>
                  <span class="console-pill">tools: ${escapeHtml(item.tool_count_final)}</span>
                  <button type="button" class="console-pill" data-request-detail-trigger="${escapeHtml(item.request_id)}">view request</button>
                </div>
              </article>`).join('')}</div>
          </section>
        </div>`;
      };

      const renderHealth = (payload) => {
        const cards = [];

        if (payload.probe_summary) {
          cards.push(`<article class="console-health-card"><p class="console-detail-label">Probe Summary</p><p class="console-list-title">${escapeHtml(payload.probe_summary)}</p></article>`);
        }

        if (payload.errors?.length) {
          cards.push(...payload.errors.map((error) => `
            <article class="console-health-card is-error">
              <p class="console-detail-label">${escapeHtml(error.title)}</p>
              <p class="console-list-title">${escapeHtml(error.subtitle)}</p>
            </article>`));
        }

        if (!cards.length) {
          return renderEmptyState(payload.empty_state || '尚未发现 probe 健康或错误提示');
        }

        return `<div class="console-health-stack">${cards.join('')}</div>`;
      };

      const refreshRegion = async (endpoint, regionId, render) => {
        const region = document.getElementById(regionId);
        if (!region) return;
        try {
          const response = await fetch(endpoint);
          if (!response.ok) throw new Error(`request failed: ${response.status}`);
          const payload = await response.json();
          region.innerHTML = render(payload);
        } catch (error) {
          region.innerHTML = renderEmptyState(`加载失败：${error.message}`);
        }
      };

      const refreshRequestDetail = async (requestId) => {
        const region = document.getElementById('request-detail-region');
        if (!region) return;
        if (!requestId) {
          region.innerHTML = renderEmptyState('请选择一条 request 查看基础详情');
          return;
        }

        await refreshRegion(`/api/requests/${requestId}`, 'request-detail-region', renderRequestDetail);
      };

      const refreshSessionDetail = async (sessionId) => {
        const region = document.getElementById('session-detail-region');
        if (!region) return;
        if (!sessionId) {
          region.innerHTML = renderEmptyState('请选择一个 session 查看 timeline');
          return;
        }

        await refreshRegion(`/api/sessions/${sessionId}`, 'session-detail-region', renderSessionDetail);
      };

      document.addEventListener('click', (event) => {
        const trigger = event.target.closest('[data-request-detail-trigger]');
        if (trigger) {
          void refreshRequestDetail(trigger.getAttribute('data-request-detail-trigger'));
          return;
        }

        const sessionTrigger = event.target.closest('[data-session-detail-trigger]');
        if (!sessionTrigger) return;
        void refreshSessionDetail(sessionTrigger.getAttribute('data-session-detail-trigger'));
      });

      void refreshRegion("/api/targets", "targets-region", renderTargets);
      void refreshRegion("/api/activity", "activity-region", renderActivity);
      void refreshRegion("/api/requests", "requests-region", renderRequests);
      void refreshRegion("/api/sessions", "sessions-region", renderSessions);
      void refreshRegion("/api/health", "health-region", renderHealth);
    "#
    .to_string();

    if let Some(session) = initial_session {
        script.push_str(&format!(
            "\n      void refreshSessionDetail(\"{}\");\n",
            escape_html(&session.session_id)
        ));
    } else {
        script.push_str("\n      void refreshSessionDetail(null);\n");
    }

    if let Some(request) = initial_request {
        script.push_str(&format!(
            "\n      void refreshRequestDetail(\"{}\");\n",
            escape_html(&request.request_id)
        ));
    } else {
        script.push_str("\n      void refreshRequestDetail(null);\n");
    }

    script
}

fn render_targets_panel_items(
    targets: &[ConsoleTargetSummary],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    if targets.is_empty() {
        return render_console_empty_state(&filtered_empty_state_message(
            filter_context,
            "尚无可观测目标",
        ));
    }

    let items = targets
        .iter()
        .map(|target| {
            format!(
                "<article class=\"console-list-item\"><p class=\"console-list-title\">{}</p><p class=\"console-list-subtitle\">PID {} · {}</p><div class=\"console-list-meta\"><span class=\"console-pill\">attach: {}</span><span class=\"console-pill\">{}</span></div></article>",
                escape_html(&target.display_name),
                target.pid,
                escape_html(&target.runtime_kind),
                escape_html(&target.attach_state),
                escape_html(&target.probe_state_summary)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!("<div class=\"console-list\">{items}</div>")
}

fn render_activity_panel_items(
    items: &[ConsoleActivityItem],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    if items.is_empty() {
        return render_console_empty_state(&filtered_empty_state_message(
            filter_context,
            "尚无观测活动",
        ));
    }

    let items = items
        .iter()
        .map(|item| {
            format!(
                "<article class=\"console-list-item\"><p class=\"console-list-title\">{}</p><p class=\"console-list-subtitle\">{}</p><div class=\"console-list-meta\"><span class=\"console-pill\">{}</span><span class=\"console-pill\">ts: {}</span></div></article>",
                escape_html(&item.title),
                escape_html(&item.subtitle),
                escape_html(&item.activity_type),
                item.occurred_at_ms
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!("<div class=\"console-list\">{items}</div>")
}

fn render_requests_panel_items(
    requests: &[ConsoleRequestSummary],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    if requests.is_empty() {
        return render_console_empty_state(&filtered_empty_state_message(
            filter_context,
            "尚无请求记录",
        ));
    }

    let items = requests
        .iter()
        .map(|request| {
            format!(
                "<article class=\"console-list-item\" data-request-id=\"{}\"><p class=\"console-list-title\">{}</p><p class=\"console-list-subtitle\">{}</p><div class=\"console-list-meta\"><span class=\"console-pill\">provider: {}</span><span class=\"console-pill\">model: {}</span><button type=\"button\" class=\"console-pill\" data-request-detail-trigger=\"{}\">view detail</button></div></article>",
                escape_html(&request.request_id),
                escape_html(&request.summary_text),
                escape_html(&request.target_display_name),
                escape_html(&request.provider),
                escape_html(request.model.as_deref().unwrap_or("unknown")),
                escape_html(&request.request_id)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!("<div class=\"console-list\">{items}</div>")
}

fn render_sessions_panel_items(
    sessions: &[ConsoleSessionSummary],
    filter_context: Option<&ConsoleFilterContext>,
) -> String {
    if sessions.is_empty() {
        return render_console_empty_state(&filtered_empty_state_message(
            filter_context,
            "尚无会话记录",
        ));
    }

    let items = sessions
        .iter()
        .map(|session| {
            format!(
                "<article class=\"console-list-item\" data-session-id=\"{}\"><p class=\"console-list-title\">{}</p><p class=\"console-list-subtitle\">PID {} · {} → {}</p><div class=\"console-list-meta\"><span class=\"console-pill\">exchanges: {}</span><span class=\"console-pill\">responses: {}</span><button type=\"button\" class=\"console-pill\" data-session-detail-trigger=\"{}\">view timeline</button></div></article>",
                escape_html(&session.session_id),
                escape_html(&session.target_display_name),
                session.pid,
                session.started_at_ms,
                session.completed_at_ms,
                session.exchange_count,
                session.response_count,
                escape_html(&session.session_id)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!("<div class=\"console-list\">{items}</div>")
}

fn render_console_empty_state(text: &str) -> String {
    format!(
        "<p class=\"muted console-placeholder\">{}</p>",
        escape_html(text)
    )
}

fn filtered_empty_state_message(
    filter_context: Option<&ConsoleFilterContext>,
    default: &str,
) -> String {
    match filter_context {
        Some(filter_context) if filter_context.is_filtered_view => match default {
            "尚无可观测目标" => "当前过滤条件下没有匹配目标".to_string(),
            "尚无观测活动" => "当前过滤条件下没有匹配活动".to_string(),
            "尚无请求记录" => "当前过滤条件下没有匹配请求".to_string(),
            _ => default.to_string(),
        },
        _ => default.to_string(),
    }
}

fn render_header_details_html(headers: &[ConsoleHeaderDetail], empty_text: &str) -> String {
    if headers.is_empty() {
        return render_console_empty_state(empty_text);
    }

    let items = headers
        .iter()
        .map(|header| {
            format!(
                "<article class=\"console-list-item\"><p class=\"console-list-title\">{}</p><p class=\"console-list-subtitle\"><code>{}</code></p></article>",
                escape_html(&header.name),
                escape_html(&header.value)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!("<div class=\"console-list\">{items}</div>")
}

fn render_body_block_html(body_text: Option<&str>, truncated: bool, empty_text: &str) -> String {
    let Some(body_text) = body_text else {
        return render_console_empty_state(empty_text);
    };

    let truncated_hint = if truncated {
        "<p class=\"console-detail-label\">captured body is truncated</p>"
    } else {
        ""
    };

    format!(
        "{truncated_hint}<pre class=\"console-code-block\">{}</pre>",
        escape_html(body_text)
    )
}

fn render_tool_summaries_html(tools: &[ConsoleToolSummary], empty_text: &str) -> String {
    if tools.is_empty() {
        return render_console_empty_state(empty_text);
    }

    let items = tools
        .iter()
        .map(|tool| {
            format!(
                "<article class=\"console-list-item\"><p class=\"console-list-title\">{}</p><p class=\"console-list-subtitle\">type: {}</p></article>",
                escape_html(&tool.name),
                escape_html(&tool.tool_type)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!("<div class=\"console-list\">{items}</div>")
}

fn render_request_detail_panel(detail: Option<&ConsoleRequestDetail>) -> String {
    match detail {
        Some(detail) => format!(
            "<div class=\"console-detail-grid\">\
                <section class=\"console-detail-section\">\
                  <p class=\"console-detail-section-title\">Request Overview</p>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Request Summary</p><p class=\"console-list-title\">{}</p></div>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Target</p><p>{}</p></div>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Provider / Model</p><p>{} · {}</p></div>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Request Route</p><p><code>{} {}</code></p></div>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Exchange / Hook</p><p>{} · {}</p></div>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Artifact Path</p><p><code>{}</code></p></div>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Probe Context</p><p>{}</p></div>\
                </section>\
                <section class=\"console-detail-section\">\
                  <p class=\"console-detail-section-title\">Request Payload</p>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Headers</p>{}</div>\
                  <div class=\"console-detail-row\"><p class=\"console-detail-label\">Body ({} bytes)</p>{}</div>\
                </section>\
                <section class=\"console-detail-section\">\
                  <p class=\"console-detail-section-title\">Tool Visibility</p>\
                  {}\
                </section>\
                <section class=\"console-detail-section\">\
                  <p class=\"console-detail-section-title\">Response Detail</p>\
                  {}\
                </section>\
             </div>",
            escape_html(&detail.request_summary),
            escape_html(&detail.target_display_name),
            escape_html(&detail.provider),
            escape_html(detail.model.as_deref().unwrap_or("unknown")),
            escape_html(&detail.method),
            escape_html(&detail.url),
            escape_html(detail.exchange_id.as_deref().unwrap_or("unknown")),
            escape_html(&detail.hook_name),
            escape_html(&detail.artifact_path),
            escape_html(
                detail
                    .probe_context
                    .as_deref()
                    .unwrap_or("暂无 probe context")
            ),
            render_header_details_html(&detail.headers, "未记录 request headers"),
            detail.body_size_bytes,
            render_body_block_html(
                detail.body_text.as_deref(),
                detail.truncated,
                "未记录 request body"
            ),
            detail
                .tool_visibility
                .as_ref()
                .map(|visibility| {
                    format!(
                        "<div class=\"console-detail-row\"><p class=\"console-detail-label\">Stage / Count</p><p class=\"console-list-title\">{} · {} tool(s)</p></div>\
                         <div class=\"console-detail-row\"><p class=\"console-detail-label\">Tool Choice</p><p><code>{}</code></p></div>\
                         <div class=\"console-detail-row\"><p class=\"console-detail-label\">Final Tools</p>{}</div>\
                         <div class=\"console-detail-row\"><p class=\"console-detail-label\">Visibility Artifact</p><p><code>{}</code></p></div>\
                         <div class=\"console-detail-row\"><p class=\"console-detail-label\">Final Tools JSON</p>{}</div>",
                        escape_html(&visibility.visibility_stage),
                        visibility.tool_count_final,
                        escape_html(
                            visibility
                                .tool_choice
                                .as_deref()
                                .unwrap_or("未记录 tool choice")
                        ),
                        render_tool_summaries_html(
                            &visibility.final_tools,
                            "final tools array is empty"
                        ),
                        escape_html(&visibility.artifact_path),
                        render_body_block_html(
                            Some(&visibility.final_tools_json),
                            false,
                            "未记录 final tools json"
                        )
                    )
                })
                .unwrap_or_else(|| render_console_empty_state("尚未关联到 tool visibility artifact")),
            detail
                .response
                .as_ref()
                .map(|response| {
                    format!(
                        "<div class=\"console-detail-row\"><p class=\"console-detail-label\">Status / Duration</p><p class=\"console-list-title\">{} · {}ms</p></div>\
                         <div class=\"console-detail-row\"><p class=\"console-detail-label\">Response Timing</p><p>{} → {}</p></div>\
                         <div class=\"console-detail-row\"><p class=\"console-detail-label\">Response Artifact</p><p><code>{}</code></p></div>\
                         <div class=\"console-detail-row\"><p class=\"console-detail-label\">Headers</p>{}</div>\
                         <div class=\"console-detail-row\"><p class=\"console-detail-label\">Body ({} bytes)</p>{}</div>",
                        response.status_code,
                        response.duration_ms,
                        response.started_at_ms,
                        response.completed_at_ms,
                        escape_html(&response.artifact_path),
                        render_header_details_html(&response.headers, "未记录 response headers"),
                        response.body_size_bytes,
                        render_body_block_html(
                            response.body_text.as_deref(),
                            response.truncated,
                            "尚未记录 response body"
                        )
                    )
                })
                .unwrap_or_else(|| render_console_empty_state("尚未关联到 response artifact"))
        ),
        None => render_console_empty_state("请选择一条 request 查看基础详情"),
    }
}

fn render_session_detail_panel(detail: Option<&ConsoleSessionDetail>) -> String {
    match detail {
        Some(detail) => {
            let items = detail
                .timeline_items
                .iter()
                .map(|item| {
                    format!(
                        "<article class=\"console-list-item\"><p class=\"console-list-title\">{}</p><p class=\"console-list-subtitle\">{} → {} · {}</p><div class=\"console-list-meta\"><span class=\"console-pill\">provider: {}</span><span class=\"console-pill\">model: {}</span><span class=\"console-pill\">status: {}</span><span class=\"console-pill\">tools: {}</span><button type=\"button\" class=\"console-pill\" data-request-detail-trigger=\"{}\">view request</button></div></article>",
                        escape_html(&item.request_summary),
                        item.started_at_ms,
                        item.completed_at_ms,
                        escape_html(&item.target_display_name),
                        escape_html(&item.provider),
                        escape_html(item.model.as_deref().unwrap_or("unknown")),
                        item.response_status
                            .map(|status| status.to_string())
                            .unwrap_or_else(|| "pending".to_string()),
                        item.tool_count_final,
                        escape_html(&item.request_id)
                    )
                })
                .collect::<Vec<_>>()
                .join("");

            format!(
                "<div class=\"console-detail-grid\"><section class=\"console-detail-section\"><p class=\"console-detail-section-title\">Session Overview</p><div class=\"console-detail-row\"><p class=\"console-detail-label\">Session</p><p class=\"console-list-title\">{} · PID {}</p></div><div class=\"console-detail-row\"><p class=\"console-detail-label\">Window</p><p>{} → {}</p></div><div class=\"console-detail-row\"><p class=\"console-detail-label\">Exchange Count</p><p>{}</p></div></section><section class=\"console-detail-section\"><p class=\"console-detail-section-title\">Timeline</p>{}</section></div>",
                escape_html(&detail.target_display_name),
                detail.pid,
                detail.started_at_ms,
                detail.completed_at_ms,
                detail.exchange_count,
                if items.is_empty() {
                    render_console_empty_state("当前 session 尚无 timeline item")
                } else {
                    format!("<div class=\"console-list\">{items}</div>")
                }
            )
        }
        None => render_console_empty_state("请选择一个 session 查看 timeline"),
    }
}

fn render_health_panel(
    targets: &[ConsoleTargetSummary],
    activity_items: &[ConsoleActivityItem],
) -> String {
    let probe_summary = targets
        .iter()
        .find(|target| target.attach_state != "idle")
        .map(|target| target.probe_state_summary.as_str())
        .or_else(|| {
            targets
                .first()
                .map(|target| target.probe_state_summary.as_str())
        });

    let errors = activity_items
        .iter()
        .filter(|item| item.activity_type == "error")
        .map(|item| {
            format!(
                "<article class=\"console-health-card is-error\"><p class=\"console-detail-label\">{}</p><p class=\"console-list-title\">{}</p></article>",
                escape_html(&item.title),
                escape_html(&item.subtitle)
            )
        })
        .collect::<Vec<_>>();

    if probe_summary.is_none() && errors.is_empty() {
        return render_console_empty_state("尚未发现 probe 健康或错误提示");
    }

    let mut cards = Vec::new();
    if let Some(summary) = probe_summary {
        cards.push(format!(
            "<article class=\"console-health-card\"><p class=\"console-detail-label\">Probe Summary</p><p class=\"console-list-title\">{}</p></article>",
            escape_html(summary)
        ));
    }
    cards.extend(errors);

    format!(
        "<div class=\"console-health-stack\">{}</div>",
        cards.join("")
    )
}

fn render_health_payload(
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

    let probe_summary = filtered_targets
        .iter()
        .find(|target| target.attach_state != "idle")
        .map(|target| target.probe_state_summary.clone())
        .or_else(|| {
            filtered_targets
                .first()
                .map(|target| target.probe_state_summary.clone())
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
        "probe_summary": probe_summary,
        "errors": errors,
        "empty_state": if probe_summary.is_none() && errors.is_empty() { Some("尚未发现 probe 健康或错误提示") } else { None::<&str> }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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

fn render_targets_payload_from_summaries(
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
                "attach_state": target.attach_state,
                "probe_state_summary": target.probe_state_summary,
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "targets": targets,
        "empty_state": if targets.is_empty() { Some(filtered_empty_state_message(filter_context, "尚无可观测目标")) } else { None::<String> }
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub fn collect_target_summaries(
    source: &impl ProcessSampleSource,
    filter: Option<&ConsoleTargetFilterConfig>,
    active_session: Option<&AttachSession>,
    probe_health: Option<&ProbeHealth>,
) -> io::Result<Vec<ConsoleTargetSummary>> {
    let (_, _, summaries) =
        collect_target_partition_and_summaries(source, filter, active_session, probe_health)?;

    Ok(summaries)
}

fn collect_target_partition_and_summaries(
    source: &impl ProcessSampleSource,
    filter: Option<&ConsoleTargetFilterConfig>,
    active_session: Option<&AttachSession>,
    probe_health: Option<&ProbeHealth>,
) -> io::Result<(
    Vec<ProcessTarget>,
    Vec<ProcessTarget>,
    Vec<ConsoleTargetSummary>,
)> {
    let discovered_targets = discover_targets(source)?;
    let (matched_targets, unmatched_targets) = partition_targets(discovered_targets, filter);
    let summaries = matched_targets
        .iter()
        .map(|target| summarize_target(target, active_session, probe_health))
        .collect();

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

fn summarize_target(
    target: &ProcessTarget,
    active_session: Option<&AttachSession>,
    probe_health: Option<&ProbeHealth>,
) -> ConsoleTargetSummary {
    let is_active = active_session
        .map(|session| session.target.pid == target.pid)
        .unwrap_or(false);

    let attach_state = active_session
        .filter(|session| session.target.pid == target.pid)
        .map(|session| session.state.label().to_string())
        .unwrap_or_else(|| "idle".to_string());

    let probe_state_summary = if is_active {
        summarize_probe_health(probe_health)
    } else {
        "probe: no active session".to_string()
    };

    ConsoleTargetSummary {
        pid: target.pid,
        display_name: target.display_name().to_string(),
        runtime_kind: target.runtime_kind.label().to_string(),
        attach_state,
        probe_state_summary,
    }
}

fn summarize_probe_health(probe_health: Option<&ProbeHealth>) -> String {
    match probe_health {
        Some(health) => {
            let mut store = ProbeHealthStore::new();
            store.health = Some(health.clone());
            store.session_state = match health.state {
                prismtrace_core::ProbeState::Attached => {
                    crate::probe_health::ProbeSessionState::Alive
                }
                prismtrace_core::ProbeState::Attaching => {
                    crate::probe_health::ProbeSessionState::Bootstrapping
                }
                prismtrace_core::ProbeState::Detached => {
                    crate::probe_health::ProbeSessionState::Disconnected
                }
                prismtrace_core::ProbeState::Failed => {
                    crate::probe_health::ProbeSessionState::TimedOut
                }
            };
            store.status_summary()
        }
        None => "probe: no health data".to_string(),
    }
}

pub fn collect_activity_items(source: ConsoleActivitySource<'_>) -> Vec<ConsoleActivityItem> {
    let mut items = Vec::new();

    if let (Some(session), Some(occurred_at_ms)) =
        (source.attach_session, source.attach_occurred_at_ms)
    {
        items.push(ConsoleActivityItem {
            activity_id: format!("attach-{}-{occurred_at_ms}", session.target.pid),
            activity_type: "attach".into(),
            occurred_at_ms,
            title: format!("Attached to {}", session.target.display_name()),
            subtitle: session.detail.clone(),
            related_pid: Some(session.target.pid),
            related_request_id: None,
        });
    }

    if let (Some(health), Some(occurred_at_ms)) = (source.probe_health, source.probe_occurred_at_ms)
    {
        items.push(ConsoleActivityItem {
            activity_id: format!("probe-{occurred_at_ms}"),
            activity_type: "probe".into(),
            occurred_at_ms,
            title: "Probe online".into(),
            subtitle: format!(
                "installed={} failed={}",
                health.installed_hooks.len(),
                health.failed_hooks.len()
            ),
            related_pid: source.attach_session.map(|session| session.target.pid),
            related_request_id: None,
        });
    }

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

fn render_activity_payload_from_items(
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
        .map(|detail| ConsoleSessionSummary {
            session_id: detail.session_id,
            pid: detail.pid,
            target_display_name: detail.target_display_name,
            started_at_ms: detail.started_at_ms,
            completed_at_ms: detail.completed_at_ms,
            exchange_count: detail.exchange_count,
            request_count: detail.exchange_count,
            response_count: detail
                .timeline_items
                .iter()
                .filter(|item| item.has_response)
                .count(),
        })
        .collect())
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
    Ok(build_response_detail_index(storage)?.get(exchange_id).cloned())
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
            index
                .by_request_id
                .insert(record.request_id.clone(), (record.captured_at_ms, detail.clone()));
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

fn render_requests_payload(
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

fn render_sessions_payload(
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

fn render_request_detail_payload(
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

fn render_session_detail_payload(
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

fn append_filter_context_fields(
    payload: &mut Value,
    filter_context: Option<&ConsoleFilterContext>,
) {
    let Some(filter_context) = filter_context else {
        return;
    };

    payload["active_filters"] = json!(filter_context.active_filters);
    payload["is_filtered_view"] = json!(filter_context.is_filtered_view);
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
mod tests {
    use super::{
        ConsoleActivityItem, ConsoleActivitySource, ConsoleKnownErrorActivity,
        ConsoleRecentRequestActivity, ConsoleRequestSummary, ConsoleSnapshot,
        ConsoleTargetFilterConfig, ConsoleTargetSummary, collect_activity_items,
        collect_activity_items_filtered, collect_target_summaries, filter_request_summaries,
        load_request_detail, load_request_summaries, load_session_detail, load_session_summaries,
        read_request_path_from_reader, render_activity_payload_from_items, run_console_server,
        start_console_server_on_bind_addr, write_console_response,
    };
    use crate::bootstrap;
    use crate::discovery::StaticProcessSampleSource;
    use prismtrace_core::{
        AttachSession, AttachSessionState, ProbeHealth, ProbeState, ProcessSample, ProcessTarget,
        RuntimeKind,
    };
    use std::fs;
    use std::io::{self, Cursor, Read};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    static UNIQUE_TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn console_target_filter_config_is_disabled_when_terms_are_empty() {
        let filter = ConsoleTargetFilterConfig::new(Vec::new());

        assert!(!filter.is_enabled());
    }

    #[test]
    fn console_target_filter_config_matches_display_name_path_and_command_line() {
        let filter = ConsoleTargetFilterConfig::new(vec!["codex".into()]);
        let target = ProcessTarget {
            pid: 42,
            app_name: "Codex CLI".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            command_line: Some("/Applications/Codex.app/Contents/MacOS/Codex --console".into()),
            runtime_kind: RuntimeKind::Electron,
        };

        assert!(filter.matches_target(&target));
    }

    #[test]
    fn console_target_filter_config_matches_when_any_term_hits() {
        let filter = ConsoleTargetFilterConfig::new(vec!["opencode".into(), "codex".into()]);
        let target = ProcessTarget {
            pid: 7,
            app_name: "yaml-language-server".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            command_line: Some("node /Users/test/bin/opencode-server.js --stdio".into()),
            runtime_kind: RuntimeKind::Node,
        };

        assert!(filter.matches_target(&target));
    }

    #[test]
    fn console_target_filter_config_does_not_match_when_term_only_appears_in_console_flag_args() {
        let filter = ConsoleTargetFilterConfig::new(vec!["definitely-no-match".into()]);
        let target = ProcessTarget {
            pid: 999,
            app_name: "prismtrace-host".into(),
            executable_path: PathBuf::from("/tmp/target/debug/prismtrace-host"),
            command_line: Some(
                "target/debug/prismtrace-host --console --target definitely-no-match".into(),
            ),
            runtime_kind: RuntimeKind::Unknown,
        };

        assert!(!filter.matches_target(&target));
    }

    #[test]
    fn console_target_filter_config_rejects_non_matching_targets() {
        let filter = ConsoleTargetFilterConfig::new(vec!["codex".into()]);
        let target = ProcessTarget {
            pid: 8,
            app_name: "Claude Code".into(),
            executable_path: PathBuf::from(
                "/Applications/Claude Code.app/Contents/MacOS/Claude Code",
            ),
            command_line: Some("/Applications/Claude Code.app/Contents/MacOS/Claude Code".into()),
            runtime_kind: RuntimeKind::Electron,
        };

        assert!(!filter.matches_target(&target));
    }

    #[test]
    fn collect_target_summaries_filters_non_matching_targets() -> io::Result<()> {
        let source = StaticProcessSampleSource::new(vec![
            ProcessSample {
                pid: 100,
                process_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                command_line: Some("node /tmp/opencode.js".into()),
            },
            ProcessSample {
                pid: 200,
                process_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                command_line: Some("node /tmp/claude.js".into()),
            },
        ]);
        let filter = ConsoleTargetFilterConfig::new(vec!["opencode".into()]);

        let summaries = collect_target_summaries(&source, Some(&filter), None, None)?;

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].pid, 100);
        Ok(())
    }

    #[test]
    fn collect_activity_items_filters_items_by_matching_pid() {
        let filter = ConsoleTargetFilterConfig::new(vec!["opencode".into()]);
        let matched_target = ProcessTarget {
            pid: 100,
            app_name: "opencode".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            command_line: Some("node /tmp/opencode.js".into()),
            runtime_kind: RuntimeKind::Node,
        };
        let unmatched_target = ProcessTarget {
            pid: 200,
            app_name: "claude".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            command_line: Some("node /tmp/claude.js".into()),
            runtime_kind: RuntimeKind::Node,
        };
        let attach_session = AttachSession {
            target: matched_target,
            state: AttachSessionState::Attached,
            detail: "probe handshake completed".into(),
            bootstrap: None,
            failure: None,
        };

        let items = collect_activity_items_filtered(
            ConsoleActivitySource {
                attach_session: Some(&attach_session),
                attach_occurred_at_ms: Some(10),
                probe_health: None,
                probe_occurred_at_ms: None,
                recent_requests: &[
                    ConsoleRecentRequestActivity {
                        request_id: "req-match".into(),
                        captured_at_ms: 20,
                        title: "matched".into(),
                        subtitle: "matched".into(),
                        related_pid: Some(100),
                    },
                    ConsoleRecentRequestActivity {
                        request_id: "req-unmatch".into(),
                        captured_at_ms: 30,
                        title: "unmatched".into(),
                        subtitle: "unmatched".into(),
                        related_pid: Some(200),
                    },
                ],
                known_errors: &[
                    ConsoleKnownErrorActivity {
                        activity_id: "error-match".into(),
                        occurred_at_ms: 40,
                        title: "matched error".into(),
                        subtitle: "matched error".into(),
                        related_pid: Some(100),
                    },
                    ConsoleKnownErrorActivity {
                        activity_id: "error-unmatch".into(),
                        occurred_at_ms: 50,
                        title: "unmatched error".into(),
                        subtitle: "unmatched error".into(),
                        related_pid: Some(200),
                    },
                ],
            },
            Some(&filter),
            &[unmatched_target],
        );

        assert_eq!(items.len(), 3);
        assert!(items.iter().all(|item| item.related_pid != Some(200)));
    }

    #[test]
    fn filter_request_summaries_keeps_only_matching_target_display_names() {
        let filter = ConsoleTargetFilterConfig::new(vec!["opencode".into()]);
        let requests = vec![
            ConsoleRequestSummary {
                request_id: "req-match".into(),
                captured_at_ms: 1,
                provider: "openai".into(),
                model: None,
                target_display_name: "opencode".into(),
                summary_text: "matched".into(),
            },
            ConsoleRequestSummary {
                request_id: "req-unmatch".into(),
                captured_at_ms: 2,
                provider: "anthropic".into(),
                model: None,
                target_display_name: "claude".into(),
                summary_text: "unmatched".into(),
            },
        ];

        let filtered = filter_request_summaries(&requests, Some(&filter));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].request_id, "req-match");
    }

    #[test]
    fn render_health_payload_filters_errors_by_matching_pid() {
        let filter = ConsoleTargetFilterConfig::new(vec!["opencode".into()]);
        let targets = vec![ConsoleTargetSummary {
            pid: 100,
            display_name: "opencode".into(),
            runtime_kind: "node".into(),
            attach_state: "attached".into(),
            probe_state_summary: "probe: healthy".into(),
        }];
        let activity = vec![
            ConsoleActivityItem {
                activity_id: "error-match".into(),
                activity_type: "error".into(),
                occurred_at_ms: 10,
                title: "matched error".into(),
                subtitle: "matched error".into(),
                related_pid: Some(100),
                related_request_id: None,
            },
            ConsoleActivityItem {
                activity_id: "error-unmatch".into(),
                activity_type: "error".into(),
                occurred_at_ms: 20,
                title: "unmatched error".into(),
                subtitle: "unmatched error".into(),
                related_pid: Some(200),
                related_request_id: None,
            },
        ];

        let payload = super::render_health_payload(&targets, &activity, Some(&filter), None);

        assert!(payload.contains("matched error"), "payload: {payload}");
        assert!(!payload.contains("unmatched error"), "payload: {payload}");
    }

    #[test]
    fn render_console_homepage_shows_filter_context_when_filters_are_active() {
        let homepage = super::render_console_homepage(&ConsoleSnapshot {
            summary: "PrismTrace host skeleton".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: Some(super::ConsoleFilterContext {
                active_filters: vec!["opencode".into(), "codex".into()],
                is_filtered_view: true,
            }),
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        });

        assert!(
            homepage.contains("Filtered monitor scope"),
            "homepage: {homepage}"
        );
        assert!(homepage.contains("opencode"), "homepage: {homepage}");
        assert!(homepage.contains("codex"), "homepage: {homepage}");
    }

    #[test]
    fn render_console_homepage_hides_filter_context_when_unfiltered() {
        let homepage = super::render_console_homepage(&ConsoleSnapshot {
            summary: "PrismTrace host skeleton".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        });

        assert!(
            !homepage.contains("Filtered monitor scope"),
            "homepage: {homepage}"
        );
    }

    #[test]
    fn render_targets_payload_includes_filter_context_when_filters_are_active() {
        let payload = super::render_targets_payload_from_summaries(
            &[],
            Some(&super::ConsoleFilterContext {
                active_filters: vec!["opencode".into()],
                is_filtered_view: true,
            }),
        );

        assert!(
            payload.contains("\"is_filtered_view\":true"),
            "payload: {payload}"
        );
        assert!(
            payload.contains("\"active_filters\":[\"opencode\"]"),
            "payload: {payload}"
        );
    }

    #[test]
    fn render_targets_payload_omits_filter_context_when_unfiltered() {
        let payload = super::render_targets_payload_from_summaries(&[], None);

        assert!(!payload.contains("active_filters"), "payload: {payload}");
        assert!(!payload.contains("is_filtered_view"), "payload: {payload}");
    }

    #[test]
    fn render_targets_payload_uses_filtered_no_match_empty_state_when_context_is_active() {
        let payload = super::render_targets_payload_from_summaries(
            &[],
            Some(&super::ConsoleFilterContext {
                active_filters: vec!["opencode".into()],
                is_filtered_view: true,
            }),
        );

        assert!(
            payload.contains("当前过滤条件下没有匹配目标"),
            "payload: {payload}"
        );
    }

    #[test]
    fn render_activity_payload_uses_filtered_no_match_empty_state_when_context_is_active() {
        let payload = super::render_activity_payload_from_items(
            &[],
            Some(&super::ConsoleFilterContext {
                active_filters: vec!["opencode".into()],
                is_filtered_view: true,
            }),
        );

        assert!(
            payload.contains("当前过滤条件下没有匹配活动"),
            "payload: {payload}"
        );
    }

    #[test]
    fn render_requests_payload_uses_filtered_no_match_empty_state_when_context_is_active() {
        let payload = super::render_requests_payload(
            &[],
            Some(&super::ConsoleFilterContext {
                active_filters: vec!["opencode".into()],
                is_filtered_view: true,
            }),
        );

        assert!(
            payload.contains("当前过滤条件下没有匹配请求"),
            "payload: {payload}"
        );
    }

    #[test]
    fn render_console_homepage_uses_filtered_no_match_empty_states_when_context_is_active() {
        let homepage = super::render_console_homepage(&ConsoleSnapshot {
            summary: "PrismTrace host skeleton".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: Some(super::ConsoleFilterContext {
                active_filters: vec!["opencode".into()],
                is_filtered_view: true,
            }),
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        });

        assert!(
            homepage.contains("当前过滤条件下没有匹配目标"),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("当前过滤条件下没有匹配活动"),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("当前过滤条件下没有匹配请求"),
            "homepage: {homepage}"
        );
    }

    #[test]
    fn start_console_server_returns_addr_in_use_when_bind_fails() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let occupied = TcpListener::bind("127.0.0.1:0")?;
        let addr = occupied.local_addr()?;

        let error = start_console_server_on_bind_addr(&result, &addr.to_string(), None)
            .expect_err("occupied port should fail");

        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);

        drop(occupied);
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_serves_homepage_over_http() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());

        let mut stream = TcpStream::connect(addr)?;
        std::io::Write::write_all(
            &mut stream,
            b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        let body = response
            .split("\r\n\r\n")
            .nth(1)
            .expect("response should include body");

        assert!(body.contains("PrismTrace Local Console"), "body: {body}");
        assert!(body.contains("Targets"), "body: {body}");
        assert!(body.contains("Activity"), "body: {body}");
        assert!(body.contains("Requests"), "body: {body}");

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn render_console_homepage_exposes_shell_and_three_primary_regions() {
        let homepage = super::render_console_homepage(&ConsoleSnapshot {
            summary: "PrismTrace host skeleton".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        });

        assert!(
            homepage.contains("<body class=\"console-shell\">"),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("<main class=\"console-layout\">"),
            "homepage: {homepage}"
        );
        assert!(
            homepage
                .contains("<section class=\"console-panel\" aria-labelledby=\"targets-heading\">"),
            "homepage: {homepage}"
        );
        assert!(
            homepage
                .contains("<section class=\"console-panel\" aria-labelledby=\"activity-heading\">"),
            "homepage: {homepage}"
        );
        assert!(
            homepage
                .contains("<section class=\"console-panel\" aria-labelledby=\"requests-heading\">"),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("id=\"targets-region\""),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("id=\"activity-region\""),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("id=\"requests-region\""),
            "homepage: {homepage}"
        );
    }

    #[test]
    fn render_console_homepage_renders_snapshot_lists_and_refresh_script() {
        let homepage = super::render_console_homepage(&ConsoleSnapshot {
            summary: "PrismTrace host skeleton".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![super::ConsoleTargetSummary {
                pid: 701,
                display_name: "Codex".into(),
                runtime_kind: "node".into(),
                attach_state: "attached".into(),
                probe_state_summary: "probe: healthy".into(),
            }],
            activity_items: vec![super::ConsoleActivityItem {
                activity_id: "probe-1".into(),
                activity_type: "probe".into(),
                occurred_at_ms: 20,
                title: "Probe online".into(),
                subtitle: "installed=1 failed=0".into(),
                related_pid: Some(701),
                related_request_id: None,
            }],
            request_summaries: vec![super::ConsoleRequestSummary {
                request_id: "req-1".into(),
                captured_at_ms: 30,
                provider: "openai".into(),
                model: Some("gpt-4.1".into()),
                target_display_name: "Codex".into(),
                summary_text: "openai POST /v1/responses".into(),
            }],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        });

        assert!(homepage.contains("Codex"), "homepage: {homepage}");
        assert!(homepage.contains("Probe online"), "homepage: {homepage}");
        assert!(
            homepage.contains("openai POST /v1/responses"),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("refreshRegion(\"/api/targets\""),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("refreshRegion(\"/api/activity\""),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("refreshRegion(\"/api/requests\""),
            "homepage: {homepage}"
        );
    }

    #[test]
    fn render_targets_payload_includes_empty_state_when_no_targets() {
        let payload = super::render_targets_payload_from_summaries(&[], None);

        assert!(payload.contains("\"targets\":[]"), "payload: {payload}");
        assert!(
            payload.contains("\"empty_state\":\"尚无可观测目标\""),
            "payload: {payload}"
        );
    }

    #[test]
    fn render_console_homepage_renders_request_detail_and_health_panel_regions() {
        let homepage = super::render_console_homepage(&ConsoleSnapshot {
            summary: "PrismTrace host skeleton".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![super::ConsoleRequestSummary {
                request_id: "req-1".into(),
                captured_at_ms: 30,
                provider: "openai".into(),
                model: Some("gpt-4.1".into()),
                target_display_name: "Codex".into(),
                summary_text: "openai POST /v1/responses".into(),
            }],
            session_summaries: vec![],
            request_details: vec![super::ConsoleRequestDetail {
                request_id: "req-1".into(),
                exchange_id: Some("ex-1".into()),
                captured_at_ms: 30,
                provider: "openai".into(),
                model: Some("gpt-4.1".into()),
                target_display_name: "Codex".into(),
                artifact_path: "/tmp/request.json".into(),
                request_summary: "openai POST /v1/responses".into(),
                hook_name: "fetch".into(),
                method: "POST".into(),
                url: "https://api.openai.com/v1/responses".into(),
                headers: vec![super::ConsoleHeaderDetail {
                    name: "content-type".into(),
                    value: "application/json".into(),
                }],
                body_text: Some("{\"model\":\"gpt-4.1\"}".into()),
                body_size_bytes: 19,
                truncated: false,
                probe_context: Some("fetch hook".into()),
                tool_visibility: Some(super::ConsoleToolVisibilityDetail {
                    artifact_path: "/tmp/tool-visibility.json".into(),
                    visibility_stage: "request-embedded".into(),
                    tool_choice: Some("auto".into()),
                    tool_count_final: 1,
                    final_tools: vec![super::ConsoleToolSummary {
                        name: "list_files".into(),
                        tool_type: "function".into(),
                    }],
                    final_tools_json:
                        "[{\"type\":\"function\",\"function\":{\"name\":\"list_files\"}}]".into(),
                }),
                response: Some(super::ConsoleResponseDetail {
                    artifact_path: "/tmp/response.json".into(),
                    status_code: 200,
                    headers: vec![super::ConsoleHeaderDetail {
                        name: "content-type".into(),
                        value: "application/json".into(),
                    }],
                    body_text: Some("{\"output\":[]}".into()),
                    body_size_bytes: 13,
                    truncated: false,
                    started_at_ms: 31,
                    completed_at_ms: 33,
                    duration_ms: 2,
                }),
            }],
            session_details: vec![],
        });

        assert!(homepage.contains("Request Detail"), "homepage: {homepage}");
        assert!(
            homepage.contains("id=\"request-detail-region\""),
            "homepage: {homepage}"
        );
        assert!(homepage.contains("Tool Visibility"), "homepage: {homepage}");
        assert!(
            homepage.contains("Observability Health"),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("id=\"health-region\""),
            "homepage: {homepage}"
        );
        assert!(
            homepage.contains("refreshRequestDetail(\"req-1\""),
            "homepage: {homepage}"
        );
    }

    #[test]
    fn render_console_homepage_renders_probe_and_error_summary_content() {
        let homepage = super::render_console_homepage(&ConsoleSnapshot {
            summary: "PrismTrace host skeleton\n[alive] probe: attached (installed: 2, failed: 1)\nprobe heartbeat timed out".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![super::ConsoleTargetSummary {
                pid: 701,
                display_name: "Codex".into(),
                runtime_kind: "node".into(),
                attach_state: "attached".into(),
                probe_state_summary: "[alive] probe: attached (installed: 2, failed: 1)".into(),
            }],
            activity_items: vec![super::ConsoleActivityItem {
                activity_id: "error-1".into(),
                activity_type: "error".into(),
                occurred_at_ms: 40,
                title: "Probe timeout".into(),
                subtitle: "probe heartbeat timed out".into(),
                related_pid: Some(701),
                related_request_id: None,
            }],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        });

        assert!(homepage.contains("probe: attached"), "homepage: {homepage}");
        assert!(homepage.contains("Probe timeout"), "homepage: {homepage}");
        assert!(
            homepage.contains("probe heartbeat timed out"),
            "homepage: {homepage}"
        );
    }

    #[test]
    fn render_request_detail_payload_marks_missing_detail_with_status() {
        let payload = super::render_request_detail_payload("missing-request", None, None);

        assert!(
            payload.contains("\"status\":\"not_found\""),
            "payload: {payload}"
        );
        assert!(
            payload.contains("request detail is not available yet"),
            "payload: {payload}"
        );
    }

    #[test]
    fn render_health_payload_includes_probe_summary_and_errors() {
        let payload = super::render_health_payload(
            &[super::ConsoleTargetSummary {
                pid: 701,
                display_name: "Codex".into(),
                runtime_kind: "node".into(),
                attach_state: "attached".into(),
                probe_state_summary: "[alive] probe: attached (installed: 2, failed: 1)".into(),
            }],
            &[super::ConsoleActivityItem {
                activity_id: "error-1".into(),
                activity_type: "error".into(),
                occurred_at_ms: 40,
                title: "Probe timeout".into(),
                subtitle: "probe heartbeat timed out".into(),
                related_pid: Some(701),
                related_request_id: None,
            }],
            None,
            None,
        );

        assert!(
            payload.contains(
                "\"probe_summary\":\"[alive] probe: attached (installed: 2, failed: 1)\""
            ),
            "payload: {payload}"
        );
        assert!(payload.contains("\"errors\":"), "payload: {payload}");
        assert!(payload.contains("Probe timeout"), "payload: {payload}");
        assert!(
            payload.contains("probe heartbeat timed out"),
            "payload: {payload}"
        );
    }

    #[test]
    fn console_server_returns_health_api_payload() -> io::Result<()> {
        let snapshot = ConsoleSnapshot {
            summary: "summary".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![super::ConsoleTargetSummary {
                pid: 777,
                display_name: "node".into(),
                runtime_kind: "node".into(),
                attach_state: "attached".into(),
                probe_state_summary: "[alive] probe: attached (installed: 2, failed: 1)".into(),
            }],
            activity_items: vec![super::ConsoleActivityItem {
                activity_id: "error-1".into(),
                activity_type: "error".into(),
                occurred_at_ms: 50,
                title: "Probe timeout".into(),
                subtitle: "probe heartbeat timed out".into(),
                related_pid: Some(777),
                related_request_id: None,
            }],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        };
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let handle = thread::spawn(move || -> io::Result<()> {
            let (mut server_stream, _) = listener.accept()?;
            write_console_response(&mut server_stream, &snapshot)
        });

        let response = send_get_request(&addr.to_string(), "/api/health")?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        assert!(
            response.contains("Content-Type: application/json"),
            "response: {response}"
        );
        assert!(
            response.contains("\"probe_summary\""),
            "response: {response}"
        );
        assert!(response.contains("Probe timeout"), "response: {response}");

        handle.join().expect("server thread should join")?;
        Ok(())
    }

    #[test]
    fn console_server_returns_not_found_for_unknown_path() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());

        let mut stream = TcpStream::connect(addr)?;
        std::io::Write::write_all(
            &mut stream,
            b"GET /missing HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;

        assert!(
            response.starts_with("HTTP/1.1 404 Not Found"),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_targets_api_payload() -> io::Result<()> {
        let snapshot = ConsoleSnapshot {
            summary: "summary".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        };
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let handle = thread::spawn(move || -> io::Result<()> {
            let (mut server_stream, _) = listener.accept()?;
            write_console_response(&mut server_stream, &snapshot)
        });

        let response = send_get_request(&addr.to_string(), "/api/targets")?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        assert!(
            response.contains("Content-Type: application/json"),
            "response: {response}"
        );
        assert!(response.contains("\"targets\""), "response: {response}");

        handle.join().expect("server thread should join")?;
        Ok(())
    }

    #[test]
    fn console_server_returns_filtered_targets_api_empty_state_and_context() -> io::Result<()> {
        let snapshot = ConsoleSnapshot {
            summary: "summary".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: Some(super::ConsoleFilterContext {
                active_filters: vec!["opencode".into()],
                is_filtered_view: true,
            }),
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        };
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let handle = thread::spawn(move || -> io::Result<()> {
            let (mut server_stream, _) = listener.accept()?;
            write_console_response(&mut server_stream, &snapshot)
        });

        let response = send_get_request(&addr.to_string(), "/api/targets")?;

        assert!(
            response.contains("\"active_filters\":[\"opencode\"]"),
            "response: {response}"
        );
        assert!(
            response.contains("\"is_filtered_view\":true"),
            "response: {response}"
        );
        assert!(
            response.contains("当前过滤条件下没有匹配目标"),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        Ok(())
    }

    #[test]
    fn write_console_response_renders_target_summary_fields_from_controlled_snapshot()
    -> io::Result<()> {
        let snapshot = ConsoleSnapshot {
            summary: "summary".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![super::ConsoleTargetSummary {
                pid: 777,
                display_name: "node".into(),
                runtime_kind: "node".into(),
                attach_state: "attached".into(),
                probe_state_summary: "[alive] probe: attached (installed: 2, failed: 1)".into(),
            }],
            activity_items: vec![],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        };
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let handle = thread::spawn(move || -> io::Result<()> {
            let (mut server_stream, _) = listener.accept()?;
            write_console_response(&mut server_stream, &snapshot)
        });

        let response = send_get_request(&addr.to_string(), "/api/targets")?;

        assert!(
            response.contains("\"display_name\":\"node\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"attach_state\":\"attached\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"probe_state_summary\""),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        Ok(())
    }

    #[test]
    fn collect_target_summaries_marks_active_target_with_probe_health() -> io::Result<()> {
        let source = StaticProcessSampleSource::new(vec![
            ProcessSample {
                pid: 701,
                process_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                command_line: None,
            },
            ProcessSample {
                pid: 702,
                process_name: "Electron".into(),
                executable_path: PathBuf::from("/Applications/TestApp.app/Contents/MacOS/TestApp"),
                command_line: None,
            },
        ]);
        let active_session = AttachSession {
            target: ProcessTarget {
                pid: 701,
                app_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                command_line: None,
                runtime_kind: RuntimeKind::Node,
            },
            state: AttachSessionState::Attached,
            detail: "probe handshake completed".into(),
            bootstrap: None,
            failure: None,
        };
        let probe_health = ProbeHealth {
            state: ProbeState::Attached,
            installed_hooks: vec!["fetch".into(), "http".into()],
            failed_hooks: vec!["undici".into()],
        };

        let summaries =
            collect_target_summaries(&source, None, Some(&active_session), Some(&probe_health))?;

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].pid, 701);
        assert_eq!(summaries[0].attach_state, "attached");
        assert!(summaries[0].probe_state_summary.contains("installed: 2"));
        assert!(summaries[0].probe_state_summary.contains("failed: 1"));
        assert_eq!(summaries[1].attach_state, "idle");
        assert_eq!(summaries[1].probe_state_summary, "probe: no active session");
        Ok(())
    }

    #[test]
    fn collect_target_summaries_uses_no_health_data_for_active_target_without_probe_snapshot()
    -> io::Result<()> {
        let source = StaticProcessSampleSource::new(vec![ProcessSample {
            pid: 703,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            command_line: None,
        }]);
        let active_session = AttachSession {
            target: ProcessTarget {
                pid: 703,
                app_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                command_line: None,
                runtime_kind: RuntimeKind::Node,
            },
            state: AttachSessionState::Attached,
            detail: "probe handshake completed".into(),
            bootstrap: None,
            failure: None,
        };

        let summaries = collect_target_summaries(&source, None, Some(&active_session), None)?;

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].attach_state, "attached");
        assert_eq!(summaries[0].probe_state_summary, "probe: no health data");
        Ok(())
    }

    #[test]
    fn collect_activity_items_returns_empty_for_no_known_activity() {
        let items = collect_activity_items(super::ConsoleActivitySource {
            attach_session: None,
            attach_occurred_at_ms: None,
            probe_health: None,
            probe_occurred_at_ms: None,
            recent_requests: &[],
            known_errors: &[],
        });

        assert!(items.is_empty());

        let payload = render_activity_payload_from_items(&items, None);
        assert!(payload.contains("\"activity\":[]"), "payload: {payload}");
        assert!(payload.contains("尚无观测活动"), "payload: {payload}");
    }

    #[test]
    fn collect_activity_items_orders_attach_probe_request_and_error_by_time() {
        let active_session = AttachSession {
            target: ProcessTarget {
                pid: 801,
                app_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                command_line: None,
                runtime_kind: RuntimeKind::Node,
            },
            state: AttachSessionState::Attached,
            detail: "probe handshake completed".into(),
            bootstrap: None,
            failure: None,
        };
        let probe_health = ProbeHealth {
            state: ProbeState::Attached,
            installed_hooks: vec!["fetch".into()],
            failed_hooks: vec![],
        };
        let recent_requests = vec![ConsoleRecentRequestActivity {
            request_id: "req-1".into(),
            captured_at_ms: 40,
            title: "Captured request".into(),
            subtitle: "openai POST /v1/responses".into(),
            related_pid: Some(801),
        }];
        let known_errors = vec![ConsoleKnownErrorActivity {
            activity_id: "error-1".into(),
            occurred_at_ms: 50,
            title: "Probe timeout".into(),
            subtitle: "probe heartbeat timed out".into(),
            related_pid: Some(801),
        }];

        let items = collect_activity_items(super::ConsoleActivitySource {
            attach_session: Some(&active_session),
            attach_occurred_at_ms: Some(10),
            probe_health: Some(&probe_health),
            probe_occurred_at_ms: Some(20),
            recent_requests: &recent_requests,
            known_errors: &known_errors,
        });

        assert_eq!(items.len(), 4);
        assert_eq!(items[0].activity_type, "error");
        assert_eq!(items[1].activity_type, "request");
        assert_eq!(items[2].activity_type, "probe");
        assert_eq!(items[3].activity_type, "attach");
    }

    #[test]
    fn write_console_response_renders_activity_items_from_controlled_snapshot() -> io::Result<()> {
        let snapshot = ConsoleSnapshot {
            summary: "summary".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![],
            activity_items: vec![super::ConsoleActivityItem {
                activity_id: "probe-1".into(),
                activity_type: "probe".into(),
                occurred_at_ms: 20,
                title: "Probe online".into(),
                subtitle: "installed=1 failed=0".into(),
                related_pid: Some(801),
                related_request_id: None,
            }],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        };
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let handle = thread::spawn(move || -> io::Result<()> {
            let (mut server_stream, _) = listener.accept()?;
            write_console_response(&mut server_stream, &snapshot)
        });

        let response = send_get_request(&addr.to_string(), "/api/activity")?;

        assert!(
            response.contains("\"activity_type\":\"probe\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"title\":\"Probe online\""),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        Ok(())
    }

    #[test]
    fn console_server_returns_activity_api_payload() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000004000-42-1.json"),
            serde_json::json!({
                "event_id": "42-1714000004000-1",
                "pid": 42,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\",\"input\":\"hello\"}",
                "body_size_bytes": 34,
                "truncated": false,
                "captured_at_ms": 1714000004000u64,
            })
            .to_string(),
        )?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());

        let response = send_get_request(&addr, "/api/activity")?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        assert!(response.contains("\"activity\""), "response: {response}");
        assert!(
            response.contains("Captured openai request"),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_requests_api_payload() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(requests_dir.join("bad.json"), "{not-json")?;
        fs::write(
            requests_dir.join("1714000004000-42-1.json"),
            serde_json::json!({
                "event_id": "42-1714000004000-1",
                "pid": 42,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\",\"input\":\"hello\"}",
                "body_size_bytes": 34,
                "truncated": false,
                "captured_at_ms": 1714000004000u64,
            })
            .to_string(),
        )?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());

        let response = send_get_request(&addr, "/api/requests")?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        assert!(response.contains("\"requests\""), "response: {response}");
        assert!(
            response.contains("42-1714000004000-1"),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_favicon_without_not_found() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());

        let response = send_get_request(&addr, "/favicon.ico")?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        assert!(
            response.contains("Content-Type: image/x-icon"),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn load_request_summaries_reads_captured_request_artifacts() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000004000-42-1.json"),
            serde_json::json!({
                "event_id": "42-1714000004000-1",
                "pid": 42,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\",\"input\":\"hello\"}",
                "body_size_bytes": 34,
                "truncated": false,
                "captured_at_ms": 1714000004000u64,
            })
            .to_string(),
        )?;

        let summaries = load_request_summaries(&result.storage)?;

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].request_id, "42-1714000004000-1");
        assert_eq!(summaries[0].provider, "openai");
        assert_eq!(summaries[0].model.as_deref(), Some("gpt-4.1"));
        assert_eq!(summaries[0].target_display_name, "Codex");
        assert!(summaries[0].summary_text.contains("POST /v1/responses"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn load_session_summaries_groups_same_pid_even_with_interleaved_other_pid() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000001000-10-1.json"),
            serde_json::json!({
                "event_id": "10-1714000001000-1",
                "exchange_id": "ex-10-1",
                "pid": 10,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000001000u64,
            })
            .to_string(),
        )?;
        fs::write(
            requests_dir.join("1714000002000-20-1.json"),
            serde_json::json!({
                "event_id": "20-1714000002000-1",
                "exchange_id": "ex-20-1",
                "pid": 20,
                "target_display_name": "Claude",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "body_text": "{\"model\":\"claude-3-7-sonnet\"}",
                "body_size_bytes": 30,
                "truncated": false,
                "captured_at_ms": 1714000002000u64,
            })
            .to_string(),
        )?;
        fs::write(
            requests_dir.join("1714000003000-10-2.json"),
            serde_json::json!({
                "event_id": "10-1714000003000-2",
                "exchange_id": "ex-10-2",
                "pid": 10,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000003000u64,
            })
            .to_string(),
        )?;

        let sessions = load_session_summaries(&result.storage)?;

        assert_eq!(sessions.len(), 2);
        let codex = sessions
            .iter()
            .find(|session| session.pid == 10)
            .expect("codex session should exist");
        assert_eq!(codex.exchange_count, 2);

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn load_session_detail_splits_same_pid_after_time_window() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000001000-10-1.json"),
            serde_json::json!({
                "event_id": "10-1714000001000-1",
                "exchange_id": "ex-10-1",
                "pid": 10,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000001000u64,
            })
            .to_string(),
        )?;
        fs::write(
            requests_dir.join("1714000301001-10-2.json"),
            serde_json::json!({
                "event_id": "10-1714000301001-2",
                "exchange_id": "ex-10-2",
                "pid": 10,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000301001u64,
            })
            .to_string(),
        )?;

        let sessions = load_session_summaries(&result.storage)?;

        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().all(|session| session.exchange_count == 1));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn load_session_summaries_do_not_merge_when_only_response_finishes_within_window()
    -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        let responses_dir = result.storage.artifacts_dir.join("responses");
        fs::create_dir_all(&requests_dir)?;
        fs::create_dir_all(&responses_dir)?;
        fs::write(
            requests_dir.join("1714000001000-10-1.json"),
            serde_json::json!({
                "event_id": "10-1714000001000-1",
                "exchange_id": "ex-10-1",
                "pid": 10,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000001000u64,
            })
            .to_string(),
        )?;
        fs::write(
            responses_dir.join("1714000360000-10-2.json"),
            serde_json::json!({
                "event_id": "10-1714000360000-2",
                "exchange_id": "ex-10-1",
                "pid": 10,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "status_code": 200,
                "headers": [],
                "body_text": "{\"output\":[]}",
                "body_size_bytes": 13,
                "truncated": false,
                "started_at_ms": 1714000002000u64,
                "completed_at_ms": 1714000360000u64,
                "duration_ms": 359000u64
            })
            .to_string(),
        )?;
        fs::write(
            requests_dir.join("1714000361000-10-3.json"),
            serde_json::json!({
                "event_id": "10-1714000361000-3",
                "exchange_id": "ex-10-2",
                "pid": 10,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000361000u64,
            })
            .to_string(),
        )?;

        let sessions = load_session_summaries(&result.storage)?;

        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().all(|session| session.exchange_count == 1));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn load_request_detail_returns_base_detail_for_existing_request() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "event_id": "77-1714000005000-1",
                "exchange_id": "ex-77",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "headers": [
                    { "name": "content-type", "value": "application/json" }
                ],
                "body_text": "{\"model\":\"claude-3-7-sonnet\",\"messages\":[]}",
                "body_size_bytes": 48,
                "truncated": false,
                "captured_at_ms": 1714000005000u64,
            })
            .to_string(),
        )?;
        let responses_dir = result.storage.artifacts_dir.join("responses");
        fs::create_dir_all(&responses_dir)?;
        fs::write(
            responses_dir.join("1714000005100-77-2.json"),
            serde_json::json!({
                "event_id": "77-1714000005100-2",
                "exchange_id": "ex-77",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "status_code": 200,
                "headers": [
                    { "name": "content-type", "value": "application/json" }
                ],
                "body_text": "{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}",
                "body_size_bytes": 42,
                "truncated": false,
                "started_at_ms": 1714000005050u64,
                "completed_at_ms": 1714000005100u64,
                "duration_ms": 50u64
            })
            .to_string(),
        )?;
        let tool_visibility_dir = result.storage.artifacts_dir.join("tool_visibility");
        fs::create_dir_all(&tool_visibility_dir)?;
        fs::write(
            tool_visibility_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "request_id": "77-1714000005000-1",
                "exchange_id": "ex-77",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "captured_at_ms": 1714000005000u64,
                "visibility_stage": "request-embedded",
                "tool_choice": "auto",
                "final_tools_json": [
                    {
                        "name": "run_command",
                        "type": "function"
                    }
                ],
                "tool_count_final": 1
            })
            .to_string(),
        )?;

        let detail = load_request_detail(&result.storage, "77-1714000005000-1")?
            .expect("detail should exist");

        assert_eq!(detail.request_id, "77-1714000005000-1");
        assert_eq!(detail.exchange_id.as_deref(), Some("ex-77"));
        assert_eq!(detail.provider, "anthropic");
        assert_eq!(detail.model.as_deref(), Some("claude-3-7-sonnet"));
        assert_eq!(detail.target_display_name, "NodeApp");
        assert!(detail.request_summary.contains("POST /v1/messages"));
        assert_eq!(detail.method, "POST");
        assert_eq!(detail.headers.len(), 1);
        assert_eq!(
            detail.body_text.as_deref(),
            Some("{\"model\":\"claude-3-7-sonnet\",\"messages\":[]}")
        );
        assert!(detail.artifact_path.ends_with("1714000005000-77-1.json"));
        assert_eq!(
            detail
                .response
                .as_ref()
                .map(|response| response.status_code),
            Some(200)
        );
        assert_eq!(
            detail
                .response
                .as_ref()
                .map(|response| response.duration_ms),
            Some(50)
        );
        assert_eq!(
            detail
                .tool_visibility
                .as_ref()
                .map(|visibility| visibility.tool_count_final),
            Some(1)
        );
        assert_eq!(
            detail
                .tool_visibility
                .as_ref()
                .and_then(|visibility| visibility.tool_choice.as_deref()),
            Some("auto")
        );
        assert_eq!(
            detail
                .tool_visibility
                .as_ref()
                .and_then(|visibility| visibility.final_tools.first())
                .map(|tool| tool.name.as_str()),
            Some("run_command")
        );

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn load_request_detail_prefers_exact_tool_visibility_request_match() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "event_id": "demo-request",
                "exchange_id": "ex-demo",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "headers": [],
                "body_text": "{\"model\":\"claude-3-7-sonnet\",\"messages\":[]}",
                "body_size_bytes": 48,
                "truncated": false,
                "captured_at_ms": 1714000005000u64,
            })
            .to_string(),
        )?;
        let tool_visibility_dir = result.storage.artifacts_dir.join("tool_visibility");
        fs::create_dir_all(&tool_visibility_dir)?;
        fs::write(
            tool_visibility_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "request_id": "demo-request",
                "exchange_id": "ex-demo",
                "captured_at_ms": 1714000005000u64,
                "visibility_stage": "request-embedded",
                "tool_choice": "auto",
                "final_tools_json": [
                    { "type": "function", "function": { "name": "exact_request_tool" } }
                ],
                "tool_count_final": 1
            })
            .to_string(),
        )?;
        fs::write(
            tool_visibility_dir.join("1714000006000-77-2.json"),
            serde_json::json!({
                "request_id": "different-request",
                "exchange_id": "ex-demo",
                "captured_at_ms": 1714000006000u64,
                "visibility_stage": "request-embedded",
                "tool_choice": "auto",
                "final_tools_json": [
                    { "type": "function", "function": { "name": "exchange_only_tool" } }
                ],
                "tool_count_final": 1
            })
            .to_string(),
        )?;

        let detail =
            load_request_detail(&result.storage, "demo-request")?.expect("detail should exist");

        assert_eq!(
            detail
                .tool_visibility
                .as_ref()
                .and_then(|visibility| visibility.final_tools.first())
                .map(|tool| tool.name.as_str()),
            Some("exact_request_tool")
        );

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_request_detail_api_payload() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "event_id": "demo-request",
                "exchange_id": "ex-demo",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "headers": [
                    { "name": "content-type", "value": "application/json" }
                ],
                "body_text": "{\"model\":\"claude-3-7-sonnet\",\"messages\":[]}",
                "body_size_bytes": 48,
                "truncated": false,
                "captured_at_ms": 1714000005000u64,
            })
            .to_string(),
        )?;
        let responses_dir = result.storage.artifacts_dir.join("responses");
        fs::create_dir_all(&responses_dir)?;
        fs::write(
            responses_dir.join("1714000005100-77-2.json"),
            serde_json::json!({
                "event_id": "demo-response",
                "exchange_id": "ex-demo",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "status_code": 200,
                "headers": [
                    { "name": "content-type", "value": "application/json" }
                ],
                "body_text": "{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}",
                "body_size_bytes": 42,
                "truncated": false,
                "started_at_ms": 1714000005050u64,
                "completed_at_ms": 1714000005100u64,
                "duration_ms": 50u64
            })
            .to_string(),
        )?;
        let tool_visibility_dir = result.storage.artifacts_dir.join("tool_visibility");
        fs::create_dir_all(&tool_visibility_dir)?;
        fs::write(
            tool_visibility_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "request_id": "demo-request",
                "exchange_id": "ex-demo",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "captured_at_ms": 1714000005000u64,
                "visibility_stage": "request-embedded",
                "tool_choice": "auto",
                "final_tools_json": [
                    {
                        "type": "function",
                        "function": {
                            "name": "list_files"
                        }
                    }
                ],
                "tool_count_final": 1
            })
            .to_string(),
        )?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());

        let response = send_get_request(&addr, "/api/requests/demo-request")?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        assert!(response.contains("\"request\""), "response: {response}");
        assert!(response.contains("demo-request"), "response: {response}");
        assert!(
            response.contains("claude-3-7-sonnet"),
            "response: {response}"
        );
        assert!(
            response.contains("\"exchange_id\":\"ex-demo\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"status_code\":200"),
            "response: {response}"
        );
        assert!(
            response.contains("\"body_text\":\"{\\\"content\\\":[{\\\"type\\\":\\\"text\\\",\\\"text\\\":\\\"hi\\\"}]}\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"tool_visibility\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"tool_count_final\":1"),
            "response: {response}"
        );
        assert!(
            response.contains("\"tool_choice\":\"auto\""),
            "response: {response}"
        );
        assert!(response.contains("list_files"), "response: {response}");

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_session_detail_api_payload() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "event_id": "demo-request",
                "exchange_id": "ex-demo",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "headers": [],
                "body_text": "{\"model\":\"claude-3-7-sonnet\",\"messages\":[]}",
                "body_size_bytes": 48,
                "truncated": false,
                "captured_at_ms": 1714000005000u64,
            })
            .to_string(),
        )?;
        let responses_dir = result.storage.artifacts_dir.join("responses");
        fs::create_dir_all(&responses_dir)?;
        fs::write(
            responses_dir.join("1714000005100-77-2.json"),
            serde_json::json!({
                "event_id": "demo-response",
                "exchange_id": "ex-demo",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "status_code": 200,
                "headers": [],
                "body_text": "{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}",
                "body_size_bytes": 42,
                "truncated": false,
                "started_at_ms": 1714000005050u64,
                "completed_at_ms": 1714000005100u64,
                "duration_ms": 50u64
            })
            .to_string(),
        )?;
        let tool_visibility_dir = result.storage.artifacts_dir.join("tool_visibility");
        fs::create_dir_all(&tool_visibility_dir)?;
        fs::write(
            tool_visibility_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "request_id": "demo-request",
                "exchange_id": "ex-demo",
                "captured_at_ms": 1714000005000u64,
                "visibility_stage": "request-embedded",
                "tool_choice": "auto",
                "final_tools_json": [
                    {
                        "type": "function",
                        "function": { "name": "list_files" }
                    }
                ],
                "tool_count_final": 1
            })
            .to_string(),
        )?;

        let detail = load_session_detail(&result.storage, "77-1714000005000-1")?
            .expect("session detail should exist");
        assert_eq!(detail.exchange_count, 1);

        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());

        let response = send_get_request(&addr, "/api/sessions/77-1714000005000-1")?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        assert!(response.contains("\"session\""), "response: {response}");
        assert!(
            response.contains("\"session_id\":\"77-1714000005000-1\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"request_id\":\"demo-request\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"response_status\":200"),
            "response: {response}"
        );
        assert!(
            response.contains("\"tool_count_final\":1"),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_filtered_sessions_api_without_unmatched_sessions() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000001000-10-1.json"),
            serde_json::json!({
                "event_id": "10-1714000001000-1",
                "exchange_id": "ex-opencode",
                "pid": 10,
                "target_display_name": "opencode",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000001000u64,
            })
            .to_string(),
        )?;
        fs::write(
            requests_dir.join("1714000002000-20-1.json"),
            serde_json::json!({
                "event_id": "20-1714000002000-1",
                "exchange_id": "ex-codex",
                "pid": 20,
                "target_display_name": "codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000002000u64,
            })
            .to_string(),
        )?;

        let filter = super::ConsoleTargetFilterConfig::new(vec!["opencode".into()]);
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", Some(&filter))?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());
        let response = send_get_request(&addr, "/api/sessions")?;

        assert!(response.contains("\"active_filters\":[\"opencode\"]"), "response: {response}");
        assert!(response.contains("\"target_display_name\":\"opencode\""), "response: {response}");
        assert!(
            !response.contains("\"target_display_name\":\"codex\""),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_filtered_session_detail_does_not_leak_unmatched_session() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "event_id": "demo-request",
                "exchange_id": "ex-demo",
                "pid": 77,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "headers": [],
                "body_text": "{\"model\":\"gpt-4.1\"}",
                "body_size_bytes": 19,
                "truncated": false,
                "captured_at_ms": 1714000005000u64,
            })
            .to_string(),
        )?;

        let filter = super::ConsoleTargetFilterConfig::new(vec!["opencode".into()]);
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", Some(&filter))?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());
        let response = send_get_request(&addr, "/api/sessions/77-1714000005000-1")?;

        assert!(response.starts_with("HTTP/1.1 200 OK"), "response: {response}");
        assert!(response.contains("\"status\":\"not_found\""), "response: {response}");
        assert!(response.contains("\"active_filters\":[\"opencode\"]"), "response: {response}");
        assert!(
            !response.contains("\"target_display_name\":\"Codex\""),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_filtered_request_detail_does_not_leak_unmatched_request() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "event_id": "demo-request",
                "exchange_id": "ex-demo",
                "pid": 77,
                "target_display_name": "Codex",
                "provider_hint": "openai",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.openai.com/v1/responses",
                "headers": [],
                "body_text": "{\"model\":\"gpt-4.1\",\"input\":\"hello\"}",
                "body_size_bytes": 34,
                "truncated": false,
                "captured_at_ms": 1714000005000u64,
            })
            .to_string(),
        )?;

        let filter = super::ConsoleTargetFilterConfig::new(vec!["opencode".into()]);
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", Some(&filter))?;
        let addr = server
            .local_url()?
            .trim_start_matches("http://")
            .to_string();

        let handle = thread::spawn(move || server.serve_once());

        let response = send_get_request(&addr, "/api/requests/demo-request")?;

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response: {response}"
        );
        assert!(
            response.contains("\"status\":\"not_found\""),
            "response: {response}"
        );
        assert!(
            response.contains("\"active_filters\":[\"opencode\"]"),
            "response: {response}"
        );
        assert!(
            !response.contains("\"provider\":\"openai\""),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn malformed_request_returns_bad_request() -> io::Result<()> {
        let snapshot = ConsoleSnapshot {
            summary: "summary".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            filter_context: None,
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            session_summaries: vec![],
            request_details: vec![],
            session_details: vec![],
        };
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let handle = thread::spawn(move || -> io::Result<String> {
            let (mut server_stream, _) = listener.accept()?;
            write_console_response(&mut server_stream, &snapshot)?;
            Ok(String::new())
        });

        let mut client_stream = TcpStream::connect(addr)?;
        std::io::Write::write_all(&mut client_stream, b"POST / HTTP/1.1\r\n\r\n")?;
        let mut response = String::new();
        client_stream.read_to_string(&mut response)?;

        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request"),
            "response: {response}"
        );

        handle.join().expect("server thread should join")?;
        Ok(())
    }

    #[test]
    fn read_request_path_parses_http_get_requests() -> io::Result<()> {
        let mut cursor = Cursor::new(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec());

        let path = read_request_path_from_reader(&mut cursor)?;

        assert_eq!(path.as_deref(), Some("/"));
        Ok(())
    }

    fn send_get_request(addr: &str, path: &str) -> io::Result<String> {
        let mut stream = TcpStream::connect(addr)?;
        std::io::Write::write_all(
            &mut stream,
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        Ok(response)
    }

    #[test]
    fn run_console_server_writes_startup_report_before_serving() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let occupied = TcpListener::bind("127.0.0.1:0")?;
        let mut result = bootstrap(&workspace_root)?;
        result.config.bind_addr = occupied.local_addr()?.to_string();
        let mut output = Vec::new();

        let error = run_console_server(&result, &mut output)
            .expect_err("occupied default port should fail before serving");

        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
        assert!(
            output.is_empty(),
            "startup report should not print on bind failure"
        );

        drop(occupied);
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let sequence = UNIQUE_TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);

        std::env::temp_dir().join(format!(
            "prismtrace-console-test-{}-{}-{}",
            process::id(),
            nanos,
            sequence
        ))
    }
}
