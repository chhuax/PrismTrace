use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, ErrorKind as IoErrorKind, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::net::UnixStream;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::client::{IntoClientRequest, client as ws_client};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Error as WsError, Message, WebSocket};

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

/// Production instrumentation runtime for Node / Electron processes.
const INSPECTOR_WAKEUP_SIGNAL: i32 = libc::SIGUSR1;
const INSPECTOR_IPC_PREFIX: &str = "__prismtraceIpc__";
const INSPECTOR_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const INSPECTOR_DISCOVERY_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CDP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_POLL_TIMEOUT: Duration = Duration::from_millis(100);
const WORKER_DETACH_GRACE_TIMEOUT: Duration = Duration::from_secs(2);
const INSPECTOR_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const INSPECTOR_HTTP_READ_TIMEOUT: Duration = Duration::from_millis(500);
const INSPECTOR_HTTP_WRITE_TIMEOUT: Duration = Duration::from_millis(500);
const INSPECTOR_WS_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const INSPECTOR_WS_WRITE_TIMEOUT: Duration = Duration::from_secs(2);
const INSPECTOR_BRIDGE_READ_TIMEOUT: Duration = Duration::from_millis(250);
const TRIGGER_DETACH_EXPRESSION: &str = r#"
(() => {
  if (typeof globalThis.__prismtraceDetach === "function") {
    globalThis.__prismtraceDetach();
    return true;
  }
  return false;
})()
"#;

pub struct NodeInstrumentationRuntime;

#[derive(Clone)]
struct InspectorControlHandle {
    worker: Arc<WorkerControl>,
}

fn active_controls() -> &'static Mutex<HashMap<u32, InspectorControlHandle>> {
    static CONTROLS: OnceLock<Mutex<HashMap<u32, InspectorControlHandle>>> = OnceLock::new();
    CONTROLS.get_or_init(|| Mutex::new(HashMap::new()))
}

enum WorkerCommand {
    RequestDetach {
        response_tx: mpsc::Sender<Result<(), InstrumentationError>>,
    },
    Stop,
}

struct WorkerControl {
    command_tx: mpsc::Sender<WorkerCommand>,
    join_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl WorkerControl {
    fn request_detach(&self) -> Result<(), InstrumentationError> {
        let (response_tx, response_rx) = mpsc::channel();
        self.command_tx
            .send(WorkerCommand::RequestDetach { response_tx })
            .map_err(|e| InstrumentationError {
                kind: InstrumentationErrorKind::DetachFailed,
                message: format!("failed to send detach command to inspector worker: {e}"),
            })?;

        response_rx
            .recv_timeout(CDP_REQUEST_TIMEOUT)
            .map_err(|e| InstrumentationError {
                kind: InstrumentationErrorKind::DetachFailed,
                message: format!("timed out waiting for detach command response: {e}"),
            })?
    }

    fn request_stop_and_join(&self) {
        let _ = self.command_tx.send(WorkerCommand::Stop);
        if let Ok(mut guard) = self.join_handle.lock()
            && let Some(handle) = guard.take()
        {
            let _ = handle.join();
        }
    }
}

struct InspectorBridge;

#[derive(Clone)]
struct InspectorBridgeWriter {
    sink: Arc<Mutex<UnixStream>>,
}

impl InspectorBridge {
    fn new() -> Result<(InspectorBridgeWriter, Box<dyn BufRead + Send>), InstrumentationError> {
        let (reader_side, writer_side) = UnixStream::pair().map_err(|e| InstrumentationError {
            kind: InstrumentationErrorKind::InjectionFailed,
            message: format!("failed to create inspector bridge: {e}"),
        })?;
        reader_side
            .set_read_timeout(Some(INSPECTOR_BRIDGE_READ_TIMEOUT))
            .map_err(|e| InstrumentationError {
                kind: InstrumentationErrorKind::InjectionFailed,
                message: format!("failed to set inspector bridge read timeout: {e}"),
            })?;

        let reader = Box::new(BufReader::new(reader_side));
        let writer = InspectorBridgeWriter {
            sink: Arc::new(Mutex::new(writer_side)),
        };
        Ok((writer, reader))
    }
}

impl InspectorBridgeWriter {
    fn write_line(&self, line: &str) -> std::io::Result<()> {
        let mut sink = self
            .sink
            .lock()
            .map_err(|_| std::io::Error::other("inspector bridge lock poisoned"))?;
        sink.write_all(line.as_bytes())?;
        if !line.ends_with('\n') {
            sink.write_all(b"\n")?;
        }
        sink.flush()
    }
}

struct InspectorSession {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    next_request_id: u64,
    bridge: Option<InspectorBridgeWriter>,
}

impl InspectorSession {
    fn new(
        socket: WebSocket<MaybeTlsStream<TcpStream>>,
        bridge: Option<InspectorBridgeWriter>,
    ) -> Self {
        Self {
            socket,
            next_request_id: 1,
            bridge,
        }
    }

