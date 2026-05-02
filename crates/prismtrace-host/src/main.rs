#[derive(Debug, Clone, PartialEq, Eq)]
enum CommandSelection {
    Console(Option<Vec<String>>),
    ClaudeObserve(prismtrace_host::sources::claude::ClaudeObserverOptions),
    CodexObserve(prismtrace_host::sources::codex::CodexObserverOptions),
    OpencodeObserve(prismtrace_host::sources::opencode::OpencodeObserverOptions),
    Discover,
    StartupSummary,
}

fn main() -> std::io::Result<()> {
    let result = prismtrace_host::bootstrap(std::env::current_dir()?)?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    match selected_command(&args)? {
        CommandSelection::Console(filters) => {
            let mut stdout = std::io::stdout().lock();
            prismtrace_host::console::run_console_server_with_target_filters(
                &result,
                filters.as_deref(),
                &mut stdout,
            )
        }
        CommandSelection::CodexObserve(options) => {
            let mut stdout = std::io::stdout().lock();
            prismtrace_host::run_codex_observer_session(&result, options, &mut stdout)?;
            Ok(())
        }
        CommandSelection::ClaudeObserve(options) => {
            let mut stdout = std::io::stdout().lock();
            prismtrace_host::run_claude_observer_session(&result, options, &mut stdout)?;
            Ok(())
        }
        CommandSelection::OpencodeObserve(options) => {
            let mut stdout = std::io::stdout().lock();
            prismtrace_host::run_opencode_observer_session(&result, options, &mut stdout)?;
            Ok(())
        }
        CommandSelection::Discover => {
            let snapshot = prismtrace_host::collect_host_snapshot(
                &result,
                &prismtrace_host::discovery::PsProcessSampleSource,
            )?;

            println!("{}", prismtrace_host::discovery_report(&snapshot));
            Ok(())
        }
        CommandSelection::StartupSummary => {
            println!("{}", prismtrace_host::startup_summary(&result));
            Ok(())
        }
    }
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> std::io::Result<Option<&'a str>> {
    let mut index = 0;

    while index < args.len() {
        if args[index] == flag {
            let value = args.get(index + 1).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("missing value after {flag}"),
                )
            })?;

            if value.starts_with("--") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("missing value after {flag}"),
                ));
            }

            return Ok(Some(value.as_str()));
        }

        index += 1;
    }

    Ok(None)
}

fn console_target_filters_arg(args: &[String]) -> std::io::Result<Option<Vec<String>>> {
    let mut filters = Vec::new();
    let mut index = 0;

    while index < args.len() {
        if args[index] == "--target" {
            let value = args.get(index + 1).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "missing value after --target",
                )
            })?;
            if value.starts_with("--") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "missing value after --target",
                ));
            }
            filters.push(value.clone());
            index += 2;
            continue;
        }

        index += 1;
    }

    if filters.is_empty() {
        Ok(None)
    } else {
        Ok(Some(filters))
    }
}

fn selected_command(args: &[String]) -> std::io::Result<CommandSelection> {
    if args.iter().any(|arg| arg == "--console") {
        return Ok(CommandSelection::Console(console_target_filters_arg(args)?));
    }

    if let Some(options) = codex_observe_args(args)? {
        return Ok(CommandSelection::CodexObserve(options));
    }

    if let Some(options) = claude_observe_args(args)? {
        return Ok(CommandSelection::ClaudeObserve(options));
    }

    if let Some(options) = opencode_observe_args(args)? {
        return Ok(CommandSelection::OpencodeObserve(options));
    }

    if args.iter().any(|arg| arg == "--discover") {
        return Ok(CommandSelection::Discover);
    }

    Ok(CommandSelection::StartupSummary)
}

fn codex_observe_args(
    args: &[String],
) -> std::io::Result<Option<prismtrace_host::sources::codex::CodexObserverOptions>> {
    if !args.iter().any(|arg| arg == "--codex-observe") {
        return Ok(None);
    }

    let socket_path = arg_value(args, "--codex-socket")?.map(std::path::PathBuf::from);
    Ok(Some(
        prismtrace_host::sources::codex::CodexObserverOptions {
            socket_path,
            ..Default::default()
        },
    ))
}

