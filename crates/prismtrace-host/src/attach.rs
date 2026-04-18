use std::time::Duration;

use prismtrace_core::{
    AttachFailure, AttachFailureKind, AttachReadiness, AttachReadinessStatus, AttachSession,
    AttachSessionState, IpcMessage, ProbeBootstrap, ProbeBootstrapState, ProcessTarget,
};

use crate::ipc::{IpcEvent, IpcListener};
use crate::runtime::InstrumentationRuntime;

pub const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(10);
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

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

    pub fn with_detach_failure(mut self, reason: impl Into<String>) -> Self {
        self.detach_result = Err(AttachFailure {
            kind: AttachFailureKind::DetachFailed,
            reason: reason.into(),
        });
        self
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

pub struct LiveAttachBackend<R: InstrumentationRuntime> {
    runtime: R,
    probe_script: &'static str,
    /// IPC listener retained after a successful attach so detach can wait for DetachAck.
    ipc_listener: Option<IpcListener>,
}

impl<R: InstrumentationRuntime> LiveAttachBackend<R> {
    pub fn new(runtime: R) -> Self {
        Self {
            runtime,
            probe_script: include_str!("../probe/bootstrap.js"),
            ipc_listener: None,
        }
    }

    pub fn listener_mut(&mut self) -> Option<&mut IpcListener> {
        self.ipc_listener.as_mut()
    }
}

pub const DETACH_TIMEOUT: Duration = Duration::from_secs(5);

impl<R: InstrumentationRuntime> AttachBackend for LiveAttachBackend<R> {
    fn attach(&mut self, target: &ProcessTarget) -> Result<BackendAttachOutcome, AttachFailure> {
        use crate::runtime::InstrumentationErrorKind;

        let reader = self
            .runtime
            .inject_probe(target.pid, self.probe_script)
            .map_err(|e| {
                let kind = match e.kind {
                    InstrumentationErrorKind::PermissionDenied
                    | InstrumentationErrorKind::ProcessNotFound => {
                        AttachFailureKind::BackendRejected
                    }
                    InstrumentationErrorKind::RuntimeIncompatible
                    | InstrumentationErrorKind::InjectionFailed
                    | InstrumentationErrorKind::DetachFailed => AttachFailureKind::HandshakeFailed,
                };
                AttachFailure {
                    kind,
                    reason: e.message,
                }
            })?;

        let mut listener = IpcListener::new(reader, BOOTSTRAP_TIMEOUT);

        loop {
            match listener.next_event() {
                IpcEvent::Message(IpcMessage::BootstrapReport {
                    installed_hooks, ..
                }) => {
                    if installed_hooks.is_empty() {
                        return Err(AttachFailure {
                            kind: AttachFailureKind::HandshakeFailed,
                            reason: "no hooks installed".into(),
                        });
                    }
                    let outcome = BackendAttachOutcome::ready(format!(
                        "probe online: {} hooks installed",
                        installed_hooks.len()
                    ));
                    // Retain the listener so detach can wait for DetachAck.
                    self.ipc_listener = Some(listener);
                    return Ok(outcome);
                }
                IpcEvent::ChannelDisconnected { reason } => {
                    return Err(AttachFailure {
                        kind: AttachFailureKind::HandshakeFailed,
                        reason,
                    });
                }
                IpcEvent::HeartbeatTimeout { .. } => {
                    return Err(AttachFailure {
                        kind: AttachFailureKind::HandshakeFailed,
                        reason: "bootstrap timeout".into(),
                    });
                }
                IpcEvent::Message(_) => {
                    // other messages — keep looping
                }
            }
        }
    }

    fn detach(&mut self, session: &AttachSession) -> Result<String, AttachFailure> {
        // Send the detach signal to the probe.
        self.runtime
            .send_detach_signal(session.target.pid)
            .map_err(|e| AttachFailure {
                kind: AttachFailureKind::DetachFailed,
                reason: e.message,
            })?;

        // Wait for DetachAck via a background thread so the deadline is enforceable
        // even though `next_event()` blocks on `read_line()`.
        if let Some(listener) = self.ipc_listener.take() {
            let deadline = std::time::Instant::now() + DETACH_TIMEOUT;
            let shutdown = listener.shutdown_handle();
            let (tx, rx) = std::sync::mpsc::channel();

            let mut worker = Some(std::thread::spawn(move || {
                let mut listener = listener;
                loop {
                    let event = listener.next_event();
                    let terminal = matches!(
                        event,
                        IpcEvent::ChannelDisconnected { .. }
                            | IpcEvent::Message(IpcMessage::DetachAck { .. })
                            | IpcEvent::HeartbeatTimeout { .. }
                    );
                    if tx.send(event).is_err() {
                        break;
                    }
                    if terminal {
                        break;
                    }
                }
            }));

            let cleanup_worker = |worker: &mut Option<std::thread::JoinHandle<()>>,
                                  request_shutdown: bool| {
                if request_shutdown && let Some(handle) = shutdown.as_ref() {
                    handle.shutdown();
                }
                if (!request_shutdown || shutdown.is_some())
                    && let Some(join_handle) = worker.take()
                {
                    let _ = join_handle.join();
                }
            };

            loop {
                let now = std::time::Instant::now();
                if now >= deadline {
                    cleanup_worker(&mut worker, true);
                    return Err(AttachFailure {
                        kind: AttachFailureKind::DetachFailed,
                        reason: "timed out waiting for DetachAck".into(),
                    });
                }
                match rx.recv_timeout(deadline.saturating_duration_since(now)) {
                    Ok(IpcEvent::Message(IpcMessage::DetachAck { .. })) => {
                        cleanup_worker(&mut worker, false);
                        break;
                    }
                    Ok(IpcEvent::ChannelDisconnected { .. }) => {
                        cleanup_worker(&mut worker, false);
                        break;
                    }
                    Ok(IpcEvent::HeartbeatTimeout { .. }) => {
                        cleanup_worker(&mut worker, true);
                        return Err(AttachFailure {
                            kind: AttachFailureKind::DetachFailed,
                            reason: "timed out waiting for DetachAck".into(),
                        });
                    }
                    Ok(IpcEvent::Message(_)) => {
                        // Stray message — keep waiting.
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        cleanup_worker(&mut worker, true);
                        return Err(AttachFailure {
                            kind: AttachFailureKind::DetachFailed,
                            reason: "timed out waiting for DetachAck".into(),
                        });
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        cleanup_worker(&mut worker, false);
                        return Err(AttachFailure {
                            kind: AttachFailureKind::DetachFailed,
                            reason: "detach listener stopped before DetachAck".into(),
                        });
                    }
                }
            }
        }

        Ok(format!("[detached] pid {}", session.target.pid))
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
        let active_session = self.active_session.take().ok_or(AttachFailure {
            kind: AttachFailureKind::NoActiveSession,
            reason: "there is no active attach session to detach".into(),
        })?;

        match self.backend.detach(&active_session) {
            Ok(detail) => Ok(AttachSession {
                target: active_session.target,
                state: AttachSessionState::Detached,
                detail,
                bootstrap: active_session.bootstrap,
                failure: None,
            }),
            Err(failure) => {
                self.active_session = Some(active_session);
                Err(failure)
            }
        }
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
    use super::{
        AttachBackend, AttachController, BackendAttachOutcome, LiveAttachBackend,
        ScriptedAttachBackend, attach_report,
    };
    use crate::ipc::IpcEvent;
    use crate::runtime::{InstrumentationErrorKind, ScriptedInstrumentationRuntime};
    use prismtrace_core::{
        AttachFailure, AttachFailureKind, AttachReadiness, AttachReadinessStatus,
        AttachSessionState, IpcMessage, ProbeBootstrapState, ProcessTarget, RuntimeKind,
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
    fn detach_restores_active_session_when_backend_fails() {
        let mut controller = AttachController::new(
            ScriptedAttachBackend::ready().with_detach_failure("backend refused detach"),
        );
        controller
            .attach(&supported_readiness(707))
            .expect("attach should succeed");

        let failure = controller
            .detach()
            .expect_err("detach failure should bubble up");

        assert_eq!(failure.kind, AttachFailureKind::DetachFailed);
        assert_eq!(
            controller
                .active_session()
                .expect("active session should be restored")
                .target
                .pid,
            707
        );
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

    fn bootstrap_report_line() -> String {
        IpcMessage::BootstrapReport {
            installed_hooks: vec!["fetch".into(), "http".into()],
            failed_hooks: vec![],
            timestamp_ms: 1000,
        }
        .to_json_line()
    }

    // --- LiveAttachBackend tests (Tasks 4.4 & 4.5) ---

    #[test]
    fn live_backend_attach_succeeds_when_bootstrap_report_arrives() {
        let runtime =
            ScriptedInstrumentationRuntime::success_with_messages(vec![bootstrap_report_line()]);
        let mut backend = LiveAttachBackend::new(runtime);

        let outcome = backend
            .attach(&supported_readiness(800).target)
            .expect("attach should succeed");

        assert_eq!(outcome.bootstrap.state, ProbeBootstrapState::Ready);
    }

    #[test]
    fn live_backend_next_event_returns_observed_request_after_bootstrap() {
        let runtime = ScriptedInstrumentationRuntime::success_with_messages(vec![
            bootstrap_report_line(),
            IpcMessage::HttpRequestObserved {
                hook_name: "fetch".into(),
                method: "POST".into(),
                url: "https://api.openai.com/v1/responses".into(),
                headers: vec![],
                body_text: Some("{}".into()),
                body_truncated: false,
                timestamp_ms: 3,
            }
            .to_json_line(),
        ]);
        let mut backend = LiveAttachBackend::new(runtime);
        let target = supported_readiness(807).target;

        backend.attach(&target).expect("attach should succeed");
        let event = backend
            .listener_mut()
            .expect("listener should still be available")
            .next_event();

        match event {
            IpcEvent::Message(IpcMessage::HttpRequestObserved { url, .. }) => {
                assert_eq!(url, "https://api.openai.com/v1/responses");
            }
            _ => panic!("expected observed request event"),
        }
    }

    #[test]
    fn live_backend_attach_fails_when_inject_returns_permission_denied() {
        let runtime = ScriptedInstrumentationRuntime::inject_fails(
            InstrumentationErrorKind::PermissionDenied,
            "permission denied",
        );
        let mut backend = LiveAttachBackend::new(runtime);

        let failure = backend
            .attach(&supported_readiness(801).target)
            .expect_err("permission denied should propagate as BackendRejected");

        assert_eq!(failure.kind, AttachFailureKind::BackendRejected);
    }

    #[test]
    fn live_backend_attach_fails_when_inject_returns_injection_failed() {
        let runtime = ScriptedInstrumentationRuntime::inject_fails(
            InstrumentationErrorKind::InjectionFailed,
            "injection failed",
        );
        let mut backend = LiveAttachBackend::new(runtime);

        let failure = backend
            .attach(&supported_readiness(802).target)
            .expect_err("injection failed should propagate as HandshakeFailed");

        assert_eq!(failure.kind, AttachFailureKind::HandshakeFailed);
    }

    #[test]
    fn live_backend_attach_fails_when_channel_disconnects_before_bootstrap() {
        // Empty reader → EOF immediately → ChannelDisconnected → HandshakeFailed
        let runtime = ScriptedInstrumentationRuntime::success_with_messages(vec![]);
        let mut backend = LiveAttachBackend::new(runtime);

        let failure = backend
            .attach(&supported_readiness(803).target)
            .expect_err("empty reader should cause handshake failure");

        assert_eq!(failure.kind, AttachFailureKind::HandshakeFailed);
    }

    #[test]
    fn live_backend_detach_succeeds() {
        // The listener needs a DetachAck after the BootstrapReport so detach can complete.
        let detach_ack_line = IpcMessage::DetachAck { timestamp_ms: 2000 }.to_json_line();
        let runtime = ScriptedInstrumentationRuntime::success_with_messages(vec![
            bootstrap_report_line(),
            detach_ack_line,
        ]);
        let mut backend = LiveAttachBackend::new(runtime);

        // First attach to populate ipc_listener.
        let outcome = backend
            .attach(&supported_readiness(804).target)
            .expect("attach should succeed");

        // Build a minimal AttachSession to pass to detach.
        let session = prismtrace_core::AttachSession {
            target: supported_readiness(804).target,
            state: prismtrace_core::AttachSessionState::Attached,
            detail: outcome.detail,
            bootstrap: Some(outcome.bootstrap),
            failure: None,
        };

        let result = backend.detach(&session);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(
            msg.contains("detached"),
            "detach message should contain 'detached': {msg}"
        );
        assert!(
            msg.contains("804"),
            "detach message should contain pid: {msg}"
        );
    }

    #[test]
    fn live_backend_detach_fails_when_runtime_detach_fails() {
        let runtime = ScriptedInstrumentationRuntime::detach_fails(
            InstrumentationErrorKind::DetachFailed,
            "could not send detach signal",
        );
        let mut backend = LiveAttachBackend::new(runtime);

        let session = AttachController::new(ScriptedAttachBackend::ready())
            .attach(&supported_readiness(805))
            .expect("attach should succeed");

        let failure = backend.detach(&session).expect_err("detach should fail");
        assert_eq!(failure.kind, AttachFailureKind::DetachFailed);
    }

    #[test]
    fn attach_controller_with_live_backend_state_machine_consistency() {
        let detach_ack_line = IpcMessage::DetachAck { timestamp_ms: 9000 }.to_json_line();
        let runtime = ScriptedInstrumentationRuntime::success_with_messages(vec![
            bootstrap_report_line(),
            detach_ack_line,
        ]);
        let mut controller = AttachController::new(LiveAttachBackend::new(runtime));

        let session = controller
            .attach(&supported_readiness(806))
            .expect("attach should succeed");

        assert_eq!(session.state, AttachSessionState::Attached);
        assert!(controller.active_session().is_some());

        let detached = controller.detach().expect("detach should succeed");

        assert_eq!(detached.state, AttachSessionState::Detached);
        assert!(controller.active_session().is_none());
    }
}