    fn call_method(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        kind: InstrumentationErrorKind,
    ) -> Result<Value, InstrumentationError> {
        let request_id = self.next_request_id;
        self.next_request_id += 1;

        let payload = json!({
            "id": request_id,
            "method": method,
            "params": params,
        });
        self.socket
            .send(Message::Text(payload.to_string().into()))
            .map_err(|e| InstrumentationError {
                kind,
                message: format!("failed to send CDP request {method}: {e}"),
            })?;

        self.wait_for_response(request_id, timeout, kind)
    }

    fn wait_for_response(
        &mut self,
        request_id: u64,
        timeout: Duration,
        kind: InstrumentationErrorKind,
    ) -> Result<Value, InstrumentationError> {
        let deadline = Instant::now() + timeout;

        loop {
            if Instant::now() >= deadline {
                return Err(InstrumentationError {
                    kind,
                    message: format!("timed out waiting for inspector response (id={request_id})"),
                });
            }

            match self.read_json_message(kind)? {
                Some(message) => {
                    if message.get("id").and_then(Value::as_u64) == Some(request_id) {
                        return Ok(message);
                    }
                    let _ = self.forward_console_ipc(&message, kind)?;
                }
                None => continue,
            }
        }
    }

    fn read_json_message(
        &mut self,
        kind: InstrumentationErrorKind,
    ) -> Result<Option<Value>, InstrumentationError> {
        match self.socket.read() {
            Ok(Message::Text(text)) => serde_json::from_str::<Value>(text.as_ref())
                .map(Some)
                .map_err(|e| InstrumentationError {
                    kind,
                    message: format!("failed to parse inspector text message as JSON: {e}"),
                }),
            Ok(Message::Binary(bytes)) => serde_json::from_slice::<Value>(&bytes)
                .map(Some)
                .map_err(|e| InstrumentationError {
                    kind,
                    message: format!("failed to parse inspector binary message as JSON: {e}"),
                }),
            Ok(Message::Ping(payload)) => {
                self.socket
                    .send(Message::Pong(payload))
                    .map_err(|e| InstrumentationError {
                        kind,
                        message: format!("failed to respond to websocket ping: {e}"),
                    })?;
                Ok(None)
            }
            Ok(Message::Pong(_)) => Ok(None),
            Ok(Message::Close(_)) => Err(InstrumentationError {
                kind,
                message: "inspector websocket closed".into(),
            }),
            Ok(_) => Ok(None),
            Err(WsError::Io(err)) if is_poll_timeout(&err) => Ok(None),
            Err(WsError::ConnectionClosed | WsError::AlreadyClosed) => Err(InstrumentationError {
                kind,
                message: "inspector websocket connection closed".into(),
            }),
            Err(err) => Err(InstrumentationError {
                kind,
                message: format!("failed to read inspector websocket message: {err}"),
            }),
        }
    }

    fn forward_console_ipc(
        &self,
        message: &Value,
        kind: InstrumentationErrorKind,
    ) -> Result<Option<String>, InstrumentationError> {
        let Some(bridge) = self.bridge.as_ref() else {
            return Ok(None);
        };
        let Some(line) = extract_bridge_line(message) else {
            return Ok(None);
        };

        bridge.write_line(&line).map_err(|e| InstrumentationError {
            kind,
            message: format!("failed to write bridge line: {e}"),
        })?;
        Ok(Some(line))
    }
}

fn is_poll_timeout(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        IoErrorKind::TimedOut | IoErrorKind::WouldBlock | IoErrorKind::Interrupted
    )
}

