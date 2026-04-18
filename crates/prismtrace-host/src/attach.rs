use prismtrace_core::{
    AttachFailure, AttachFailureKind, AttachReadiness, AttachReadinessStatus, AttachSession,
    AttachSessionState, ProbeBootstrap, ProbeBootstrapState, ProcessTarget,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendAttachOutcome {
    pub detail: String,
    pub bootstrap: ProbeBootstrap,
}

impl BackendAttachOutcome {
    pub fn ready(message: impl Into<String>) -> Self {
        let message = message.into();

        Self {
            detail: "probe handshake completed".into(),
            bootstrap: ProbeBootstrap {
                state: ProbeBootstrapState::Ready,
                message,
            },
        }
    }

    pub fn handshake_failed(message: impl Into<String>) -> Self {
        Self {
            detail: "probe handshake failed".into(),
            bootstrap: ProbeBootstrap {
                state: ProbeBootstrapState::Failed,
                message: message.into(),
            },
        }
    }
}

pub trait AttachBackend {
    fn attach(&mut self, target: &ProcessTarget) -> Result<BackendAttachOutcome, AttachFailure>;
    fn detach(&mut self, session: &AttachSession) -> Result<String, AttachFailure>;
}

#[derive(Debug, Clone)]
pub struct ScriptedAttachBackend {
    attach_result: Result<BackendAttachOutcome, AttachFailure>,
    detach_result: Result<String, AttachFailure>,
}

impl ScriptedAttachBackend {
    pub fn ready() -> Self {
        Self::with_outcome(BackendAttachOutcome::ready("probe online"))
    }

    pub fn handshake_failed(message: impl Into<String>) -> Self {
        Self::with_outcome(BackendAttachOutcome::handshake_failed(message))
    }

    pub fn with_outcome(outcome: BackendAttachOutcome) -> Self {
        Self {
            attach_result: Ok(outcome),
            detach_result: Ok("attach session detached".into()),
        }
    }
}

impl AttachBackend for ScriptedAttachBackend {
    fn attach(&mut self, _target: &ProcessTarget) -> Result<BackendAttachOutcome, AttachFailure> {
        self.attach_result.clone()
    }

    fn detach(&mut self, _session: &AttachSession) -> Result<String, AttachFailure> {
        self.detach_result.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AttachController<B> {
    backend: B,
    active_session: Option<AttachSession>,
}

impl<B: AttachBackend> AttachController<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            active_session: None,
        }
    }

    pub fn active_session(&self) -> Option<&AttachSession> {
        self.active_session.as_ref()
    }

    pub fn attach(&mut self, readiness: &AttachReadiness) -> Result<AttachSession, AttachFailure> {
        if self.active_session.is_some() {
            return Err(AttachFailure {
                kind: AttachFailureKind::ActiveSessionExists,
                reason: "an active attach session already exists".into(),
            });
        }

        if readiness.status != AttachReadinessStatus::Supported {
            return Err(AttachFailure {
                kind: AttachFailureKind::NotReady,
                reason: format!(
                    "target is not attachable yet because readiness is {}",
                    readiness.status.label()
                ),
            });
        }

        let outcome = self.backend.attach(&readiness.target)?;

        if outcome.bootstrap.state != ProbeBootstrapState::Ready {
            return Err(AttachFailure {
                kind: AttachFailureKind::HandshakeFailed,
                reason: outcome.bootstrap.message,
            });
        }

        let session = AttachSession {
            target: readiness.target.clone(),
            state: AttachSessionState::Attached,
            detail: outcome.detail,
            bootstrap: Some(outcome.bootstrap),
            failure: None,
        };

        self.active_session = Some(session.clone());
        Ok(session)
    }

    pub fn detach(&mut self) -> Result<AttachSession, AttachFailure> {
        let active_session = self.active_session.clone().ok_or(AttachFailure {
            kind: AttachFailureKind::NoActiveSession,
            reason: "there is no active attach session to detach".into(),
        })?;

        let detail = self.backend.detach(&active_session)?;
        let detached_session = AttachSession {
            target: active_session.target,
            state: AttachSessionState::Detached,
            detail,
            bootstrap: active_session.bootstrap,
            failure: None,
        };

        self.active_session = None;
        Ok(detached_session)
    }
}

pub fn attach_report(result: &Result<AttachSession, AttachFailure>) -> String {
    match result {
        Ok(session) => session.summary(),
        Err(failure) => failure.summary(),
    }
}

