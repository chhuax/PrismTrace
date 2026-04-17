fn main() -> std::io::Result<()> {
    let result = prismtrace_host::bootstrap(std::env::current_dir()?)?;
    let args: Vec<String> = std::env::args().skip(1).collect();

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
