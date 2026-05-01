use prismtrace_core::ProcessTarget;
use std::path::PathBuf;

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
    pub source_state: String,
    pub source_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleTargetFilterConfig {
    pub(super) terms: Vec<String>,
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

pub(crate) fn console_filter_context(
    filter: Option<&ConsoleTargetFilterConfig>,
) -> Option<ConsoleFilterContext> {
    filter
        .filter(|filter| filter.is_enabled())
        .map(|filter| ConsoleFilterContext {
            active_filters: filter.terms.clone(),
            is_filtered_view: true,
        })
}

pub(crate) fn filter_request_summaries(
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

pub(crate) fn filter_session_summaries(
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
    pub title: String,
    pub subtitle: String,
    pub cwd: Option<String>,
    pub artifact_path: Option<String>,
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
