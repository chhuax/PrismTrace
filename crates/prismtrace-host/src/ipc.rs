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

    /// Read the next line from the reader (blocking).
    /// Returns:
    /// - IpcEvent::Message on successful parse
    /// - IpcEvent::ChannelDisconnected on EOF or IO error (never panics)
    /// Updates last_heartbeat when a Heartbeat message is received.
    pub fn next_event(&mut self) -> IpcEvent {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => IpcEvent::ChannelDisconnected {
                reason: "EOF".to_string(),
            },
            Ok(_) => match IpcMessage::from_json_line(&line) {
                Ok(msg) => {
                    if matches!(msg, IpcMessage::Heartbeat { .. }) {
                        self.last_heartbeat = Instant::now();
                    }
                    IpcEvent::Message(msg)
                }
                Err(e) => IpcEvent::ChannelDisconnected {
                    reason: format!("parse error: {e}"),
                },
            },
            Err(e) => IpcEvent::ChannelDisconnected {
                reason: format!("IO error: {e}"),
            },
        }
    }

    /// Non-blocking poll: reads one line and returns the parsed message.
    /// Returns None on EOF. Updates last_heartbeat on Heartbeat messages.
    pub fn poll_message(&mut self) -> Option<IpcMessage> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => match IpcMessage::from_json_line(&line) {
                Ok(msg) => {
                    if matches!(msg, IpcMessage::Heartbeat { .. }) {
                        self.last_heartbeat = Instant::now();
                    }
                    Some(msg)
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    }

    pub fn last_heartbeat_at(&self) -> Option<Instant> {
        Some(self.last_heartbeat)
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
            other => panic!("expected Message(Heartbeat), got something else: {:?}", std::mem::discriminant(&other)),
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
    fn check_heartbeat_timeout_returns_some_when_timeout_exceeded() {
        // Use 0ms timeout so it's immediately exceeded
        let listener = make_listener("", Duration::from_millis(0));
        // Sleep briefly to ensure elapsed > 0ms
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
