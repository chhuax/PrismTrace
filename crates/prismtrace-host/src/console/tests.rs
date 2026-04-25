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

    assert!(body.contains("PrismTrace macOS Console"), "body: {body}");
    assert!(body.contains("Targets"), "body: {body}");
    assert!(body.contains("Activity"), "body: {body}");
    assert!(body.contains("Requests"), "body: {body}");

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn render_console_homepage_includes_title_and_heading() {
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
        homepage.contains("<title>PrismTrace macOS Console</title>"),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("<h1>PrismTrace macOS Console</h1>"),
        "homepage: {homepage}"
    );
}

#[test]
fn render_console_homepage_exposes_theme_switcher() {
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

    assert!(homepage.contains("Theme</p>"), "homepage: {homepage}");
    assert!(homepage.contains("?theme=light"), "homepage: {homepage}");
    assert!(
        homepage.contains("data-theme=\"system\""),
        "homepage: {homepage}"
    );
}

#[test]
fn render_console_homepage_renders_session_timeline_content() {
    let homepage = super::render_console_homepage(&ConsoleSnapshot {
        summary: "PrismTrace host skeleton".into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![],
        activity_items: vec![],
        request_summaries: vec![],
        session_summaries: vec![super::ConsoleSessionSummary {
            session_id: "session-1".into(),
            pid: 701,
            target_display_name: "Codex".into(),
            started_at_ms: 10,
            completed_at_ms: 25,
            exchange_count: 1,
            request_count: 1,
            response_count: 1,
        }],
        request_details: vec![],
        session_details: vec![super::ConsoleSessionDetail {
            session_id: "session-1".into(),
            pid: 701,
            target_display_name: "Codex".into(),
            started_at_ms: 10,
            completed_at_ms: 25,
            last_exchange_started_at_ms: 10,
            exchange_count: 1,
            timeline_items: vec![super::ConsoleSessionTimelineItem {
                request_id: "req-1".into(),
                exchange_id: Some("exchange-1".into()),
                pid: 701,
                provider: "openai".into(),
                model: Some("gpt-4.1".into()),
                started_at_ms: 10,
                completed_at_ms: 25,
                duration_ms: 15,
                target_display_name: "Codex".into(),
                request_summary: "openai POST /v1/responses".into(),
                response_status: Some(200),
                tool_count_final: 3,
                has_response: true,
                has_tool_visibility: true,
            }],
        }],
    });

    assert!(
        homepage.contains("Session Timeline"),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("openai POST /v1/responses"),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("provider: openai"),
        "homepage: {homepage}"
    );
    assert!(homepage.contains("tools: 3"), "homepage: {homepage}");
}

#[test]
fn render_console_homepage_renders_request_summary_content() {
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
        request_details: vec![],
        session_details: vec![],
    });

    assert!(
        homepage.contains("openai POST /v1/responses"),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("provider: openai"),
        "homepage: {homepage}"
    );
    assert!(homepage.contains("model: gpt-4.1"), "homepage: {homepage}");
    assert!(homepage.contains("view detail"), "homepage: {homepage}");
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

    assert!(
        response.contains("\"active_filters\":[\"opencode\"]"),
        "response: {response}"
    );
    assert!(
        response.contains("\"target_display_name\":\"opencode\""),
        "response: {response}"
    );
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