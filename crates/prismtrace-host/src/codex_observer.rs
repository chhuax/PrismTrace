use crate::observer::{
    ObservedEvent, ObservedEventKind, ObserverChannelKind, ObserverHandshake, ObserverSession,
    ObserverSource, ObserverSourceFactory,
};
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const CODEX_APP_SERVER_BIN: &str = "/Applications/Codex.app/Contents/Resources/codex";
const DEFAULT_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_millis(750);
const DEFAULT_MAX_EVENTS: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexObserverOptions {
    pub socket_path: Option<PathBuf>,
    pub initialize_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_events: usize,
}

impl Default for CodexObserverOptions {
    fn default() -> Self {
        Self {
            socket_path: None,
            initialize_timeout: DEFAULT_INITIALIZE_TIMEOUT,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            max_events: DEFAULT_MAX_EVENTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexTransport {
    ProxySocket { socket_path: PathBuf },
    StandaloneAppServer,
}

impl CodexTransport {
    fn label(&self) -> String {
        match self {
            Self::ProxySocket { socket_path } => {
                format!("proxy-socket ({})", socket_path.display())
            }
            Self::StandaloneAppServer => "standalone-app-server".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexObserverSource {
    transport: CodexTransport,
    initialize_timeout: Duration,
}

impl ObserverSource for CodexObserverSource {
    fn channel_kind(&self) -> ObserverChannelKind {
        ObserverChannelKind::CodexAppServer
    }

    fn transport_label(&self) -> String {
        self.transport.label()
    }

    fn connect(&self) -> io::Result<Box<dyn ObserverSession>> {
        if let CodexTransport::ProxySocket { socket_path } = &self.transport {
            validate_proxy_socket_endpoint(socket_path, self.initialize_timeout)?;
        }
        Ok(Box::new(CodexObserverSession::spawn(
            self.transport.clone(),
            self.initialize_timeout,
        )?))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CodexObserverFactory;

impl ObserverSourceFactory<CodexObserverOptions> for CodexObserverFactory {
    fn build_sources(
        &self,
        request: &CodexObserverOptions,
    ) -> io::Result<Vec<Box<dyn ObserverSource>>> {
        let mut sources: Vec<Box<dyn ObserverSource>> = Vec::new();

        if let Some(socket_path) = &request.socket_path {
            sources.push(Box::new(CodexObserverSource {
                transport: CodexTransport::ProxySocket {
                    socket_path: socket_path.clone(),
                },
                initialize_timeout: request.initialize_timeout,
            }));
            return Ok(sources);
        }

        if let Some(socket_path) = discover_latest_codex_socket(None)? {
            sources.push(Box::new(CodexObserverSource {
                transport: CodexTransport::ProxySocket { socket_path },
                initialize_timeout: request.initialize_timeout,
            }));
        }

        sources.push(Box::new(CodexObserverSource {
            transport: CodexTransport::StandaloneAppServer,
            initialize_timeout: request.initialize_timeout,
        }));

        Ok(sources)
    }
}

pub fn run_codex_observer(
    storage: &StorageLayout,
    output: &mut impl Write,
    options: CodexObserverOptions,
) -> io::Result<()> {
    let factory = CodexObserverFactory;
    let mut last_error = None;

    for source in factory.build_sources(&options)? {
        writeln!(
            output,
            "[codex-observer] attempting {} via {}",
            source.channel_kind().label(),
            source.transport_label()
        )?;

        match source.connect() {
            Ok(mut session) => match session.initialize() {
                Ok(handshake) => {
                    let artifact_writer = CodexObserverArtifactWriter::create(storage, &handshake)?;
                    writeln!(
                        output,
                        "{}",
                        serde_json::to_string(&json!({
                            "type": "codex_observer_handshake",
                            "channel": handshake.channel_kind.label(),
                            "transport": handshake.transport_label,
                            "server_label": handshake.server_label,
                            "raw": handshake.raw_json,
                        }))?
                    )?;

                    for event in session.collect_capability_events()? {
                        artifact_writer.append_event(&event)?;
                        writeln!(output, "{}", serde_json::to_string(&event_as_json(&event))?)?;
                    }

                    for _ in 0..options.max_events {
                        match session.next_event(options.idle_timeout)? {
                            Some(event) => {
                                artifact_writer.append_event(&event)?;
                                writeln!(
                                    output,
                                    "{}",
                                    serde_json::to_string(&event_as_json(&event))?
                                )?;
                            }
                            None => break,
                        }
                    }

                    return Ok(());
                }
                Err(error) => {
                    writeln!(
                        output,
                        "[codex-observer] {} initialize failed: {}",
                        source.transport_label(),
                        error
                    )?;
                    last_error = Some(error);
                }
            },
            Err(error) => {
                writeln!(
                    output,
                    "[codex-observer] {} connect failed: {}",
                    source.transport_label(),
                    error
                )?;
                last_error = Some(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no Codex observer source could be constructed",
        )
    }))
}

fn event_as_json(event: &ObservedEvent) -> Value {
    json!({
        "type": "codex_observer_event",
        "channel": event.channel_kind.label(),
        "event_kind": event.event_kind.label(),
        "summary": event.summary,
        "method": event.method,
        "thread_id": event.thread_id,
        "turn_id": event.turn_id,
        "item_id": event.item_id,
        "timestamp": event.timestamp,
        "raw": event.raw_json,
    })
}

struct CodexObserverArtifactWriter {
    artifact_path: PathBuf,
}

impl CodexObserverArtifactWriter {
    fn create(storage: &StorageLayout, handshake: &ObserverHandshake) -> io::Result<Self> {
        let observer_dir = storage.artifacts_dir.join("observer_events").join("codex");
        fs::create_dir_all(&observer_dir)?;

        let started_at_ms = current_time_ms()?;
        let artifact_path =
            observer_dir.join(format!("{started_at_ms}-{}.jsonl", std::process::id()));
        let writer = Self { artifact_path };
        writer.append_json_line(&json!({
            "record_type": "handshake",
            "channel": handshake.channel_kind.label(),
            "transport": handshake.transport_label,
            "server_label": handshake.server_label,
            "recorded_at_ms": started_at_ms,
            "raw_json": handshake.raw_json,
        }))?;

        Ok(writer)
    }

    fn append_event(&self, event: &ObservedEvent) -> io::Result<()> {
        self.append_json_line(&json!({
            "record_type": "event",
            "channel": event.channel_kind.label(),
            "event_kind": event.event_kind.label(),
            "summary": event.summary,
            "method": event.method,
            "thread_id": event.thread_id,
            "turn_id": event.turn_id,
            "item_id": event.item_id,
            "timestamp": event.timestamp,
            "recorded_at_ms": current_time_ms()?,
            "raw_json": event.raw_json,
        }))
    }

    #[cfg(test)]
    fn artifact_path(&self) -> &Path {
        &self.artifact_path
    }

    fn append_json_line(&self, value: &Value) -> io::Result<()> {
        let mut artifact = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.artifact_path)?;
        serde_json::to_writer(&mut artifact, value)?;
        artifact.write_all(b"\n")?;
        artifact.flush()?;
        Ok(())
    }
}

fn current_time_ms() -> io::Result<u64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    Ok(duration.as_millis() as u64)
}

fn validate_proxy_socket_endpoint(socket_path: &Path, timeout: Duration) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let payload = json!({
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "prismtrace-socket-probe",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "experimentalApi": true,
            }
        }
    });

    serde_json::to_writer(&mut stream, &payload)
        .map_err(|error| invalid_socket_endpoint_error(socket_path, error.into()))?;
    stream
        .write_all(b"\n")
        .map_err(|error| invalid_socket_endpoint_error(socket_path, error))?;
    stream
        .flush()
        .map_err(|error| invalid_socket_endpoint_error(socket_path, error))?;

    let mut buffer = vec![0_u8; 4096];
    let read = stream.read(&mut buffer)?;
    if read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            format!(
                "socket {} accepted the connection but closed immediately after initialize; this does not look like a live Codex app-server protocol endpoint",
                socket_path.display()
            ),
        ));
    }

