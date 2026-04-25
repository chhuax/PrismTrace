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
    let path = target
        .executable_path
        .to_string_lossy()
        .to_ascii_lowercase();
    let app_name = target.app_name.to_ascii_lowercase();
    let command_line = target
        .command_line
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();

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

    if is_codex_target(&path, &command_line) {
        return (
            AttachReadinessStatus::Unsupported,
            "Codex.app currently has no safe attach path: PrismTrace wakes Node inspector with SIGUSR1, and that signal crashes the live Codex runtime".into(),
        );
    }

    match target.runtime_kind {
        RuntimeKind::Node
            if app_name.contains("language-server")
                || command_line.contains("language-server")
                || command_line.contains("--stdio") =>
        {
            (
                AttachReadinessStatus::Unknown,
                "node helper process looks like an auxiliary stdio or language-server target, not the app's model-facing runtime".into(),
            )
        }
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

fn is_codex_target(path: &str, command_line: &str) -> bool {
    path.ends_with("/applications/codex.app/contents/macos/codex")
        || path.ends_with("/applications/codex.app/contents/resources/codex")
        || path.ends_with("/applications/codex.app/contents/resources/node_repl")
        || command_line.contains("/applications/codex.app/contents/resources/codex app-server")
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
            command_line: None,
            runtime_kind: RuntimeKind::Node,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::Supported);
        assert!(readiness.reason.contains("node"));
    }

    #[test]
    fn evaluate_target_marks_packaged_opencode_binary_as_supported() {
        let target = ProcessTarget {
            pid: 401,
            app_name: "opencode".into(),
            executable_path: PathBuf::from("/Users/test/.opencode/bin/opencode"),
            command_line: Some("/Users/test/.opencode/bin/opencode".into()),
            runtime_kind: RuntimeKind::Node,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::Supported);
        assert!(readiness.reason.contains("node"));
    }

    #[test]
    fn evaluate_target_marks_codex_app_server_as_unsupported() {
        let target = ProcessTarget {
            pid: 401,
            app_name: "codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/Resources/codex"),
            command_line: Some(
                "/Applications/Codex.app/Contents/Resources/codex app-server --analytics-default-enabled"
                    .into(),
            ),
            runtime_kind: RuntimeKind::Node,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::Unsupported);
        assert!(readiness.reason.contains("safe attach path"));
    }

    #[test]
    fn evaluate_target_marks_codex_main_app_as_unsupported() {
        let target = ProcessTarget {
            pid: 402,
            app_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            command_line: Some("/Applications/Codex.app/Contents/MacOS/Codex".into()),
            runtime_kind: RuntimeKind::Electron,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::Unsupported);
        assert!(readiness.reason.contains("Codex.app"));
    }

    #[test]
    fn evaluate_target_marks_system_process_as_permission_denied() {
        let target = ProcessTarget {
            pid: 403,
            app_name: "launchd".into(),
            executable_path: PathBuf::from("/sbin/launchd"),
            command_line: None,
            runtime_kind: RuntimeKind::Unknown,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::PermissionDenied);
    }

    #[test]
    fn evaluate_target_marks_codex_path_as_unsupported_even_when_runtime_is_unknown() {
        let target = ProcessTarget {
            pid: 404,
            app_name: "Codex".into(),
            executable_path: PathBuf::from("/Applications/Codex.app/Contents/MacOS/Codex"),
            command_line: None,
            runtime_kind: RuntimeKind::Unknown,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::Unsupported);
    }

    #[test]
    fn evaluate_targets_preserves_target_count() {
        let targets = vec![
            ProcessTarget {
                pid: 405,
                app_name: "node".into(),
                executable_path: PathBuf::from("/usr/local/bin/node"),
                command_line: None,
                runtime_kind: RuntimeKind::Node,
            },
            ProcessTarget {
                pid: 406,
                app_name: "python3".into(),
                executable_path: PathBuf::from("/usr/bin/python3"),
                command_line: None,
                runtime_kind: RuntimeKind::Unknown,
            },
        ];

        let readiness_results = evaluate_targets(&targets);

        assert_eq!(readiness_results.len(), 2);
    }

    #[test]
    fn evaluate_target_marks_language_server_helpers_as_unknown() {
        let target = ProcessTarget {
            pid: 407,
            app_name: "yaml-language-server".into(),
            executable_path: PathBuf::from("/usr/local/bin/node"),
            command_line: Some(
                "node /Users/test/.cache/opencode/packages/yaml-language-server/node_modules/.bin/yaml-language-server --stdio".into(),
            ),
            runtime_kind: RuntimeKind::Node,
        };

        let readiness = evaluate_target(&target);

        assert_eq!(readiness.status, AttachReadinessStatus::Unknown);
        assert!(readiness.reason.contains("helper"));
    }
}