fn claude_observe_args(
    args: &[String],
) -> std::io::Result<Option<prismtrace_host::sources::claude::ClaudeObserverOptions>> {
    if !args.iter().any(|arg| arg == "--claude-observe") {
        return Ok(None);
    }

    let mut options = prismtrace_host::sources::claude::ClaudeObserverOptions::default();
    if let Some(transcript_root) = arg_value(args, "--claude-transcript-root")? {
        options.transcript_root = std::path::PathBuf::from(transcript_root);
    }

    Ok(Some(options))
}

fn opencode_observe_args(
    args: &[String],
) -> std::io::Result<Option<prismtrace_host::sources::opencode::OpencodeObserverOptions>> {
    if !args.iter().any(|arg| arg == "--opencode-observe") {
        return Ok(None);
    }

    let mut options = prismtrace_host::sources::opencode::OpencodeObserverOptions::default();
    if let Some(base_url) = arg_value(args, "--opencode-url")? {
        options.base_url = base_url.to_string();
    }

    Ok(Some(options))
}

#[cfg(test)]
mod tests {
    use super::{
        CommandSelection, claude_observe_args, codex_observe_args, console_target_filters_arg,
        opencode_observe_args, selected_command,
    };

    #[test]
    fn console_flag_is_detected_without_conflicting_with_codex_observe_parsing() {
        let args = vec!["--console".to_string()];

        assert!(
            codex_observe_args(&args)
                .expect("parse should succeed")
                .is_none()
        );
    }

    #[test]
    fn console_target_filters_arg_returns_none_when_flag_is_missing() {
        let args = vec!["--console".to_string()];

        assert_eq!(
            console_target_filters_arg(&args).expect("parse should succeed"),
            None
        );
    }

    #[test]
    fn console_target_filters_arg_parses_single_filter_term() {
        let args = vec![
            "--console".to_string(),
            "--target".to_string(),
            "codex".to_string(),
        ];

        assert_eq!(
            console_target_filters_arg(&args).expect("parse should succeed"),
            Some(vec!["codex".to_string()])
        );
    }

    #[test]
    fn console_target_filters_arg_parses_multiple_filter_terms_in_order() {
        let args = vec![
            "--console".to_string(),
            "--target".to_string(),
            "codex".to_string(),
            "--target".to_string(),
            "observer".to_string(),
        ];

        assert_eq!(
            console_target_filters_arg(&args).expect("parse should succeed"),
            Some(vec!["codex".to_string(), "observer".to_string()])
        );
    }

    #[test]
    fn codex_observe_args_returns_none_when_flag_is_missing() {
        let args = vec!["--discover".to_string()];

        assert!(
            codex_observe_args(&args)
                .expect("parse should succeed")
                .is_none()
        );
    }

    #[test]
    fn codex_observe_args_returns_default_options_without_socket() {
        let args = vec!["--codex-observe".to_string()];
        let options = codex_observe_args(&args)
            .expect("parse should succeed")
            .expect("options should exist");

        assert_eq!(options.socket_path, None);
    }

    #[test]
    fn codex_observe_args_parses_explicit_socket_path() {
        let args = vec![
            "--codex-observe".to_string(),
            "--codex-socket".to_string(),
            "/tmp/codex-ipc/ipc-501.sock".to_string(),
        ];
        let options = codex_observe_args(&args)
            .expect("parse should succeed")
            .expect("options should exist");

        assert_eq!(
            options.socket_path,
            Some(std::path::PathBuf::from("/tmp/codex-ipc/ipc-501.sock"))
        );
    }

