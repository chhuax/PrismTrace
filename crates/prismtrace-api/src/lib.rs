use prismtrace_analysis::{
    CapabilityProjection, CapabilityVisibilityDiff, PromptEventInput, SkillDiagnostic,
    SkillDiagnosticStatus, diagnose_skill_visibility, diff_adjacent_prompts,
    project_prompts_from_events, skill_visibility_diffs, tool_visibility_diffs,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiFilterContext {
    pub active_filters: Vec<String>,
    pub is_filtered_view: bool,
}

pub fn render_capability_projection_payload(
    session_id: &str,
    capabilities: &[CapabilityProjection],
    filter_context: Option<&ApiFilterContext>,
) -> String {
    let items = capabilities
        .iter()
        .map(|capability| {
            json!({
                "capability_id": &capability.capability_id,
                "session_id": &capability.session_id,
                "event_id": &capability.event_id,
                "source_kind": &capability.source_kind,
                "capability_type": &capability.capability_type,
                "capability_name": &capability.capability_name,
                "visibility_stage": &capability.visibility_stage,
                "observed_at_ms": capability.observed_at_ms,
                "raw_ref": {
                    "path": capability.raw_ref.path.display().to_string(),
                    "line_index": capability.raw_ref.line_index,
                },
            })
        })
        .collect::<Vec<_>>();
    let empty_state = if items.is_empty() {
        Some("尚无 capability projection")
    } else {
        None
    };
    let capabilities_json = items.clone();

    let mut payload = json!({
        "session_id": session_id,
        "items": items,
        "capabilities": capabilities_json,
        "next_cursor": Value::Null,
        "empty_state": empty_state,
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub fn render_empty_capability_projection_payload(
    session_id: &str,
    filter_context: Option<&ApiFilterContext>,
) -> String {
    render_capability_projection_payload(session_id, &[], filter_context)
}

pub fn render_session_diagnostics_payload(
    session_id: &str,
    prompt_events: &[PromptEventInput<'_>],
    capabilities: &[CapabilityProjection],
    filter_context: Option<&ApiFilterContext>,
) -> String {
    let prompt_projections = project_prompts_from_events(prompt_events);
    let prompt_diffs = diff_adjacent_prompts(&prompt_projections);
    let tool_diffs = tool_visibility_diffs(capabilities);
    let skill_diffs = skill_visibility_diffs(capabilities);
    let capability_inventory = capability_inventory_by_type(capabilities);
    let visible_skills = visible_skill_names(capabilities);
    let visible_mcp_servers = visible_capability_names(capabilities, "mcp");
    let skill_diagnostics = skill_diagnostics_for_visible_skills(capabilities, &visible_skills);
    let skill_status = aggregate_skill_status(&skill_diagnostics);
    let capability_type_count = capability_inventory.len();

    let prompt_diff_items = prompt_diffs
        .iter()
        .map(|diff| {
            json!({
                "session_id": &diff.session_id,
                "from_event_id": &diff.from_event_id,
                "to_event_id": &diff.to_event_id,
                "added_lines": &diff.added_lines,
                "removed_lines": &diff.removed_lines,
            })
        })
        .collect::<Vec<_>>();
    let tool_diff_items = tool_diffs
        .iter()
        .map(capability_visibility_diff_json)
        .collect::<Vec<_>>();
    let skill_diff_items = skill_diffs
        .iter()
        .map(capability_visibility_diff_json)
        .collect::<Vec<_>>();
    let skill_diagnostic_items = skill_diagnostics
        .iter()
        .map(skill_diagnostic_json)
        .collect::<Vec<_>>();

    let mut payload = json!({
        "session_id": session_id,
        "diagnostics": {
            "prompt_projection_count": prompt_projections.len(),
            "prompt_diff_count": prompt_diff_items.len(),
            "tool_diff_count": tool_diff_items.len(),
            "skill_diff_count": skill_diff_items.len(),
            "skill_diagnostic_count": skill_diagnostic_items.len(),
            "capability_fact_count": capabilities.len(),
            "capability_type_count": capability_type_count,
            "capability_inventory": capability_inventory,
            "skill_status": skill_status_label(&skill_status),
            "visible_skills": visible_skills,
            "visible_mcp_servers": visible_mcp_servers,
        },
        "prompt_diffs": prompt_diff_items,
        "tool_visibility_diffs": tool_diff_items,
        "skill_visibility_diffs": skill_diff_items,
        "skill_diagnostics": skill_diagnostic_items,
        "next_cursor": Value::Null,
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

pub fn render_empty_session_diagnostics_payload(
    session_id: &str,
    filter_context: Option<&ApiFilterContext>,
) -> String {
    let mut payload = json!({
        "session_id": session_id,
        "diagnostics": {
            "prompt_projection_count": 0,
            "prompt_diff_count": 0,
            "tool_diff_count": 0,
            "skill_diff_count": 0,
            "skill_diagnostic_count": 0,
            "skill_status": "partial",
            "visible_skills": [],
        },
        "prompt_diffs": [],
        "tool_visibility_diffs": [],
        "skill_visibility_diffs": [],
        "skill_diagnostics": [],
        "next_cursor": Value::Null,
        "empty_state": "尚无 diagnostics projection",
    });
    append_filter_context_fields(&mut payload, filter_context);
    payload.to_string()
}

fn visible_skill_names(capabilities: &[CapabilityProjection]) -> Vec<String> {
    visible_capability_names(capabilities, "skill")
}

fn visible_capability_names(
    capabilities: &[CapabilityProjection],
    capability_type: &str,
) -> Vec<String> {
    capabilities
        .iter()
        .filter(|capability| capability.capability_type == capability_type)
        .map(|capability| capability.capability_name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn capability_inventory_by_type(capabilities: &[CapabilityProjection]) -> Vec<Value> {
    let mut inventory = BTreeMap::<String, BTreeSet<String>>::new();
    for capability in capabilities {
        inventory
            .entry(capability.capability_type.clone())
            .or_default()
            .insert(capability.capability_name.clone());
    }

    inventory
        .into_iter()
        .map(|(capability_type, capability_names)| {
            let capability_names = capability_names.into_iter().collect::<Vec<_>>();
            let capability_count = capability_names.len();
            json!({
                "capability_type": capability_type,
                "capability_names": capability_names,
                "count": capability_count,
            })
        })
        .collect()
}

fn skill_diagnostics_for_visible_skills(
    capabilities: &[CapabilityProjection],
    visible_skills: &[String],
) -> Vec<SkillDiagnostic> {
    if visible_skills.is_empty() {
        return vec![diagnose_skill_visibility(capabilities, "*")];
    }

    visible_skills
        .iter()
        .map(|skill_name| diagnose_skill_visibility(capabilities, skill_name))
        .collect()
}

fn aggregate_skill_status(diagnostics: &[SkillDiagnostic]) -> SkillDiagnosticStatus {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.status == SkillDiagnosticStatus::Available)
    {
        return SkillDiagnosticStatus::Available;
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.status == SkillDiagnosticStatus::Partial)
    {
        return SkillDiagnosticStatus::Partial;
    }
    SkillDiagnosticStatus::Unavailable
}

fn skill_status_label(status: &SkillDiagnosticStatus) -> &'static str {
    match status {
        SkillDiagnosticStatus::Available => "available",
        SkillDiagnosticStatus::Partial => "partial",
        SkillDiagnosticStatus::Unavailable => "unavailable",
    }
}

fn capability_visibility_diff_json(diff: &CapabilityVisibilityDiff) -> Value {
    json!({
        "session_id": &diff.session_id,
        "from_event_id": &diff.from_event_id,
        "to_event_id": &diff.to_event_id,
        "capability_type": &diff.capability_type,
        "added": &diff.added,
        "removed": &diff.removed,
        "hidden": &diff.hidden,
        "rename_candidates": diff
            .rename_candidates
            .iter()
            .map(|candidate| json!({
                "from": &candidate.from,
                "to": &candidate.to,
            }))
            .collect::<Vec<_>>(),
    })
}

fn skill_diagnostic_json(diagnostic: &SkillDiagnostic) -> Value {
    json!({
        "skill_name": &diagnostic.skill_name,
        "status": skill_status_label(&diagnostic.status),
        "evidence_event_ids": &diagnostic.evidence_event_ids,
        "reason": &diagnostic.reason,
    })
}

fn append_filter_context_fields(payload: &mut Value, filter_context: Option<&ApiFilterContext>) {
    let Some(filter_context) = filter_context else {
        return;
    };

    payload["active_filters"] = json!(filter_context.active_filters);
    payload["is_filtered_view"] = json!(filter_context.is_filtered_view);
}

#[cfg(test)]
mod tests {
    use super::{
        ApiFilterContext, render_capability_projection_payload,
        render_empty_session_diagnostics_payload, render_session_diagnostics_payload,
    };
    use prismtrace_analysis::{CapabilityProjection, CapabilityRawRef, PromptEventInput};
    use serde_json::{Value, json};
    use std::path::PathBuf;

    #[test]
    fn api_renders_capability_projection_payload_with_filter_context() {
        let payload = render_capability_projection_payload(
            "session-1",
            &[capability("session-1", "event-1", "mcp", "github")],
            Some(&ApiFilterContext {
                active_filters: vec!["codex".into()],
                is_filtered_view: true,
            }),
        );

        let value = serde_json::from_str::<Value>(&payload).expect("payload should be json");

        assert_eq!(value["session_id"], "session-1");
        assert_eq!(value["items"][0]["capability_type"], "mcp");
        assert_eq!(value["items"][0]["capability_name"], "github");
        assert_eq!(value["active_filters"][0], "codex");
        assert_eq!(value["is_filtered_view"], true);
    }

    #[test]
    fn api_renders_session_diagnostics_payload_from_prompt_and_capability_facts() {
        let first_raw = json!({ "payload": { "role": "user" } });
        let first_detail = json!({ "role": "user", "full_text": "hello\nuse cargo test" });
        let second_raw = json!({ "payload": { "role": "user" } });
        let second_detail = json!({ "role": "user", "full_text": "hello\nuse cargo clippy" });
        let prompt_events = vec![
            prompt_event("event-1", 10, &first_raw, &first_detail),
            prompt_event("event-2", 20, &second_raw, &second_detail),
        ];
        let capabilities = vec![
            capability("session-1", "event-1", "skill", "review"),
            capability("session-1", "event-2", "mcp", "github"),
        ];

        let payload =
            render_session_diagnostics_payload("session-1", &prompt_events, &capabilities, None);
        let value = serde_json::from_str::<Value>(&payload).expect("payload should be json");

        assert_eq!(value["diagnostics"]["prompt_diff_count"], 1);
        assert_eq!(
            value["prompt_diffs"][0]["added_lines"][0],
            "use cargo clippy"
        );
        assert_eq!(value["diagnostics"]["visible_skills"][0], "review");
        assert_eq!(value["diagnostics"]["visible_mcp_servers"][0], "github");
    }

    #[test]
    fn api_renders_empty_session_diagnostics_payload() {
        let payload = render_empty_session_diagnostics_payload("session-1", None);
        let value = serde_json::from_str::<Value>(&payload).expect("payload should be json");

        assert_eq!(value["session_id"], "session-1");
        assert_eq!(value["diagnostics"]["skill_status"], "partial");
        assert_eq!(value["empty_state"], "尚无 diagnostics projection");
    }

    fn capability(
        session_id: &str,
        event_id: &str,
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
            observed_at_ms: 10,
            raw_ref: CapabilityRawRef {
                path: PathBuf::from("/tmp/capabilities.jsonl"),
                line_index: Some(1),
            },
        }
    }

    fn prompt_event<'a>(
        event_id: &'a str,
        occurred_at_ms: u64,
        raw_json: &'a Value,
        detail_json: &'a Value,
    ) -> PromptEventInput<'a> {
        PromptEventInput {
            session_id: "session-1",
            event_id,
            event_kind: "message",
            summary: "message",
            occurred_at_ms,
            raw_ref: CapabilityRawRef {
                path: PathBuf::from("/tmp/session.jsonl"),
                line_index: Some(1),
            },
            raw_json,
            detail_json,
        }
    }
}
