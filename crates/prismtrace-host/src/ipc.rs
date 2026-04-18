use prismtrace_core::IpcMessage;
use std::io::BufRead;
use std::time::{Duration, Instant};

pub struct IpcListener {
    reader: Box<dyn BufRead + Send>,
    heartbeat_timeout: Duration,
    last_heartbeat: Instant,
}

pub enum IpcEvent {
    Message(IpcMessage),
    HeartbeatTimeout { elapsed_ms: u64 },
    ChannelDisconnected { reason: String },
}

impl IpcListener {
    pub fn new(reader: Box<dyn BufRead + Send>, heartbeat_timeout: Duration) -> Self {
        Self {
            reader,
            heartbeat_timeout,
            last_heartbeat: Instant::now(),
        }
    }

    /// Non-blocking check: returns HeartbeatTimeout if elapsed > timeout, else None.
    pub fn check_heartbeat_timeout(&self) -> Option<IpcEvent> {
        let elapsed = self.last_heartbeat.elapsed();
        if elapsed > self.heartbeat_timeout {
            Some(IpcEvent::HeartbeatTimeout {
                elapsed_ms: elapsed.as_millis() as u64,
            })
        } else {
            None
        }
    }

    /// Read the next IPC message from the reader (blocking).
    /// Returns:
    /// - `IpcEvent::Message` on successful parse
    /// - `IpcEvent::ChannelDisconnected` on EOF or IO error (never panics)
    ///
    /// Non-IPC lines (e.g. application log output) are silently skipped so that
    /// a single unparsable line does not terminate the session.
    ///
    /// Updates `last_heartbeat` when a `Heartbeat` message is received.
    pub fn next_event(&mut self) -> IpcEvent {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    return IpcEvent::ChannelDisconnected {
                        reason: "EOF".to_string(),
                    };
                }
                Ok(_) => match IpcMessage::from_json_line(&line) {
                    Ok(msg) => {
                        if matches!(msg, IpcMessage::Heartbeat { .. }) {
                            self.last_heartbeat = Instant::now();
                        }
                        return IpcEvent::Message(msg);
                    }
                    Err(_) => {
                        // Non-IPC line (e.g. app log) — skip and keep reading.
                        continue;
                    }
                },
                Err(e) => {
                    return IpcEvent::ChannelDisconnected {
                        reason: format!("IO error: {e}"),
                    };
                }
            }
        }
    }

    /// Read one line from the reader and return the parsed message.
    ///
    /// This method **blocks** until a line is available, EOF is reached, or an IO
    /// error occurs. Non-IPC lines are skipped. Returns `None` on EOF or IO error.
    /// Updates `last_heartbeat` on `Heartbeat` messages.
    pub fn poll_message(&mut self) -> Option<IpcMessage> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => match IpcMessage::from_json_line(&line) {
                    Ok(msg) => {
                        if matches!(msg, IpcMessage::Heartbeat { .. }) {
                            self.last_heartbeat = Instant::now();
                        }
                        return Some(msg);
                    }
                    Err(_) => continue,
                },
                Err(_) => return None,
            }
        }
    }

    pub fn last_heartbeat_at(&self) -> Option<Instant> {
        Some(self.last_heartbeat)
    }

    pub fn heartbeat_timeout(&self) -> Duration {
        self.heartbeat_timeout
    }
}

#[cfg(test)]
mod tests {
    use super::{IpcEvent, IpcListener};
    use prismtrace_core::IpcMessage;
    use std::io::Cursor;
    use std::time::Duration;

    fn make_listener(data: &str, timeout: Duration) -> IpcListener {
        let reader = Box::new(Cursor::new(data.to_string().into_bytes()));
        IpcListener::new(reader, timeout)
    }

    #[test]
    fn next_event_parses_heartbeat_message() {
        let line = IpcMessage::Heartbeat { timestamp_ms: 1000 }.to_json_line();
        let mut listener = make_listener(&line, Duration::from_secs(60));

        match listener.next_event() {
            IpcEvent::Message(IpcMessage::Heartbeat { timestamp_ms }) => {
                assert_eq!(timestamp_ms, 1000);
            }
            other => panic!(
                "expected Message(Heartbeat), got something else: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn next_event_returns_channel_disconnected_on_eof() {
        let mut listener = make_listener("", Duration::from_secs(60));

        match listener.next_event() {
            IpcEvent::ChannelDisconnected { reason } => {
                assert_eq!(reason, "EOF");
            }
            _ => panic!("expected ChannelDisconnected on EOF"),
        }
    }

    #[test]
    fn next_event_skips_non_ipc_lines_and_returns_next_valid_message() {
        // A non-JSON app log line followed by a valid IPC message.
        let data = format!(
            "not json at all\n{}",
            IpcMessage::Heartbeat { timestamp_ms: 42 }.to_json_line()
        );
        let mut listener = make_listener(&data, Duration::from_secs(60));

        match listener.next_event() {
            IpcEvent::Message(IpcMessage::Heartbeat { timestamp_ms }) => {
                assert_eq!(timestamp_ms, 42);
            }
            _ => panic!("expected Heartbeat after skipping non-IPC line"),
        }
    }

    #[test]
    fn next_event_returns_disconnected_when_only_non_ipc_lines_then_eof() {
        let mut listener = make_listener("not json\nalso not json\n", Duration::from_secs(60));

        match listener.next_event() {
            IpcEvent::ChannelDisconnected { reason } => {
                assert_eq!(reason, "EOF");
            }
            _ => panic!("expected ChannelDisconnected after exhausting non-IPC lines"),
        }
    }

    #[test]
    fn check_heartbeat_timeout_returns_some_when_timeout_exceeded() {
        let listener = make_listener("", Duration::from_millis(0));
        std::thread::sleep(Duration::from_millis(1));

        match listener.check_heartbeat_timeout() {
            Some(IpcEvent::HeartbeatTimeout { elapsed_ms }) => {
                assert!(elapsed_ms >= 1, "elapsed_ms should be >= 1");
            }
            _ => panic!("expected HeartbeatTimeout"),
        }
    }

    #[test]
    fn check_heartbeat_timeout_returns_none_within_window() {
        let listener = make_listener("", Duration::from_secs(60));

        assert!(
            listener.check_heartbeat_timeout().is_none(),
            "should not timeout within 60s window"
        );
    }
}
