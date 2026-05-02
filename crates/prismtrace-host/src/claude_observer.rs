use prismtrace_sources::{
    ObservedEvent, ObservedEventKind, ObserverArtifactSource, ObserverArtifactWriter,
    ObserverChannelKind, ObserverHandshake, ObserverSession, ObserverSource, ObserverSourceFactory,
};
use prismtrace_storage::StorageLayout;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const DEFAULT_CLAUDE_TRANSCRIPT_ROOT: &str = ".claude/projects";
const DEFAULT_MAX_FILES: usize = 8;
const DEFAULT_MAX_EVENTS: usize = 256;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_millis(750);
const DEFAULT_MAX_FOLLOW_EVENTS: usize = 12;
const CLAUDE_UNOBSERVABLE_TYPE: &str = "claude_observer_unobservable";
const CLAUDE_UNOBSERVABLE_REASON_NO_TRANSCRIPTS: &str = "no_transcripts";
const CLAUDE_UNOBSERVABLE_REASON_TRANSCRIPT_ROOT_UNAVAILABLE: &str = "transcript_root_unavailable";
const CLAUDE_UNOBSERVABLE_REASON_TRANSCRIPT_UNAVAILABLE: &str = "transcript_unavailable";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeObserverOptions {
    pub transcript_root: PathBuf,
    pub max_files: usize,
    pub max_events: usize,
    pub idle_timeout: Duration,
    pub max_follow_events: usize,
}

