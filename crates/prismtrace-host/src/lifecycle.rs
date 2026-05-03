use crate::discovery::{ProcessSampleSource, discover_targets};
use crate::sources;
use prismtrace_core::ProcessTarget;
use prismtrace_storage::StorageLayout;
use std::env;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:7799";
pub const STATE_ROOT_ENV: &str = "PRISMTRACE_STATE_ROOT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub state_root: PathBuf,
    pub bind_addr: String,
}

impl AppConfig {
    pub fn from_state_root(state_root: impl Into<PathBuf>) -> Self {
        Self {
            state_root: state_root.into(),
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

pub fn default_user_state_root_from_home(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref()
        .join("Library")
        .join("Application Support")
        .join("PrismTrace")
}

pub fn default_user_state_root() -> io::Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is not set; pass --state-root <path>",
        )
    })?;
    Ok(default_user_state_root_from_home(home))
}

pub fn resolve_state_root(explicit_state_root: Option<PathBuf>) -> io::Result<PathBuf> {
    if let Some(state_root) = explicit_state_root {
        return Ok(state_root);
    }
    if let Some(state_root) = env::var_os(STATE_ROOT_ENV) {
        return Ok(PathBuf::from(state_root));
    }
    default_user_state_root()
}

pub fn bootstrap(state_root: impl Into<PathBuf>) -> io::Result<BootstrapResult> {
    let config = AppConfig::from_state_root(state_root);
    let storage = StorageLayout::new(&config.state_root);

    storage.initialize()?;

    Ok(BootstrapResult { config, storage })
}

pub fn bootstrap_for_invocation(
    explicit_state_root: Option<PathBuf>,
    invocation_cwd: impl AsRef<Path>,
) -> io::Result<BootstrapResult> {
    let state_root = resolve_state_root(explicit_state_root)?;
    let result = bootstrap(state_root)?;
    let legacy_root = invocation_cwd.as_ref().join(".prismtrace");
    let _ = import_legacy_workspace_artifacts(&legacy_root, &result.storage);
    Ok(result)
}

pub fn import_legacy_workspace_artifacts(
    legacy_root: &Path,
    storage: &StorageLayout,
) -> io::Result<usize> {
    if legacy_root == storage.root {
        return Ok(0);
    }

    let source = legacy_root.join("state").join("artifacts");
    if !source.is_dir() {
        return Ok(0);
    }

    copy_missing_files_recursively(&source, &storage.artifacts_dir)
}

fn copy_missing_files_recursively(source: &Path, destination: &Path) -> io::Result<usize> {
    let mut copied = 0;
    fs::create_dir_all(destination)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            copied += copy_missing_files_recursively(&source_path, &destination_path)?;
            continue;
        }

        if source_path.is_file() && !destination_path.exists() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
            copied += 1;
        }
    }

    Ok(copied)
}

