use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Node,
    Electron,
    Unknown,
}

impl RuntimeKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Electron => "electron",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTarget {
    pub pid: u32,
    pub app_name: String,
    pub executable_path: PathBuf,
    pub runtime_kind: RuntimeKind,
}

impl ProcessTarget {
    pub fn display_name(&self) -> &str {
        if !self.app_name.is_empty() {
            return &self.app_name;
        }

        self.executable_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSample {
    pub pid: u32,
    pub process_name: String,
    pub executable_path: PathBuf,
}

impl ProcessSample {
    pub fn runtime_kind(&self) -> RuntimeKind {
        let process_name = self.process_name.to_ascii_lowercase();
        let executable_name = self
            .executable_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if process_name == "node" || executable_name == "node" {
            RuntimeKind::Node
        } else if process_name == "electron"
            || executable_name == "electron"
            || self
                .executable_path
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("electron")
        {
            RuntimeKind::Electron
        } else {
            RuntimeKind::Unknown
        }
    }

    pub fn normalized_app_name(&self) -> String {
        let is_generic_runtime_name = matches!(
            (
                self.runtime_kind(),
                self.process_name.to_ascii_lowercase().as_str()
            ),
            (RuntimeKind::Node, "node") | (RuntimeKind::Electron, "electron")
        );

        if !self.process_name.trim().is_empty() && !is_generic_runtime_name {
            return self.process_name.clone();
        }

        self.executable_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    pub fn into_target(&self) -> ProcessTarget {
        ProcessTarget {
            pid: self.pid,
            app_name: self.normalized_app_name(),
            executable_path: self.executable_path.clone(),
            runtime_kind: self.runtime_kind(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachReadinessStatus {
    Supported,
    Unsupported,
    PermissionDenied,
    Unknown,
}

impl AttachReadinessStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
            Self::PermissionDenied => "permission_denied",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachReadiness {
    pub target: ProcessTarget,
    pub status: AttachReadinessStatus,
    pub reason: String,
}

impl AttachReadiness {
    pub fn summary(&self) -> String {
        format!(
            "[{}] {} (pid {}): {}",
            self.status.label(),
            self.target.display_name(),
            self.target.pid,
            self.reason
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachSessionState {
    Attaching,
    Attached,
    Detached,
    Failed,
}

impl AttachSessionState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Attaching => "attaching",
            Self::Attached => "attached",
            Self::Detached => "detached",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachFailureKind {
    NotReady,
    ActiveSessionExists,
    BackendRejected,
    HandshakeFailed,
    NoActiveSession,
    DetachFailed,
}

impl AttachFailureKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotReady => "not_ready",
            Self::ActiveSessionExists => "active_session_exists",
            Self::BackendRejected => "backend_rejected",
            Self::HandshakeFailed => "handshake_failed",
            Self::NoActiveSession => "no_active_session",
            Self::DetachFailed => "detach_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachFailure {
    pub kind: AttachFailureKind,
    pub reason: String,
}

impl AttachFailure {
    pub fn summary(&self) -> String {
        format!("[{}] {}", self.kind.label(), self.reason)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeBootstrapState {
    Pending,
    Ready,
    Failed,
}

impl ProbeBootstrapState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeBootstrap {
    pub state: ProbeBootstrapState,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachSession {
    pub target: ProcessTarget,
    pub state: AttachSessionState,
    pub detail: String,
    pub bootstrap: Option<ProbeBootstrap>,
    pub failure: Option<AttachFailure>,
}

impl AttachSession {
    pub fn summary(&self) -> String {
        format!(
            "[{}] {} (pid {}): {}",
            self.state.label(),
            self.target.display_name(),
            self.target.pid,
            self.detail
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeState {
    Detached,
    Attaching,
    Attached,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeHealth {
    pub state: ProbeState,
    pub installed_hooks: Vec<String>,
    pub failed_hooks: Vec<String>,
}

impl ProbeHealth {
    pub fn summary(&self) -> String {
        format!(
            "{} (installed: {}, failed: {})",
            match self.state {
                ProbeState::Detached => "detached",
                ProbeState::Attaching => "attaching",
                ProbeState::Attached => "attached",
                ProbeState::Failed => "failed",
            },
            self.installed_hooks.len(),
            self.failed_hooks.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AttachFailure, AttachFailureKind, AttachReadiness, AttachReadinessStatus, AttachSession,
        AttachSessionState, ProbeBootstrap, ProbeBootstrapState, ProbeHealth, ProbeState,
        ProcessSample, ProcessTarget, RuntimeKind,
    };
    use std::path::PathBuf;

    #[test]
    fn runtime_kind_labels_are_stable() {
        assert_eq!(RuntimeKind::Node.label(), "node");
        assert_eq!(RuntimeKind::Electron.label(), "electron");
        assert_eq!(RuntimeKind::Unknown.label(), "unknown");
    }

    #[test]
    fn process_target_display_name_falls_back_to_executable_name() {
        let target = ProcessTarget {
            pid: 42,
            app_name: String::new(),
            executable_path: PathBuf::from("/Applications/Example.app/Contents/MacOS/Example"),
            runtime_kind: RuntimeKind::Electron,
        };

        assert_eq!(target.display_name(), "Example");
    }

    #[test]
    fn probe_health_summary_mentions_state_and_hook_counts() {
        let health = ProbeHealth {
            state: ProbeState::Attached,
            installed_hooks: vec!["fetch".into(), "undici".into()],
            failed_hooks: vec!["http".into()],
        };

        assert_eq!(health.summary(), "attached (installed: 2, failed: 1)");
    }

    #[test]
    fn process_sample_classifies_node_processes() {
        let sample = ProcessSample {
            pid: 7,
            process_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Node);
    }

    #[test]
    fn process_sample_classifies_electron_processes() {
        let sample = ProcessSample {
            pid: 8,
            process_name: "Electron".into(),
            executable_path: PathBuf::from("/Applications/Electron.app/Contents/MacOS/Electron"),
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Electron);
    }

    #[test]
    fn process_sample_keeps_unknown_when_no_runtime_matches() {
        let sample = ProcessSample {
            pid: 9,
            process_name: "python3".into(),
            executable_path: PathBuf::from("/usr/bin/python3"),
        };

        assert_eq!(sample.runtime_kind(), RuntimeKind::Unknown);
    }

    #[test]
    fn process_sample_normalizes_generic_runtime_names_to_executable_name() {
        let sample = ProcessSample {
            pid: 10,
            process_name: "node".into(),
            executable_path: PathBuf::from(
                "/Applications/Claude Code.app/Contents/MacOS/Claude Code",
            ),
        };

        assert_eq!(sample.normalized_app_name(), "Claude Code");
    }

    #[test]
    fn process_sample_converts_to_structured_target() {
        let sample = ProcessSample {
            pid: 11,
            process_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
        };

        let target = sample.into_target();

        assert_eq!(target.pid, 11);
        assert_eq!(target.app_name, "Codex");
        assert_eq!(target.runtime_kind, RuntimeKind::Unknown);
    }

    #[test]
    fn attach_readiness_status_labels_are_stable() {
        assert_eq!(AttachReadinessStatus::Supported.label(), "supported");
        assert_eq!(AttachReadinessStatus::Unsupported.label(), "unsupported");
        assert_eq!(
            AttachReadinessStatus::PermissionDenied.label(),
            "permission_denied"
        );
        assert_eq!(AttachReadinessStatus::Unknown.label(), "unknown");
    }

    #[test]
    fn attach_readiness_summary_includes_status_and_reason() {
        let readiness = AttachReadiness {
            target: ProcessTarget {
                pid: 501,
                app_name: "Codex".into(),
                executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
                runtime_kind: RuntimeKind::Unknown,
            },
            status: AttachReadinessStatus::Unknown,
            reason: "runtime classification is not strong enough yet".into(),
        };

        assert_eq!(
            readiness.summary(),
            "[unknown] Codex (pid 501): runtime classification is not strong enough yet"
        );
    }

    #[test]
    fn attach_session_state_labels_are_stable() {
        assert_eq!(AttachSessionState::Attaching.label(), "attaching");
        assert_eq!(AttachSessionState::Attached.label(), "attached");
        assert_eq!(AttachSessionState::Detached.label(), "detached");
        assert_eq!(AttachSessionState::Failed.label(), "failed");
    }

    #[test]
    fn attach_failure_kind_labels_are_stable() {
        assert_eq!(AttachFailureKind::NotReady.label(), "not_ready");
        assert_eq!(
            AttachFailureKind::ActiveSessionExists.label(),
            "active_session_exists"
        );
        assert_eq!(
            AttachFailureKind::BackendRejected.label(),
            "backend_rejected"
        );
        assert_eq!(
            AttachFailureKind::HandshakeFailed.label(),
            "handshake_failed"
        );
        assert_eq!(
            AttachFailureKind::NoActiveSession.label(),
            "no_active_session"
        );
        assert_eq!(AttachFailureKind::DetachFailed.label(), "detach_failed");
    }

    #[test]
    fn probe_bootstrap_state_labels_are_stable() {
        assert_eq!(ProbeBootstrapState::Pending.label(), "pending");
        assert_eq!(ProbeBootstrapState::Ready.label(), "ready");
        assert_eq!(ProbeBootstrapState::Failed.label(), "failed");
    }

    #[test]
    fn attach_failure_summary_includes_kind_and_reason() {
        let failure = AttachFailure {
            kind: AttachFailureKind::HandshakeFailed,
            reason: "probe handshake did not complete".into(),
        };

        assert_eq!(
            failure.summary(),
            "[handshake_failed] probe handshake did not complete"
        );
    }

    #[test]
    fn attach_session_summary_includes_state_target_and_detail() {
        let session = AttachSession {
            target: ProcessTarget {
                pid: 601,
                app_name: "Electron".into(),
                executable_path: PathBuf::from(
                    "/Applications/Electron.app/Contents/MacOS/Electron",
                ),
                runtime_kind: RuntimeKind::Electron,
            },
            state: AttachSessionState::Attached,
            detail: "probe handshake completed".into(),
            bootstrap: Some(ProbeBootstrap {
                state: ProbeBootstrapState::Ready,
                message: "probe online".into(),
            }),
            failure: None,
        };

        assert_eq!(
            session.summary(),
            "[attached] Electron (pid 601): probe handshake completed"
        );
    }
}
