pub mod discovery;

use discovery::{ProcessSampleSource, discover_targets};
use prismtrace_core::ProcessTarget;
use prismtrace_storage::StorageLayout;
use std::io;
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

#[cfg(test)]
mod tests {
    use super::{
        AppConfig, DEFAULT_BIND_ADDR, bootstrap, collect_host_snapshot, startup_summary,
    };
    use crate::discovery::StaticProcessSampleSource;
    use prismtrace_core::ProcessSample;
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn app_config_uses_a_hidden_state_directory_inside_the_workspace() {
        let config = AppConfig::from_workspace_root("/tmp/prismtrace-workspace");

        assert_eq!(config.workspace_root, PathBuf::from("/tmp/prismtrace-workspace"));
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
            result.config
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
        assert!(summary.contains(
            result.storage.db_path.to_string_lossy().as_ref()
        ));

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
        assert_eq!(snapshot.discovered_targets[1].runtime_kind.label(), "electron");
        assert_eq!(snapshot.discovered_targets[2].runtime_kind.label(), "unknown");

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

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "prismtrace-host-test-{}-{}",
            process::id(),
            nanos
        ))
    }
}