    let payload = std::str::from_utf8(&buffer[..read]).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "socket {} returned non-UTF8 bytes after initialize: {error}",
                socket_path.display()
            ),
        )
    })?;

    let first_line = payload
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "socket {} returned only empty bytes after initialize",
                    socket_path.display()
                ),
            )
        })?;

    let value: Value = serde_json::from_str(first_line).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "socket {} returned non-JSON payload after initialize: {error}",
                socket_path.display()
            ),
        )
    })?;

    if value.get("result").is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "socket {} returned JSON but not an initialize result",
                socket_path.display()
            ),
        ));
    }

    Ok(())
}

fn invalid_socket_endpoint_error(socket_path: &Path, error: io::Error) -> io::Error {
    match error.kind() {
        io::ErrorKind::BrokenPipe
        | io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::NotConnected => io::Error::new(
            io::ErrorKind::ConnectionAborted,
            format!(
                "socket {} closed during initialize; this does not look like a live Codex app-server protocol endpoint",
                socket_path.display()
            ),
        ),
        _ => error,
    }
}

pub fn discover_latest_codex_socket(temp_dir: Option<&Path>) -> io::Result<Option<PathBuf>> {
    let enforce_desktop_owner = temp_dir.is_none();
    let root = temp_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
        .join("codex-ipc");

    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    let mut sockets = Vec::new();

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_socket() {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified = metadata.modified().ok();
        sockets.push((entry.path(), modified));
    }

    sockets.sort_by(|left, right| right.1.cmp(&left.1));

    if !enforce_desktop_owner {
        return Ok(sockets.into_iter().map(|(path, _)| path).next());
    }

    Ok(select_latest_desktop_codex_socket(
        sockets.into_iter().map(|(path, _)| path).collect(),
        |path| socket_owned_by_desktop_codex(path).unwrap_or(false),
    ))
}

