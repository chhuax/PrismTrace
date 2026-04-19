use crate::BootstrapResult;
use crate::discovery::{ProcessSampleSource, PsProcessSampleSource, discover_targets};
use crate::probe_health::ProbeHealthStore;
use prismtrace_core::{AttachSession, ProbeHealth, ProcessTarget};
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleSnapshot {
    pub summary: String,
    pub bind_addr: String,
    pub target_summaries: Vec<ConsoleTargetSummary>,
    pub activity_items: Vec<ConsoleActivityItem>,
    pub request_summaries: Vec<ConsoleRequestSummary>,
    pub request_details: Vec<ConsoleRequestDetail>,
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
pub struct ConsoleRequestDetail {
    pub request_id: String,
    pub captured_at_ms: u64,
    pub provider: String,
    pub model: Option<String>,
    pub target_display_name: String,
    pub artifact_path: String,
    pub request_summary: String,
    pub probe_context: Option<String>,
}

#[derive(Debug, Clone)]
struct RequestArtifactRecord {
    request_id: String,
    captured_at_ms: u64,
    provider: String,
    model: Option<String>,
    target_display_name: String,
    method: String,
    url: String,
    artifact_path: PathBuf,
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
        write_console_response(&mut stream, &self.snapshot)
    }

    pub fn serve_forever(&self) -> io::Result<()> {
        loop {
            let (mut stream, _) = self.listener.accept()?;
            write_console_response(&mut stream, &self.snapshot)?;
        }
    }
}

pub fn collect_console_snapshot(result: &BootstrapResult) -> ConsoleSnapshot {
    let target_summaries =
        collect_target_summaries(&PsProcessSampleSource, None, None).unwrap_or_else(|_| Vec::new());

    ConsoleSnapshot {
        summary: crate::startup_summary(result),
        bind_addr: format!("http://{}", result.config.bind_addr),
        target_summaries,
        activity_items: collect_activity_items(ConsoleActivitySource {
            attach_session: None,
            attach_occurred_at_ms: None,
            probe_health: None,
            probe_occurred_at_ms: None,
            recent_requests: &[],
            known_errors: &[],
        }),
        request_summaries: load_request_summaries(&result.storage).unwrap_or_else(|_| Vec::new()),
        request_details: load_request_details(&result.storage).unwrap_or_else(|_| Vec::new()),
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
    start_console_server_on_bind_addr(result, &result.config.bind_addr)
}

pub fn run_console_server(result: &BootstrapResult, output: &mut impl Write) -> io::Result<()> {
    let server = start_console_server(result)?;
    writeln!(output, "{}", console_startup_report(server.snapshot()))?;
    server.serve_forever()
}

pub fn start_console_server_on_bind_addr(
    result: &BootstrapResult,
    bind_addr: &str,
) -> io::Result<ConsoleServer> {
    let listener = TcpListener::bind(bind_addr)?;
    let local_addr = listener.local_addr()?;

    Ok(ConsoleServer {
        listener,
        snapshot: ConsoleSnapshot {
            summary: crate::startup_summary(result),
            bind_addr: format!("http://{local_addr}"),
            target_summaries: collect_target_summaries(&PsProcessSampleSource, None, None)
                .unwrap_or_else(|_| Vec::new()),
            activity_items: collect_activity_items(ConsoleActivitySource {
                attach_session: None,
                attach_occurred_at_ms: None,
                probe_health: None,
                probe_occurred_at_ms: None,
                recent_requests: &[],
                known_errors: &[],
            }),
            request_summaries: load_request_summaries(&result.storage)
                .unwrap_or_else(|_| Vec::new()),
            request_details: load_request_details(&result.storage).unwrap_or_else(|_| Vec::new()),
        },
    })
}

fn write_console_response(stream: &mut TcpStream, snapshot: &ConsoleSnapshot) -> io::Result<()> {
    let request_path = read_request_path(stream)?;
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
            render_targets_payload_from_summaries(&snapshot.target_summaries),
        ),
        Some("/api/activity") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_activity_payload_from_items(&snapshot.activity_items),
        ),
        Some("/api/requests") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_requests_payload(&snapshot.request_summaries),
        ),
        Some("/api/health") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_health_payload(&snapshot.target_summaries, &snapshot.activity_items),
        ),
        Some(path) if path.starts_with("/api/requests/") => (
            "HTTP/1.1 200 OK",
            "application/json; charset=utf-8",
            render_request_detail_payload(
                path.trim_start_matches("/api/requests/"),
                &snapshot.request_details,
            ),
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

    let response = format!(
        "{status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()
}

fn read_request_path(stream: &mut TcpStream) -> io::Result<Option<String>> {
    read_request_path_from_reader(stream)
}

fn render_console_homepage(snapshot: &ConsoleSnapshot) -> String {
    let targets_html = render_targets_panel_items(&snapshot.target_summaries);
    let activity_html = render_activity_panel_items(&snapshot.activity_items);
    let requests_html = render_requests_panel_items(&snapshot.request_summaries);
    let request_detail_html = render_request_detail_panel(snapshot.request_details.first());
    let health_html = render_health_panel(&snapshot.target_summaries, &snapshot.activity_items);
    let script = render_console_homepage_script(snapshot.request_summaries.first());

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
      </main>
      <section class=\"console-secondary-layout\">
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
        targets_html,
        activity_html,
        requests_html,
        request_detail_html,
        health_html,
        script
    )
}