impl Default for ClaudeObserverOptions {
    fn default() -> Self {
        Self {
            transcript_root: default_transcript_root(),
            max_files: DEFAULT_MAX_FILES,
            max_events: DEFAULT_MAX_EVENTS,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            max_follow_events: DEFAULT_MAX_FOLLOW_EVENTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeObserverSource {
    transcript_root: PathBuf,
    max_files: usize,
    max_events: usize,
}

impl ObserverSource for ClaudeObserverSource {
    fn channel_kind(&self) -> ObserverChannelKind {
        ObserverChannelKind::ClaudeCodeTranscript
    }

    fn transport_label(&self) -> String {
        self.transcript_root.display().to_string()
    }

    fn connect(&self) -> io::Result<Box<dyn ObserverSession>> {
        Ok(Box::new(ClaudeObserverSession::new(
            self.transcript_root.clone(),
            self.max_files,
            self.max_events,
        )?))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeObserverFactory;

impl ObserverSourceFactory<ClaudeObserverOptions> for ClaudeObserverFactory {
    fn build_sources(
        &self,
        request: &ClaudeObserverOptions,
    ) -> io::Result<Vec<Box<dyn ObserverSource>>> {
        Ok(vec![Box::new(ClaudeObserverSource {
            transcript_root: request.transcript_root.clone(),
            max_files: request.max_files,
            max_events: request.max_events,
        })])
    }
}

pub fn run_claude_observer(
    storage: &StorageLayout,
    output: &mut impl Write,
    options: ClaudeObserverOptions,
) -> io::Result<()> {
    let factory = ClaudeObserverFactory;
    let source = factory
        .build_sources(&options)?
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no claude source available"))?;

    writeln!(
        output,
        "[claude-observer] attempting {} via {}",
        source.channel_kind().label(),
        source.transport_label()
    )?;

    let mut session = source.connect()?;
    let handshake = session.initialize()?;
    let artifact_writer =
        ObserverArtifactWriter::create(storage, ObserverArtifactSource::ClaudeCode, &handshake)?;
    writeln!(
        output,
        "{}",
        serde_json::to_string(&json!({
            "type": "claude_observer_handshake",
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

    for _ in 0..options.max_follow_events {
        match session.next_event(options.idle_timeout)? {
            Some(event) => {
                artifact_writer.append_event(&event)?;
                writeln!(output, "{}", serde_json::to_string(&event_as_json(&event))?)?;
            }
            None => break,
        }
    }

    Ok(())
}

fn event_as_json(event: &ObservedEvent) -> Value {
    json!({
        "type": "claude_observer_event",
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

#[derive(Debug, Clone, PartialEq)]
struct ClaudeObserverSession {
    transcript_root: PathBuf,
    transcripts: Vec<TrackedTranscript>,
    max_events: usize,
    status_events: VecDeque<ObservedEvent>,
    observation_closed: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct TrackedTranscript {
    path: PathBuf,
    offset: u64,
    partial_line: Vec<u8>,
    pending_events: VecDeque<ObservedEvent>,
}

impl ClaudeObserverSession {
    fn new(transcript_root: PathBuf, max_files: usize, max_events: usize) -> io::Result<Self> {
        let mut status_events = VecDeque::new();
        let transcripts = match discover_transcript_files(&transcript_root, max_files) {
            Ok(transcript_files) => {
                let transcripts: Vec<TrackedTranscript> = transcript_files
                    .into_iter()
                    .map(TrackedTranscript::new)
                    .collect();
                if transcripts.is_empty() {
                    status_events.push_back(claude_unobservable_event(
                        &transcript_root,
                        None,
                        CLAUDE_UNOBSERVABLE_REASON_NO_TRANSCRIPTS,
                        format!(
                            "no claude transcript files found under {}",
                            transcript_root.display()
                        ),
                        None,
                    ));
                }
                transcripts
            }
            Err(error) => {
                status_events.push_back(claude_unobservable_event(
                    &transcript_root,
                    None,
                    CLAUDE_UNOBSERVABLE_REASON_TRANSCRIPT_ROOT_UNAVAILABLE,
                    format!(
                        "unable to discover claude transcript root {}: {error}",
                        transcript_root.display()
                    ),
                    Some(error.kind()),
                ));
                Vec::new()
            }
        };
        let observation_closed = transcripts.is_empty();

        Ok(Self {
            transcript_root,
            transcripts,
            max_events,
            status_events,
            observation_closed,
        })
    }

    #[cfg(test)]
    fn new_with_limits(
        transcript_root: PathBuf,
        max_files: usize,
        max_events: usize,
    ) -> io::Result<Self> {
        Self::new(transcript_root, max_files, max_events)
    }
}

impl TrackedTranscript {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            partial_line: Vec::new(),
            pending_events: VecDeque::new(),
        }
    }
}

impl ObserverSession for ClaudeObserverSession {
    fn initialize(&mut self) -> io::Result<ObserverHandshake> {
        Ok(ObserverHandshake {
            channel_kind: ObserverChannelKind::ClaudeCodeTranscript,
            transport_label: self.transcript_root.display().to_string(),
            server_label: "claude transcript jsonl".into(),
            raw_json: json!({
                "transcript_root": self.transcript_root,
                "transcript_files": self
                    .transcripts
                    .iter()
                    .map(|transcript| transcript.path.clone())
                    .collect::<Vec<_>>(),
                "transcript_file_count": self.transcripts.len(),
            }),
        })
    }

    fn collect_capability_events(&mut self) -> io::Result<Vec<ObservedEvent>> {
        let mut events = Vec::new();
        while let Some(event) = self.status_events.pop_front() {
            events.push(event);
        }

        let mut retained = Vec::with_capacity(self.transcripts.len());
        for mut transcript in std::mem::take(&mut self.transcripts) {
            let file_bytes = match fs::read(&transcript.path) {
                Ok(file_bytes) => file_bytes,
                Err(error) => {
                    events.push(claude_unobservable_event(
                        &self.transcript_root,
                        Some(&transcript.path),
                        CLAUDE_UNOBSERVABLE_REASON_TRANSCRIPT_UNAVAILABLE,
                        format!(
                            "unable to read transcript {} during history scan: {error}",
                            transcript.path.display()
                        ),
                        Some(error.kind()),
                    ));
                    continue;
                }
            };
            let (consumed, parsed_events) = parse_transcript_events(&transcript.path, &file_bytes);

            transcript.offset = file_bytes.len() as u64;
            transcript.partial_line = file_bytes[consumed..].to_vec();
            transcript.pending_events.clear();

            let remaining_budget = self.max_events.saturating_sub(events.len());
            if remaining_budget > 0 {
                let tail_start = parsed_events.len().saturating_sub(remaining_budget);
                events.extend(parsed_events.into_iter().skip(tail_start));
            }

            retained.push(transcript);
        }
        self.transcripts = retained;
        self.observation_closed = self.transcripts.is_empty();

        Ok(events)
    }

    fn next_event(&mut self, timeout: Duration) -> io::Result<Option<ObservedEvent>> {
        if let Some(event) = self.status_events.pop_front() {
            return Ok(Some(event));
        }

        if self.observation_closed {
            return Ok(None);
        }

        let deadline = Instant::now() + timeout;
        let transcript_root = self.transcript_root.clone();

        loop {
            let mut index = 0;
            while index < self.transcripts.len() {
                match read_appended_event(&mut self.transcripts[index]) {
                    Ok(Some(event)) => return Ok(Some(event)),
                    Ok(None) => index += 1,
                    Err(error) => {
                        let failed = self.transcripts.remove(index);
                        if self.transcripts.is_empty() {
                            self.observation_closed = true;
                        }
                        return Ok(Some(claude_unobservable_event(
                            &transcript_root,
                            Some(&failed.path),
                            CLAUDE_UNOBSERVABLE_REASON_TRANSCRIPT_UNAVAILABLE,
                            format!(
                                "unable to continue observing transcript {}: {error}",
                                failed.path.display()
                            ),
                            Some(error.kind()),
                        )));
                    }
                }
            }

            if Instant::now() >= deadline {
                return Ok(None);
            }

            thread::sleep(Duration::from_millis(25));
        }
    }
}

fn read_appended_event(transcript: &mut TrackedTranscript) -> io::Result<Option<ObservedEvent>> {
    if let Some(event) = transcript.pending_events.pop_front() {
        return Ok(Some(event));
    }

    let mut file = fs::File::open(&transcript.path)?;
    let file_len = file.metadata()?.len();
    if transcript.offset > file_len {
        transcript.offset = 0;
        transcript.partial_line.clear();
        transcript.pending_events.clear();
    }

    file.seek(SeekFrom::Start(transcript.offset))?;
    let mut appended = Vec::new();
    file.read_to_end(&mut appended)?;
    if appended.is_empty() {
        Ok(None)
    } else {
        let partial_len = transcript.partial_line.len();
        let mut buffer = Vec::with_capacity(partial_len + appended.len());
        buffer.extend_from_slice(&transcript.partial_line);
        buffer.extend_from_slice(&appended);

        let (consumed, parsed_events) = parse_transcript_events(&transcript.path, &buffer);
        let mut parsed_events = parsed_events.into_iter();
        let emitted_event = parsed_events.next();
        transcript.pending_events.extend(parsed_events);

        transcript.offset += appended.len() as u64;
        transcript.partial_line = buffer[consumed..].to_vec();
        Ok(emitted_event)
    }
}

pub fn default_transcript_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(DEFAULT_CLAUDE_TRANSCRIPT_ROOT)
}

pub fn discover_transcript_files(root: &Path, max_files: usize) -> io::Result<Vec<PathBuf>> {
    if !root.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("claude transcript root not found: {}", root.display()),
        ));
    }

    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "claude transcript root is not a directory: {}",
                root.display()
            ),
        ));
    }

    let mut files = Vec::new();
    collect_jsonl_files(root, &mut files, max_files)?;
    Ok(files.into_iter().map(|candidate| candidate.path).collect())
}

#[derive(Debug, Clone)]
struct TranscriptCandidate {
    path: PathBuf,
    modified_at: SystemTime,
}

fn collect_jsonl_files(
    root: &Path,
    files: &mut Vec<TranscriptCandidate>,
    max_files: usize,
) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            let _ = collect_jsonl_files(&path, files, max_files);
            continue;
        }

        if file_type.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let modified_at = match metadata.modified() {
                Ok(modified_at) => modified_at,
                Err(_) => continue,
            };
            files.push(TranscriptCandidate { path, modified_at });
            files.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified_at));
            if files.len() > max_files {
                files.truncate(max_files);
            }
        }
    }

    Ok(())
}