fn select_latest_desktop_codex_socket<F>(
    sockets: Vec<PathBuf>,
    mut owner_matches: F,
) -> Option<PathBuf>
where
    F: FnMut(&Path) -> bool,
{
    sockets.into_iter().find(|path| owner_matches(path))
}

fn socket_owned_by_desktop_codex(socket_path: &Path) -> io::Result<bool> {
    let pids = socket_owner_pids(socket_path)?;
    Ok(pids
        .into_iter()
        .filter_map(|pid| process_command_line(pid).ok())
        .any(|command| command_looks_like_desktop_codex_owner(&command)))
}

fn socket_owner_pids(socket_path: &Path) -> io::Result<Vec<u32>> {
    let output = Command::new("lsof")
        .arg("-t")
        .arg("--")
        .arg(socket_path)
        .output()?;

    if !output.status.success() && !output.stdout.is_empty() {
        return Err(io::Error::other(format!(
            "failed to inspect socket owners via lsof for {}",
            socket_path.display()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect())
}

fn process_command_line(pid: u32) -> io::Result<String> {
    let output = Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("command=")
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "failed to inspect process command for pid {pid}"
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn command_looks_like_desktop_codex_owner(command: &str) -> bool {
    let normalized = command.trim();
    normalized.contains("/Applications/Codex.app/Contents/Resources/codex")
        || normalized.contains("/Applications/Codex.app/Contents/MacOS/Codex")
}

struct CodexObserverSession {
    transport: CodexTransport,
    initialize_timeout: Duration,
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<io::Result<String>>,
    reader_thread: Option<thread::JoinHandle<()>>,
    pending: VecDeque<Value>,
    next_request_id: u64,
    inflight_capability_requests: HashMap<u64, (String, ObservedEventKind)>,
}

impl CodexObserverSession {
    fn spawn(transport: CodexTransport, initialize_timeout: Duration) -> io::Result<Self> {
        let mut command = Command::new(CODEX_APP_SERVER_BIN);
        command.arg("app-server");

        match &transport {
            CodexTransport::ProxySocket { socket_path } => {
                command.arg("proxy").arg("--sock").arg(socket_path);
            }
            CodexTransport::StandaloneAppServer => {}
        }

        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("codex observer stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("codex observer stdout unavailable"))?;

        let (sender, receiver) = mpsc::channel();
        let reader_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let line = line.trim().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        if sender.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            transport,
            initialize_timeout,
            child,
            stdin,
            lines: receiver,
            reader_thread: Some(reader_thread),
            pending: VecDeque::new(),
            next_request_id: 2,
            inflight_capability_requests: HashMap::new(),
        })
    }

    fn send_request(&mut self, id: u64, method: &str, params: Value) -> io::Result<()> {
        let request = json!({
            "id": id,
            "method": method,
            "params": params,
        });

        serde_json::to_writer(&mut self.stdin, &request)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn send_next_request(&mut self, method: &str, params: Value) -> io::Result<u64> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(id, method, params)?;
        Ok(id)
    }

    fn read_value_with_timeout(&mut self, timeout: Duration) -> io::Result<Option<Value>> {
        match self.lines.recv_timeout(timeout) {
            Ok(Ok(line)) => serde_json::from_str::<Value>(&line)
                .map(Some)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Ok(Err(error)) => Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    fn take_response(&mut self, id: u64, timeout: Duration) -> io::Result<Value> {
        let deadline = Instant::now() + timeout;

        loop {
            if let Some(index) = self
                .pending
                .iter()
                .position(|value| value.get("id").and_then(Value::as_u64) == Some(id))
            {
                return Ok(self
                    .pending
                    .remove(index)
                    .expect("pending index should exist"));
            }

            let now = Instant::now();
            if now >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("timed out waiting for codex response id={id}"),
                ));
            }

            let remaining = deadline.saturating_duration_since(now);
            match self.read_value_with_timeout(remaining)? {
                Some(value) => {
                    if value.get("id").and_then(Value::as_u64) == Some(id) {
                        return Ok(value);
                    }
                    self.pending.push_back(value);
                }
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("timed out waiting for codex response id={id}"),
                    ));
                }
            }
        }
    }
}