    #[test]
    fn codex_observe_args_errors_when_socket_value_is_missing() {
        let args = vec!["--codex-observe".to_string(), "--codex-socket".to_string()];

        let error = codex_observe_args(&args).expect_err("parse should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn codex_observe_args_errors_when_socket_value_is_another_flag() {
        let args = vec![
            "--codex-observe".to_string(),
            "--codex-socket".to_string(),
            "--discover".to_string(),
        ];

        let error = codex_observe_args(&args).expect_err("parse should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn claude_observe_args_returns_none_when_flag_is_missing() {
        let args = vec!["--discover".to_string()];

        assert!(
            claude_observe_args(&args)
                .expect("parse should succeed")
                .is_none()
        );
    }

    #[test]
    fn claude_observe_args_uses_default_transcript_root() {
        let args = vec!["--claude-observe".to_string()];
        let options = claude_observe_args(&args)
            .expect("parse should succeed")
            .expect("options should exist");

        assert!(options.transcript_root.ends_with(".claude/projects"));
    }

    #[test]
    fn claude_observe_args_parses_explicit_transcript_root() {
        let args = vec![
            "--claude-observe".to_string(),
            "--claude-transcript-root".to_string(),
            "/tmp/claude/projects".to_string(),
        ];
        let options = claude_observe_args(&args)
            .expect("parse should succeed")
            .expect("options should exist");

        assert_eq!(
            options.transcript_root,
            std::path::PathBuf::from("/tmp/claude/projects")
        );
    }

    #[test]
    fn claude_observe_args_errors_when_transcript_root_value_is_missing() {
        let args = vec![
            "--claude-observe".to_string(),
            "--claude-transcript-root".to_string(),
        ];

        let error = claude_observe_args(&args).expect_err("parse should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn claude_observe_args_errors_when_transcript_root_value_is_another_flag() {
        let args = vec![
            "--claude-observe".to_string(),
            "--claude-transcript-root".to_string(),
            "--discover".to_string(),
        ];

        let error = claude_observe_args(&args).expect_err("parse should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn opencode_observe_args_returns_none_when_flag_is_missing() {
        let args = vec!["--discover".to_string()];

        assert!(
            opencode_observe_args(&args)
                .expect("parse should succeed")
                .is_none()
        );
    }

    #[test]
    fn opencode_observe_args_returns_default_options_without_url() {
        let args = vec!["--opencode-observe".to_string()];
        let options = opencode_observe_args(&args)
            .expect("parse should succeed")
            .expect("options should exist");

        assert_eq!(
            options.base_url,
            prismtrace_host::sources::opencode::OpencodeObserverOptions::default().base_url
        );
    }

    #[test]
    fn opencode_observe_args_parses_explicit_base_url() {
        let args = vec![
            "--opencode-observe".to_string(),
            "--opencode-url".to_string(),
            "http://127.0.0.1:4999".to_string(),
        ];
        let options = opencode_observe_args(&args)
            .expect("parse should succeed")
            .expect("options should exist");

        assert_eq!(options.base_url, "http://127.0.0.1:4999");
    }

    #[test]
    fn opencode_observe_args_errors_when_url_value_is_missing() {
        let args = vec![
            "--opencode-observe".to_string(),
            "--opencode-url".to_string(),
        ];

        let error = opencode_observe_args(&args).expect_err("parse should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn opencode_observe_args_errors_when_url_value_is_another_flag() {
        let args = vec![
            "--opencode-observe".to_string(),
            "--opencode-url".to_string(),
            "--discover".to_string(),
        ];

        let error = opencode_observe_args(&args).expect_err("parse should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn selected_command_picks_opencode_observe_command() {
        let args = vec!["--opencode-observe".to_string()];

        assert!(matches!(
            selected_command(&args).expect("parse should succeed"),
            CommandSelection::OpencodeObserve(_)
        ));
    }

    #[test]
    fn selected_command_picks_claude_observe_command() {
        let args = vec!["--claude-observe".to_string()];

        assert!(matches!(
            selected_command(&args).expect("parse should succeed"),
            CommandSelection::ClaudeObserve(_)
        ));
    }

    #[test]
    fn selected_command_prioritizes_console_over_opencode_observe() {
        let args = vec!["--console".to_string(), "--opencode-observe".to_string()];

        assert!(matches!(
            selected_command(&args).expect("parse should succeed"),
            CommandSelection::Console(_)
        ));
    }

    #[test]
    fn selected_command_prioritizes_codex_over_opencode_observe() {
        let args = vec![
            "--codex-observe".to_string(),
            "--opencode-observe".to_string(),
        ];

        assert!(matches!(
            selected_command(&args).expect("parse should succeed"),
            CommandSelection::CodexObserve(_)
        ));
    }

    #[test]
    fn console_target_filters_arg_errors_when_target_value_is_missing() {
        let args = vec!["--console".to_string(), "--target".to_string()];

        let error = console_target_filters_arg(&args).expect_err("parse should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn console_target_filters_arg_errors_when_target_value_is_another_flag() {
        let args = vec![
            "--console".to_string(),
            "--target".to_string(),
            "--discover".to_string(),
        ];

        let error = console_target_filters_arg(&args).expect_err("parse should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}
