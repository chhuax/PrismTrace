use std::io::{BufRead, Cursor};

/// Replaceable instrumentation runtime adapter.
/// Production implementations call a real dynamic instrumentation backend;
/// test implementations return controlled results.
pub trait InstrumentationRuntime: Send + 'static {
    /// Inject a bootstrap probe script into the target process.
    /// Returns the read end of the IPC channel (implemented as `BufRead`).
    fn inject_probe(
        &self,
        pid: u32,
        probe_script: &str,
    ) -> Result<Box<dyn BufRead + Send>, InstrumentationError>;

    /// Send a detach signal to the target process (via IPC or runtime API).
    fn send_detach_signal(&self, pid: u32) -> Result<(), InstrumentationError>;
}

/// Structured error returned by `InstrumentationRuntime` operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentationError {
    pub kind: InstrumentationErrorKind,
    pub message: String,
}

/// Discriminant for `InstrumentationError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrumentationErrorKind {
    PermissionDenied,
    ProcessNotFound,
    RuntimeIncompatible,
    InjectionFailed,
    DetachFailed,
}

impl InstrumentationErrorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::ProcessNotFound => "process_not_found",
            Self::RuntimeIncompatible => "runtime_incompatible",
            Self::InjectionFailed => "injection_failed",
            Self::DetachFailed => "detach_failed",
        }
    }
}

/// A test double for `InstrumentationRuntime` that returns pre-configured results.
pub struct ScriptedInstrumentationRuntime {
    inject_result: Result<Vec<String>, InstrumentationError>,
    detach_result: Result<(), InstrumentationError>,
}

impl ScriptedInstrumentationRuntime {
    /// Inject succeeds and returns a reader over the given IPC lines.
    pub fn success_with_messages(messages: Vec<String>) -> Self {
        Self {
            inject_result: Ok(messages),
            detach_result: Ok(()),
        }
    }

    /// Inject fails with the given error kind and message.
    pub fn inject_fails(kind: InstrumentationErrorKind, message: impl Into<String>) -> Self {
        Self {
            inject_result: Err(InstrumentationError {
                kind,
                message: message.into(),
            }),
            detach_result: Ok(()),
        }
    }

    /// Detach fails with the given error kind and message.
    pub fn detach_fails(kind: InstrumentationErrorKind, message: impl Into<String>) -> Self {
        Self {
            inject_result: Ok(vec![]),
            detach_result: Err(InstrumentationError {
                kind,
                message: message.into(),
            }),
        }
    }
}

impl InstrumentationRuntime for ScriptedInstrumentationRuntime {
    fn inject_probe(
        &self,
        _pid: u32,
        _probe_script: &str,
    ) -> Result<Box<dyn BufRead + Send>, InstrumentationError> {
        match &self.inject_result {
            Ok(lines) => {
                let content = lines.join("\n");
                let content = if content.is_empty() {
                    content
                } else {
                    format!("{}\n", content)
                };
                Ok(Box::new(Cursor::new(content.into_bytes())))
            }
            Err(e) => Err(e.clone()),
        }
    }

    fn send_detach_signal(&self, _pid: u32) -> Result<(), InstrumentationError> {
        self.detach_result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InstrumentationError, InstrumentationErrorKind, InstrumentationRuntime,
        ScriptedInstrumentationRuntime,
    };
    use std::io::BufRead;

    #[test]
    fn instrumentation_error_kind_labels_are_stable() {
        assert_eq!(
            InstrumentationErrorKind::PermissionDenied.label(),
            "permission_denied"
        );
        assert_eq!(
            InstrumentationErrorKind::ProcessNotFound.label(),
            "process_not_found"
        );
        assert_eq!(
            InstrumentationErrorKind::RuntimeIncompatible.label(),
            "runtime_incompatible"
        );
        assert_eq!(
            InstrumentationErrorKind::InjectionFailed.label(),
            "injection_failed"
        );
        assert_eq!(
            InstrumentationErrorKind::DetachFailed.label(),
            "detach_failed"
        );
    }

