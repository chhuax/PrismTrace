fn main() -> std::io::Result<()> {
    let result = prismtrace_host::bootstrap(std::env::current_dir()?)?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(pid) = arg_value(&args, "--attach") {
        let pid = pid.parse::<u32>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid pid for --attach: {pid}"),
            )
        })?;

        let snapshot = prismtrace_host::collect_attach_snapshot(
            &result,
            &prismtrace_host::discovery::PsProcessSampleSource,
            prismtrace_host::attach::ScriptedAttachBackend::ready(),
            pid,
        )?;

        println!("{}", prismtrace_host::attach_snapshot_report(&snapshot));
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

    println!("{}", prismtrace_host::startup_summary(&result));

    Ok(())
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}