pub fn startup_summary(result: &BootstrapResult) -> String {
    format!(
        "PrismTrace host skeleton\nscope: local machine\nbind: {}\nstate root: {}\ndb: {}\nartifacts: {}",
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

pub fn run_codex_observer_session(
    result: &BootstrapResult,
    options: sources::codex::CodexObserverOptions,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(output, "{}", startup_summary(result))?;
    sources::codex::run_codex_observer(&result.storage, output, options)
}

pub fn run_claude_observer_session(
    result: &BootstrapResult,
    options: sources::claude::ClaudeObserverOptions,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(output, "{}", startup_summary(result))?;
    sources::claude::run_claude_observer(&result.storage, output, options)
}

pub fn run_opencode_observer_session(
    result: &BootstrapResult,
    options: sources::opencode::OpencodeObserverOptions,
    output: &mut impl Write,
) -> io::Result<()> {
    writeln!(output, "{}", startup_summary(result))?;
    sources::opencode::run_opencode_observer(&result.storage, output, options)
}

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, DEFAULT_BIND_ADDR, bootstrap, collect_host_snapshot,
        default_user_state_root_from_home, import_legacy_workspace_artifacts,
        run_claude_observer_session, run_opencode_observer_session, startup_summary,
    };
    use crate::claude_observer::ClaudeObserverOptions;
    use crate::discovery::StaticProcessSampleSource;
    use crate::opencode_observer::{OpencodeObserverOptions, spawn_test_opencode_server};
    use prismtrace_core::ProcessSample;
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn app_config_uses_explicit_state_root() {
        let config = AppConfig::from_state_root("/tmp/prismtrace-state");

        assert_eq!(config.state_root, PathBuf::from("/tmp/prismtrace-state"));
        assert_eq!(config.bind_addr, DEFAULT_BIND_ADDR);
    }

    #[test]
    fn default_user_state_root_uses_macos_application_support() {
        assert_eq!(
            default_user_state_root_from_home("/Users/tester"),
            PathBuf::from("/Users/tester/Library/Application Support/PrismTrace")
        );
    }

    #[test]
    fn bootstrap_creates_storage_under_the_state_root() -> io::Result<()> {
        let state_root = unique_test_dir();
        let result = bootstrap(&state_root)?;

        assert_eq!(result.config.state_root, state_root);
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
    fn import_legacy_workspace_artifacts_copies_missing_files_without_overwrite() -> io::Result<()>
    {
        let legacy_workspace = unique_test_dir();
        let state_root = unique_test_dir();
        let result = bootstrap(&state_root)?;
        let legacy_artifacts = legacy_workspace
            .join(".prismtrace")
            .join("state")
            .join("artifacts")
            .join("observer_events")
            .join("opencode");
        fs::create_dir_all(&legacy_artifacts)?;
        fs::write(legacy_artifacts.join("session-1.jsonl"), "legacy\n")?;

        let destination = result
            .storage
            .artifacts_dir
            .join("observer_events")
            .join("opencode");
        fs::create_dir_all(&destination)?;
        fs::write(destination.join("session-2.jsonl"), "existing\n")?;

        let copied = import_legacy_workspace_artifacts(
            &legacy_workspace.join(".prismtrace"),
            &result.storage,
        )?;

        assert_eq!(copied, 1);
        assert_eq!(
            fs::read_to_string(destination.join("session-1.jsonl"))?,
            "legacy\n"
        );
        assert_eq!(
            fs::read_to_string(destination.join("session-2.jsonl"))?,
            "existing\n"
        );

        fs::remove_dir_all(legacy_workspace)?;
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
                command_line: None,
            },
            ProcessSample {
                pid: 102,
                process_name: "Electron".into(),
                executable_path: PathBuf::from(
                    "/Applications/Electron.app/Contents/MacOS/Electron",
                ),
                command_line: None,
            },
            ProcessSample {
                pid: 103,
                process_name: "python3".into(),
                executable_path: PathBuf::from("/usr/bin/python3"),
                command_line: None,
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
                command_line: None,
            },
            ProcessSample {
                pid: 221,
                process_name: "python3".into(),
                executable_path: PathBuf::from("/usr/bin/python3"),
                command_line: None,
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
    fn run_claude_observer_session_passes_storage_to_artifact_writer() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let transcript_root = unique_test_dir().join("transcripts");
        fs::create_dir_all(&transcript_root)?;
        fs::write(
            transcript_root.join("session.jsonl"),
            "{\"type\":\"user\",\"session_id\":\"session-1\",\"id\":\"turn-1\"}\n",
        )?;

        let mut output = Vec::new();
        run_claude_observer_session(
            &result,
            ClaudeObserverOptions {
                transcript_root: transcript_root.clone(),
                max_follow_events: 0,
                ..ClaudeObserverOptions::default()
            },
            &mut output,
        )?;

        let observer_dir = result
            .storage
            .artifacts_dir
            .join("observer_events")
            .join("claude-code");
        assert!(observer_dir.is_dir());
        assert!(
            fs::read_dir(&observer_dir)?.next().is_some(),
            "expected claude observer artifact file"
        );
        assert!(String::from_utf8_lossy(&output).contains("PrismTrace host skeleton"));

        fs::remove_dir_all(result.config.state_root)?;
        fs::remove_dir_all(transcript_root)?;
        Ok(())
    }

    #[test]
    fn run_opencode_observer_session_passes_storage_to_artifact_writer() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = bootstrap(&workspace_root)?;
        let server = spawn_test_opencode_server()?;

        let mut output = Vec::new();
        run_opencode_observer_session(
            &result,
            OpencodeObserverOptions {
                base_url: server.base_url().into(),
                session_limit: 8,
                message_limit: 8,
            },
            &mut output,
        )?;

        let observer_dir = result
            .storage
            .artifacts_dir
            .join("observer_events")
            .join("opencode");
        assert!(observer_dir.is_dir());
        let artifact_path = fs::read_dir(&observer_dir)?
            .find_map(|entry| entry.ok().map(|entry| entry.path()))
            .expect("expected opencode observer artifact file");
        let artifact = fs::read_to_string(artifact_path)?;
        assert!(artifact.contains("\"record_type\":\"handshake\""));
        assert!(artifact.contains("\"record_type\":\"event\""));
        assert!(String::from_utf8_lossy(&output).contains("PrismTrace host skeleton"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    fn unique_test_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);

        std::env::temp_dir().join(format!(
            "prismtrace-host-lifecycle-test-{}-{}-{}",
            process::id(),
            nanos,
            counter
        ))
    }
}