impl ObserverSession for CodexObserverSession {
    fn initialize(&mut self) -> io::Result<ObserverHandshake> {
        self.send_request(
            1,
            "initialize",
            json!({
                "clientInfo": {
                    "name": "prismtrace",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "experimentalApi": true,
                }
            }),
        )?;

        let response = self.take_response(1, self.initialize_timeout)?;
        if let Some(message) = response_error_message(&response) {
            return Err(io::Error::other(format!(
                "codex initialize failed: {message}"
            )));
        }
        let result = response
            .get("result")
            .cloned()
            .ok_or_else(|| io::Error::other("codex initialize response missing result"))?;
        let user_agent = result
            .get("userAgent")
            .and_then(Value::as_str)
            .unwrap_or("unknown codex app-server");

        Ok(ObserverHandshake {
            channel_kind: ObserverChannelKind::CodexAppServer,
            transport_label: self.transport.label(),
            server_label: user_agent.to_string(),
            raw_json: response,
        })
    }

    fn collect_capability_events(&mut self) -> io::Result<Vec<ObservedEvent>> {
        let mut events = Vec::new();

        let requests = [
            ("skills/list", json!({}), ObservedEventKind::Skill),
            ("plugin/list", json!({}), ObservedEventKind::Plugin),
            ("app/list", json!({ "limit": 10 }), ObservedEventKind::App),
        ];

        for (method, params, event_kind) in requests {
            let id = self.send_next_request(method, params)?;
            self.inflight_capability_requests
                .insert(id, (method.to_string(), event_kind));
            match self.take_response(id, self.initialize_timeout) {
                Ok(value) => {
                    self.inflight_capability_requests.remove(&id);
                    events.push(capability_event_from_response(method, event_kind, value));
                }
                Err(error) => events.push(ObservedEvent {
                    channel_kind: ObserverChannelKind::CodexAppServer,
                    event_kind: ObservedEventKind::Unknown,
                    summary: format!("{method} unavailable: {error}"),
                    method: Some(method.to_string()),
                    thread_id: None,
                    turn_id: None,
                    item_id: None,
                    timestamp: None,
                    raw_json: json!({
                        "method": method,
                        "error": error.to_string(),
                    }),
                }),
            }
        }

        Ok(events)
    }

    fn next_event(&mut self, timeout: Duration) -> io::Result<Option<ObservedEvent>> {
        if let Some(value) = self.pending.pop_front() {
            return Ok(Some(self.normalize_value(value)));
        }

        self.read_value_with_timeout(timeout)
            .map(|value| value.map(|value| self.normalize_value(value)))
    }
}

impl Drop for CodexObserverSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
    }
}