    #[test]
    fn scripted_runtime_success_returns_reader_over_messages() {
        let runtime = ScriptedInstrumentationRuntime::success_with_messages(vec![
            r#"{"type":"heartbeat","timestamp_ms":1}"#.into(),
            r#"{"type":"detach_ack","timestamp_ms":2}"#.into(),
        ]);

        let mut reader = runtime.inject_probe(42, "").expect("inject should succeed");

        let mut lines = Vec::new();
        let mut buf = String::new();
        while reader.read_line(&mut buf).unwrap() > 0 {
            lines.push(buf.trim_end_matches('\n').to_string());
            buf.clear();
        }

        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("heartbeat"));
        assert!(lines[1].contains("detach_ack"));
    }

    #[test]
    fn scripted_runtime_inject_fails_returns_error() {
        let runtime = ScriptedInstrumentationRuntime::inject_fails(
            InstrumentationErrorKind::InjectionFailed,
            "could not inject",
        );

        let result = runtime.inject_probe(99, "");
        match result {
            Err(err) => {
                assert_eq!(err.kind, InstrumentationErrorKind::InjectionFailed);
                assert_eq!(err.message, "could not inject");
            }
            Ok(_) => panic!("inject should have failed"),
        }
    }

    #[test]
    fn scripted_runtime_detach_fails_returns_error() {
        let runtime = ScriptedInstrumentationRuntime::detach_fails(
            InstrumentationErrorKind::DetachFailed,
            "could not detach",
        );

        let err = runtime
            .send_detach_signal(99)
            .expect_err("detach should fail");

        assert_eq!(err.kind, InstrumentationErrorKind::DetachFailed);
        assert_eq!(err.message, "could not detach");
    }

    #[test]
    fn scripted_runtime_detach_succeeds_by_default_in_success_variant() {
        let runtime = ScriptedInstrumentationRuntime::success_with_messages(vec!["line".into()]);

        runtime
            .send_detach_signal(1)
            .expect("detach should succeed");
    }

    #[test]
    fn scripted_runtime_inject_succeeds_in_detach_fails_variant() {
        let runtime = ScriptedInstrumentationRuntime::detach_fails(
            InstrumentationErrorKind::DetachFailed,
            "fail",
        );

        runtime
            .inject_probe(1, "")
            .expect("inject should succeed in detach_fails variant");
    }

    #[test]
    fn scripted_runtime_success_with_empty_messages_returns_empty_reader() {
        let runtime = ScriptedInstrumentationRuntime::success_with_messages(vec![]);

        let mut reader = runtime.inject_probe(1, "").expect("inject should succeed");

        let mut buf = String::new();
        let n = reader.read_line(&mut buf).unwrap();
        assert_eq!(n, 0, "reader should be empty");
    }

    #[test]
    fn instrumentation_error_fields_are_accessible() {
        let err = InstrumentationError {
            kind: InstrumentationErrorKind::ProcessNotFound,
            message: "pid 9999 not found".into(),
        };

        assert_eq!(err.kind, InstrumentationErrorKind::ProcessNotFound);
        assert_eq!(err.message, "pid 9999 not found");
    }
}

/// Production instrumentation runtime for Node / Electron processes.
///
/// V1 placeholder — real dynamic instrumentation (e.g. via Frida or node --inspect)
/// will be wired here in a future iteration. Currently returns `InjectionFailed`
/// so the CLI surfaces a clear error rather than silently doing nothing.
pub struct NodeInstrumentationRuntime;

impl InstrumentationRuntime for NodeInstrumentationRuntime {
    fn inject_probe(
        &self,
        pid: u32,
        _probe_script: &str,
    ) -> Result<Box<dyn BufRead + Send>, InstrumentationError> {
        Err(InstrumentationError {
            kind: InstrumentationErrorKind::InjectionFailed,
            message: format!(
                "real instrumentation backend not yet implemented (pid {pid}); \
                 use a supported dynamic instrumentation tool"
            ),
        })
    }

    fn send_detach_signal(&self, pid: u32) -> Result<(), InstrumentationError> {
        Err(InstrumentationError {
            kind: InstrumentationErrorKind::DetachFailed,
            message: format!("real instrumentation backend not yet implemented (pid {pid})"),
        })
    }
}
