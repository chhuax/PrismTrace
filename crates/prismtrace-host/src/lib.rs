pub mod attach;
pub mod discovery;
pub mod ipc;
pub mod probe_health;
pub mod readiness;
pub mod request_capture;
pub mod runtime;

use attach::{AttachBackend, AttachController, LiveAttachBackend, attach_report};
use discovery::{ProcessSampleSource, discover_targets};
use prismtrace_core::{
    AttachFailure, AttachFailureKind, AttachReadiness, AttachSession, ProbeHealth, ProcessTarget,
};
use prismtrace_storage::StorageLayout;
use readiness::evaluate_targets;
use request_capture::ProbeConsumeExit;
use runtime::InstrumentationRuntime;
use std::io;
use std::io::Write;
use std::path::PathBuf;

pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:7799";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub workspace_root: PathBuf,
    pub state_root: PathBuf,
    pub bind_addr: String,
}

impl AppConfig {
    pub fn from_workspace_root(root: impl Into<PathBuf>) -> Self {
        let workspace_root = root.into();
        let state_root = workspace_root.join(".prismtrace");

        Self {
            workspace_root,
            state_root,
            bind_addr: DEFAULT_BIND_ADDR.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapResult {
    pub config: AppConfig,
    pub storage: StorageLayout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSnapshot {
    pub summary: String,
    pub discovered_targets: Vec<ProcessTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessSnapshot {
    pub summary: String,
    pub readiness_results: Vec<AttachReadiness>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachSnapshot {
    pub summary: String,
    pub attach_result: Result<AttachSession, AttachFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachSnapshot {
    pub summary: String,
    pub detach_result: Result<AttachSession, AttachFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachStatusSnapshot {
    pub summary: String,
    pub active_session: Option<AttachSession>,
    pub probe_health: Option<ProbeHealth>,
}

pub fn bootstrap(root: impl Into<PathBuf>) -> io::Result<BootstrapResult> {
    let config = AppConfig::from_workspace_root(root);
    let storage = StorageLayout::new(&config.state_root);

    storage.initialize()?;

    Ok(BootstrapResult { config, storage })
}

pub fn startup_summary(result: &BootstrapResult) -> String {
    format!(
        "PrismTrace host skeleton\nbind: {}\nstate root: {}\ndb: {}\nartifacts: {}",
        result.config.bind_addr,
        result.config.state_root.display(),
        result.storage.db_path.display(),
        result.storage.artifacts_dir.display()
    )
}

pub fn collect_host_snapshot(
    result: &BootstrapResult,
    source: &impl ProcessSampleSource,
) -> io::Result<HostSnapshot> {
    let discovered_targets = discover_targets(source)?;

    Ok(HostSnapshot {
        summary: startup_summary(result),
        discovered_targets,
    })
}

pub fn discovery_report(snapshot: &HostSnapshot) -> String {
    let mut report = vec![
        snapshot.summary.clone(),
        format!(
            "Discovered {} process targets",
            snapshot.discovered_targets.len()
        ),
    ];

    report.extend(snapshot.discovered_targets.iter().map(|target| {
        format!(
            "[{}] {} (pid {}) {}",
            target.runtime_kind.label(),
            target.display_name(),
            target.pid,
            target.executable_path.display()
        )
    }));

    report.join("\n")
}

pub fn collect_readiness_snapshot(
    result: &BootstrapResult,
    source: &impl ProcessSampleSource,
) -> io::Result<ReadinessSnapshot> {
    let discovered_targets = discover_targets(source)?;
    let readiness_results = evaluate_targets(&discovered_targets);

    Ok(ReadinessSnapshot {
        summary: startup_summary(result),
        readiness_results,
    })
}

pub fn readiness_report(snapshot: &ReadinessSnapshot) -> String {
    let mut report = vec![
        snapshot.summary.clone(),
        format!(
            "Evaluated {} attach readiness results",
            snapshot.readiness_results.len()
        ),
    ];

    report.extend(
        snapshot
            .readiness_results
            .iter()
            .map(AttachReadiness::summary),
    );

    report.join("\n")
}

pub fn collect_attach_snapshot<B: AttachBackend>(
    result: &BootstrapResult,
    source: &impl ProcessSampleSource,
    backend: B,
    pid: u32,
) -> io::Result<AttachSnapshot> {
    let discovered_targets = discover_targets(source)?;
    let readiness_results = evaluate_targets(&discovered_targets);
    let attach_result = match readiness_results
        .iter()
        .find(|readiness| readiness.target.pid == pid)
    {
        Some(readiness) => {
            let mut controller = AttachController::new(backend);
            controller.attach(readiness)
        }
        None => Err(AttachFailure {
            kind: prismtrace_core::AttachFailureKind::NotReady,
            reason: format!("no discovered target with pid {pid} is available for attach"),
        }),
    };

    Ok(AttachSnapshot {
        summary: startup_summary(result),
        attach_result,
    })
}

pub fn attach_snapshot_report(snapshot: &AttachSnapshot) -> String {
    [
        snapshot.summary.clone(),
        attach_report(&snapshot.attach_result),
    ]
    .join("\n")
}

pub fn run_foreground_attach_session<R: InstrumentationRuntime>(
    result: &BootstrapResult,
    source: &impl ProcessSampleSource,
    runtime: R,
    pid: u32,
    output: &mut impl Write,
) -> io::Result<()> {
    let discovered_targets = discover_targets(source)?;
    let readiness_results = evaluate_targets(&discovered_targets);
    let readiness = readiness_results
        .iter()
        .find(|entry| entry.target.pid == pid)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no discovered target with pid {pid} is available for attach"),
            )
        })?;

    let mut controller = AttachController::new(LiveAttachBackend::new(runtime));
    let attached_session = controller
        .attach(readiness)
        .map_err(attach_failure_as_io_error)?;

    writeln!(output, "{}", attached_session.summary())?;

    let listener = controller.take_listener().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            format!(
                "ipc listener is unavailable after successful attach for pid {}",
                attached_session.target.pid
            ),
        )
    })?;

    let consume_outcome = request_capture::consume_probe_events(
        &result.storage,
        &attached_session.target,
        listener,
        output,
    )?;

    if let Some(listener) = consume_outcome.listener {
        controller.restore_listener(listener);
    }

    let detach_result = if matches!(&consume_outcome.exit, ProbeConsumeExit::DetachAck) {
        Ok(())
    } else {
        controller
            .detach()
            .map(|_| ())
            .map_err(attach_failure_as_io_error)
    };

    match (
        foreground_exit_as_error(&consume_outcome.exit),
        detach_result,
    ) {
        (None, Ok(())) => Ok(()),
        (Some(err), Ok(())) => Err(err),
        (None, Err(detach_err)) => Err(detach_err),
        (Some(exit_err), Err(detach_err)) => Err(io::Error::new(
            exit_err.kind(),
            format!("{exit_err}; detach cleanup failed: {detach_err}"),
        )),
    }
}

fn attach_failure_as_io_error(failure: AttachFailure) -> io::Error {
    let kind = match failure.kind {
        AttachFailureKind::NotReady => io::ErrorKind::InvalidInput,
        AttachFailureKind::BackendRejected => io::ErrorKind::PermissionDenied,
        AttachFailureKind::HandshakeFailed => io::ErrorKind::ConnectionAborted,
        AttachFailureKind::ActiveSessionExists => io::ErrorKind::AlreadyExists,
        AttachFailureKind::NoActiveSession => io::ErrorKind::NotFound,
        AttachFailureKind::DetachFailed => io::ErrorKind::BrokenPipe,
    };

    io::Error::new(kind, failure.summary())
}

fn foreground_exit_as_error(exit: &ProbeConsumeExit) -> Option<io::Error> {
    match exit {
        ProbeConsumeExit::HeartbeatTimeout { elapsed_ms } => Some(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("probe heartbeat timed out after {elapsed_ms} ms"),
        )),
        ProbeConsumeExit::DetachAck | ProbeConsumeExit::ChannelDisconnected { .. } => None,
    }
}

/// Detach the active session using the given backend-aware controller.
pub fn collect_detach_snapshot<B: AttachBackend>(
    result: &BootstrapResult,
    controller: &mut AttachController<B>,
) -> io::Result<DetachSnapshot> {
    let detach_result = controller.detach();
    Ok(DetachSnapshot {
        summary: startup_summary(result),
        detach_result,
    })
}

/// Read-only query of current attach status. Does NOT modify session state.
pub fn collect_attach_status_snapshot(
    result: &BootstrapResult,
    active_session: Option<AttachSession>,
    probe_health: Option<ProbeHealth>,
) -> io::Result<AttachStatusSnapshot> {
    Ok(AttachStatusSnapshot {
        summary: startup_summary(result),
        active_session,
        probe_health,
    })
}

pub fn detach_report(snapshot: &DetachSnapshot) -> String {
    let result_line = match &snapshot.detach_result {
        Ok(session) => format!(
            "[detached] {} (pid {})",
            session.target.display_name(),
            session.target.pid
        ),
        Err(failure) => failure.summary(),
    };
    [snapshot.summary.clone(), result_line].join("\n")
}

pub fn attach_status_report(snapshot: &AttachStatusSnapshot) -> String {
    let status_line = match &snapshot.active_session {
        None => "no active attach session".to_string(),
        Some(session) => {
            let health_summary = match &snapshot.probe_health {
                Some(health) => format!(
                    "probe: installed={}, failed={}{}",
                    health.installed_hooks.len(),
                    health.failed_hooks.len(),
                    if health.failed_hooks.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", health.failed_hooks.join(", "))
                    }
                ),
                None => "probe: no health data".to_string(),
            };
            format!(
                "[{}] {} (pid {})\n{}",
                session.state.label(),
                session.target.display_name(),
                session.target.pid,
                health_summary
            )
        }
    };
    [snapshot.summary.clone(), status_line].join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, AttachStatusSnapshot, DEFAULT_BIND_ADDR, DetachSnapshot, attach_status_report,
        bootstrap, collect_attach_status_snapshot, collect_detach_snapshot, collect_host_snapshot,
        collect_readiness_snapshot, detach_report, startup_summary,
    };
    use crate::attach::{AttachController, ScriptedAttachBackend};
    use crate::discovery::StaticProcessSampleSource;
    use prismtrace_core::{
        AttachFailureKind, AttachReadiness, AttachReadinessStatus, AttachSession,
        AttachSessionState, HttpHeader, IpcMessage, ProbeHealth, ProbeState, ProcessSample,
        ProcessTarget, RuntimeKind,
    };
    use std::fs;
    use std::io;
    use std::io::{BufRead, Cursor};
    use std::path::PathBuf;
    use std::process;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn app_config_uses_a_hidden_state_directory_inside_the_workspace() {
        let config = AppConfig::from_workspace_root("/tmp/prismtrace-workspace");

        assert_eq!(
            config.workspace_root,
            PathBuf::from("/tmp/prismtrace-workspace")
        );
        assert_eq!(
            config.state_root,
            PathBuf::from("/tmp/prismtrace-workspace/.prismtrace")
        );
        assert_eq!(config.bind_addr, DEFAULT_BIND_ADDR);
    }

    #[test]
    fn bootstrap_creates_storage_under_the_hidden_state_directory() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;

        assert_eq!(result.config.workspace_root, workspace_root);
        assert_eq!(
            result.storage.db_path,
            result
                .config
                .state_root
                .join("state")
                .join("observability.db")
        );
        assert!(result.storage.artifacts_dir.is_dir());

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn startup_summary_mentions_bind_address_and_storage_paths() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let summary = startup_summary(&result);

        assert!(summary.contains("PrismTrace host skeleton"));
        assert!(summary.contains(DEFAULT_BIND_ADDR));
        assert!(summary.contains(result.storage.db_path.to_string_lossy().as_ref()));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn collect_host_snapshot_returns_discovered_targets() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![
            ProcessSample {
                pid: 101,
                process_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
            },
            ProcessSample {
                pid: 102,
                process_name: "Electron".into(),
                executable_path: PathBuf::from(
                    "/Applications/Electron.app/Contents/MacOS/Electron",
                ),
            },
            ProcessSample {
                pid: 103,
                process_name: "python3".into(),
                executable_path: PathBuf::from("/usr/bin/python3"),
            },
        ]);

        let snapshot = collect_host_snapshot(&result, &source)?;

        assert_eq!(snapshot.discovered_targets.len(), 3);
        assert!(snapshot.summary.contains("PrismTrace host skeleton"));
        assert_eq!(snapshot.discovered_targets[0].app_name, "node");
        assert_eq!(
            snapshot.discovered_targets[1].runtime_kind.label(),
            "electron"
        );
        assert_eq!(
            snapshot.discovered_targets[2].runtime_kind.label(),
            "unknown"
        );

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn discovery_report_lists_targets_with_runtime_labels() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![
            ProcessSample {
                pid: 220,
                process_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
            },
            ProcessSample {
                pid: 221,
                process_name: "python3".into(),
                executable_path: PathBuf::from("/usr/bin/python3"),
            },
        ]);

        let snapshot = collect_host_snapshot(&result, &source)?;
        let report = super::discovery_report(&snapshot);

        assert!(report.contains("Discovered 2 process targets"));
        assert!(report.contains("[node] node (pid 220)"));
        assert!(report.contains("[unknown] python3 (pid 221)"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn collect_readiness_snapshot_returns_structured_results() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![
            ProcessSample {
                pid: 301,
                process_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
            },
            ProcessSample {
                pid: 302,
                process_name: "python3".into(),
                executable_path: PathBuf::from("/usr/bin/python3"),
            },
        ]);

        let snapshot = collect_readiness_snapshot(&result, &source)?;

        assert_eq!(snapshot.readiness_results.len(), 2);
        assert_eq!(snapshot.readiness_results[0].status.label(), "supported");
        assert_eq!(snapshot.readiness_results[1].status.label(), "unknown");

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn readiness_report_lists_status_and_reason() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![
            ProcessSample {
                pid: 303,
                process_name: "Electron".into(),
                executable_path: PathBuf::from(
                    "/Applications/Electron.app/Contents/MacOS/Electron",
                ),
            },
            ProcessSample {
                pid: 304,
                process_name: "launchd".into(),
                executable_path: PathBuf::from("/sbin/launchd"),
            },
        ]);

        let snapshot = collect_readiness_snapshot(&result, &source)?;
        let report = super::readiness_report(&snapshot);

        assert!(report.contains("Evaluated 2 attach readiness results"));
        assert!(report.contains("[supported] Electron"));
        assert!(report.contains("[permission_denied] launchd"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn collect_attach_snapshot_returns_structured_attached_session() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![ProcessSample {
            pid: 401,
            process_name: "Electron".into(),
            executable_path: PathBuf::from("/Applications/Electron.app/Contents/MacOS/Electron"),
        }]);

        let snapshot = super::collect_attach_snapshot(
            &result,
            &source,
            crate::attach::ScriptedAttachBackend::ready(),
            401,
        )?;

        assert_eq!(
            snapshot
                .attach_result
                .as_ref()
                .expect("attach should succeed")
                .state
                .label(),
            "attached"
        );

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn collect_attach_snapshot_returns_structured_failure_for_unknown_pid() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![ProcessSample {
            pid: 402,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
        }]);

        let snapshot = super::collect_attach_snapshot(
            &result,
            &source,
            crate::attach::ScriptedAttachBackend::ready(),
            999,
        )?;

        assert_eq!(
            snapshot
                .attach_result
                .as_ref()
                .expect_err("missing pid should fail")
                .kind
                .label(),
            "not_ready"
        );

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn run_foreground_attach_session_returns_error_for_missing_pid_target() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![ProcessSample {
            pid: 410,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
        }]);
        let runtime = crate::runtime::ScriptedInstrumentationRuntime::success_with_messages(vec![]);
        let mut output = Vec::new();

        let error =
            super::run_foreground_attach_session(&result, &source, runtime, 999_999, &mut output)
                .expect_err("missing pid target should fail");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(error.to_string().contains("999999"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn run_foreground_attach_session_attempts_detach_cleanup_after_capture_loop() -> io::Result<()>
    {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![ProcessSample {
            pid: 411,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
        }]);
        let detach_called = Arc::new(AtomicBool::new(false));
        let runtime = TrackingRuntime::new(
            vec![
                IpcMessage::BootstrapReport {
                    installed_hooks: vec!["fetch".into()],
                    failed_hooks: vec![],
                    timestamp_ms: 1,
                }
                .to_json_line(),
                IpcMessage::HttpRequestObserved {
                    hook_name: "fetch".into(),
                    method: "POST".into(),
                    url: "https://api.openai.com/v1/responses".into(),
                    headers: vec![HttpHeader {
                        name: "content-type".into(),
                        value: "application/json".into(),
                    }],
                    body_text: Some("{}".into()),
                    body_truncated: false,
                    timestamp_ms: 2,
                }
                .to_json_line(),
            ],
            Arc::clone(&detach_called),
        );
        let mut output = Vec::new();

        super::run_foreground_attach_session(&result, &source, runtime, 411, &mut output)?;

        assert!(
            detach_called.load(Ordering::SeqCst),
            "foreground attach should attempt detach cleanup before returning"
        );

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn attach_report_lists_startup_summary_and_attach_result() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let source = StaticProcessSampleSource::new(vec![ProcessSample {
            pid: 403,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
        }]);

        let snapshot = super::collect_attach_snapshot(
            &result,
            &source,
            crate::attach::ScriptedAttachBackend::ready(),
            403,
        )?;
        let report = super::attach_snapshot_report(&snapshot);

        assert!(report.contains("PrismTrace host skeleton"));
        assert!(report.contains("[attached]"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("prismtrace-host-test-{}-{}", process::id(), nanos))
    }

    // --- Task 6.4 tests ---

    #[test]
    fn collect_detach_snapshot_returns_no_active_session_when_no_session_exists() -> io::Result<()>
    {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let mut controller = AttachController::new(ScriptedAttachBackend::ready());

        let snapshot = collect_detach_snapshot(&result, &mut controller)?;

        let failure = snapshot
            .detach_result
            .expect_err("detach without session should fail");
        assert_eq!(failure.kind, AttachFailureKind::NoActiveSession);

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn collect_detach_snapshot_returns_detached_session_when_session_exists() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let mut controller = AttachController::new(ScriptedAttachBackend::ready());
        controller
            .attach(&supported_readiness(501))
            .expect("attach should succeed");

        let snapshot = collect_detach_snapshot(&result, &mut controller)?;

        let session = snapshot
            .detach_result
            .expect("detach with active session should succeed");
        assert_eq!(session.state, AttachSessionState::Detached);

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn collect_attach_status_snapshot_returns_no_session_when_none() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;

        let snapshot = collect_attach_status_snapshot(&result, None, None)?;

        assert!(snapshot.active_session.is_none());

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn attach_status_report_contains_no_active_session_message() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;

        let snapshot = collect_attach_status_snapshot(&result, None, None)?;
        let report = attach_status_report(&snapshot);

        assert!(report.contains("no active attach session"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn attach_status_report_contains_probe_health_summary() {
        let session = AttachSession {
            target: ProcessTarget {
                pid: 503,
                app_name: "TestApp".into(),
                executable_path: PathBuf::from("/usr/bin/testapp"),
                runtime_kind: RuntimeKind::Node,
            },
            state: AttachSessionState::Attached,
            detail: "probe handshake completed".into(),
            bootstrap: None,
            failure: None,
        };
        let health = ProbeHealth {
            state: ProbeState::Attached,
            installed_hooks: vec!["fetch".into(), "http".into()],
            failed_hooks: vec!["undici".into()],
        };
        let snapshot = AttachStatusSnapshot {
            summary: "test summary".into(),
            active_session: Some(session),
            probe_health: Some(health),
        };

        let report = attach_status_report(&snapshot);

        assert!(report.contains("installed=2"), "report: {report}");
        assert!(report.contains("failed=1"), "report: {report}");
        assert!(report.contains("undici"), "report: {report}");
    }

    #[test]
    fn detach_report_contains_detached_and_pid() {
        let session = AttachSession {
            target: ProcessTarget {
                pid: 502,
                app_name: "TestApp".into(),
                executable_path: PathBuf::from("/usr/bin/testapp"),
                runtime_kind: RuntimeKind::Node,
            },
            state: AttachSessionState::Detached,
            detail: "attach session detached".into(),
            bootstrap: None,
            failure: None,
        };
        let snapshot = DetachSnapshot {
            summary: "test summary".into(),
            detach_result: Ok(session),
        };

        let report = detach_report(&snapshot);

        assert!(report.contains("[detached]"), "report: {report}");
        assert!(report.contains("502"), "report: {report}");
    }

    #[test]
    fn attach_status_report_does_not_modify_session_state() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;

        let snapshot1 = collect_attach_status_snapshot(&result, None, None)?;
        let snapshot2 = collect_attach_status_snapshot(&result, None, None)?;

        assert_eq!(snapshot1, snapshot2);

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    fn supported_readiness(pid: u32) -> AttachReadiness {
        AttachReadiness {
            target: ProcessTarget {
                pid,
                app_name: format!("TestApp-{pid}"),
                executable_path: PathBuf::from("/Applications/TestApp.app/Contents/MacOS/TestApp"),
                runtime_kind: RuntimeKind::Electron,
            },
            status: AttachReadinessStatus::Supported,
            reason: "electron runtime target suitable for attach".into(),
        }
    }

    struct TrackingRuntime {
        messages: Vec<String>,
        detach_called: Arc<AtomicBool>,
    }

    impl TrackingRuntime {
        fn new(messages: Vec<String>, detach_called: Arc<AtomicBool>) -> Self {
            Self {
                messages,
                detach_called,
            }
        }
    }

    impl crate::runtime::InstrumentationRuntime for TrackingRuntime {
        fn inject_probe(
            &self,
            _pid: u32,
            _probe_script: &str,
        ) -> Result<Box<dyn BufRead + Send>, crate::runtime::InstrumentationError> {
            let content = self.messages.join("");
            Ok(Box::new(Cursor::new(content.into_bytes())))
        }

        fn send_detach_signal(
            &self,
            _pid: u32,
        ) -> Result<(), crate::runtime::InstrumentationError> {
            self.detach_called.store(true, Ordering::SeqCst);
            Ok(())
        }
    }
}