pub fn normalize_transcript_record(
    transcript_path: &Path,
    record: &Value,
) -> Option<ObservedEvent> {
    let record_type = record
        .get("type")
        .and_then(Value::as_str)
        .or_else(|| record.get("event").and_then(Value::as_str))
        .unwrap_or("unknown");

    let event_kind = match record_type {
        "user" => ObservedEventKind::Turn,
        "assistant" | "progress" | "attachment" => ObservedEventKind::Item,
        "system/local_command" => ObservedEventKind::Tool,
        "system/stop_hook_summary" => ObservedEventKind::Hook,
        "permission-mode" => ObservedEventKind::Approval,
        _ => ObservedEventKind::Unknown,
    };

    let summary = transcript_summary(record_type, record);
    let thread_id =
        string_field(record, &["session_id", "sessionId", "conversation_id"]).or_else(|| {
            transcript_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        });
    let generic_id = string_field(record, &["id"]);
    let turn_id = string_field(
        record,
        &["parentUuid", "parent_uuid", "turn_id", "message_id"],
    )
    .or_else(|| generic_id.clone());
    let item_id = string_field(record, &["uuid", "item_id"]).or_else(|| {
        if generic_id.is_some() && turn_id == generic_id {
            None
        } else {
            generic_id.clone()
        }
    });
    let timestamp = string_field(record, &["timestamp", "created_at", "createdAt"]);

    Some(ObservedEvent {
        channel_kind: ObserverChannelKind::ClaudeCodeTranscript,
        event_kind,
        summary,
        method: Some("transcript.jsonl".into()),
        thread_id,
        turn_id,
        item_id,
        timestamp,
        raw_json: record.clone(),
    })
}

fn transcript_summary(record_type: &str, record: &Value) -> String {
    let text = record_text(record);
    match record_type {
        "user" => format!("user: {}", truncate(&text, 120)),
        "assistant" => format!("assistant: {}", truncate(&text, 120)),
        "progress" => format!("progress: {}", truncate(&text, 120)),
        "attachment" => format!("attachment: {}", truncate(&text, 120)),
        "system/local_command" => {
            let command = string_field(record, &["command", "cmd"]).unwrap_or(text);
            format!("local command: {}", truncate(&command, 120))
        }
        "system/stop_hook_summary" => format!("stop hook: {}", truncate(&text, 120)),
        "permission-mode" => {
            let mode = string_field(record, &["mode", "permission_mode"]).unwrap_or(text);
            format!("permission mode: {}", truncate(&mode, 120))
        }
        other => format!("unknown transcript event: {other}"),
    }
}

