fn main() -> std::io::Result<()> {
    let result = prismtrace_host::bootstrap(std::env::current_dir()?)?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|arg| arg == "--console") {
        let filters = console_target_filters_arg(&args)?;
        let mut stdout = std::io::stdout().lock();
        return prismtrace_host::console::run_console_server_with_target_filters(
            &result,
            filters.as_deref(),
            &mut stdout,
        );
    }

    if let Some(options) = codex_observe_args(&args)? {
        let mut stdout = std::io::stdout().lock();
        prismtrace_host::run_codex_observer_session(&result, options, &mut stdout)?;
        return Ok(());
    }

    if args.iter().any(|arg| arg == "--discover") {
        let snapshot = prismtrace_host::collect_host_snapshot(
            &result,
            &prismtrace_host::discovery::PsProcessSampleSource,
        )?;

        println!("{}", prismtrace_host::discovery_report(&snapshot));
        return Ok(());
    }

    println!("{}", prismtrace_host::startup_summary(&result));

    Ok(())
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
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

fn codex_observe_args(
    args: &[String],
) -> std::io::Result<Option<prismtrace_host::codex_observer::CodexObserverOptions>> {
    if !args.iter().any(|arg| arg == "--codex-observe") {
        return Ok(None);
    }

    let socket_path = arg_value(args, "--codex-socket").map(std::path::PathBuf::from);
    Ok(Some(
        prismtrace_host::codex_observer::CodexObserverOptions {
            socket_path,
            ..Default::default()
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{codex_observe_args, console_target_filters_arg};

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
}
