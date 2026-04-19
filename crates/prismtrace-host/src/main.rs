fn main() -> std::io::Result<()> {
    let result = prismtrace_host::bootstrap(std::env::current_dir()?)?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(pid) = attach_pid_arg(&args)? {
        let mut stdout = std::io::stdout().lock();
        prismtrace_host::run_foreground_attach_session(
            &result,
            &prismtrace_host::discovery::PsProcessSampleSource,
            prismtrace_host::runtime::NodeInstrumentationRuntime,
            pid,
            &mut stdout,
        )?;
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

    if args.iter().any(|arg| arg == "--readiness") {
        let snapshot = prismtrace_host::collect_readiness_snapshot(
            &result,
            &prismtrace_host::discovery::PsProcessSampleSource,
        )?;

        println!("{}", prismtrace_host::readiness_report(&snapshot));
        return Ok(());
    }

    if args.iter().any(|arg| arg == "--detach") {
        let mut controller = prismtrace_host::attach::AttachController::new(
            prismtrace_host::attach::ScriptedAttachBackend::ready(),
        );
        let snapshot = prismtrace_host::collect_detach_snapshot(&result, &mut controller)?;
        println!("{}", prismtrace_host::detach_report(&snapshot));
        return Ok(());
    }

    if args.iter().any(|arg| arg == "--attach-status") {
        let snapshot = prismtrace_host::collect_attach_status_snapshot(&result, None, None)?;
        println!("{}", prismtrace_host::attach_status_report(&snapshot));
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

fn attach_pid_arg(args: &[String]) -> std::io::Result<Option<u32>> {
    if !args.iter().any(|arg| arg == "--attach") {
        return Ok(None);
    }

    let pid = arg_value(args, "--attach").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "missing pid after --attach",
        )
    })?;

    let pid = pid.parse::<u32>().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid pid for --attach: {pid}"),
        )
    })?;

    Ok(Some(pid))
}

#[cfg(test)]
mod tests {
    use super::attach_pid_arg;

    #[test]
    fn attach_pid_arg_returns_none_when_flag_is_missing() {
        let args = vec!["--discover".to_string()];

        assert_eq!(attach_pid_arg(&args).expect("parse should succeed"), None);
    }

    #[test]
    fn attach_pid_arg_returns_error_when_value_is_missing() {
        let args = vec!["--attach".to_string()];

        let error = attach_pid_arg(&args).expect_err("missing pid should error");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("missing pid"));
    }

    #[test]
    fn attach_pid_arg_returns_error_for_non_numeric_pid() {
        let args = vec!["--attach".to_string(), "abc".to_string()];

        let error = attach_pid_arg(&args).expect_err("invalid pid should error");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("invalid pid"));
    }

    #[test]
    fn attach_pid_arg_parses_valid_pid() {
        let args = vec!["--attach".to_string(), "123".to_string()];

        assert_eq!(
            attach_pid_arg(&args).expect("parse should succeed"),
            Some(123)
        );
    }
}