fn record_text(record: &Value) -> String {
    if let Some(text) = string_field(record, &["text", "content", "summary", "message"]) {
        return text;
    }

    if let Some(message) = record.get("message")
        && let Some(content) = message.get("content")
        && let Some(text) = value_to_text(content)
    {
        return text;
    }

    String::new()
}

fn string_field(record: &Value, field_names: &[&str]) -> Option<String> {
    field_names
        .iter()
        .find_map(|field_name| record.get(field_name).and_then(value_to_text))
}

fn value_to_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().filter_map(value_to_text).collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        Value::Object(map) => map
            .get("text")
            .and_then(value_to_text)
            .or_else(|| map.get("content").and_then(value_to_text))
            .or_else(|| map.get("value").and_then(value_to_text)),
        other => Some(other.to_string()),
    }
}

fn truncate(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn claude_unobservable_event(
    transcript_root: &Path,
    transcript_path: Option<&Path>,
    reason: &str,
    detail: String,
    error_kind: Option<io::ErrorKind>,
) -> ObservedEvent {
    ObservedEvent {
        channel_kind: ObserverChannelKind::ClaudeCodeTranscript,
        event_kind: ObservedEventKind::Unknown,
        summary: format!("claude transcript unobservable: {detail}"),
        method: Some("transcript.jsonl".into()),
        thread_id: None,
        turn_id: None,
        item_id: None,
        timestamp: None,
        raw_json: json!({
            "type": CLAUDE_UNOBSERVABLE_TYPE,
            "reason": reason,
            "detail": detail,
            "transcript_root": transcript_root.display().to_string(),
            "transcript_path": transcript_path.map(|path| path.display().to_string()),
            "io_error_kind": error_kind.map(io_error_kind_label),
        }),
    }
}

fn io_error_kind_label(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::AlreadyExists => "already_exists",
        io::ErrorKind::WouldBlock => "would_block",
        io::ErrorKind::InvalidInput => "invalid_input",
        io::ErrorKind::InvalidData => "invalid_data",
        io::ErrorKind::TimedOut => "timed_out",
        io::ErrorKind::WriteZero => "write_zero",
        io::ErrorKind::Interrupted => "interrupted",
        io::ErrorKind::UnexpectedEof => "unexpected_eof",
        _ => "other",
    }
}

