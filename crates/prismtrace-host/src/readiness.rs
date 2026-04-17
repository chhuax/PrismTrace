use prismtrace_core::{AttachReadiness, AttachReadinessStatus, ProcessTarget, RuntimeKind};

pub fn evaluate_targets(targets: &[ProcessTarget]) -> Vec<AttachReadiness> {
    targets.iter().map(evaluate_target).collect()
}

pub fn evaluate_target(target: &ProcessTarget) -> AttachReadiness {
    let (status, reason) = classify_target(target);

    AttachReadiness {
        target: target.clone(),
        status,
        reason,
    }
}

fn classify_target(target: &ProcessTarget) -> (AttachReadinessStatus, String) {
    let path = target.executable_path.to_string_lossy().to_ascii_lowercase();

    if path.starts_with("/system/")
        || path.starts_with("/usr/libexec/")
        || path.starts_with("/sbin/")
        || path.starts_with("/usr/sbin/")
    {
        return (
            AttachReadinessStatus::PermissionDenied,
            "target appears to live in a protected system location".into(),
        );
    }

    match target.runtime_kind {
        RuntimeKind::Node => (
            AttachReadinessStatus::Supported,
            "node runtime target looks suitable for attach readiness checks".into(),
        ),
        RuntimeKind::Electron => (
            AttachReadinessStatus::Supported,
            "electron runtime target looks suitable for attach readiness checks".into(),
        ),
        RuntimeKind::Unknown => (
            AttachReadinessStatus::Unknown,
            "runtime classification is not strong enough to recommend attach yet".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate_target, evaluate_targets};
    use prismtrace_core::{AttachReadinessStatus, ProcessTarget, RuntimeKind};
    use std::path::PathBuf;

    #[test]
    fn evaluate_target_marks_userland_node_process_as_supported() {
        let target = ProcessTarget {
            pid: 401,
            app_name: "node".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            runtime_kind: RuntimeKind::Node,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::Supported);
        assert!(readiness.reason.contains("node"));
    }

    #[test]
    fn evaluate_target_marks_system_process_as_permission_denied() {
        let target = ProcessTarget {
            pid: 402,
            app_name: "launchd".into(),
            executable_path: PathBuf::from("/sbin/launchd"),
            runtime_kind: RuntimeKind::Unknown,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::PermissionDenied);
    }

    #[test]
    fn evaluate_target_keeps_unknown_when_runtime_is_not_actionable() {
        let target = ProcessTarget {
            pid: 403,
            app_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            runtime_kind: RuntimeKind::Unknown,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::Unknown);
    }

    #[test]
    fn evaluate_targets_preserves_target_count() {
        let targets = vec![
            ProcessTarget {
                pid: 404,
                app_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                runtime_kind: RuntimeKind::Node,
            },
            ProcessTarget {
                pid: 405,
                app_name: "python3".into(),
                executable_path: PathBuf::from("/usr/bin/python3"),
                runtime_kind: RuntimeKind::Unknown,
            },
        ];

        let readiness_results = evaluate_targets(&targets);

        assert_eq!(readiness_results.len(), 2);
    }
}