impl CodexObserverSession {
    fn normalize_value(&mut self, value: Value) -> ObservedEvent {
        if let Some(id) = value.get("id").and_then(Value::as_u64)
            && let Some((method, event_kind)) = self.inflight_capability_requests.remove(&id)
        {
            return capability_event_from_response(&method, event_kind, value);
        }

        normalize_server_value(value)
    }
}

fn capability_event_from_response(
    method: &str,
    event_kind: ObservedEventKind,
    value: Value,
) -> ObservedEvent {
    if let Some(message) = response_error_message(&value) {
        return ObservedEvent {
            channel_kind: ObserverChannelKind::CodexAppServer,
            event_kind: ObservedEventKind::Unknown,
            summary: format!("{method} unavailable: {message}"),
            method: Some(method.to_string()),
            thread_id: None,
            turn_id: None,
            item_id: None,
            timestamp: value
                .get("timestamp")
                .and_then(Value::as_str)
                .map(str::to_string),
            raw_json: value,
        };
    }

    let count = match method {
        "skills/list" => value
            .get("result")
            .and_then(|result| result.get("data"))
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("skills").and_then(Value::as_array))
                    .map(Vec::len)
                    .sum::<usize>()
            })
            .unwrap_or(0),
        "plugin/list" => value
            .get("result")
            .and_then(|result| result.get("marketplaces"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        "app/list" => value
            .get("result")
            .and_then(|result| result.get("data"))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        _ => 0,
    };
    let raw_json = summarize_capability_payload(method, &value);

    ObservedEvent {
        channel_kind: ObserverChannelKind::CodexAppServer,
        event_kind,
        summary: format!("{method} returned {count} entries"),
        method: Some(method.to_string()),
        thread_id: None,
        turn_id: None,
        item_id: None,
        timestamp: value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string),
        raw_json,
    }
}

fn response_error_message(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    error
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| Some(error.to_string()))
}

