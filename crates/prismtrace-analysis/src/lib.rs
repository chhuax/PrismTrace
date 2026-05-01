use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRawRef {
    pub path: PathBuf,
    pub line_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityProjection {
    pub capability_id: String,
    pub session_id: String,
    pub event_id: String,
    pub source_kind: String,
    pub capability_type: String,
    pub capability_name: String,
    pub visibility_stage: String,
    pub observed_at_ms: u64,
    pub raw_ref: CapabilityRawRef,
}

pub struct EventCapabilityInput<'a> {
    pub session_id: &'a str,
    pub event_id: &'a str,
    pub source_kind: &'a str,
    pub event_kind: &'a str,
    pub summary: &'a str,
    pub observed_at_ms: u64,
    pub raw_ref: CapabilityRawRef,
    pub raw_json: &'a Value,
    pub detail_json: &'a Value,
}

pub struct ToolVisibilityCapabilityInput<'a> {
    pub session_id: &'a str,
    pub event_id: &'a str,
    pub source_kind: &'a str,
    pub observed_at_ms: u64,
    pub visibility_stage: &'a str,
    pub raw_ref: CapabilityRawRef,
    pub final_tools_json: &'a Value,
}

pub struct PromptEventInput<'a> {
    pub session_id: &'a str,
    pub event_id: &'a str,
    pub event_kind: &'a str,
    pub summary: &'a str,
    pub occurred_at_ms: u64,
    pub raw_ref: CapabilityRawRef,
    pub raw_json: &'a Value,
    pub detail_json: &'a Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptProjection {
    pub session_id: String,
    pub event_id: String,
    pub role: String,
    pub text: String,
    pub observed_at_ms: u64,
    pub raw_ref: CapabilityRawRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptDiff {
    pub session_id: String,
    pub from_event_id: String,
    pub to_event_id: String,
    pub added_lines: Vec<String>,
    pub removed_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRenameCandidate {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityVisibilityDiff {
    pub session_id: String,
    pub from_event_id: String,
    pub to_event_id: String,
    pub capability_type: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub hidden: Vec<String>,
    pub rename_candidates: Vec<CapabilityRenameCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillDiagnosticStatus {
    Available,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDiagnostic {
    pub skill_name: String,
    pub status: SkillDiagnosticStatus,
    pub evidence_event_ids: Vec<String>,
    pub reason: String,
}

pub fn project_event_capabilities(input: EventCapabilityInput<'_>) -> Vec<CapabilityProjection> {
    let mut capabilities = match input.event_kind {
        "agent" | "app" | "mcp" | "plugin" | "provider" | "skill" => {
            capability_names_for_kind(input.event_kind, input.raw_json, input.detail_json)
                .into_iter()
                .map(|name| {
                    make_event_projection(&input, input.event_kind, &name, "capability-snapshot")
                })
                .collect::<Vec<_>>()
        }
        "tool" | "tool_call" => {
            tool_entries_from_event(input.raw_json, input.detail_json, input.summary)
                .into_iter()
                .map(|(capability_type, name)| {
                    make_event_projection(&input, &capability_type, &name, "observed")
                })
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };

    sort_and_dedup_capabilities(&mut capabilities);
    capabilities
}

pub fn project_tool_visibility_capabilities(
    input: ToolVisibilityCapabilityInput<'_>,
) -> Vec<CapabilityProjection> {
    let mut capabilities = collect_tool_entries(input.final_tools_json)
        .into_iter()
        .map(|(capability_type, name)| CapabilityProjection {
            capability_id: capability_id(input.session_id, input.event_id, &capability_type, &name),
            session_id: input.session_id.to_string(),
            event_id: input.event_id.to_string(),
            source_kind: input.source_kind.to_string(),
            capability_type,
            capability_name: name,
            visibility_stage: input.visibility_stage.to_string(),
            observed_at_ms: input.observed_at_ms,
            raw_ref: input.raw_ref.clone(),
        })
        .collect::<Vec<_>>();

    sort_and_dedup_capabilities(&mut capabilities);
    capabilities
}

pub fn project_prompts_from_events(events: &[PromptEventInput<'_>]) -> Vec<PromptProjection> {
    let mut prompts = events
        .iter()
        .filter_map(prompt_projection_from_event)
        .collect::<Vec<_>>();
    prompts.sort_by(|left, right| {
        left.observed_at_ms
            .cmp(&right.observed_at_ms)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    prompts
}

pub fn diff_adjacent_prompts(prompts: &[PromptProjection]) -> Vec<PromptDiff> {
    let mut prompts = prompts.to_vec();
    prompts.sort_by(|left, right| {
        left.observed_at_ms
            .cmp(&right.observed_at_ms)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });

    prompts
        .windows(2)
        .filter_map(|pair| {
            let from = &pair[0];
            let to = &pair[1];
            if from.session_id != to.session_id {
                return None;
            }
            let from_lines = normalized_line_set(&from.text);
            let to_lines = normalized_line_set(&to.text);
            let added_lines = to_lines
                .difference(&from_lines)
                .cloned()
                .collect::<Vec<_>>();
            let removed_lines = from_lines
                .difference(&to_lines)
                .cloned()
                .collect::<Vec<_>>();

            Some(PromptDiff {
                session_id: from.session_id.clone(),
                from_event_id: from.event_id.clone(),
                to_event_id: to.event_id.clone(),
                added_lines,
                removed_lines,
            })
        })
        .collect()
}

pub fn tool_visibility_diffs(
    capabilities: &[CapabilityProjection],
) -> Vec<CapabilityVisibilityDiff> {
    visibility_diffs_for_types(capabilities, &["tool", "function"], "tool")
}

pub fn skill_visibility_diffs(
    capabilities: &[CapabilityProjection],
) -> Vec<CapabilityVisibilityDiff> {
    visibility_diffs_for_types(capabilities, &["skill"], "skill")
}

pub fn diagnose_skill_visibility(
    capabilities: &[CapabilityProjection],
    skill_name: &str,
) -> SkillDiagnostic {
    let needle = skill_name.trim();
    let skill_capabilities = capabilities
        .iter()
        .filter(|capability| capability.capability_type == "skill")
        .collect::<Vec<_>>();

    if skill_capabilities.is_empty() {
        return SkillDiagnostic {
            skill_name: needle.to_string(),
            status: SkillDiagnosticStatus::Partial,
            evidence_event_ids: Vec::new(),
            reason: "no skill capability snapshot is available for this scope".into(),
        };
    }

    let matching_events = skill_capabilities
        .iter()
        .filter(|capability| capability.capability_name.eq_ignore_ascii_case(needle))
        .map(|capability| capability.event_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    if matching_events.is_empty() {
        SkillDiagnostic {
            skill_name: needle.to_string(),
            status: SkillDiagnosticStatus::Unavailable,
            evidence_event_ids: skill_capabilities
                .iter()
                .map(|capability| capability.event_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            reason: "skill was not present in the observed capability snapshots".into(),
        }
    } else {
        SkillDiagnostic {
            skill_name: needle.to_string(),
            status: SkillDiagnosticStatus::Available,
            evidence_event_ids: matching_events,
            reason: "skill was present in at least one observed capability snapshot".into(),
        }
    }
}

fn make_event_projection(
    input: &EventCapabilityInput<'_>,
    capability_type: &str,
    name: &str,
    visibility_stage: &str,
) -> CapabilityProjection {
    CapabilityProjection {
        capability_id: capability_id(input.session_id, input.event_id, capability_type, name),
        session_id: input.session_id.to_string(),
        event_id: input.event_id.to_string(),
        source_kind: input.source_kind.to_string(),
        capability_type: capability_type.to_string(),
        capability_name: name.to_string(),
        visibility_stage: visibility_stage.to_string(),
        observed_at_ms: input.observed_at_ms,
        raw_ref: input.raw_ref.clone(),
    }
}

fn capability_names_for_kind(
    event_kind: &str,
    raw_json: &Value,
    detail_json: &Value,
) -> Vec<String> {
    let keys = match event_kind {
        "skill" => &[
            "skill_names_preview",
            "skill_names",
            "skills",
            "capabilities",
        ][..],
        "agent" => &[
            "agent_names_preview",
            "agent_names",
            "agents",
            "capabilities",
        ][..],
        "plugin" => &[
            "marketplace_names_preview",
            "plugin_names_preview",
            "plugin_names",
            "marketplaces",
            "plugins",
            "capabilities",
        ][..],
        "mcp" => &[
            "mcp_server_names_preview",
            "mcp_server_names",
            "mcp_servers",
            "mcp",
            "servers",
            "capabilities",
        ][..],
        "provider" => &[
            "provider_names_preview",
            "provider_names",
            "providers",
            "all",
            "data",
            "capabilities",
        ][..],
        "app" => &[
            "app_names_preview",
            "app_names",
            "apps",
            "data",
            "capabilities",
        ][..],
        _ => &[][..],
    };

    let mut names = Vec::new();
    collect_names_from_known_keys(raw_json, keys, &mut names);
    collect_names_from_known_keys(detail_json, keys, &mut names);

    sort_and_dedup_names(&mut names);
    names
}

fn collect_names_from_known_keys(value: &Value, keys: &[&str], names: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(child) = object.get(*key) {
                    collect_names_from_value(child, names);
                }
            }
            for key in ["result", "params", "payload", "raw_json", "detail"] {
                if let Some(child) = object.get(key) {
                    collect_names_from_known_keys(child, keys, names);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_names_from_known_keys(item, keys, names);
            }
        }
        _ => {}
    }
}

fn collect_names_from_value(value: &Value, names: &mut Vec<String>) {
    match value {
        Value::String(name) => push_name(names, name),
        Value::Array(items) => {
            for item in items {
                collect_names_from_value(item, names);
            }
        }
        Value::Object(object) => {
            if let Some(name) = ["name", "id", "title", "label", "slug"]
                .into_iter()
                .find_map(|key| object.get(key).and_then(Value::as_str))
            {
                push_name(names, name);
                return;
            }
            for child in object.values() {
                collect_names_from_value(child, names);
            }
        }
        _ => {}
    }
}

fn tool_entries_from_event(
    raw_json: &Value,
    detail_json: &Value,
    summary: &str,
) -> Vec<(String, String)> {
    let mut entries = collect_tool_entries(raw_json);
    entries.extend(collect_tool_entries(detail_json));

    if entries.is_empty()
        && let Some(name) = tool_name_from_summary(summary)
    {
        entries.push(("tool".into(), name));
    }

    sort_and_dedup_tool_entries(&mut entries);
    entries
}

fn collect_tool_entries(value: &Value) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    collect_tool_entries_into(value, &mut entries);
    sort_and_dedup_tool_entries(&mut entries);
    entries
}

fn collect_tool_entries_into(value: &Value, entries: &mut Vec<(String, String)>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_tool_entries_into(item, entries);
            }
        }
        Value::Object(object) => {
            if let Some(function) = object.get("function")
                && let Some(name) = function.get("name").and_then(Value::as_str)
            {
                let capability_type = object
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("function");
                push_tool_entry(entries, capability_type, name);
                return;
            }

            if let Some(name) = object.get("tool_name").and_then(Value::as_str) {
                push_tool_entry(entries, "tool", name);
                return;
            }
            if let Some(name) = object.get("tool").and_then(Value::as_str) {
                push_tool_entry(entries, "tool", name);
                return;
            }
            if let Some(name) = object.get("name").and_then(Value::as_str) {
                let capability_type = object.get("type").and_then(Value::as_str).unwrap_or("tool");
                push_tool_entry(entries, capability_type, name);
                return;
            }

            for key in [
                "tools",
                "functions",
                "final_tools_json",
                "payload",
                "detail",
            ] {
                if let Some(child) = object.get(key) {
                    collect_tool_entries_into(child, entries);
                }
            }
        }
        Value::String(name) => push_tool_entry(entries, "tool", name),
        _ => {}
    }
}

fn prompt_projection_from_event(event: &PromptEventInput<'_>) -> Option<PromptProjection> {
    let text = prompt_text_from_event(event)?;
    Some(PromptProjection {
        session_id: event.session_id.to_string(),
        event_id: event.event_id.to_string(),
        role: prompt_role_from_event(event),
        text,
        observed_at_ms: event.occurred_at_ms,
        raw_ref: event.raw_ref.clone(),
    })
}

fn prompt_text_from_event(event: &PromptEventInput<'_>) -> Option<String> {
    for value in [event.detail_json, event.raw_json] {
        if let Some(text) = string_at(value, &["full_text"])
            .or_else(|| string_at(value, &["prompt"]))
            .or_else(|| string_at(value, &["body_text"]))
            .or_else(|| string_at(value, &["input"]))
            .or_else(|| messages_text(value))
            && !text.trim().is_empty()
        {
            return Some(text.trim().to_string());
        }
    }
    None
}

fn prompt_role_from_event(event: &PromptEventInput<'_>) -> String {
    string_at(event.detail_json, &["role"])
        .or_else(|| string_at(event.raw_json, &["payload", "role"]))
        .or_else(|| summary_role(event.summary))
        .unwrap_or_else(|| event.event_kind.to_string())
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    cursor.as_str().map(str::to_string)
}

fn messages_text(value: &Value) -> Option<String> {
    let messages = value.get("messages")?.as_array()?;
    let text = messages
        .iter()
        .filter_map(message_text)
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then(|| text.trim().to_string())
}

fn message_text(value: &Value) -> Option<String> {
    if let Some(content) = value.get("content").and_then(Value::as_str) {
        return Some(content.to_string());
    }

    let parts = value.get("content")?.as_array()?;
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

fn summary_role(summary: &str) -> Option<String> {
    if summary.starts_with("用户:") || summary.starts_with("User:") {
        Some("user".into())
    } else if summary.starts_with("助手:") || summary.starts_with("Assistant:") {
        Some("assistant".into())
    } else if summary.starts_with("开发者指令:") || summary.starts_with("Instruction:") {
        Some("developer".into())
    } else {
        None
    }
}

fn normalized_line_set(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn visibility_diffs_for_types(
    capabilities: &[CapabilityProjection],
    capability_types: &[&str],
    diff_type: &str,
) -> Vec<CapabilityVisibilityDiff> {
    let mut grouped = BTreeMap::<(String, u64, String), BTreeSet<String>>::new();
    for capability in capabilities {
        if !capability_types
            .iter()
            .any(|capability_type| capability.capability_type == *capability_type)
        {
            continue;
        }
        grouped
            .entry((
                capability.session_id.clone(),
                capability.observed_at_ms,
                capability.event_id.clone(),
            ))
            .or_default()
            .insert(capability.capability_name.clone());
    }

    let mut by_session = BTreeMap::<String, Vec<CapabilitySnapshot>>::new();
    for ((session_id, observed_at_ms, event_id), names) in grouped {
        by_session
            .entry(session_id.clone())
            .or_default()
            .push(CapabilitySnapshot {
                session_id,
                event_id,
                observed_at_ms,
                names,
            });
    }

    let mut diffs = Vec::new();
    for snapshots in by_session.values_mut() {
        snapshots.sort_by(|left, right| {
            left.observed_at_ms
                .cmp(&right.observed_at_ms)
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        for pair in snapshots.windows(2) {
            diffs.push(capability_snapshot_diff(&pair[0], &pair[1], diff_type));
        }
    }
    diffs
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapabilitySnapshot {
    session_id: String,
    event_id: String,
    observed_at_ms: u64,
    names: BTreeSet<String>,
}

fn capability_snapshot_diff(
    from: &CapabilitySnapshot,
    to: &CapabilitySnapshot,
    diff_type: &str,
) -> CapabilityVisibilityDiff {
    let added = to
        .names
        .difference(&from.names)
        .cloned()
        .collect::<Vec<_>>();
    let removed = from
        .names
        .difference(&to.names)
        .cloned()
        .collect::<Vec<_>>();
    let rename_candidates = if added.len() == 1 && removed.len() == 1 {
        vec![CapabilityRenameCandidate {
            from: removed[0].clone(),
            to: added[0].clone(),
        }]
    } else {
        Vec::new()
    };

    CapabilityVisibilityDiff {
        session_id: from.session_id.clone(),
        from_event_id: from.event_id.clone(),
        to_event_id: to.event_id.clone(),
        capability_type: diff_type.to_string(),
        added,
        hidden: removed.clone(),
        removed,
        rename_candidates,
    }
}

fn push_tool_entry(entries: &mut Vec<(String, String)>, capability_type: &str, name: &str) {
    let name = name.trim();
    if !name.is_empty() {
        entries.push((normalize_capability_type(capability_type), name.to_string()));
    }
}

fn normalize_capability_type(capability_type: &str) -> String {
    match capability_type {
        "function_call" => "tool".into(),
        "function" => "function".into(),
        "tool" => "tool".into(),
        other if other.trim().is_empty() => "tool".into(),
        other => other.trim().to_string(),
    }
}

fn tool_name_from_summary(summary: &str) -> Option<String> {
    [
        "工具调用:",
        "Tool Call:",
        "tool call:",
        "tool:",
        "Ran tool:",
    ]
    .into_iter()
    .find_map(|prefix| summary.strip_prefix(prefix))
    .map(str::trim)
    .filter(|name| !name.is_empty())
    .map(ToString::to_string)
}

fn push_name(names: &mut Vec<String>, name: &str) {
    let name = name.trim();
    if !name.is_empty() {
        names.push(name.to_string());
    }
}

fn sort_and_dedup_capabilities(capabilities: &mut Vec<CapabilityProjection>) {
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
}

fn sort_and_dedup_names(names: &mut Vec<String>) {
    names.sort();
    names.dedup();
}

fn sort_and_dedup_tool_entries(entries: &mut Vec<(String, String)>) {
    entries.sort();
    entries.dedup();
}

fn capability_id(
    session_id: &str,
    event_id: &str,
    capability_type: &str,
    capability_name: &str,
) -> String {
    format!(
        "{}:{}:{}:{}",
        stable_component(session_id),
        stable_component(event_id),
        stable_component(capability_type),
        stable_component(capability_name)
    )
}

fn stable_component(value: &str) -> String {
    let text = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if text.is_empty() {
        "unknown".into()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CapabilityProjection, CapabilityRawRef, PromptEventInput, SkillDiagnosticStatus,
        diagnose_skill_visibility, diff_adjacent_prompts, project_prompts_from_events,
        skill_visibility_diffs, tool_visibility_diffs,
    };
    use serde_json::{Value, json};
    use std::path::PathBuf;

    #[test]
    fn analysis_projects_prompt_diff_between_adjacent_events() {
        let first_raw = json!({ "payload": { "role": "user" } });
        let first_detail = json!({
            "kind": "message",
            "role": "user",
            "full_text": "hello\nuse cargo test",
        });
        let second_raw = json!({ "payload": { "role": "user" } });
        let second_detail = json!({
            "kind": "message",
            "role": "user",
            "full_text": "hello\nuse cargo clippy",
        });
        let events = vec![
            message_event(
                "event-1",
                10,
                "user",
                "hello\nuse cargo test",
                &first_raw,
                &first_detail,
            ),
            message_event(
                "event-2",
                20,
                "user",
                "hello\nuse cargo clippy",
                &second_raw,
                &second_detail,
            ),
        ];

        let prompts = project_prompts_from_events(&events);
        let diffs = diff_adjacent_prompts(&prompts);

        assert_eq!(prompts.len(), 2);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].removed_lines, vec!["use cargo test"]);
        assert_eq!(diffs[0].added_lines, vec!["use cargo clippy"]);
    }

    #[test]
    fn analysis_diffs_tool_visibility_snapshots() {
        let capabilities = vec![
            capability("session-1", "event-1", 10, "function", "list_files"),
            capability("session-1", "event-2", 20, "function", "run_command"),
        ];

        let diffs = tool_visibility_diffs(&capabilities);

        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].added, vec!["run_command"]);
        assert_eq!(diffs[0].removed, vec!["list_files"]);
        assert_eq!(diffs[0].hidden, vec!["list_files"]);
        assert_eq!(diffs[0].rename_candidates[0].from, "list_files");
        assert_eq!(diffs[0].rename_candidates[0].to, "run_command");
    }

    #[test]
    fn analysis_diffs_skill_visibility_and_diagnoses_missing_facts() {
        let capabilities = vec![
            capability("session-1", "event-1", 10, "skill", "review"),
            capability("session-1", "event-2", 20, "skill", "test"),
        ];

        let diffs = skill_visibility_diffs(&capabilities);
        let available = diagnose_skill_visibility(&capabilities, "review");
        let unavailable = diagnose_skill_visibility(&capabilities, "docs");
        let partial = diagnose_skill_visibility(&[], "review");

        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].added, vec!["test"]);
        assert_eq!(diffs[0].removed, vec!["review"]);
        assert_eq!(available.status, SkillDiagnosticStatus::Available);
        assert_eq!(unavailable.status, SkillDiagnosticStatus::Unavailable);
        assert_eq!(partial.status, SkillDiagnosticStatus::Partial);
    }

    fn message_event<'a>(
        event_id: &'a str,
        occurred_at_ms: u64,
        _role: &'a str,
        text: &'a str,
        raw_json: &'a Value,
        detail_json: &'a Value,
    ) -> PromptEventInput<'a> {
        PromptEventInput {
            event_id,
            session_id: "session-1",
            event_kind: "message",
            summary: text,
            occurred_at_ms,
            raw_ref: CapabilityRawRef {
                path: PathBuf::from("/tmp/session.jsonl"),
                line_index: Some(1),
            },
            raw_json,
            detail_json,
        }
    }

    fn capability(
        session_id: &str,
        event_id: &str,
        observed_at_ms: u64,
        capability_type: &str,
        capability_name: &str,
    ) -> CapabilityProjection {
        CapabilityProjection {
            capability_id: format!("{session_id}:{event_id}:{capability_type}:{capability_name}"),
            session_id: session_id.into(),
            event_id: event_id.into(),
            source_kind: "test".into(),
            capability_type: capability_type.into(),
            capability_name: capability_name.into(),
            visibility_stage: "snapshot".into(),
            observed_at_ms,
            raw_ref: CapabilityRawRef {
                path: PathBuf::from("/tmp/capabilities.jsonl"),
                line_index: Some(1),
            },
        }
    }
}