fn extract_bridge_line(message: &Value) -> Option<String> {
    if message.get("method")?.as_str()? != "Runtime.consoleAPICalled" {
        return None;
    }

    let args = message.get("params")?.get("args")?.as_array()?;
    for arg in args {
        if let Some(value) = arg.get("value").and_then(Value::as_str)
            && let Some(raw_line) = value.strip_prefix(INSPECTOR_IPC_PREFIX)
        {
            return Some(raw_line.to_string());
        }
    }

    None
}

fn install_emit_bridge_expression() -> String {
    format!(
        r#"
(() => {{
  globalThis.__prismtraceEmit = (line) => {{
    try {{
      console.log("{INSPECTOR_IPC_PREFIX}" + String(line));
    }} catch (_) {{}}
  }};
  return true;
}})()
"#
    )
}

fn is_detach_ack_line(line: &str) -> bool {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .as_deref()
        == Some("detach_ack")
}

fn ensure_cdp_success(
    response: &Value,
    kind: InstrumentationErrorKind,
    context: &str,
) -> Result<(), InstrumentationError> {
    if let Some(error) = response.get("error") {
        return Err(InstrumentationError {
            kind,
            message: format!("{context} failed: {error}"),
        });
    }

    if let Some(exception) = response
        .get("result")
        .and_then(|result| result.get("exceptionDetails"))
    {
        return Err(InstrumentationError {
            kind,
            message: format!("{context} raised exception: {exception}"),
        });
    }

    Ok(())
}

fn evaluate_expression(
    session: &mut InspectorSession,
    expression: &str,
    timeout: Duration,
    kind: InstrumentationErrorKind,
    context: &str,
) -> Result<Value, InstrumentationError> {
    let response = session.call_method(
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
        }),
        timeout,
        kind,
    )?;
    ensure_cdp_success(&response, kind, context)?;
    Ok(response)
}

fn activate_node_inspector(pid: u32) -> Result<(), InstrumentationError> {
    let result = unsafe { libc::kill(pid as i32, INSPECTOR_WAKEUP_SIGNAL) };
    if result == 0 {
        return Ok(());
    }

    let os_error = std::io::Error::last_os_error();
    let kind = match os_error.raw_os_error() {
        Some(libc::ESRCH) => InstrumentationErrorKind::ProcessNotFound,
        Some(libc::EPERM) => InstrumentationErrorKind::PermissionDenied,
        _ => InstrumentationErrorKind::InjectionFailed,
    };
    Err(InstrumentationError {
        kind,
        message: format!("failed to send SIGUSR1 to pid {pid}: {os_error}"),
    })
}

fn parse_listener_ports(output: &str) -> Vec<u16> {
    let mut ports = Vec::new();

    for line in output.lines() {
        if !line.contains("(LISTEN)") {
            continue;
        }
        let Some(marker_pos) = line.find("(LISTEN)") else {
            continue;
        };
        let prefix = line[..marker_pos].trim_end();
        let Some(colon_pos) = prefix.rfind(':') else {
            continue;
        };

        let mut digits = String::new();
        for ch in prefix[colon_pos + 1..].chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
            } else {
                break;
            }
        }

        if let Ok(port) = digits.parse::<u16>()
            && !ports.contains(&port)
        {
            ports.push(port);
        }
    }

    ports
}

fn query_listener_ports(pid: u32) -> Result<Vec<u16>, InstrumentationError> {
    let output = Command::new("lsof")
        .args(["-nP", "-a", "-p", &pid.to_string(), "-iTCP", "-sTCP:LISTEN"])
        .output()
        .map_err(|e| {
            let kind = if e.kind() == IoErrorKind::NotFound {
                InstrumentationErrorKind::RuntimeIncompatible
            } else {
                InstrumentationErrorKind::InjectionFailed
            };
            InstrumentationError {
                kind,
                message: format!("failed to execute lsof for pid {pid}: {e}"),
            }
        })?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(parse_listener_ports(&stdout));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim().to_string();
    let stderr_lower = stderr.to_lowercase();
    if stderr_lower.contains("no such process") {
        return Err(InstrumentationError {
            kind: InstrumentationErrorKind::ProcessNotFound,
            message: format!("target process {pid} disappeared while probing inspector"),
        });
    }
    if stderr_lower.contains("permission denied") {
        return Err(InstrumentationError {
            kind: InstrumentationErrorKind::PermissionDenied,
            message: format!("permission denied while probing inspector for pid {pid}"),
        });
    }

    if stderr.is_empty() {
        return Ok(vec![]);
    }

    Ok(vec![])
}