#[cfg(test)]
mod tests {
    use super::{AttachController, BackendAttachOutcome, ScriptedAttachBackend, attach_report};
    use prismtrace_core::{
        AttachFailure, AttachFailureKind, AttachReadiness, AttachReadinessStatus,
        AttachSessionState, ProbeBootstrapState, ProcessTarget, RuntimeKind,
    };
    use std::path::PathBuf;

    #[test]
    fn attach_supported_target_transitions_to_attached_when_handshake_ready() {
        let mut controller = AttachController::new(ScriptedAttachBackend::ready());

        let session = controller
            .attach(&supported_readiness(701))
            .expect("supported target should attach");

        assert_eq!(session.state, AttachSessionState::Attached);
        assert_eq!(
            session
                .bootstrap
                .expect("attached session should have bootstrap")
                .state,
            ProbeBootstrapState::Ready
        );
        assert_eq!(
            controller
                .active_session()
                .expect("active session should be stored")
                .target
                .pid,
            701
        );
    }

    #[test]
    fn attach_rejects_second_target_when_active_session_exists() {
        let mut controller = AttachController::new(ScriptedAttachBackend::ready());
        controller
            .attach(&supported_readiness(702))
            .expect("first attach should succeed");

        let failure = controller
            .attach(&supported_readiness(703))
            .expect_err("second attach should be rejected");

        assert_eq!(failure.kind, AttachFailureKind::ActiveSessionExists);
    }

    #[test]
    fn attach_returns_not_ready_failure_for_non_supported_target() {
        let mut controller = AttachController::new(ScriptedAttachBackend::ready());

        let failure = controller
            .attach(&unknown_readiness(704))
            .expect_err("unknown readiness should not attach");

        assert_eq!(failure.kind, AttachFailureKind::NotReady);
    }

    #[test]
    fn attach_returns_handshake_failed_when_backend_does_not_finish_bootstrap() {
        let mut controller = AttachController::new(ScriptedAttachBackend::handshake_failed(
            "probe handshake did not complete",
        ));

        let failure = controller
            .attach(&supported_readiness(705))
            .expect_err("handshake failure should bubble up");

        assert_eq!(failure.kind, AttachFailureKind::HandshakeFailed);
        assert!(controller.active_session().is_none());
    }

    #[test]
    fn detach_clears_active_session_and_returns_detached_result() {
        let mut controller = AttachController::new(ScriptedAttachBackend::ready());
        controller
            .attach(&supported_readiness(706))
            .expect("attach should succeed");

        let detached = controller.detach().expect("detach should succeed");

        assert_eq!(detached.state, AttachSessionState::Detached);
        assert!(controller.active_session().is_none());
    }

    #[test]
    fn detach_without_active_session_returns_structured_failure() {
        let mut controller = AttachController::new(ScriptedAttachBackend::ready());

        let failure = controller
            .detach()
            .expect_err("detach without active session should fail");

        assert_eq!(failure.kind, AttachFailureKind::NoActiveSession);
    }

    #[test]
    fn attach_report_renders_success_and_failure_paths() {
        let success = attach_report(&Ok(AttachController::new(
            ScriptedAttachBackend::with_outcome(BackendAttachOutcome::ready("probe online")),
        )
        .attach(&supported_readiness(707))
        .expect("attach should succeed")));

        let failure = attach_report(&Err(AttachFailure {
            kind: AttachFailureKind::BackendRejected,
            reason: "backend refused target".into(),
        }));

        assert!(success.contains("[attached]"));
        assert!(failure.contains("[backend_rejected]"));
    }

    fn supported_readiness(pid: u32) -> AttachReadiness {
        AttachReadiness {
            target: ProcessTarget {
                pid,
                app_name: format!("Codex-{pid}"),
                executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
                runtime_kind: RuntimeKind::Electron,
            },
            status: AttachReadinessStatus::Supported,
            reason: "electron runtime target looks suitable for attach readiness checks".into(),
        }
    }

    fn unknown_readiness(pid: u32) -> AttachReadiness {
        AttachReadiness {
            target: ProcessTarget {
                pid,
                app_name: format!("Unknown-{pid}"),
                executable_path: PathBuf::from("/usr/bin/python3"),
                runtime_kind: RuntimeKind::Unknown,
            },
            status: AttachReadinessStatus::Unknown,
            reason: "runtime classification is not strong enough to recommend attach yet".into(),
        }
    }
}
