use crate::ipc::IpcEvent;
use prismtrace_core::{IpcMessage, ProbeHealth, ProbeState};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeSessionState {
    Idle,
    Bootstrapping,
    Alive,
    TimedOut,
    Disconnected,
}

impl ProbeSessionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Bootstrapping => "bootstrapping",
            Self::Alive => "alive",
            Self::TimedOut => "timed_out",
            Self::Disconnected => "disconnected",
        }
    }
}

pub struct ProbeHealthStore {
    pub health: Option<ProbeHealth>,
    pub last_heartbeat_at: Option<Instant>,
    pub session_state: ProbeSessionState,
}

impl ProbeHealthStore {
    pub fn new() -> Self {
        Self {
            health: None,
            last_heartbeat_at: None,
            session_state: ProbeSessionState::Idle,
        }
    }

    /// Apply an IpcEvent to update state.
    pub fn apply_event(&mut self, event: &IpcEvent) {
        match event {
            IpcEvent::Message(IpcMessage::BootstrapReport {
                installed_hooks,
                failed_hooks,
                ..
            }) => {
                self.health = Some(ProbeHealth {
                    state: ProbeState::Attached,
                    installed_hooks: installed_hooks.clone(),
                    failed_hooks: failed_hooks.clone(),
                });
                self.session_state = ProbeSessionState::Alive;
            }
            IpcEvent::Message(IpcMessage::Heartbeat { .. }) => {
                self.last_heartbeat_at = Some(Instant::now());
                self.session_state = ProbeSessionState::Alive;
            }
            IpcEvent::Message(IpcMessage::DetachAck { .. }) => {
                self.session_state = ProbeSessionState::Disconnected;
            }
            IpcEvent::Message(IpcMessage::HttpRequestObserved { .. }) => {}
            IpcEvent::Message(IpcMessage::HttpResponseObserved { .. }) => {}
            IpcEvent::HeartbeatTimeout { .. } => {
                self.session_state = ProbeSessionState::TimedOut;
            }
            IpcEvent::ChannelDisconnected { .. } => {
                self.session_state = ProbeSessionState::Disconnected;
            }
        }
    }

    /// Human-readable status summary.
    pub fn status_summary(&self) -> String {
        match &self.health {
            Some(h) => format!(
                "[{}] probe: {} (installed: {}, failed: {})",
                self.session_state.label(),
                match h.state {
                    ProbeState::Attached => "attached",
                    ProbeState::Attaching => "attaching",
                    ProbeState::Detached => "detached",
                    ProbeState::Failed => "failed",
                },
                h.installed_hooks.len(),
                h.failed_hooks.len()
            ),
            None => format!("[{}] probe: no health data", self.session_state.label()),
        }
    }
}

impl Default for ProbeHealthStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{ProbeHealthStore, ProbeSessionState};
    use crate::ipc::IpcEvent;
    use prismtrace_core::IpcMessage;

    #[test]
    fn bootstrap_report_fills_probe_health_correctly() {
        let mut store = ProbeHealthStore::new();
        let event = IpcEvent::Message(IpcMessage::BootstrapReport {
            installed_hooks: vec!["fetch".to_string(), "undici".to_string()],
            failed_hooks: vec![],
            timestamp_ms: 1000,
        });

        store.apply_event(&event);

        let health = store.health.as_ref().expect("health should be set");
        assert_eq!(health.installed_hooks, vec!["fetch", "undici"]);
        assert!(health.failed_hooks.is_empty());
        assert_eq!(store.session_state, ProbeSessionState::Alive);
    }

    #[test]
    fn failed_hooks_are_recorded() {
        let mut store = ProbeHealthStore::new();
        let event = IpcEvent::Message(IpcMessage::BootstrapReport {
            installed_hooks: vec!["fetch".to_string()],
            failed_hooks: vec!["http".to_string()],
            timestamp_ms: 2000,
        });

        store.apply_event(&event);

        let health = store.health.as_ref().expect("health should be set");
        assert_eq!(health.failed_hooks, vec!["http"]);
    }

    #[test]
    fn heartbeat_timeout_sets_session_state_to_timed_out() {
        let mut store = ProbeHealthStore::new();
        let event = IpcEvent::HeartbeatTimeout { elapsed_ms: 20000 };

        store.apply_event(&event);

        assert_eq!(store.session_state, ProbeSessionState::TimedOut);
    }

    #[test]
    fn channel_disconnected_sets_session_state_to_disconnected() {
        let mut store = ProbeHealthStore::new();
        let event = IpcEvent::ChannelDisconnected {
            reason: "EOF".to_string(),
        };

        store.apply_event(&event);

        assert_eq!(store.session_state, ProbeSessionState::Disconnected);
    }
}