fn render_console_homepage_script(initial_request: Option<&ConsoleRequestSummary>) -> String {
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

      const renderRequestDetail = (payload) => {
        const request = payload.request;
        if (!request || request.status === 'not_found') {
          return renderEmptyState(request?.detail || 'request detail is not available yet');
        }

        return `<div class="console-detail-grid">
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
            <p class="console-detail-label">Artifact Path</p>
            <p><code>${escapeHtml(request.artifact_path)}</code></p>
          </div>
          <div class="console-detail-row">
            <p class="console-detail-label">Probe Context</p>
            <p>${escapeHtml(request.probe_context || '暂无 probe context')}</p>
          </div>
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

      document.addEventListener('click', (event) => {
        const trigger = event.target.closest('[data-request-detail-trigger]');
        if (!trigger) return;
        void refreshRequestDetail(trigger.getAttribute('data-request-detail-trigger'));
      });

      void refreshRegion("/api/targets", "targets-region", renderTargets);
      void refreshRegion("/api/activity", "activity-region", renderActivity);
      void refreshRegion("/api/requests", "requests-region", renderRequests);
      void refreshRegion("/api/health", "health-region", renderHealth);
    "#
    .to_string();

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

fn render_targets_panel_items(targets: &[ConsoleTargetSummary]) -> String {
    if targets.is_empty() {
        return render_console_empty_state("尚无可观测目标");
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

fn render_activity_panel_items(items: &[ConsoleActivityItem]) -> String {
    if items.is_empty() {
        return render_console_empty_state("尚无观测活动");
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

fn render_requests_panel_items(requests: &[ConsoleRequestSummary]) -> String {
    if requests.is_empty() {
        return render_console_empty_state("尚无请求记录");
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

fn render_console_empty_state(text: &str) -> String {
    format!(
        "<p class=\"muted console-placeholder\">{}</p>",
        escape_html(text)
    )
}

fn render_request_detail_panel(detail: Option<&ConsoleRequestDetail>) -> String {
    match detail {
        Some(detail) => format!(
            "<div class=\"console-detail-grid\"><div class=\"console-detail-row\"><p class=\"console-detail-label\">Request Summary</p><p class=\"console-list-title\">{}</p></div><div class=\"console-detail-row\"><p class=\"console-detail-label\">Target</p><p>{}</p></div><div class=\"console-detail-row\"><p class=\"console-detail-label\">Provider / Model</p><p>{} · {}</p></div><div class=\"console-detail-row\"><p class=\"console-detail-label\">Artifact Path</p><p><code>{}</code></p></div><div class=\"console-detail-row\"><p class=\"console-detail-label\">Probe Context</p><p>{}</p></div></div>",
            escape_html(&detail.request_summary),
            escape_html(&detail.target_display_name),
            escape_html(&detail.provider),
            escape_html(detail.model.as_deref().unwrap_or("unknown")),
            escape_html(&detail.artifact_path),
            escape_html(
                detail
                    .probe_context
                    .as_deref()
                    .unwrap_or("暂无 probe context")
            )
        ),
        None => render_console_empty_state("请选择一条 request 查看基础详情"),
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
) -> String {
    let probe_summary = targets
        .iter()
        .find(|target| target.attach_state != "idle")
        .map(|target| target.probe_state_summary.clone())
        .or_else(|| {
            targets
                .first()
                .map(|target| target.probe_state_summary.clone())
        });

    let errors = activity_items
        .iter()
        .filter(|item| item.activity_type == "error")
        .map(|item| {
            json!({
                "title": item.title,
                "subtitle": item.subtitle,
                "related_pid": item.related_pid,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "probe_summary": probe_summary,
        "errors": errors,
        "empty_state": if probe_summary.is_none() && errors.is_empty() { Some("尚未发现 probe 健康或错误提示") } else { None::<&str> }
    })
    .to_string()
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

fn render_targets_payload_from_summaries(targets: &[ConsoleTargetSummary]) -> String {
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

    json!({
        "targets": targets,
        "empty_state": if targets.is_empty() { Some("尚无可观测目标") } else { None::<&str> }
    })
    .to_string()
}

pub fn collect_target_summaries(
    source: &impl ProcessSampleSource,
    active_session: Option<&AttachSession>,
    probe_health: Option<&ProbeHealth>,
) -> io::Result<Vec<ConsoleTargetSummary>> {
    let discovered_targets = discover_targets(source)?;

    Ok(discovered_targets
        .iter()
        .map(|target| summarize_target(target, active_session, probe_health))
        .collect())
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

fn render_activity_payload_from_items(items: &[ConsoleActivityItem]) -> String {
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

    json!({
        "activity": activity,
        "empty_state": if items.is_empty() { Some("尚无观测活动") } else { None::<&str> }
    })
    .to_string()
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

pub fn load_request_detail(
    storage: &StorageLayout,
    request_id: &str,
) -> io::Result<Option<ConsoleRequestDetail>> {
    Ok(load_request_details(storage)?
        .into_iter()
        .find(|detail| detail.request_id == request_id))
}

fn load_request_details(storage: &StorageLayout) -> io::Result<Vec<ConsoleRequestDetail>> {
    let mut details = load_request_records(storage)?
        .into_iter()
        .map(|record| {
            let request_summary = format!(
                "{} {} {}",
                record.provider,
                record.method,
                request_path_only(&record.url)
            );

            ConsoleRequestDetail {
                request_id: record.request_id,
                captured_at_ms: record.captured_at_ms,
                provider: record.provider,
                model: record.model,
                target_display_name: record.target_display_name,
                artifact_path: record.artifact_path.display().to_string(),
                request_summary,
                probe_context: None,
            }
        })
        .collect::<Vec<_>>();

    details.sort_by(|left, right| {
        right
            .captured_at_ms
            .cmp(&left.captured_at_ms)
            .then_with(|| left.request_id.cmp(&right.request_id))
    });

    Ok(details)
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

        if let Some(record) = read_request_record(&path)? {
            records.push(record);
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
    let model = value
        .get("body_text")
        .and_then(Value::as_str)
        .and_then(extract_model_from_body_text);

    Ok(Some(RequestArtifactRecord {
        request_id: request_id.to_string(),
        captured_at_ms,
        provider,
        model,
        target_display_name,
        method,
        url,
        artifact_path: path.to_path_buf(),
    }))
}

fn extract_model_from_body_text(body_text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body_text).ok()?;
    value
        .get("model")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn render_requests_payload(requests: &[ConsoleRequestSummary]) -> String {
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

    json!({
        "requests": requests,
        "empty_state": if requests.is_empty() { Some("尚无请求记录") } else { None::<&str> }
    })
    .to_string()
}

fn render_request_detail_payload(request_id: &str, details: &[ConsoleRequestDetail]) -> String {
    let detail = details
        .iter()
        .find(|detail| detail.request_id == request_id);
    match detail {
        Some(detail) => json!({
            "request": {
                "request_id": detail.request_id,
                "captured_at_ms": detail.captured_at_ms,
                "provider": detail.provider,
                "model": detail.model,
                "target_display_name": detail.target_display_name,
                "artifact_path": detail.artifact_path,
                "request_summary": detail.request_summary,
                "probe_context": detail.probe_context,
            }
        })
        .to_string(),
        None => json!({
            "request": {
                "request_id": request_id,
                "status": "not_found",
                "detail": "request detail is not available yet"
            }
        })
        .to_string(),
    }
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
        ConsoleKnownErrorActivity, ConsoleRecentRequestActivity, ConsoleSnapshot,
        collect_activity_items, collect_target_summaries, load_request_detail,
        load_request_summaries, read_request_path_from_reader, render_activity_payload_from_items,
        run_console_server, start_console_server_on_bind_addr, write_console_response,
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
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn start_console_server_returns_addr_in_use_when_bind_fails() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let occupied = TcpListener::bind("127.0.0.1:0")?;
        let addr = occupied.local_addr()?;

        let error = start_console_server_on_bind_addr(&result, &addr.to_string())
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
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0")?;
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
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            request_details: vec![],
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
            request_details: vec![],
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
        let payload = super::render_targets_payload_from_summaries(&[]);

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
            request_details: vec![super::ConsoleRequestDetail {
                request_id: "req-1".into(),
                captured_at_ms: 30,
                provider: "openai".into(),
                model: Some("gpt-4.1".into()),
                target_display_name: "Codex".into(),
                artifact_path: "/tmp/request.json".into(),
                request_summary: "openai POST /v1/responses".into(),
                probe_context: Some("fetch hook".into()),
            }],
        });

        assert!(homepage.contains("Request Detail"), "homepage: {homepage}");
        assert!(
            homepage.contains("id=\"request-detail-region\""),
            "homepage: {homepage}"
        );
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
            request_details: vec![],
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
        let payload = super::render_request_detail_payload("missing-request", &[]);

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
            request_details: vec![],
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
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0")?;
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
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            request_details: vec![],
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
    fn write_console_response_renders_target_summary_fields_from_controlled_snapshot()
    -> io::Result<()> {
        let snapshot = ConsoleSnapshot {
            summary: "summary".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            target_summaries: vec![super::ConsoleTargetSummary {
                pid: 777,
                display_name: "node".into(),
                runtime_kind: "node".into(),
                attach_state: "attached".into(),
                probe_state_summary: "[alive] probe: attached (installed: 2, failed: 1)".into(),
            }],
            activity_items: vec![],
            request_summaries: vec![],
            request_details: vec![],
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
            },
            ProcessSample {
                pid: 702,
                process_name: "Electron".into(),
                executable_path: PathBuf::from("/Applications/TestApp.app/Contents/MacOS/TestApp"),
            },
        ]);
        let active_session = AttachSession {
            target: ProcessTarget {
                pid: 701,
                app_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
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
            collect_target_summaries(&source, Some(&active_session), Some(&probe_health))?;

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
        }]);
        let active_session = AttachSession {
            target: ProcessTarget {
                pid: 703,
                app_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                runtime_kind: RuntimeKind::Node,
            },
            state: AttachSessionState::Attached,
            detail: "probe handshake completed".into(),
            bootstrap: None,
            failure: None,
        };

        let summaries = collect_target_summaries(&source, Some(&active_session), None)?;

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

        let payload = render_activity_payload_from_items(&items);
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
            request_details: vec![],
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
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0")?;
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

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_requests_api_payload() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0")?;
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

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_favicon_without_not_found() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0")?;
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
    fn load_request_detail_returns_base_detail_for_existing_request() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let requests_dir = result.storage.artifacts_dir.join("requests");
        fs::create_dir_all(&requests_dir)?;
        fs::write(
            requests_dir.join("1714000005000-77-1.json"),
            serde_json::json!({
                "event_id": "77-1714000005000-1",
                "pid": 77,
                "target_display_name": "NodeApp",
                "provider_hint": "anthropic",
                "hook_name": "fetch",
                "method": "POST",
                "url": "https://api.anthropic.com/v1/messages",
                "body_text": "{\"model\":\"claude-3-7-sonnet\",\"messages\":[]}",
                "body_size_bytes": 48,
                "truncated": false,
                "captured_at_ms": 1714000005000u64,
            })
            .to_string(),
        )?;

        let detail = load_request_detail(&result.storage, "77-1714000005000-1")?
            .expect("detail should exist");

        assert_eq!(detail.request_id, "77-1714000005000-1");
        assert_eq!(detail.provider, "anthropic");
        assert_eq!(detail.model.as_deref(), Some("claude-3-7-sonnet"));
        assert_eq!(detail.target_display_name, "NodeApp");
        assert!(detail.request_summary.contains("POST /v1/messages"));
        assert!(detail.artifact_path.ends_with("1714000005000-77-1.json"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_server_returns_request_detail_api_payload() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0")?;
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

        handle.join().expect("server thread should join")?;
        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn malformed_request_returns_bad_request() -> io::Result<()> {
        let snapshot = ConsoleSnapshot {
            summary: "summary".into(),
            bind_addr: "http://127.0.0.1:7799".into(),
            target_summaries: vec![],
            activity_items: vec![],
            request_summaries: vec![],
            request_details: vec![],
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

        std::env::temp_dir().join(format!(
            "prismtrace-console-test-{}-{}",
            process::id(),
            nanos
        ))
    }
}