fn parse_transcript_events(transcript_path: &Path, buffer: &[u8]) -> (usize, Vec<ObservedEvent>) {
    let mut consumed = 0usize;
    let mut events = Vec::new();

    for line in buffer.split_inclusive(|byte| *byte == b'\n') {
        if !line.ends_with(b"\n") {
            break;
        }
        consumed += line.len();

        let line = String::from_utf8_lossy(line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let record: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(event) = normalize_transcript_record(transcript_path, &record) {
            events.push(event);
        }
    }

    (consumed, events)
}

#[cfg(test)]
mod tests {
    use super::{
        ClaudeObserverOptions, ClaudeObserverSession, default_transcript_root,
        discover_transcript_files, normalize_transcript_record, run_claude_observer,
    };
    use prismtrace_sources::{
        ObservedEvent, ObservedEventKind, ObserverArtifactSource, ObserverArtifactWriter,
        ObserverChannelKind, ObserverHandshake, ObserverSession,
    };
    use serde_json::json;
    use std::fs;
    use std::io;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn default_options_use_default_transcript_root() {
        assert_eq!(
            ClaudeObserverOptions::default().transcript_root,
            default_transcript_root()
        );
        assert_eq!(ClaudeObserverOptions::default().max_files, 8);
        assert_eq!(ClaudeObserverOptions::default().max_events, 256);
        assert_eq!(
            ClaudeObserverOptions::default().idle_timeout,
            Duration::from_millis(750)
        );
        assert_eq!(ClaudeObserverOptions::default().max_follow_events, 12);
    }

    #[test]
    fn discover_transcript_files_orders_recent_jsonl_first() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(root.join("nested"))?;

        let older = root.join("older.jsonl");
        let newer = root.join("nested").join("newer.jsonl");
        let ignored = root.join("ignore.txt");
        fs::write(&older, b"{}\n")?;
        thread::sleep(Duration::from_millis(20));
        fs::write(&newer, b"{}\n")?;
        fs::write(&ignored, b"{}\n")?;

        let files = discover_transcript_files(&root, 8)?;

        assert_eq!(files, vec![newer, older]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn discover_transcript_files_limits_results_by_recency() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;

        let oldest = root.join("oldest.jsonl");
        let middle = root.join("middle.jsonl");
        let newest = root.join("newest.jsonl");
        fs::write(&oldest, b"{}\n")?;
        thread::sleep(Duration::from_millis(20));
        fs::write(&middle, b"{}\n")?;
        thread::sleep(Duration::from_millis(20));
        fs::write(&newest, b"{}\n")?;

        let files = discover_transcript_files(&root, 2)?;

        assert_eq!(files, vec![newest, middle]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn discover_transcript_files_skips_vanished_entries() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;

        let keep = root.join("keep.jsonl");
        let vanish = root.join("vanish.jsonl");
        fs::write(&keep, b"{}\n")?;
        fs::write(&vanish, b"{}\n")?;
        fs::remove_file(&vanish)?;

        let files = discover_transcript_files(&root, 8)?;

        assert_eq!(files, vec![keep]);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn transcript_user_record_maps_to_turn_event() {
        let event = normalize_transcript_record(
            Path::new("/tmp/session.jsonl"),
            &json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": "Summarize this transcript"
                },
                "session_id": "session-1",
                "timestamp": "2026-04-26T09:00:00Z",
                "id": "turn-1"
            }),
        )
        .expect("event should exist");

        assert_eq!(event.event_kind, ObservedEventKind::Turn);
        assert_eq!(event.thread_id.as_deref(), Some("session-1"));
        assert_eq!(event.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(event.channel_kind.label(), "claude-code");
        assert!(event.summary.contains("Summarize this transcript"));
    }

    #[test]
    fn transcript_unknown_record_falls_back_to_unknown_event() {
        let event = normalize_transcript_record(
            Path::new("/tmp/session.jsonl"),
            &json!({
                "type": "mystery",
                "session_id": "session-2"
            }),
        )
        .expect("event should exist");

        assert_eq!(event.event_kind, ObservedEventKind::Unknown);
        assert_eq!(event.thread_id.as_deref(), Some("session-2"));
        assert_eq!(event.channel_kind.label(), "claude-code");
        assert!(event.summary.contains("unknown transcript event"));
    }

    #[test]
    fn normalize_transcript_record_prefers_parent_uuid_for_turn_id_and_uuid_for_item_id() {
        let event = normalize_transcript_record(
            Path::new("/tmp/session.jsonl"),
            &json!({
                "type": "assistant",
                "session_id": "session-3",
                "parentUuid": "turn-parent",
                "uuid": "item-1",
                "id": "generic-id",
                "message": {
                    "content": "Linked response"
                }
            }),
        )
        .expect("event should exist");

        assert_eq!(event.turn_id.as_deref(), Some("turn-parent"));
        assert_eq!(event.item_id.as_deref(), Some("item-1"));
    }

    #[test]
    fn collect_capability_events_stops_after_max_events() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let transcript = root.join("session.jsonl");
        let mut file = fs::File::create(&transcript)?;
        writeln!(
            file,
            "{}",
            json!({"type":"user","session_id":"s1","id":"1"})
        )?;
        writeln!(
            file,
            "{}",
            json!({"type":"assistant","session_id":"s1","id":"2"})
        )?;
        writeln!(
            file,
            "{}",
            json!({"type":"progress","session_id":"s1","id":"3"})
        )?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 2)?;
        let events = session.collect_capability_events()?;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].turn_id.as_deref(), Some("2"));
        assert_eq!(events[1].turn_id.as_deref(), Some("3"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn collect_capability_events_prefers_most_recent_history_when_transcript_exceeds_max_events()
    -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let transcript = root.join("session.jsonl");
        let mut file = fs::File::create(&transcript)?;
        for id in 1..=4 {
            writeln!(
                file,
                "{}",
                json!({
                    "type":"assistant",
                    "session_id":"s1",
                    "id":id.to_string(),
                    "message":{"content":format!("event-{id}")},
                })
            )?;
        }
        file.flush()?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 2)?;
        let events = session.collect_capability_events()?;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].turn_id.as_deref(), Some("3"));
        assert_eq!(events[1].turn_id.as_deref(), Some("4"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn collect_capability_events_marks_existing_backlog_as_consumed_for_follow() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let transcript = root.join("session.jsonl");
        let mut file = fs::File::create(&transcript)?;
        writeln!(
            file,
            "{}",
            json!({"type":"user","session_id":"s1","id":"1","message":{"content":"first"}})
        )?;
        writeln!(
            file,
            "{}",
            json!({"type":"assistant","session_id":"s1","id":"2","message":{"content":"second"}})
        )?;
        writeln!(
            file,
            "{}",
            json!({"type":"assistant","session_id":"s1","id":"3","message":{"content":"third"}})
        )?;
        file.flush()?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 2)?;
        let events = session.collect_capability_events()?;

        assert_eq!(events.len(), 2);
        assert_eq!(session.next_event(Duration::from_millis(50))?, None);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn collect_capability_events_handles_file_without_trailing_newline_without_replaying_history()
    -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let transcript = root.join("session.jsonl");
        let initial = format!(
            "{}\n{}",
            json!({"type":"user","session_id":"s1","id":"1","message":{"content":"first"}}),
            r#"{"type":"assistant","session_id":"s1","uuid":"item-2","parentUuid":"turn-1","message":{"content":"sec"#
        );
        fs::write(&transcript, initial)?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        let events = session.collect_capability_events()?;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "user: first");

        {
            let mut file = fs::OpenOptions::new().append(true).open(&transcript)?;
            file.write_all(b"ond\"}}\n")?;
            file.flush()?;
        }

        let followed = session
            .next_event(Duration::from_millis(200))?
            .expect("should read completed appended line");
        assert_eq!(followed.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(followed.item_id.as_deref(), Some("item-2"));
        assert!(followed.summary.contains("second"));
        assert_eq!(session.next_event(Duration::from_millis(50))?, None);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn collect_capability_events_surfaces_missing_file_and_keeps_healthy_history() -> io::Result<()>
    {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let keep = root.join("keep.jsonl");
        let missing = root.join("missing.jsonl");
        fs::write(
            &keep,
            format!("{}\n", json!({"type":"user","session_id":"ok","id":"1"})),
        )?;
        fs::write(
            &missing,
            format!("{}\n", json!({"type":"user","session_id":"gone","id":"2"})),
        )?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        session.transcripts = vec![
            super::TrackedTranscript::new(keep.clone()),
            super::TrackedTranscript::new(missing.clone()),
        ];
        fs::remove_file(&missing)?;

        let events = session.collect_capability_events()?;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].thread_id.as_deref(), Some("ok"));
        assert_eq!(events[1].event_kind, ObservedEventKind::Unknown);
        assert_eq!(
            events[1]
                .raw_json
                .get("reason")
                .and_then(|value| value.as_str()),
            Some("transcript_unavailable")
        );
        assert_eq!(session.transcripts.len(), 1);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn claude_observer_session_surfaces_unobservable_event_when_no_transcripts_are_available()
    -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        let events = session.collect_capability_events()?;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_kind, ObservedEventKind::Unknown);
        assert_eq!(
            events[0]
                .raw_json
                .get("type")
                .and_then(|value| value.as_str()),
            Some("claude_observer_unobservable")
        );
        assert_eq!(
            events[0]
                .raw_json
                .get("reason")
                .and_then(|value| value.as_str()),
            Some("no_transcripts")
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn claude_observer_session_surfaces_structured_event_when_transcript_root_is_missing()
    -> io::Result<()> {
        let root = unique_test_dir();
        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        let events = session.collect_capability_events()?;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_kind, ObservedEventKind::Unknown);
        assert_eq!(
            events[0]
                .raw_json
                .get("reason")
                .and_then(|value| value.as_str()),
            Some("transcript_root_unavailable")
        );
        assert_eq!(
            events[0]
                .raw_json
                .get("io_error_kind")
                .and_then(|value| value.as_str()),
            Some("not_found")
        );
        assert_eq!(session.next_event(Duration::from_millis(50))?, None);

        Ok(())
    }

    #[test]
    fn claude_observer_artifact_writer_persists_handshake_and_event() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = crate::bootstrap(&workspace_root)?;

        let handshake = ObserverHandshake {
            channel_kind: ObserverChannelKind::ClaudeCodeTranscript,
            transport_label: "/tmp/claude".into(),
            server_label: "claude test".into(),
            raw_json: json!({ "version": "test" }),
        };
        let writer = ObserverArtifactWriter::create(
            &result.storage,
            ObserverArtifactSource::ClaudeCode,
            &handshake,
        )?;
        writer.append_event(&ObservedEvent {
            channel_kind: ObserverChannelKind::ClaudeCodeTranscript,
            event_kind: ObservedEventKind::Turn,
            summary: "demo".into(),
            method: Some("transcript.jsonl".into()),
            thread_id: Some("session-1".into()),
            turn_id: Some("turn-1".into()),
            item_id: None,
            timestamp: Some("1".into()),
            raw_json: json!({ "id": "turn-1" }),
        })?;

        let artifact = fs::read_to_string(writer.artifact_path())?;
        assert!(artifact.contains("\"record_type\":\"handshake\""));
        assert!(artifact.contains("\"record_type\":\"event\""));
        assert!(
            writer
                .artifact_path()
                .to_string_lossy()
                .contains(".prismtrace/state/artifacts/observer_events/claude-code/")
        );

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    #[test]
    fn run_claude_observer_writes_artifact_records() -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = crate::bootstrap(&workspace_root)?;
        let transcript_root = workspace_root.join("transcripts");
        fs::create_dir_all(&transcript_root)?;
        let transcript = transcript_root.join("session.jsonl");
        fs::write(
            &transcript,
            format!(
                "{}\n",
                json!({
                    "type":"user",
                    "session_id":"session-1",
                    "id":"turn-1",
                    "message":{"content":"hello"}
                })
            ),
        )?;

        let mut output = Vec::new();
        run_claude_observer(
            &result.storage,
            &mut output,
            ClaudeObserverOptions {
                transcript_root: transcript_root.clone(),
                max_files: 8,
                max_events: 8,
                idle_timeout: Duration::from_millis(20),
                max_follow_events: 0,
            },
        )?;

        let observer_dir = result
            .storage
            .artifacts_dir
            .join("observer_events")
            .join("claude-code");
        let artifact_path = fs::read_dir(&observer_dir)?
            .find_map(|entry| entry.ok().map(|entry| entry.path()))
            .expect("artifact should exist");
        let artifact = fs::read_to_string(artifact_path)?;
        assert!(artifact.contains("\"record_type\":\"handshake\""));
        assert!(artifact.contains("\"record_type\":\"event\""));
        assert!(String::from_utf8_lossy(&output).contains("claude_observer_event"));

        fs::remove_dir_all(result.config.state_root)?;
        fs::remove_dir_all(transcript_root)?;
        Ok(())
    }

    #[test]
    fn next_event_reads_appended_transcript_line() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let transcript = root.join("session.jsonl");
        fs::write(
            &transcript,
            format!(
                "{}\n",
                json!({
                    "type":"user",
                    "session_id":"session-1",
                    "id":"turn-1",
                    "message":{"content":"hello"}
                })
            ),
        )?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        let historical = session.collect_capability_events()?;
        assert_eq!(historical.len(), 1);

        {
            let mut file = fs::OpenOptions::new().append(true).open(&transcript)?;
            writeln!(
                file,
                "{}",
                json!({
                    "type":"assistant",
                    "session_id":"session-1",
                    "id":"turn-2",
                    "message":{"content":"world"}
                })
            )?;
            file.flush()?;
        }

        let event = session
            .next_event(Duration::from_millis(200))?
            .expect("should read appended event");
        assert_eq!(event.turn_id.as_deref(), Some("turn-2"));
        assert!(event.summary.contains("world"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn next_event_consumes_multiple_appended_lines_without_repeating() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let transcript = root.join("session.jsonl");
        fs::write(
            &transcript,
            format!(
                "{}\n",
                json!({
                    "type":"user",
                    "session_id":"session-1",
                    "id":"turn-1",
                    "message":{"content":"hello"}
                })
            ),
        )?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        let historical = session.collect_capability_events()?;
        assert_eq!(historical.len(), 1);

        {
            let mut file = fs::OpenOptions::new().append(true).open(&transcript)?;
            writeln!(
                file,
                "{}",
                json!({
                    "type":"assistant",
                    "session_id":"session-1",
                    "id":"turn-2",
                    "message":{"content":"world-1"}
                })
            )?;
            writeln!(
                file,
                "{}",
                json!({
                    "type":"assistant",
                    "session_id":"session-1",
                    "id":"turn-3",
                    "message":{"content":"world-2"}
                })
            )?;
            file.flush()?;
        }

        let first = session
            .next_event(Duration::from_millis(200))?
            .expect("should read first appended event");
        let second = session
            .next_event(Duration::from_millis(200))?
            .expect("should read second appended event");
        let third = session.next_event(Duration::from_millis(50))?;

        assert_eq!(first.turn_id.as_deref(), Some("turn-2"));
        assert_eq!(second.turn_id.as_deref(), Some("turn-3"));
        assert_eq!(third, None);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn next_event_returns_none_when_no_new_lines_arrive() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let transcript = root.join("session.jsonl");
        fs::write(
            &transcript,
            format!(
                "{}\n",
                json!({
                    "type":"user",
                    "session_id":"session-1",
                    "id":"turn-1"
                })
            ),
        )?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        let _ = session.collect_capability_events()?;

        assert_eq!(session.next_event(Duration::from_millis(50))?, None);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn next_event_surfaces_unobservable_transcript_failure_as_structured_event_or_result()
    -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let transcript = root.join("session.jsonl");
        fs::write(
            &transcript,
            format!(
                "{}\n",
                json!({
                    "type":"user",
                    "session_id":"session-1",
                    "id":"turn-1",
                    "message":{"content":"hello"}
                })
            ),
        )?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        let historical = session.collect_capability_events()?;
        assert_eq!(historical.len(), 1);

        fs::remove_file(&transcript)?;

        let event = session
            .next_event(Duration::from_millis(200))?
            .expect("expected structured unobservable signal");
        assert_eq!(event.event_kind, ObservedEventKind::Unknown);
        assert_eq!(
            event.raw_json.get("type").and_then(|value| value.as_str()),
            Some("claude_observer_unobservable")
        );
        assert_eq!(
            event
                .raw_json
                .get("reason")
                .and_then(|value| value.as_str()),
            Some("transcript_unavailable")
        );
        assert_eq!(
            event
                .raw_json
                .get("io_error_kind")
                .and_then(|value| value.as_str()),
            Some("not_found")
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn next_event_removes_failed_transcript_but_continues_following_other_files() -> io::Result<()>
    {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let failed = root.join("failed.jsonl");
        let healthy = root.join("healthy.jsonl");
        fs::write(
            &failed,
            format!(
                "{}\n",
                json!({
                    "type":"user",
                    "session_id":"failed-session",
                    "id":"failed-turn-1"
                })
            ),
        )?;
        fs::write(
            &healthy,
            format!(
                "{}\n",
                json!({
                    "type":"assistant",
                    "session_id":"healthy-session",
                    "id":"healthy-turn-1"
                })
            ),
        )?;

        let mut session = ClaudeObserverSession::new_with_limits(root.clone(), 8, 8)?;
        session.transcripts = vec![
            super::TrackedTranscript::new(failed.clone()),
            super::TrackedTranscript::new(healthy.clone()),
        ];

        let historical = session.collect_capability_events()?;
        assert_eq!(historical.len(), 2);

        fs::remove_file(&failed)?;
        {
            let mut file = fs::OpenOptions::new().append(true).open(&healthy)?;
            writeln!(
                file,
                "{}",
                json!({
                    "type":"assistant",
                    "session_id":"healthy-session",
                    "id":"healthy-turn-2",
                    "message":{"content":"still streaming"}
                })
            )?;
            file.flush()?;
        }

        let degraded = session
            .next_event(Duration::from_millis(200))?
            .expect("expected structured unobservable signal");
        assert_eq!(
            degraded
                .raw_json
                .get("reason")
                .and_then(|value| value.as_str()),
            Some("transcript_unavailable")
        );

        let followed = session
            .next_event(Duration::from_millis(200))?
            .expect("healthy transcript should keep streaming");
        assert_eq!(followed.thread_id.as_deref(), Some("healthy-session"));
        assert_eq!(followed.turn_id.as_deref(), Some("healthy-turn-2"));
        assert_eq!(session.next_event(Duration::from_millis(50))?, None);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn run_claude_observer_emits_structured_unobservable_output_when_no_transcripts_exist()
    -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = crate::bootstrap(&workspace_root)?;
        let transcript_root = workspace_root.join("transcripts");
        fs::create_dir_all(&transcript_root)?;

        let mut output = Vec::new();
        run_claude_observer(
            &result.storage,
            &mut output,
            ClaudeObserverOptions {
                transcript_root: transcript_root.clone(),
                max_files: 8,
                max_events: 8,
                idle_timeout: Duration::from_millis(20),
                max_follow_events: 0,
            },
        )?;

        let json_lines: Vec<serde_json::Value> = String::from_utf8_lossy(&output)
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let unobservable = json_lines
            .iter()
            .find(|value| {
                value
                    .get("raw")
                    .and_then(|raw| raw.get("type"))
                    .and_then(|value| value.as_str())
                    == Some("claude_observer_unobservable")
            })
            .expect("expected structured unobservable output");
        assert_eq!(
            unobservable
                .get("raw")
                .and_then(|raw| raw.get("reason"))
                .and_then(|value| value.as_str()),
            Some("no_transcripts")
        );

        fs::remove_dir_all(result.config.state_root)?;
        fs::remove_dir_all(transcript_root)?;
        Ok(())
    }

    #[test]
    fn run_claude_observer_emits_structured_unobservable_output_when_root_is_missing()
    -> io::Result<()> {
        let workspace_root = unique_test_dir();
        let result = crate::bootstrap(&workspace_root)?;
        let transcript_root = workspace_root.join("missing-transcripts");

        let mut output = Vec::new();
        run_claude_observer(
            &result.storage,
            &mut output,
            ClaudeObserverOptions {
                transcript_root: transcript_root.clone(),
                max_files: 8,
                max_events: 8,
                idle_timeout: Duration::from_millis(20),
                max_follow_events: 0,
            },
        )?;

        let json_lines: Vec<serde_json::Value> = String::from_utf8_lossy(&output)
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let unobservable = json_lines
            .iter()
            .find(|value| {
                value
                    .get("raw")
                    .and_then(|raw| raw.get("reason"))
                    .and_then(|value| value.as_str())
                    == Some("transcript_root_unavailable")
            })
            .expect("expected structured root-unavailable output");
        assert_eq!(
            unobservable
                .get("raw")
                .and_then(|raw| raw.get("io_error_kind"))
                .and_then(|value| value.as_str()),
            Some("not_found")
        );

        fs::remove_dir_all(result.config.state_root)?;
        Ok(())
    }

    fn unique_test_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "prismtrace-claude-observer-test-{}-{}-{}",
            process::id(),
            nanos,
            counter
        ))
    }
}