fn parse_websocket_debugger_url(payload: &str) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(payload).ok()?;
    if let Some(entries) = parsed.as_array() {
        for entry in entries {
            if let Some(url) = entry
                .get("webSocketDebuggerUrl")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                return Some(url);
            }
        }
    }

    parsed
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn fetch_websocket_debugger_url(port: u16) -> Result<Option<String>, InstrumentationError> {
    let endpoint = format!("http://127.0.0.1:{port}/json/list");
    let agent = ureq::builder()
        .timeout_connect(INSPECTOR_HTTP_CONNECT_TIMEOUT)
        .timeout_read(INSPECTOR_HTTP_READ_TIMEOUT)
        .timeout_write(INSPECTOR_HTTP_WRITE_TIMEOUT)
        .build();

    match agent.get(&endpoint).call() {
        Ok(response) => {
            let payload = response.into_string().map_err(|e| InstrumentationError {
                kind: InstrumentationErrorKind::InjectionFailed,
                message: format!("failed to read inspector /json/list response body: {e}"),
            })?;
            Ok(parse_websocket_debugger_url(&payload))
        }
        Err(ureq::Error::Status(_, _)) => Ok(None),
        Err(err) => Err(InstrumentationError {
            kind: InstrumentationErrorKind::InjectionFailed,
            message: format!("request to inspector /json/list failed for port {port}: {err}"),
        }),
    }
}

fn pick_debugger_url_from_candidates<F>(
    ports: &[u16],
    mut fetch: F,
) -> Result<Option<String>, InstrumentationError>
where
    F: FnMut(u16) -> Result<Option<String>, InstrumentationError>,
{
    let mut last_error = None;
    for port in ports {
        match fetch(*port) {
            Ok(Some(url)) => return Ok(Some(url)),
            Ok(None) => continue,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        }
    }

    if let Some(error) = last_error {
        return Err(error);
    }

    Ok(None)
}

fn discover_websocket_debugger_url(pid: u32) -> Result<String, InstrumentationError> {
    let deadline = Instant::now() + INSPECTOR_DISCOVERY_TIMEOUT;
    let mut last_error = String::new();

    loop {
        let ports = query_listener_ports(pid)?;

        match pick_debugger_url_from_candidates(&ports, fetch_websocket_debugger_url) {
            Ok(Some(url)) => return Ok(url),
            Ok(None) => {
                if !ports.is_empty() {
                    last_error = format!("no inspector endpoint on listening ports {:?}", ports);
                }
            }
            Err(error) => {
                last_error = error.message;
            }
        }

        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(INSPECTOR_DISCOVERY_POLL_INTERVAL);
    }

    Err(InstrumentationError {
        kind: InstrumentationErrorKind::InjectionFailed,
        message: if last_error.is_empty() {
            format!("failed to discover inspector websocket url for pid {pid}")
        } else {
            format!("failed to discover inspector websocket url for pid {pid}: {last_error}")
        },
    })
}

