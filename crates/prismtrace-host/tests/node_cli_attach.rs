use prismtrace_core::ProcessSample;
use prismtrace_host::discovery::StaticProcessSampleSource;
use prismtrace_host::runtime::NodeInstrumentationRuntime;
use prismtrace_host::{bootstrap, run_foreground_attach_session};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn attach_to_running_node_cli_captures_request_and_writes_artifact() -> io::Result<()> {
    let workspace = TempWorkspace::new("node-cli-attach")?;
    let script_path = write_node_target_script(workspace.path())?;
    let node_bin = resolve_node_binary()?;
    let fake_server = FakeLlmServer::start()?;

    let child = Command::new(&node_bin)
        .arg(&script_path)
        .env("PRISMTRACE_TEST_ENDPOINT", fake_server.url())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut child = ChildGuard::new(child);
    thread::sleep(Duration::from_millis(150));

    let target_pid = child.pid();
    let bootstrap_result = bootstrap(workspace.path())?;
    let source = StaticProcessSampleSource::new(vec![ProcessSample {
        pid: target_pid,
        process_name: "node".into(),
        executable_path: node_bin,
    }]);

    let mut output = Vec::new();
    run_foreground_attach_session(
        &bootstrap_result,
        &source,
        NodeInstrumentationRuntime,
        target_pid,
        &mut output,
    )?;

    let output_text = String::from_utf8(output)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("non-utf8 output: {e}")))?;
    assert!(
        output_text.contains("[attached]"),
        "foreground output should include attached summary: {output_text}"
    );
    assert!(
        output_text.contains("[captured]"),
        "foreground output should include captured summary: {output_text}"
    );

    assert!(
        fake_server.request_count() >= 1,
        "fake llm server should receive at least one request"
    );

    let request_artifacts_dir = workspace
        .path()
        .join(".prismtrace")
        .join("state")
        .join("artifacts")
        .join("requests");
    let artifact_count = fs::read_dir(&request_artifacts_dir)?.count();
    assert!(
        artifact_count >= 1,
        "expected at least one request artifact under {}",
        request_artifacts_dir.display()
    );

    let exit_status = child.wait_for_exit(Duration::from_secs(5))?;
    assert!(
        matches!(exit_status, Some(status) if status.success()),
        "node child should exit cleanly after detach, got: {exit_status:?}"
    );

    Ok(())
}

fn write_node_target_script(root: &Path) -> io::Result<PathBuf> {
    let script_path = root.join("node-cli-target.js");
    let script = r#"
const endpoint = process.env.PRISMTRACE_TEST_ENDPOINT;
if (!endpoint) {
  process.exit(3);
}

function postWithBuiltinHttp(endpoint, body) {
  return new Promise((resolve, reject) => {
    const url = new URL(endpoint);
    const transport = url.protocol === "https:" ? require("https") : require("http");
    const req = transport.request(
      url,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "authorization": "Bearer prismtrace-test-token"
        }
      },
      (res) => {
        res.on("data", () => {});
        res.on("end", resolve);
      }
    );
    req.on("error", reject);
    req.write(body);
    req.end();
  });
}

async function sendRequestAndDetach() {
  try {
    const body = JSON.stringify({
      model: "gpt-test",
      messages: [{ role: "user", content: "ping from node cli attach test" }]
    });
    if (typeof globalThis.fetch === "function") {
      await globalThis.fetch(endpoint, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "authorization": "Bearer prismtrace-test-token"
        },
        body
      });
    } else {
      await postWithBuiltinHttp(endpoint, body);
    }
  } catch (_) {
    // Capture should still happen even if transport fails.
  }

  try {
    if (typeof globalThis.__prismtraceDetach === "function") {
      globalThis.__prismtraceDetach();
    }
  } finally {
    setTimeout(() => process.exit(0), 50);
  }
}

const startedAt = Date.now();
const poll = setInterval(() => {
  if (typeof globalThis.__prismtraceDetach === "function") {
    clearInterval(poll);
    void sendRequestAndDetach();
    return;
  }

  if (Date.now() - startedAt > 20000) {
    process.exit(4);
  }
}, 20);
"#;

    fs::write(&script_path, script.as_bytes())?;
    Ok(script_path)
}

fn resolve_node_binary() -> io::Result<PathBuf> {
    if let Ok(explicit) = env::var("PRISMTRACE_NODE_BIN") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
    }

    let Some(path_var) = env::var_os("PATH") else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "PATH is not set; cannot locate node binary",
        ));
    };

    for dir in env::split_paths(&path_var) {
        let candidate = dir.join("node");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "node binary not found in PATH",
    ))
}

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new(label: &str) -> io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let root = env::temp_dir().join(format!(
            "prismtrace-host-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn pid(&self) -> u32 {
        self.child.as_ref().expect("child should be available").id()
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> io::Result<Option<ExitStatus>> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if let Some(status) = self
                .child
                .as_mut()
                .expect("child should be available")
                .try_wait()?
            {
                return Ok(Some(status));
            }
            thread::sleep(Duration::from_millis(25));
        }
        Ok(None)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let is_still_running = child.try_wait().ok().flatten().is_none();
            if is_still_running {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

struct FakeLlmServer {
    addr: SocketAddr,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    join_handle: Option<thread::JoinHandle<io::Result<()>>>,
}

impl FakeLlmServer {
    fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let requests = Arc::new(AtomicUsize::new(0));
        let requests_in_thread = Arc::clone(&requests);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_in_thread = Arc::clone(&stop);

        let join_handle = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            while !stop_in_thread.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _peer)) => {
                        respond_ok(&mut stream)?;
                        requests_in_thread.fetch_add(1, Ordering::SeqCst);
                        return Ok(());
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        });

        Ok(Self {
            addr,
            requests,
            stop,
            join_handle: Some(join_handle),
        })
    }

    fn url(&self) -> String {
        format!("http://{}/v1/fake-llm", self.addr)
    }

    fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for FakeLlmServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

fn respond_ok(stream: &mut TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;

    let mut buffer = [0_u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read_bytes) if read_bytes < buffer.len() => break,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => return Err(error),
        }
    }

    let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";
    stream.write_all(response)?;
    stream.flush()?;
    Ok(())
}
