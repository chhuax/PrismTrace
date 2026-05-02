pub mod analysis;
pub mod capability_projection;
pub mod claude_observer;
pub mod codex_observer;
pub mod console;
pub mod discovery;
pub(crate) mod index;
pub mod ingest;
pub mod ipc;
pub mod lifecycle;
pub mod observability_read_model;
pub mod observer;
pub mod opencode_observer;
pub mod request_capture;
pub mod response_capture;
pub mod runtime;
pub mod sources;
pub mod tool_visibility;

pub use lifecycle::{
    AppConfig, BootstrapResult, DEFAULT_BIND_ADDR, HostSnapshot, bootstrap, collect_host_snapshot,
    discovery_report, run_claude_observer_session, run_codex_observer_session,
    run_opencode_observer_session, startup_summary,
};

#[cfg(test)]
mod tests {
    use crate::console::{collect_console_snapshot, console_startup_report};
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sources_boundary_exposes_observer_options() {
        let _codex = crate::sources::codex::CodexObserverOptions::default();
        let _claude = crate::sources::claude::ClaudeObserverOptions::default();
        let _opencode = crate::sources::opencode::OpencodeObserverOptions::default();
    }

    #[test]
    fn analysis_projection_types_live_in_prismtrace_analysis_crate() -> io::Result<()> {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("host crate should have a parent")
            .parent()
            .expect("crates directory should have a parent")
            .to_path_buf();
        let analysis_lib =
            fs::read_to_string(workspace.join("crates/prismtrace-analysis/src/lib.rs"))?;
        let host_analysis =
            fs::read_to_string(workspace.join("crates/prismtrace-host/src/analysis.rs"))?;
        let host_capability_projection = fs::read_to_string(
            workspace.join("crates/prismtrace-host/src/capability_projection.rs"),
        )?;

        assert!(analysis_lib.contains("pub struct PromptDiff"));
        assert!(analysis_lib.contains("pub struct CapabilityProjection"));
        assert!(!host_analysis.contains(concat!("pub struct ", "PromptDiff")));
        assert!(
            !host_capability_projection.contains(concat!("pub struct ", "CapabilityProjection"))
        );

        Ok(())
    }

    #[test]
    fn api_payload_renderers_live_in_prismtrace_api_crate() -> io::Result<()> {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("host crate should have a parent")
            .parent()
            .expect("crates directory should have a parent")
            .to_path_buf();
        let api_lib = fs::read_to_string(workspace.join("crates/prismtrace-api/src/lib.rs"))?;
        let console_api =
            fs::read_to_string(workspace.join("crates/prismtrace-host/src/console/api.rs"))?;

        assert!(api_lib.contains("pub fn render_session_diagnostics_payload"));
        assert!(api_lib.contains("pub fn render_capability_projection_payload"));
        assert!(!console_api.contains(concat!("fn ", "render_session_diagnostics_payload")));
        assert!(!console_api.contains(concat!("fn ", "render_capability_projection_payload")));

        Ok(())
    }

    #[test]
    fn lifecycle_orchestration_lives_outside_lib_module() -> io::Result<()> {
        let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let lifecycle = fs::read_to_string(src_dir.join("lifecycle.rs"))?;
        let lib = fs::read_to_string(src_dir.join("lib.rs"))?;
        let lib_production = lib
            .split("#[cfg(test)]")
            .next()
            .expect("lib production module should be present");

        assert!(lifecycle.contains("pub struct AppConfig"));
        assert!(lifecycle.contains("pub fn bootstrap"));
        assert!(lifecycle.contains("pub fn run_opencode_observer_session"));
        assert!(!lib_production.contains(concat!("pub struct ", "AppConfig")));
        assert!(!lib_production.contains(concat!("pub fn ", "bootstrap")));
        assert!(!lib_production.contains(concat!("pub fn ", "run_opencode_observer_session")));

        Ok(())
    }

    #[test]
    fn source_contracts_live_in_prismtrace_sources_crate() -> io::Result<()> {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("host crate should have a parent")
            .parent()
            .expect("crates directory should have a parent")
            .to_path_buf();
        let sources_lib =
            fs::read_to_string(workspace.join("crates/prismtrace-sources/src/lib.rs"))?;
        let host_observer =
            fs::read_to_string(workspace.join("crates/prismtrace-host/src/observer.rs"))?;
        let host_ingest =
            fs::read_to_string(workspace.join("crates/prismtrace-host/src/ingest.rs"))?;

        assert!(sources_lib.contains("pub trait ObserverSource"));
        assert!(sources_lib.contains("pub struct ObserverArtifactWriter"));
        assert!(!host_observer.contains(concat!("pub trait ", "ObserverSource")));
        assert!(!host_ingest.contains(concat!("pub struct ", "ObserverArtifactWriter")));

        Ok(())
    }

    #[test]
    fn collect_console_snapshot_exposes_local_console_url() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = crate::bootstrap(&workspace_root)?;

        let snapshot = collect_console_snapshot(&result, None);

        assert_eq!(
            snapshot.bind_addr,
            format!("http://{}", crate::DEFAULT_BIND_ADDR)
        );
        assert!(snapshot.summary.contains("PrismTrace host skeleton"));

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn console_startup_report_mentions_browser_entrypoint() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = crate::bootstrap(&workspace_root)?;
        let snapshot = collect_console_snapshot(&result, None);
        let report = console_startup_report(&snapshot);

        assert!(
            report.contains("PrismTrace Local Console"),
            "report: {report}"
        );
        assert!(report.contains("http://127.0.0.1:7799"), "report: {report}");

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
            "prismtrace-host-test-{}-{}-{}",
            process::id(),
            nanos,
            counter
        ))
    }
}