fn connect_websocket(
    ws_url: &str,
    kind: InstrumentationErrorKind,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, InstrumentationError> {
    let request = ws_url
        .into_client_request()
        .map_err(|e| InstrumentationError {
            kind,
            message: format!("failed to build websocket request for {ws_url}: {e}"),
        })?;
    let uri = request.uri();
    let scheme = uri.scheme_str().ok_or_else(|| InstrumentationError {
        kind,
        message: format!("inspector websocket URL has no scheme: {ws_url}"),
    })?;
    if scheme != "ws" {
        return Err(InstrumentationError {
            kind: InstrumentationErrorKind::RuntimeIncompatible,
            message: format!("unsupported inspector websocket scheme '{scheme}' in {ws_url}"),
        });
    }

    let host = uri.host().ok_or_else(|| InstrumentationError {
        kind,
        message: format!("inspector websocket URL has no host: {ws_url}"),
    })?;
    let port = uri.port_u16().unwrap_or(80);
    let mut last_connect_error = None;
    let mut connected_stream = None;

    for addr in (host, port)
        .to_socket_addrs()
        .map_err(|e| InstrumentationError {
            kind,
            message: format!("failed to resolve websocket host {host}:{port}: {e}"),
        })?
    {
        match TcpStream::connect_timeout(&addr, INSPECTOR_WS_CONNECT_TIMEOUT) {
            Ok(stream) => {
                connected_stream = Some(stream);
                break;
            }
            Err(e) => {
                last_connect_error = Some(format!("{addr}: {e}"));
            }
        }
    }

    let stream = connected_stream.ok_or_else(|| InstrumentationError {
        kind,
        message: if let Some(error) = last_connect_error {
            format!("failed to connect to inspector websocket {ws_url}: {error}")
        } else {
            format!("failed to connect to inspector websocket {ws_url}: no address resolved")
        },
    })?;

    stream
        .set_read_timeout(Some(WORKER_POLL_TIMEOUT))
        .map_err(|e| InstrumentationError {
            kind,
            message: format!("failed to set websocket read timeout: {e}"),
        })?;
    stream
        .set_write_timeout(Some(INSPECTOR_WS_WRITE_TIMEOUT))
        .map_err(|e| InstrumentationError {
            kind,
            message: format!("failed to set websocket write timeout: {e}"),
        })?;

    let (socket, _) =
        ws_client(request, MaybeTlsStream::Plain(stream)).map_err(|e| InstrumentationError {
            kind,
            message: format!("failed websocket handshake for {ws_url}: {e}"),
        })?;

    Ok(socket)
}

fn extract_eval_number(response: &Value, context: &str) -> Result<u64, InstrumentationError> {
    response
        .get("result")
        .and_then(|result| result.get("result"))
        .and_then(|result| result.get("value"))
        .and_then(Value::as_u64)
        .ok_or_else(|| InstrumentationError {
            kind: InstrumentationErrorKind::InjectionFailed,
            message: format!("invalid numeric evaluation result for {context}"),
        })
}

fn extract_eval_bool(response: &Value, context: &str) -> Result<bool, InstrumentationError> {
    response
        .get("result")
        .and_then(|result| result.get("result"))
        .and_then(|result| result.get("value"))
        .and_then(Value::as_bool)
        .ok_or_else(|| InstrumentationError {
            kind: InstrumentationErrorKind::DetachFailed,
            message: format!("invalid boolean evaluation result for {context}"),
        })
}

fn run_worker_loop(mut session: InspectorSession, command_rx: mpsc::Receiver<WorkerCommand>) {
    let mut detach_deadline = None;

    loop {
        loop {
            match command_rx.try_recv() {
                Ok(WorkerCommand::Stop) => return,
                Ok(WorkerCommand::RequestDetach { response_tx }) => {
                    let result = evaluate_expression(
                        &mut session,
                        TRIGGER_DETACH_EXPRESSION,
                        CDP_REQUEST_TIMEOUT,
                        InstrumentationErrorKind::DetachFailed,
                        "detach trigger",
                    )
                    .and_then(|response| {
                        if extract_eval_bool(&response, "detach trigger")? {
                            Ok(())
                        } else {
                            Err(InstrumentationError {
                                kind: InstrumentationErrorKind::DetachFailed,
                                message:
                                    "detach hook not installed (globalThis.__prismtraceDetach missing)"
                                        .into(),
                            })
                        }
                    });

                    if result.is_ok() {
                        detach_deadline = Some(Instant::now() + WORKER_DETACH_GRACE_TIMEOUT);
                    }
                    let _ = response_tx.send(result);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        if let Some(deadline) = detach_deadline
            && Instant::now() >= deadline
        {
            break;
        }

        let message = match session.read_json_message(InstrumentationErrorKind::InjectionFailed) {
            Ok(Some(msg)) => msg,
            Ok(None) => continue,
            Err(_) => break,
        };

        match session.forward_console_ipc(&message, InstrumentationErrorKind::InjectionFailed) {
            Ok(Some(line)) if is_detach_ack_line(&line) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

impl InstrumentationRuntime for NodeInstrumentationRuntime {
    fn inject_probe(
        &self,
        pid: u32,
        probe_script: &str,
    ) -> Result<Box<dyn BufRead + Send>, InstrumentationError> {
        activate_node_inspector(pid)?;

        let websocket_debugger_url = discover_websocket_debugger_url(pid)?;

        let (bridge_writer, reader) = InspectorBridge::new()?;
        let socket = connect_websocket(
            &websocket_debugger_url,
            InstrumentationErrorKind::InjectionFailed,
        )?;
        let mut session = InspectorSession::new(socket, Some(bridge_writer));

        let enable_response = session.call_method(
            "Runtime.enable",
            json!({}),
            CDP_REQUEST_TIMEOUT,
            InstrumentationErrorKind::InjectionFailed,
        )?;
        ensure_cdp_success(
            &enable_response,
            InstrumentationErrorKind::InjectionFailed,
            "Runtime.enable",
        )?;

        let pid_response = evaluate_expression(
            &mut session,
            "process.pid",
            CDP_REQUEST_TIMEOUT,
            InstrumentationErrorKind::InjectionFailed,
            "process.pid verification",
        )?;
        let actual_pid = extract_eval_number(&pid_response, "process.pid verification")? as u32;
        if actual_pid != pid {
            return Err(InstrumentationError {
                kind: InstrumentationErrorKind::RuntimeIncompatible,
                message: format!(
                    "inspector attached to unexpected process: expected pid {pid}, got {actual_pid}"
                ),
            });
        }

        let bridge_expression = install_emit_bridge_expression();
        evaluate_expression(
            &mut session,
            &bridge_expression,
            CDP_REQUEST_TIMEOUT,
            InstrumentationErrorKind::InjectionFailed,
            "bridge bootstrap",
        )?;
        evaluate_expression(
            &mut session,
            probe_script,
            CDP_REQUEST_TIMEOUT,
            InstrumentationErrorKind::InjectionFailed,
            "probe bootstrap",
        )?;

        let (command_tx, command_rx) = mpsc::channel();
        let worker = Arc::new(WorkerControl {
            command_tx,
            join_handle: Mutex::new(Some(thread::spawn(move || {
                run_worker_loop(session, command_rx)
            }))),
        });

        let replaced = {
            let mut controls = active_controls().lock().map_err(|_| InstrumentationError {
                kind: InstrumentationErrorKind::InjectionFailed,
                message: "inspector control map lock poisoned".into(),
            })?;
            controls.insert(
                pid,
                InspectorControlHandle {
                    worker: Arc::clone(&worker),
                },
            )
        };
        if let Some(previous) = replaced {
            previous.worker.request_stop_and_join();
        }

        Ok(reader)
    }

    fn send_detach_signal(&self, pid: u32) -> Result<(), InstrumentationError> {
        let worker = {
            let controls = active_controls().lock().map_err(|_| InstrumentationError {
                kind: InstrumentationErrorKind::DetachFailed,
                message: "inspector control map lock poisoned".into(),
            })?;
            controls
                .get(&pid)
                .map(|control| Arc::clone(&control.worker))
        }
        .ok_or_else(|| InstrumentationError {
            kind: InstrumentationErrorKind::DetachFailed,
            message: format!("no active inspector control found for pid {pid}"),
        })?;

        worker.request_detach()?;

        {
            let mut controls = active_controls().lock().map_err(|_| InstrumentationError {
                kind: InstrumentationErrorKind::DetachFailed,
                message: "inspector control map lock poisoned".into(),
            })?;
            controls.remove(&pid);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InstrumentationError, InstrumentationErrorKind, InstrumentationRuntime,
        NodeInstrumentationRuntime, ScriptedInstrumentationRuntime, WorkerCommand, WorkerControl,
        active_controls, parse_listener_ports, parse_websocket_debugger_url,
        pick_debugger_url_from_candidates,
    };
    use std::io::BufRead;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, OnceLock, mpsc};
    use std::thread;

    fn control_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn cleanup_active_controls_for_tests() {
        let mut controls = active_controls().lock().unwrap();
        for (_, control) in controls.drain() {
            control.worker.request_stop_and_join();
        }
    }

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

    #[test]
    fn parse_listener_ports_returns_all_listening_ports_from_lsof_output() {
        let output = "\
COMMAND   PID  USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
node    42424 huaxin   10u  IPv4 0x11111111      0t0  TCP 127.0.0.1:3000 (LISTEN)
node    42424 huaxin   23u  IPv4 0x75a73a76      0t0  TCP 127.0.0.1:9229 (LISTEN)
";

        assert_eq!(parse_listener_ports(output), vec![3000, 9229]);
    }

    #[test]
    fn parse_websocket_debugger_url_extracts_ws_url_from_json_list() {
        let payload = r#"[
  {
    "description": "node.js instance",
    "id": "abc123",
    "title": "node",
    "type": "node",
    "webSocketDebuggerUrl": "ws://127.0.0.1:9229/abc123"
  }
]"#;

        assert_eq!(
            parse_websocket_debugger_url(payload),
            Some("ws://127.0.0.1:9229/abc123".to_string())
        );
    }

    #[test]
    fn inspector_bridge_new_returns_reader_with_complete_lines() {
        let (writer, mut reader) = super::InspectorBridge::new().expect("bridge should initialize");
        writer
            .write_line(r#"{"type":"heartbeat","timestamp_ms":1}"#)
            .expect("line write should succeed");
        writer
            .write_line(r#"{"type":"detach_ack","timestamp_ms":2}"#)
            .expect("line write should succeed");

        let mut first = String::new();
        let mut second = String::new();
        reader
            .read_line(&mut first)
            .expect("first line should read");
        reader
            .read_line(&mut second)
            .expect("second line should read");

        assert_eq!(first, "{\"type\":\"heartbeat\",\"timestamp_ms\":1}\n");
        assert_eq!(second, "{\"type\":\"detach_ack\",\"timestamp_ms\":2}\n");
    }

    #[test]
    fn pick_debugger_url_from_candidates_uses_port_with_valid_json_list() {
        let ports = vec![3000_u16, 9229_u16];
        let selected = pick_debugger_url_from_candidates(&ports, |port| match port {
            3000 => Ok(None),
            9229 => Ok(Some("ws://127.0.0.1:9229/uuid".to_string())),
            _ => Ok(None),
        })
        .expect("selection should succeed");

        assert_eq!(selected, Some("ws://127.0.0.1:9229/uuid".to_string()));
    }

    #[test]
    fn send_detach_signal_keeps_control_handle_when_detach_fails() {
        let _guard = control_test_guard();
        cleanup_active_controls_for_tests();

        let pid = 42424_u32;
        let (command_tx, command_rx) = mpsc::channel::<WorkerCommand>();
        let worker = Arc::new(WorkerControl {
            command_tx,
            join_handle: Mutex::new(Some(thread::spawn(move || {
                while let Ok(command) = command_rx.recv() {
                    match command {
                        WorkerCommand::RequestDetach { response_tx } => {
                            let _ = response_tx.send(Err(InstrumentationError {
                                kind: InstrumentationErrorKind::DetachFailed,
                                message: "synthetic detach failure".into(),
                            }));
                        }
                        WorkerCommand::Stop => break,
                    }
                }
            }))),
        });

        active_controls().lock().unwrap().insert(
            pid,
            super::InspectorControlHandle {
                worker: Arc::clone(&worker),
            },
        );

        let runtime = NodeInstrumentationRuntime;
        let err = runtime
            .send_detach_signal(pid)
            .expect_err("detach should fail from synthetic worker");
        assert_eq!(err.kind, InstrumentationErrorKind::DetachFailed);

        assert!(
            active_controls().lock().unwrap().contains_key(&pid),
            "control handle should remain for retry after detach failure"
        );

        cleanup_active_controls_for_tests();
    }

    #[test]
    fn worker_control_stop_request_terminates_worker_thread() {
        let terminated = Arc::new(AtomicBool::new(false));
        let terminated_in_worker = Arc::clone(&terminated);
        let (command_tx, command_rx) = mpsc::channel::<WorkerCommand>();
        let worker = WorkerControl {
            command_tx,
            join_handle: Mutex::new(Some(thread::spawn(move || {
                while let Ok(command) = command_rx.recv() {
                    if matches!(command, WorkerCommand::Stop) {
                        break;
                    }
                }
                terminated_in_worker.store(true, Ordering::SeqCst);
            }))),
        };

        worker.request_stop_and_join();

        assert!(
            terminated.load(Ordering::SeqCst),
            "worker thread should terminate after explicit stop request"
        );
    }
}
