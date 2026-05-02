use super::model::filter_request_summaries;
use super::{
    ConsoleActivityItem, ConsoleActivitySource, ConsoleKnownErrorActivity,
    ConsoleRecentRequestActivity, ConsoleRequestSummary, ConsoleSnapshot,
    ConsoleTargetFilterConfig, ConsoleTargetSummary, collect_activity_items,
    collect_activity_items_filtered, collect_target_summaries, load_request_detail,
    load_request_summaries, load_session_detail, load_session_summaries,
    read_request_path_from_reader, render_activity_payload_from_items, run_console_server,
    start_console_server_on_bind_addr, write_console_response,
};
use crate::bootstrap;
use crate::discovery::StaticProcessSampleSource;
use prismtrace_core::{ProcessSample, ProcessTarget, RuntimeKind};
use std::fs;
use std::io::{self, Cursor, Read};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        executable_path: PathBuf::from("/Applications/Claude Code.app/Contents/MacOS/Claude Code"),
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

    let summaries = collect_target_summaries(&source, Some(&filter))?;

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].pid, 100);
    Ok(())
}

#[test]
fn collect_activity_items_filters_items_by_matching_pid() {
    let filter = ConsoleTargetFilterConfig::new(vec!["opencode".into()]);
    let unmatched_target = ProcessTarget {
        pid: 200,
        app_name: "claude".into(),
        executable_path: PathBuf::from("/usr/local/bin/node"),
        command_line: Some("node /tmp/claude.js".into()),
        runtime_kind: RuntimeKind::Node,
    };

    let items = collect_activity_items_filtered(
        ConsoleActivitySource {
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

    assert_eq!(items.len(), 2);
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
        source_state: "discoverable".into(),
        source_summary: "local process target · node runtime".into(),
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
fn render_console_homepage_keeps_ia_shell_when_filters_are_active() {
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

    assert!(homepage.contains("PrismTrace"), "homepage: {homepage}");
    assert!(
        homepage.contains("id=\"session-list-region\""),
        "homepage: {homepage}"
    );
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

    assert!(homepage.contains("data-theme="), "homepage: {homepage}");
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
        payload.contains("当前过滤条件下没有匹配 source"),
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
fn render_requests_payload_exposes_compatible_list_envelope() {
    let payload = super::render_requests_payload(
        &[super::ConsoleRequestSummary {
            request_id: "request-1".into(),
            captured_at_ms: 100,
            provider: "openai".into(),
            model: Some("gpt-4.1".into()),
            target_display_name: "Codex Desktop".into(),
            summary_text: "openai POST /v1/responses".into(),
        }],
        None,
    );

    assert!(payload.contains("\"requests\":["), "payload: {payload}");
    assert!(payload.contains("\"items\":["), "payload: {payload}");
    assert!(
        payload.contains("\"next_cursor\":null"),
        "payload: {payload}"
    );
}

#[test]
fn render_sessions_payload_exposes_compatible_list_envelope() {
    let payload = super::render_sessions_payload_with_pagination(
        &[super::ConsoleSessionSummary {
            session_id: "session-1".into(),
            title: "Debug prompt drift".into(),
            subtitle: "thread session-1".into(),
            cwd: Some("/tmp/workspace".into()),
            artifact_path: Some("/tmp/session.jsonl".into()),
            pid: 0,
            target_display_name: "Codex Desktop".into(),
            started_at_ms: 100,
            completed_at_ms: 200,
            exchange_count: 2,
            request_count: 2,
            response_count: 1,
        }],
        Some(&super::ConsoleFilterContext {
            active_filters: vec!["codex".into()],
            is_filtered_view: true,
        }),
        None,
    );

    assert!(payload.contains("\"sessions\":["), "payload: {payload}");
    assert!(payload.contains("\"items\":["), "payload: {payload}");
    assert!(
        payload.contains("\"next_cursor\":null"),
        "payload: {payload}"
    );
    assert!(
        payload.contains("\"active_filters\":[\"codex\"]"),
        "payload: {payload}"
    );
}

#[test]
fn render_console_homepage_exposes_ia_data_regions_when_context_is_active() {
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
        homepage.contains("id=\"runtime-list-region\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("id=\"session-list-region\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("id=\"transcript-region\""),
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

    assert!(body.contains("PrismTrace"), "body: {body}");
    assert!(body.contains("Sessions"), "body: {body}");
    assert!(body.contains("Agent Runtime"), "body: {body}");
    assert!(body.contains("id=\"right-sidebar\""), "body: {body}");

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_server_handles_static_asset_while_previous_connection_is_idle() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();

    let handle = thread::spawn(move || server.serve_connections_for_test(2));
    let idle_stream = TcpStream::connect(&addr)?;

    let response = send_get_request(&addr, "/assets/console.js")?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(
        response.contains("const escapeHtml"),
        "response: {response}"
    );

    drop(idle_stream);
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

    assert!(homepage.contains("<title"), "homepage: {homepage}");
    assert!(homepage.contains("PrismTrace"), "homepage: {homepage}");
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

    assert!(homepage.contains("class=\"light\""), "homepage: {homepage}");
    assert!(homepage.contains("data-theme="), "homepage: {homepage}");
}

#[test]
fn render_console_homepage_seeds_initial_session_selection_for_js_hydration() {
    let homepage = super::render_console_homepage(&ConsoleSnapshot {
        summary: "PrismTrace host skeleton".into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![],
        activity_items: vec![],
        request_summaries: vec![],
        session_summaries: vec![super::ConsoleSessionSummary {
            session_id: "session-1".into(),
            title: "openai POST /v1/responses".into(),
            subtitle: "openai · responses".into(),
            cwd: None,
            artifact_path: None,
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
        homepage.contains("data-initial-session-id=\"session-1\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("id=\"transcript-region\""),
        "homepage: {homepage}"
    );
    assert!(homepage.contains("Sessions"), "homepage: {homepage}");
}

#[test]
fn render_console_homepage_seeds_initial_request_selection_for_js_hydration() {
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
        homepage.contains("data-initial-request-id=\"req-1\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("/assets/console.js"),
        "homepage: {homepage}"
    );
}

#[test]
fn render_console_homepage_renders_empty_regions_and_refresh_script() {
    let homepage = super::render_console_homepage(&ConsoleSnapshot {
        summary: "PrismTrace host skeleton".into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![super::ConsoleTargetSummary {
            pid: 701,
            display_name: "Codex".into(),
            runtime_kind: "node".into(),
            source_state: "discoverable".into(),
            source_summary: "local process target · node runtime".into(),
        }],
        activity_items: vec![super::ConsoleActivityItem {
            activity_id: "source-1".into(),
            activity_type: "source".into(),
            occurred_at_ms: 20,
            title: "Source discovered".into(),
            subtitle: "Codex runtime visible to PrismTrace".into(),
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

    assert!(
        homepage.contains("id=\"runtime-list-region\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("id=\"session-list-region\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("id=\"transcript-region\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("/assets/console.js"),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("data-initial-request-id=\"req-1\""),
        "homepage: {homepage}"
    );
}

#[test]
fn render_console_homepage_uses_observer_first_shell_copy() {
    let homepage = super::render_console_homepage(&ConsoleSnapshot {
        summary: "PrismTrace host skeleton".into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![super::ConsoleTargetSummary {
            pid: 0,
            display_name: "Codex App Server".into(),
            runtime_kind: "observer".into(),
            source_state: "active".into(),
            source_summary:
                "official observer · channel: codex-app-server · sessions: 2 · events: 9 · last seen: 1714000005100"
                    .into(),
        }],
        activity_items: vec![],
        request_summaries: vec![],
        session_summaries: vec![super::ConsoleSessionSummary {
            session_id: "observer:codex:1714000005000-77".into(),
            title: "Thread started".into(),
            subtitle: "thread thread-1 · thread".into(),
            cwd: None,
            artifact_path: None,
            pid: 0,
            target_display_name: "Codex App Server".into(),
            started_at_ms: 1714000005000,
            completed_at_ms: 1714000005100,
            exchange_count: 9,
            request_count: 9,
            response_count: 2,
        }],
        request_details: vec![],
        session_details: vec![],
    });

    assert!(homepage.contains("PrismTrace"), "homepage: {homepage}");
    assert!(homepage.contains("Sessions"), "homepage: {homepage}");
    assert!(
        homepage.contains("id=\"right-sidebar\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("data-initial-session-id=\"observer:codex:1714000005000-77\""),
        "homepage: {homepage}"
    );
}

#[test]
fn render_targets_payload_includes_empty_state_when_no_targets() {
    let payload = super::render_targets_payload_from_summaries(&[], None);

    assert!(payload.contains("\"targets\":[]"), "payload: {payload}");
    assert!(
        payload.contains("\"empty_state\":\"尚无可观测 source\""),
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

    assert!(
        homepage.contains("id=\"analysis-message-region\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("id=\"analysis-context-summary\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("data-initial-request-id=\"req-1\""),
        "homepage: {homepage}"
    );
}

#[test]
fn render_console_homepage_renders_health_shell_region() {
    let homepage = super::render_console_homepage(&ConsoleSnapshot {
        summary: "PrismTrace host skeleton\nCodex observer active\nsource heartbeat timed out"
            .into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![super::ConsoleTargetSummary {
            pid: 701,
            display_name: "Codex".into(),
            runtime_kind: "node".into(),
            source_state: "discoverable".into(),
            source_summary: "local process target · node runtime".into(),
        }],
        activity_items: vec![super::ConsoleActivityItem {
            activity_id: "error-1".into(),
            activity_type: "error".into(),
            occurred_at_ms: 40,
            title: "Source timeout".into(),
            subtitle: "source heartbeat timed out".into(),
            related_pid: Some(701),
            related_request_id: None,
        }],
        request_summaries: vec![],
        session_summaries: vec![],
        request_details: vec![],
        session_details: vec![],
    });

    assert!(
        homepage.contains("id=\"runtime-list-region\""),
        "homepage: {homepage}"
    );
    assert!(
        homepage.contains("/assets/console.js"),
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
fn render_health_payload_includes_source_summary_and_errors() {
    let payload = super::render_health_payload(
        &[super::ConsoleTargetSummary {
            pid: 701,
            display_name: "Codex".into(),
            runtime_kind: "node".into(),
            source_state: "discoverable".into(),
            source_summary: "local process target · node runtime".into(),
        }],
        &[super::ConsoleActivityItem {
            activity_id: "error-1".into(),
            activity_type: "error".into(),
            occurred_at_ms: 40,
            title: "Source timeout".into(),
            subtitle: "source heartbeat timed out".into(),
            related_pid: Some(701),
            related_request_id: None,
        }],
        None,
        None,
    );

    assert!(
        payload.contains("\"source_summary\":\"local process target · node runtime\""),
        "payload: {payload}"
    );
    assert!(payload.contains("\"errors\":"), "payload: {payload}");
    assert!(payload.contains("Source timeout"), "payload: {payload}");
    assert!(
        payload.contains("source heartbeat timed out"),
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
            source_state: "discoverable".into(),
            source_summary: "local process target · node runtime".into(),
        }],
        activity_items: vec![super::ConsoleActivityItem {
            activity_id: "error-1".into(),
            activity_type: "error".into(),
            occurred_at_ms: 50,
            title: "Source timeout".into(),
            subtitle: "source heartbeat timed out".into(),
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
        response.contains("\"source_summary\""),
        "response: {response}"
    );
    assert!(response.contains("Source timeout"), "response: {response}");

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
fn console_server_returns_json_error_for_unknown_api_path() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();

    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(&addr, "/api/missing")?;

    assert!(
        response.starts_with("HTTP/1.1 404 Not Found"),
        "response: {response}"
    );
    assert!(
        response.contains("Content-Type: application/json; charset=utf-8"),
        "response: {response}"
    );
    assert!(
        response.contains("\"code\":\"not_found\""),
        "response: {response}"
    );

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_server_returns_json_error_when_request_read_times_out() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();

    let handle =
        thread::spawn(move || server.serve_once_with_timeout_for_test(Duration::from_millis(25)));
    let mut client_stream = TcpStream::connect(&addr)?;
    client_stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut response = String::new();
    client_stream.read_to_string(&mut response)?;

    assert!(
        response.starts_with("HTTP/1.1 408 Request Timeout"),
        "response: {response}"
    );
    assert!(
        response.contains("Content-Type: application/json; charset=utf-8"),
        "response: {response}"
    );
    assert!(
        response.contains("\"code\":\"request_timeout\""),
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
        response.contains("当前过滤条件下没有匹配 source"),
        "response: {response}"
    );

    handle.join().expect("server thread should join")?;
    Ok(())
}

#[test]
fn console_server_sessions_api_applies_limit_and_cursor() -> io::Result<()> {
    let snapshot = ConsoleSnapshot {
        summary: "summary".into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![],
        activity_items: vec![],
        request_summaries: vec![],
        session_summaries: vec![
            super::ConsoleSessionSummary {
                session_id: "session-new".into(),
                title: "New".into(),
                subtitle: "new session".into(),
                cwd: None,
                artifact_path: None,
                pid: 0,
                target_display_name: "Codex Desktop".into(),
                started_at_ms: 200,
                completed_at_ms: 200,
                exchange_count: 1,
                request_count: 1,
                response_count: 1,
            },
            super::ConsoleSessionSummary {
                session_id: "session-old".into(),
                title: "Old".into(),
                subtitle: "old session".into(),
                cwd: None,
                artifact_path: None,
                pid: 0,
                target_display_name: "Codex Desktop".into(),
                started_at_ms: 100,
                completed_at_ms: 100,
                exchange_count: 1,
                request_count: 1,
                response_count: 1,
            },
        ],
        request_details: vec![],
        session_details: vec![],
    };
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let handle = thread::spawn(move || -> io::Result<()> {
        let (mut server_stream, _) = listener.accept()?;
        write_console_response(&mut server_stream, &snapshot)
    });

    let response = send_get_request(&addr.to_string(), "/api/sessions?limit=1")?;

    assert!(
        response.contains("\"session_id\":\"session-new\""),
        "response: {response}"
    );
    assert!(
        !response.contains("\"session_id\":\"session-old\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"next_cursor\":\"1\""),
        "response: {response}"
    );

    handle.join().expect("server thread should join")?;
    Ok(())
}

#[test]
fn console_server_session_events_api_applies_limit_and_cursor() -> io::Result<()> {
    let snapshot = ConsoleSnapshot {
        summary: "summary".into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![],
        activity_items: vec![],
        request_summaries: vec![],
        session_summaries: vec![],
        request_details: vec![],
        session_details: vec![super::ConsoleSessionDetail {
            session_id: "session-1".into(),
            pid: 0,
            target_display_name: "Codex Desktop".into(),
            started_at_ms: 100,
            completed_at_ms: 300,
            last_exchange_started_at_ms: 200,
            exchange_count: 2,
            timeline_items: vec![
                super::ConsoleSessionTimelineItem {
                    request_id: "event-1".into(),
                    exchange_id: Some("turn-1".into()),
                    pid: 0,
                    target_display_name: "Codex Desktop".into(),
                    provider: "codex-rollout".into(),
                    model: Some("message".into()),
                    started_at_ms: 100,
                    completed_at_ms: 100,
                    duration_ms: 0,
                    request_summary: "first".into(),
                    response_status: Some(200),
                    tool_count_final: 0,
                    has_response: true,
                    has_tool_visibility: false,
                },
                super::ConsoleSessionTimelineItem {
                    request_id: "event-2".into(),
                    exchange_id: Some("turn-2".into()),
                    pid: 0,
                    target_display_name: "Codex Desktop".into(),
                    provider: "codex-rollout".into(),
                    model: Some("tool".into()),
                    started_at_ms: 200,
                    completed_at_ms: 200,
                    duration_ms: 0,
                    request_summary: "second".into(),
                    response_status: Some(200),
                    tool_count_final: 1,
                    has_response: true,
                    has_tool_visibility: true,
                },
            ],
        }],
    };
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;

    let handle = thread::spawn(move || -> io::Result<()> {
        let (mut server_stream, _) = listener.accept()?;
        write_console_response(&mut server_stream, &snapshot)
    });

    let response = send_get_request(&addr.to_string(), "/api/sessions/session-1/events?limit=1")?;

    assert!(
        response.contains("\"session_id\":\"session-1\""),
        "response: {response}"
    );
    assert!(response.contains("\"items\":["), "response: {response}");
    assert!(
        response.contains("\"request_id\":\"event-1\""),
        "response: {response}"
    );
    assert!(
        !response.contains("\"request_id\":\"event-2\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"next_cursor\":\"1\""),
        "response: {response}"
    );

    handle.join().expect("server thread should join")?;
    Ok(())
}

#[test]
fn write_console_response_renders_target_summary_fields_from_controlled_snapshot() -> io::Result<()>
{
    let snapshot = ConsoleSnapshot {
        summary: "summary".into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![super::ConsoleTargetSummary {
            pid: 777,
            display_name: "node".into(),
            runtime_kind: "node".into(),
            source_state: "discoverable".into(),
            source_summary: "local process target · node runtime".into(),
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
        response.contains("\"source_state\":\"discoverable\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"source_summary\""),
        "response: {response}"
    );

    handle.join().expect("server thread should join")?;
    Ok(())
}

#[test]
fn collect_target_summaries_marks_local_targets_as_discoverable() -> io::Result<()> {
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
    let summaries = collect_target_summaries(&source, None)?;

    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].pid, 701);
    assert_eq!(summaries[0].source_state, "discoverable");
    assert_eq!(
        summaries[0].source_summary,
        "local process target · node runtime"
    );
    assert_eq!(summaries[1].source_state, "discoverable");
    assert_eq!(
        summaries[1].source_summary,
        "local process target · electron runtime"
    );
    Ok(())
}

#[test]
fn collect_target_summaries_uses_runtime_summary_for_single_target() -> io::Result<()> {
    let source = StaticProcessSampleSource::new(vec![ProcessSample {
        pid: 703,
        process_name: "node".into(),
        executable_path: PathBuf::from("/usr/local/bin/node"),
        command_line: None,
    }]);
    let summaries = collect_target_summaries(&source, None)?;

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].source_state, "discoverable");
    assert_eq!(
        summaries[0].source_summary,
        "local process target · node runtime"
    );
    Ok(())
}

#[test]
fn collect_activity_items_returns_empty_for_no_known_activity() {
    let items = collect_activity_items(super::ConsoleActivitySource {
        recent_requests: &[],
        known_errors: &[],
    });

    assert!(items.is_empty());

    let payload = render_activity_payload_from_items(&items, None);
    assert!(payload.contains("\"activity\":[]"), "payload: {payload}");
    assert!(payload.contains("尚无观测活动"), "payload: {payload}");
}

#[test]
fn collect_activity_items_orders_request_and_error_by_time() {
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
        recent_requests: &recent_requests,
        known_errors: &known_errors,
    });

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].activity_type, "error");
    assert_eq!(items[1].activity_type, "request");
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
fn console_route_handler_renders_health_without_tcp() {
    let snapshot = ConsoleSnapshot {
        summary: "summary".into(),
        bind_addr: "http://127.0.0.1:7799".into(),
        filter_context: None,
        target_summaries: vec![super::ConsoleTargetSummary {
            pid: 42,
            display_name: "Codex".into(),
            runtime_kind: "electron".into(),
            source_state: "discoverable".into(),
            source_summary: "running".into(),
        }],
        activity_items: vec![],
        request_summaries: vec![],
        session_summaries: vec![],
        request_details: vec![],
        session_details: vec![],
    };

    let response = super::render_console_route_response(Some("/api/health"), &snapshot, None, None);

    assert_eq!(response.status_line, "HTTP/1.1 200 OK");
    assert_eq!(response.content_type, "application/json; charset=utf-8");
    let body = String::from_utf8(response.body).expect("health response should be utf-8");
    assert!(body.contains("\"source_summary\":\"running\""));
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
fn load_session_summaries_do_not_merge_when_only_response_finishes_within_window() -> io::Result<()>
{
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

    let detail =
        load_request_detail(&result.storage, "77-1714000005000-1")?.expect("detail should exist");

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
fn console_server_returns_request_embedded_tool_capabilities_api_payload() -> io::Result<()> {
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

    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();
    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(&addr, "/api/sessions/77-1714000005000-1/capabilities")?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(
        response.contains("\"session_id\":\"77-1714000005000-1\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_type\":\"function\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_name\":\"list_files\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"visibility_stage\":\"request-embedded\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"source_kind\":\"tool_visibility\""),
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
fn console_server_returns_observer_requests_api_payload() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let observer_dir = result
        .storage
        .artifacts_dir
        .join("observer_events")
        .join("codex");
    fs::create_dir_all(&observer_dir)?;
    fs::write(
        observer_dir.join("1714000005000-77.jsonl"),
        concat!(
            "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"ipc\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1714000005000,\"raw_json\":{}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"tool\",\"summary\":\"Ran shell command\",\"method\":\"shell.exec\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"item_id\":\"item-1\",\"timestamp\":\"2026-04-26T10:00:00Z\",\"recorded_at_ms\":1714000005100,\"raw_json\":{\"tool\":\"exec_command\"}}\n"
        ),
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
    assert!(
        response.contains("\"request_id\":\"observer:codex:1714000005000-77:1\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"provider\":\"codex-app-server\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"model\":\"tool\""),
        "response: {response}"
    );
    assert!(
        response.contains("Ran shell command"),
        "response: {response}"
    );

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_server_returns_observer_request_detail_api_payload() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let observer_dir = result
        .storage
        .artifacts_dir
        .join("observer_events")
        .join("codex");
    fs::create_dir_all(&observer_dir)?;
    fs::write(
        observer_dir.join("1714000005000-77.jsonl"),
        concat!(
            "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"ipc\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1714000005000,\"raw_json\":{}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"tool\",\"summary\":\"Ran shell command\",\"method\":\"shell.exec\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"item_id\":\"item-1\",\"timestamp\":\"2026-04-26T10:00:00Z\",\"recorded_at_ms\":1714000005100,\"raw_json\":{\"tool\":\"exec_command\",\"args\":\"cargo test\"}}\n"
        ),
    )?;

    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();
    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(&addr, "/api/requests/observer:codex:1714000005000-77:1")?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(
        response.contains("\"detail_kind\":\"observer_event\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"provider\":\"codex-app-server\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"thread_id\":\"thread-1\""),
        "response: {response}"
    );
    assert!(response.contains("exec_command"), "response: {response}");

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_server_returns_observer_event_detail_api_payload() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let observer_dir = result
        .storage
        .artifacts_dir
        .join("observer_events")
        .join("codex");
    fs::create_dir_all(&observer_dir)?;
    fs::write(
        observer_dir.join("1714000005000-77.jsonl"),
        concat!(
            "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"ipc\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1714000005000,\"raw_json\":{}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"tool\",\"summary\":\"Ran shell command\",\"method\":\"shell.exec\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"item_id\":\"item-1\",\"timestamp\":\"2026-04-26T10:00:00Z\",\"recorded_at_ms\":1714000005100,\"raw_json\":{\"tool\":\"exec_command\",\"args\":\"cargo test\"}}\n"
        ),
    )?;

    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();
    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(&addr, "/api/events/observer:codex:1714000005000-77:1")?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(
        response.contains("\"event_id\":\"observer:codex:1714000005000-77:1\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"session_id\":\"observer:codex:1714000005000-77\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"kind\":\"observer_event\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"channel\":\"codex-app-server\""),
        "response: {response}"
    );
    assert!(response.contains("\"raw_json\""), "response: {response}");
    assert!(response.contains("exec_command"), "response: {response}");
    assert!(
        !response.contains("\"request\":"),
        "response should not use legacy request envelope: {response}"
    );

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_server_returns_observer_session_capabilities_api_payload() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let observer_dir = result
        .storage
        .artifacts_dir
        .join("observer_events")
        .join("codex");
    fs::create_dir_all(&observer_dir)?;
    fs::write(
        observer_dir.join("1714000005000-77.jsonl"),
        concat!(
            "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"ipc\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1714000005000,\"raw_json\":{}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"skill\",\"summary\":\"skills/list returned 2 entries\",\"method\":\"skills/list\",\"recorded_at_ms\":1714000005100,\"raw_json\":{\"method\":\"skills/list\",\"skill_names_preview\":[\"review\",\"test\"]}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"mcp\",\"summary\":\"mcpServer/listStatus returned 1 entries\",\"method\":\"mcpServer/listStatus\",\"recorded_at_ms\":1714000005200,\"raw_json\":{\"method\":\"mcpServer/listStatus\",\"mcp_server_names_preview\":[\"github\"]}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"plugin\",\"summary\":\"plugin/list returned 1 entries\",\"method\":\"plugin/list\",\"recorded_at_ms\":1714000005300,\"raw_json\":{\"method\":\"plugin/list\",\"marketplace_names_preview\":[\"github\"]}}\n"
        ),
    )?;

    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();
    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(
        &addr,
        "/api/sessions/observer:codex:1714000005000-77/capabilities",
    )?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(
        response.contains("\"session_id\":\"observer:codex:1714000005000-77\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_type\":\"skill\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_name\":\"review\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_type\":\"plugin\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_type\":\"mcp\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_name\":\"github\""),
        "response: {response}"
    );
    assert!(response.contains("\"raw_ref\""), "response: {response}");

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_server_returns_observer_session_diagnostics_api_payload() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let observer_dir = result
        .storage
        .artifacts_dir
        .join("observer_events")
        .join("codex");
    fs::create_dir_all(&observer_dir)?;
    fs::write(
        observer_dir.join("1714000005000-77.jsonl"),
        concat!(
            "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"ipc\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1714000005000,\"raw_json\":{}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"message\",\"summary\":\"User: hello\",\"recorded_at_ms\":1714000005100,\"raw_json\":{\"full_text\":\"hello\\nuse cargo test\"}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"message\",\"summary\":\"User: hello\",\"recorded_at_ms\":1714000005200,\"raw_json\":{\"full_text\":\"hello\\nuse cargo clippy\"}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"skill\",\"summary\":\"skills/list returned 1 entries\",\"method\":\"skills/list\",\"recorded_at_ms\":1714000005300,\"raw_json\":{\"method\":\"skills/list\",\"skill_names_preview\":[\"review\"]}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"mcp\",\"summary\":\"mcpServer/listStatus returned 1 entries\",\"method\":\"mcpServer/listStatus\",\"recorded_at_ms\":1714000005400,\"raw_json\":{\"method\":\"mcpServer/listStatus\",\"mcp_server_names_preview\":[\"github\"]}}\n"
        ),
    )?;

    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();
    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(
        &addr,
        "/api/sessions/observer:codex:1714000005000-77/diagnostics",
    )?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(response.contains("\"diagnostics\""), "response: {response}");
    assert!(
        response.contains("\"prompt_diff_count\":1"),
        "response: {response}"
    );
    assert!(
        response.contains("\"skill_status\":\"available\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"skill_name\":\"review\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_type\":\"mcp\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"visible_mcp_servers\":[\"github\"]"),
        "response: {response}"
    );
    assert!(
        response.contains("\"capability_type_count\":2"),
        "response: {response}"
    );
    assert!(
        response.contains("use cargo clippy"),
        "response: {response}"
    );

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_script_prefers_event_detail_api_for_read_model_event_ids() {
    let script = include_str!("../../assets/console.js");

    assert!(
        script.contains("const detailApiPathForRequestId"),
        "console.js should centralize timeline detail route selection"
    );
    assert!(
        script.contains("requestId.startsWith(\"observer:\")"),
        "observer event ids should use the event detail API"
    );
    assert!(
        script.contains("requestId.startsWith(\"codex-thread:\")"),
        "codex rollout event ids should use the event detail API"
    );
    assert!(
        script.contains("`/api/events/${requestId}`"),
        "read model event ids should fetch /api/events/:event_id"
    );
    assert!(
        script.contains("`/api/requests/${requestId}`"),
        "legacy request ids should keep the /api/requests/:request_id adapter"
    );
}

#[test]
fn console_script_fetches_and_renders_session_capabilities() {
    let script = include_str!("../../assets/console.js");

    assert!(
        script.contains("renderCapabilityStrip"),
        "console.js should render the capability projection strip"
    );
    assert!(
        script.contains("`/api/sessions/${sessionId}/capabilities`"),
        "session detail loading should fetch session capability projections"
    );
    assert!(
        script.contains("state.sessionCapabilities"),
        "capabilities should be kept separate from timeline detail state"
    );
    assert!(
        script.contains("[\"agent\", \"app\", \"mcp\", \"plugin\", \"provider\", \"skill\"]"),
        "console timeline should classify source-specific capability facts as capability snapshots"
    );
    assert!(
        script.contains("mcp: \"hub\""),
        "console capability strip should render MCP as its own capability type"
    );
}

#[test]
fn console_script_uses_paginated_session_and_timeline_apis() {
    let script = include_str!("../../assets/console.js");

    assert!(
        script.contains("SESSION_PAGE_LIMIT"),
        "console.js should define a bounded session page size"
    );
    assert!(
        script.contains("`/api/sessions?limit=${SESSION_PAGE_LIMIT}`"),
        "initial session load should use the paginated sessions API"
    );
    assert!(
        script.contains("data-load-more-sessions"),
        "session list should expose an explicit load-more control"
    );
    assert!(
        script.contains("SESSION_EVENT_PAGE_LIMIT"),
        "console.js should define a bounded session event page size"
    );
    assert!(
        script.contains("`/api/sessions/${sessionId}/events?limit=${SESSION_EVENT_PAGE_LIMIT}`"),
        "session detail loading should use the paginated session events API"
    );
    assert!(
        script.contains("data-load-more-session-events"),
        "timeline should expose an explicit load-more control"
    );
}

#[test]
fn console_script_fetches_and_renders_session_diagnostics() {
    let script = include_str!("../../assets/console.js");
    let html = include_str!("../../assets/console.html");

    assert!(
        html.contains("diagnostics-panel-region"),
        "console.html should provide an explicit diagnostics panel region"
    );
    assert!(
        script.contains("state.sessionDiagnostics"),
        "diagnostics should be stored separately from request detail state"
    );
    assert!(
        script.contains("`/api/sessions/${sessionId}/diagnostics`"),
        "session detail loading should fetch the diagnostics API"
    );
    assert!(
        script.contains("renderDiagnosticsPanel"),
        "console.js should render an explicit diagnostics panel"
    );
    assert!(
        script.contains("diagnostics.capability_inventory"),
        "diagnostics panel should read grouped capability inventory"
    );
    assert!(
        script.contains("diagnostics.visible_skills"),
        "diagnostics panel should keep the existing visible skill summary"
    );
    assert!(
        script.contains("capabilityIcon(type)"),
        "diagnostics panel should render source-specific capability icons"
    );
}

#[test]
fn console_observer_module_no_longer_owns_legacy_detail_adapters() {
    let module = include_str!("observer.rs");

    assert!(
        !module.contains("load_observer_request_detail_payload"),
        "observer detail payloads should be served by the read model API adapter"
    );
    assert!(
        !module.contains("load_observer_session_detail_payload"),
        "observer session detail payloads should be served by the read model API adapter"
    );
    assert!(
        !module.contains("load_codex_rollout_request_detail_payload"),
        "codex rollout detail payloads should be served by the read model API adapter"
    );
    assert!(
        !module.contains("load_codex_rollout_session_detail_payload"),
        "codex rollout session detail payloads should be served by the read model API adapter"
    );
}

#[test]
fn console_server_returns_observer_session_detail_api_payload() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let observer_dir = result
        .storage
        .artifacts_dir
        .join("observer_events")
        .join("codex");
    fs::create_dir_all(&observer_dir)?;
    fs::write(
        observer_dir.join("1714000005000-77.jsonl"),
        concat!(
            "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"ipc\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1714000005000,\"raw_json\":{}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Thread started\",\"method\":\"thread.start\",\"thread_id\":\"thread-1\",\"turn_id\":null,\"item_id\":null,\"timestamp\":\"2026-04-26T10:00:00Z\",\"recorded_at_ms\":1714000005100,\"raw_json\":{\"thread\":\"thread-1\"}}\n"
        ),
    )?;

    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();
    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(&addr, "/api/sessions/observer:codex:1714000005000-77")?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(
        response.contains("\"detail_kind\":\"observer_session\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"target_display_name\":\"Codex App Server\""),
        "response: {response}"
    );
    assert!(response.contains("Thread started"), "response: {response}");

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_server_returns_observer_session_events_api_payload() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let observer_dir = result
        .storage
        .artifacts_dir
        .join("observer_events")
        .join("codex");
    fs::create_dir_all(&observer_dir)?;
    fs::write(
        observer_dir.join("1714000005000-77.jsonl"),
        concat!(
            "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"ipc\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1714000005000,\"raw_json\":{}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Thread started\",\"method\":\"thread.start\",\"thread_id\":\"thread-1\",\"turn_id\":null,\"item_id\":null,\"timestamp\":\"2026-04-26T10:00:00Z\",\"recorded_at_ms\":1714000005100,\"raw_json\":{\"thread\":\"thread-1\"}}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"tool\",\"summary\":\"Ran shell command\",\"method\":\"shell.exec\",\"thread_id\":\"thread-1\",\"turn_id\":\"turn-1\",\"item_id\":\"item-1\",\"timestamp\":\"2026-04-26T10:00:01Z\",\"recorded_at_ms\":1714000005200,\"raw_json\":{\"tool\":\"exec_command\"}}\n"
        ),
    )?;

    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();
    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(
        &addr,
        "/api/sessions/observer:codex:1714000005000-77/events?limit=1",
    )?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(
        response.contains("\"session_id\":\"observer:codex:1714000005000-77\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"request_id\":\"observer:codex:1714000005000-77:1\""),
        "response: {response}"
    );
    assert!(
        !response.contains("\"request_id\":\"observer:codex:1714000005000-77:2\""),
        "response: {response}"
    );
    assert!(
        response.contains("\"next_cursor\":\"1\""),
        "response: {response}"
    );

    handle.join().expect("server thread should join")?;
    fs::remove_dir_all(result.config.state_root)?;
    Ok(())
}

#[test]
fn console_server_sessions_api_keeps_observer_artifacts_out_of_session_list() -> io::Result<()> {
    let workspace_root = unique_test_dir();
    let result = bootstrap(&workspace_root)?;
    let observer_dir = result
        .storage
        .artifacts_dir
        .join("observer_events")
        .join("codex");
    fs::create_dir_all(&observer_dir)?;
    fs::write(
        observer_dir.join("1714000005000-77.jsonl"),
        concat!(
            "{\"record_type\":\"handshake\",\"channel\":\"codex-app-server\",\"transport\":\"ipc\",\"server_label\":\"Codex App Server\",\"recorded_at_ms\":1714000005000,\"raw_json\":{}}\n",
            "{not-json}\n",
            "{\"record_type\":\"event\",\"channel\":\"codex-app-server\",\"event_kind\":\"thread\",\"summary\":\"Thread survived malformed line\",\"method\":\"thread.start\",\"thread_id\":\"thread-1\",\"recorded_at_ms\":1714000005100,\"raw_json\":{\"thread\":\"thread-1\"}}\n"
        ),
    )?;

    let server = start_console_server_on_bind_addr(&result, "127.0.0.1:0", None)?;
    let addr = server
        .local_url()?
        .trim_start_matches("http://")
        .to_string();
    let handle = thread::spawn(move || server.serve_once());

    let response = send_get_request(&addr, "/api/sessions")?;

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response: {response}"
    );
    assert!(
        !response.contains("observer:codex:1714000005000-77"),
        "response: {response}"
    );
    assert!(
        !response.contains("Thread survived malformed line"),
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
        format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").as_bytes(),
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