fn normalize_server_value(value: Value) -> ObservedEvent {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string);
    let event_kind = method
        .as_deref()
        .map(observed_event_kind_for_method)
        .unwrap_or(ObservedEventKind::Unknown);
    let summary = method
        .clone()
        .unwrap_or_else(|| "unclassified codex observer message".into());
    let params = value.get("params");
    let raw_json = method
        .as_deref()
        .map(|method| summarize_notification_payload(method, &value))
        .unwrap_or_else(|| value.clone());

    ObservedEvent {
        channel_kind: ObserverChannelKind::CodexAppServer,
        event_kind,
        summary,
        method,
        thread_id: nested_string(params, &["threadId"])
            .or_else(|| nested_string(params, &["thread_id"])),
        turn_id: nested_string(params, &["turnId"]).or_else(|| nested_string(params, &["turn_id"])),
        item_id: nested_string(params, &["itemId"]).or_else(|| nested_string(params, &["item_id"])),
        timestamp: value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                params
                    .and_then(|params| params.get("timestamp"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        raw_json,
    }
}

fn summarize_capability_payload(method: &str, value: &Value) -> Value {
    match method {
        "skills/list" => {
            let entries = value
                .get("result")
                .and_then(|result| result.get("data"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let cwd_count = entries.len();
            let skill_names = entries
                .iter()
                .filter_map(|entry| entry.get("skills").and_then(Value::as_array))
                .flat_map(|skills| skills.iter())
                .filter_map(|skill| skill.get("name").and_then(Value::as_str))
                .take(12)
                .map(str::to_string)
                .collect::<Vec<_>>();

            json!({
                "method": method,
                "cwd_count": cwd_count,
                "skill_names_preview": skill_names,
            })
        }
        "plugin/list" => {
            let marketplaces = value
                .get("result")
                .and_then(|result| result.get("marketplaces"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let marketplace_count = marketplaces.len();
            let marketplace_names = marketplaces
                .iter()
                .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                .take(12)
                .map(str::to_string)
                .collect::<Vec<_>>();

            json!({
                "method": method,
                "marketplace_count": marketplace_count,
                "marketplace_names_preview": marketplace_names,
            })
        }
        "app/list" => {
            let apps = value
                .get("result")
                .and_then(|result| result.get("data"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let app_count = apps.len();
            let app_names = apps
                .iter()
                .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                .take(20)
                .map(str::to_string)
                .collect::<Vec<_>>();

            json!({
                "method": method,
                "app_count": app_count,
                "app_names_preview": app_names,
            })
        }
        _ => value.clone(),
    }
}

fn summarize_notification_payload(method: &str, value: &Value) -> Value {
    if method.starts_with("skills/") || method.starts_with("plugin/") || method.starts_with("app/")
    {
        json!({
            "method": method,
            "keys": top_level_keys(value.get("params")),
        })
    } else {
        value.clone()
    }
}

fn top_level_keys(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn nested_string(root: Option<&Value>, path: &[&str]) -> Option<String> {
    let mut cursor = root?;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    cursor.as_str().map(str::to_string)
}

fn observed_event_kind_for_method(method: &str) -> ObservedEventKind {
    if method.starts_with("thread/") {
        ObservedEventKind::Thread
    } else if method.starts_with("turn/") {
        ObservedEventKind::Turn
    } else if method.starts_with("item/") {
        ObservedEventKind::Item
    } else if method.starts_with("hook/") {
        ObservedEventKind::Hook
    } else if method.starts_with("plugin/") {
        ObservedEventKind::Plugin
    } else if method.starts_with("skills/") {
        ObservedEventKind::Skill
    } else if method.starts_with("app/") {
        ObservedEventKind::App
    } else if method.contains("approval") || method.contains("permission") {
        ObservedEventKind::Approval
    } else if method.contains("tool")
        || method.contains("command")
        || method.contains("server_request")
    {
        ObservedEventKind::Tool
    } else {
        ObservedEventKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CodexObserverFactory, CodexObserverOptions, CodexTransport, ObservedEventKind,
        capability_event_from_response, command_looks_like_desktop_codex_owner,
        discover_latest_codex_socket, normalize_server_value, observed_event_kind_for_method,
        select_latest_desktop_codex_socket,
    };
    use crate::observer::{
        ObservedEvent, ObserverChannelKind, ObserverHandshake, ObserverSourceFactory,
    };
    use prismtrace_storage::StorageLayout;
    use serde_json::{Value, json};
    use std::collections::{HashMap, VecDeque};
    use std::fs;
    use std::io;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::process;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::time::{SystemTime, UNIX_EPOCH};

    static UNIQUE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn unique_temp_dir() -> PathBuf {
        let mut path = PathBuf::from("/tmp");
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let seq = UNIQUE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        path.push(format!("ptco-{}-{nanos}-{seq}", process::id()));
        path
    }

    fn session_with_pending(values: Vec<Value>) -> io::Result<super::CodexObserverSession> {
        let mut child = std::process::Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("stdin missing"))?;
        let (_sender, receiver) = mpsc::channel();

        Ok(super::CodexObserverSession {
            transport: CodexTransport::StandaloneAppServer,
            initialize_timeout: std::time::Duration::from_secs(1),
            child,
            stdin,
            lines: receiver,
            reader_thread: None,
            pending: values.into_iter().collect(),
            next_request_id: 2,
            inflight_capability_requests: HashMap::new(),
        })
    }

    #[test]
    fn discover_latest_codex_socket_returns_most_recent_socket() -> io::Result<()> {
        let root = unique_temp_dir();
        let socket_root = root.join("codex-ipc");
        fs::create_dir_all(&socket_root)?;

        let older = socket_root.join("ipc-001.sock");
        let newer = socket_root.join("ipc-002.sock");
        let _older_listener = UnixListener::bind(&older)?;
        std::thread::sleep(std::time::Duration::from_millis(5));
        let _newer_listener = UnixListener::bind(&newer)?;

        let discovered = discover_latest_codex_socket(Some(&root))?;
        assert_eq!(discovered, Some(newer));

        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn select_latest_desktop_codex_socket_skips_non_matching_latest_candidate() {
        let selected = select_latest_desktop_codex_socket(
            vec![
                PathBuf::from("/tmp/codex-ipc/ipc-999.sock"),
                PathBuf::from("/tmp/codex-ipc/ipc-100.sock"),
            ],
            |path| path.ends_with("ipc-100.sock"),
        );

        assert_eq!(selected, Some(PathBuf::from("/tmp/codex-ipc/ipc-100.sock")));
    }

    #[test]
    fn command_looks_like_desktop_codex_owner_rejects_vscode_extension_host() {
        assert!(!command_looks_like_desktop_codex_owner(
            "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin) --type=utility"
        ));
    }

    #[test]
    fn command_looks_like_desktop_codex_owner_accepts_desktop_codex_processes() {
        assert!(command_looks_like_desktop_codex_owner(
            "/Applications/Codex.app/Contents/Resources/codex app-server --analytics-default-enabled"
        ));
        assert!(command_looks_like_desktop_codex_owner(
            "/Applications/Codex.app/Contents/MacOS/Codex"
        ));
    }

    #[test]
    fn factory_prioritizes_explicit_socket_path() -> io::Result<()> {
        let factory = CodexObserverFactory;
        let options = CodexObserverOptions {
            socket_path: Some(PathBuf::from("/tmp/codex-ipc/ipc-501.sock")),
            ..CodexObserverOptions::default()
        };

        let sources = factory.build_sources(&options)?;

        assert_eq!(sources.len(), 1);
        assert!(sources[0].transport_label().contains("ipc-501.sock"));
        Ok(())
    }

    #[test]
    fn normalize_server_value_maps_known_method_prefixes() {
        let event = normalize_server_value(json!({
            "method": "thread/started",
            "params": {
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
            }
        }));

        assert_eq!(event.event_kind, ObservedEventKind::Thread);
        assert_eq!(event.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(event.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(event.item_id.as_deref(), Some("item-1"));
    }

    #[test]
    fn observed_event_kind_for_method_handles_approval_and_tool_markers() {
        assert_eq!(
            observed_event_kind_for_method("permission/requested"),
            ObservedEventKind::Approval
        );
        assert_eq!(
            observed_event_kind_for_method("server_request/resolved"),
            ObservedEventKind::Tool
        );
    }

    #[test]
    fn capability_event_from_response_counts_entries() {
        let event = capability_event_from_response(
            "skills/list",
            ObservedEventKind::Skill,
            json!({
                "id": 2,
                "result": {
                    "data": [
                        { "cwd": "/tmp/a", "skills": [{ "name": "a" }, { "name": "b" }] },
                        { "cwd": "/tmp/b", "skills": [{ "name": "c" }] }
                    ]
                }
            }),
        );

        assert_eq!(event.event_kind, ObservedEventKind::Skill);
        assert!(event.summary.contains("3 entries"));
    }

    #[test]
    fn validate_proxy_socket_endpoint_reports_immediate_eof() -> io::Result<()> {
        let root = unique_temp_dir();
        fs::create_dir_all(&root)?;
        let socket_path = root.join("probe.sock");
        let listener = UnixListener::bind(&socket_path)?;

        let worker = std::thread::spawn(move || -> io::Result<()> {
            let (stream, _) = listener.accept()?;
            drop(stream);
            Ok(())
        });

        let error = super::validate_proxy_socket_endpoint(
            &socket_path,
            std::time::Duration::from_millis(200),
        )
        .expect_err("socket should be rejected");

        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert!(
            error
                .to_string()
                .contains("does not look like a live Codex app-server protocol endpoint")
        );

        worker.join().expect("worker should join")?;
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn validate_proxy_socket_endpoint_accepts_initialize_result() -> io::Result<()> {
        let root = unique_temp_dir();
        fs::create_dir_all(&root)?;
        let socket_path = root.join("probe.sock");
        let listener = UnixListener::bind(&socket_path)?;

        let worker = std::thread::spawn(move || -> io::Result<()> {
            let (mut stream, _) = listener.accept()?;
            let mut request = String::new();
            let mut reader = BufReader::new(stream.try_clone()?);
            reader.read_line(&mut request)?;
            stream.write_all(
                br#"{"id":1,"result":{"userAgent":"Codex Test","codexHome":"/tmp/.codex","platformFamily":"unix","platformOs":"macos"}}"#,
            )?;
            stream.write_all(b"\n")?;
            stream.flush()?;
            Ok(())
        });

        super::validate_proxy_socket_endpoint(&socket_path, std::time::Duration::from_secs(1))?;

        worker.join().expect("worker should join")?;
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn normalize_value_reclassifies_late_capability_response() -> io::Result<()> {
        let (left, mut right) = UnixStream::pair()?;
        let mut child = std::process::Command::new("/usr/bin/true")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("stdin missing"))?;
        let (_sender, receiver) = mpsc::channel();

        let mut session = super::CodexObserverSession {
            transport: super::CodexTransport::StandaloneAppServer,
            initialize_timeout: std::time::Duration::from_secs(1),
            child,
            stdin,
            lines: receiver,
            reader_thread: None,
            pending: VecDeque::new(),
            next_request_id: 2,
            inflight_capability_requests: HashMap::from([(
                4,
                ("app/list".to_string(), ObservedEventKind::App),
            )]),
        };

        drop(left);
        right.write_all(b"")?;

        let event = session.normalize_value(json!({
            "id": 4,
            "result": {
                "data": [
                    { "name": "A" },
                    { "name": "B" }
                ]
            }
        }));

        assert_eq!(event.event_kind, ObservedEventKind::App);
        assert!(event.summary.contains("2 entries"));
        Ok(())
    }

    #[test]
    fn initialize_surfaces_remote_error_message() -> io::Result<()> {
        let mut session = session_with_pending(vec![json!({
            "id": 1,
            "error": {
                "message": "permission denied by codex app server"
            }
        })])?;

        let error = crate::observer::ObserverSession::initialize(&mut session)
            .expect_err("initialize should fail");

        assert!(
            error
                .to_string()
                .contains("permission denied by codex app server")
        );
        Ok(())
    }

    #[test]
    fn collect_capability_events_surfaces_remote_error_reply() -> io::Result<()> {
        let mut session = session_with_pending(vec![
            json!({
                "id": 2,
                "error": {
                    "message": "skills/list disabled"
                }
            }),
            json!({
                "id": 3,
                "result": {
                    "marketplaces": []
                }
            }),
            json!({
                "id": 4,
                "result": {
                    "data": []
                }
            }),
        ])?;

        let events = crate::observer::ObserverSession::collect_capability_events(&mut session)?;

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_kind, ObservedEventKind::Unknown);
        assert_eq!(events[0].method.as_deref(), Some("skills/list"));
        assert!(events[0].summary.contains("skills/list disabled"));
        Ok(())
    }

    #[test]
    fn normalize_server_value_preserves_raw_json_for_unknown_messages() {
        let value = json!({
            "method": "custom/runtime_event",
            "params": {
                "foo": "bar"
            }
        });

        let event = normalize_server_value(value.clone());

        assert_eq!(event.event_kind, ObservedEventKind::Unknown);
        assert_eq!(event.summary, "custom/runtime_event");
        assert_eq!(event.raw_json, value);
    }

    #[test]
    fn codex_observer_artifact_writer_persists_handshake_and_event() -> io::Result<()> {
        let root = unique_temp_dir();
        let storage = StorageLayout::new(&root);
        storage.initialize()?;

        let writer = super::CodexObserverArtifactWriter::create(
            &storage,
            &ObserverHandshake {
                channel_kind: ObserverChannelKind::CodexAppServer,
                transport_label: "proxy-socket (/tmp/codex.sock)".into(),
                server_label: "Codex Test".into(),
                raw_json: json!({
                    "result": {
                        "userAgent": "Codex Test"
                    }
                }),
            },
        )?;
        let event = ObservedEvent {
            channel_kind: ObserverChannelKind::CodexAppServer,
            event_kind: ObservedEventKind::Thread,
            summary: "thread/started".into(),
            method: Some("thread/started".into()),
            thread_id: Some("thread-1".into()),
            turn_id: Some("turn-1".into()),
            item_id: None,
            timestamp: Some("1714000004000".into()),
            raw_json: json!({
                "method": "thread/started",
                "params": {
                    "threadId": "thread-1",
                    "turnId": "turn-1"
                }
            }),
        };

        writer.append_event(&event)?;

        let artifact = fs::read_to_string(writer.artifact_path())?;
        assert!(artifact.contains(r#""record_type":"handshake""#));
        assert!(artifact.contains(r#""record_type":"event""#));
        assert!(artifact.contains(r#""event_kind":"thread""#));
        assert!(artifact.contains(r#""thread_id":"thread-1""#));

        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
