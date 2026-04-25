use super::{ConsoleSnapshot, ConsoleTargetFilterConfig, write_live_console_response};
use crate::BootstrapResult;
use std::io;
use std::io::Write;
use std::net::TcpListener;

#[derive(Debug)]
pub struct ConsoleServer {
    listener: TcpListener,
    snapshot: ConsoleSnapshot,
    result: BootstrapResult,
    bind_addr: String,
    filter: Option<ConsoleTargetFilterConfig>,
}

impl ConsoleServer {
    pub fn snapshot(&self) -> &ConsoleSnapshot {
        &self.snapshot
    }

    pub fn local_url(&self) -> io::Result<String> {
        Ok(format!("http://{}", self.listener.local_addr()?))
    }

    pub fn serve_once(&self) -> io::Result<()> {
        let (mut stream, _) = self.listener.accept()?;
        write_live_console_response(
            &mut stream,
            &self.result,
            &self.bind_addr,
            self.filter.as_ref(),
        )
    }

    pub fn serve_forever(&self) -> io::Result<()> {
        loop {
            let (mut stream, _) = self.listener.accept()?;
            write_live_console_response(
                &mut stream,
                &self.result,
                &self.bind_addr,
                self.filter.as_ref(),
            )?;
        }
    }
}

pub fn collect_console_snapshot(
    result: &BootstrapResult,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> ConsoleSnapshot {
    collect_console_snapshot_for_bind_addr(result, &result.config.bind_addr, filter, false)
}

pub(crate) fn collect_console_snapshot_for_bind_addr(
    result: &BootstrapResult,
    bind_addr: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
    include_sessions: bool,
) -> ConsoleSnapshot {
    let (_, unmatched_targets, target_summaries) = super::collect_target_partition_and_summaries(
        &crate::discovery::PsProcessSampleSource,
        filter,
    )
    .unwrap_or_else(|_| (Vec::new(), Vec::new(), Vec::new()));
    let mut request_summaries = super::model::filter_request_summaries(
        &super::load_request_summaries(&result.storage).unwrap_or_else(|_| Vec::new()),
        filter,
    );
    request_summaries.extend(super::load_observer_request_summaries(
        &result.storage,
        filter,
    ));
    super::dedup_request_summaries(&mut request_summaries);
    super::sort_request_summaries(&mut request_summaries);

    let mut session_summaries = if include_sessions {
        super::model::filter_session_summaries(
            &super::load_session_summaries(&result.storage).unwrap_or_else(|_| Vec::new()),
            filter,
        )
    } else {
        Vec::new()
    };
    if include_sessions {
        session_summaries.extend(super::load_observer_session_summaries(
            &result.storage,
            filter,
        ));
        super::dedup_session_summaries(&mut session_summaries);
        super::sort_session_summaries(&mut session_summaries);
    }
    let recent_requests = super::load_recent_request_activity(&result.storage);
    let mut activity_items = super::collect_activity_items_filtered(
        super::ConsoleActivitySource {
            recent_requests: &recent_requests,
            known_errors: &[],
        },
        filter,
        &unmatched_targets,
    );
    activity_items.extend(super::load_observer_activity_items(&result.storage, filter));
    super::dedup_activity_items(&mut activity_items);
    super::sort_activity_items(&mut activity_items);

    let mut target_summaries = target_summaries;
    target_summaries.extend(super::load_observer_target_summaries(
        &result.storage,
        filter,
    ));
    super::dedup_target_summaries(&mut target_summaries);
    super::sort_target_summaries(&mut target_summaries);

    ConsoleSnapshot {
        summary: crate::startup_summary(result),
        bind_addr: format!("http://{bind_addr}"),
        filter_context: super::model::console_filter_context(filter),
        target_summaries,
        activity_items,
        request_summaries,
        session_summaries,
        request_details: Vec::new(),
        session_details: Vec::new(),
    }
}

pub fn console_startup_report(snapshot: &ConsoleSnapshot) -> String {
    format!(
        "{}\nPrismTrace Local Console\nopen: {}",
        snapshot.summary, snapshot.bind_addr
    )
}

pub fn start_console_server(result: &BootstrapResult) -> io::Result<ConsoleServer> {
    start_console_server_with_target_filters(result, None)
}

pub fn run_console_server(result: &BootstrapResult, output: &mut impl Write) -> io::Result<()> {
    run_console_server_with_target_filters(result, None, output)
}

pub fn run_console_server_with_target_filters(
    result: &BootstrapResult,
    target_filters: Option<&[String]>,
    output: &mut impl Write,
) -> io::Result<()> {
    let server = start_console_server_with_target_filters(result, target_filters)?;
    writeln!(output, "{}", console_startup_report(server.snapshot()))?;
    server.serve_forever()
}

pub fn start_console_server_with_target_filters(
    result: &BootstrapResult,
    target_filters: Option<&[String]>,
) -> io::Result<ConsoleServer> {
    let filter = target_filters.map(|terms| ConsoleTargetFilterConfig::new(terms.to_vec()));
    start_console_server_on_bind_addr(result, &result.config.bind_addr, filter.as_ref())
}

pub fn start_console_server_on_bind_addr(
    result: &BootstrapResult,
    bind_addr: &str,
    filter: Option<&ConsoleTargetFilterConfig>,
) -> io::Result<ConsoleServer> {
    let listener = TcpListener::bind(bind_addr)?;
    let local_addr = listener.local_addr()?;
    let bind_addr = local_addr.to_string();
    let snapshot = collect_console_snapshot_for_bind_addr(result, &bind_addr, filter, false);

    Ok(ConsoleServer {
        listener,
        snapshot,
        result: result.clone(),
        bind_addr,
        filter: filter.cloned(),
    })
}
