use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ArtifactRef {
    pub path: PathBuf,
    pub line_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SessionIndexEntry {
    pub session_id: String,
    pub source_kind: String,
    pub updated_at_ms: u64,
    pub artifact: ArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct EventIndexEntry {
    pub event_id: String,
    pub session_id: String,
    pub source_kind: String,
    pub occurred_at_ms: u64,
    pub artifact: ArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CapabilityIndexEntry {
    pub capability_id: String,
    pub session_id: String,
    pub event_id: String,
    pub source_kind: String,
    pub capability_type: String,
    pub capability_name: String,
    pub visibility_stage: String,
    pub observed_at_ms: u64,
    pub artifact: ArtifactRef,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservabilityIndex {
    sessions: Vec<SessionIndexEntry>,
    events: Vec<EventIndexEntry>,
    capabilities: Vec<CapabilityIndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SourceIndexManifestEntry {
    pub source_path: PathBuf,
    pub source_kind: String,
    pub size_bytes: u64,
    pub mtime_ms: u64,
    pub indexed_at_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ObservabilityIndexManifest {
    pub sources: Vec<SourceIndexManifestEntry>,
}

impl SourceIndexManifestEntry {
    pub fn from_file(
        source_path: impl Into<PathBuf>,
        source_kind: impl Into<String>,
        indexed_at_ms: u64,
    ) -> io::Result<Self> {
        let source_path = source_path.into();
        let metadata = fs::metadata(&source_path)?;
        let mtime_ms = metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .as_millis() as u64;

        Ok(Self {
            source_path,
            source_kind: source_kind.into(),
            size_bytes: metadata.len(),
            mtime_ms,
            indexed_at_ms,
        })
    }

    pub fn matches_current_file(&self) -> io::Result<bool> {
        let current = Self::from_file(&self.source_path, self.source_kind.clone(), 0)?;
        Ok(self.size_bytes == current.size_bytes && self.mtime_ms == current.mtime_ms)
    }
}

impl ObservabilityIndexManifest {
    pub fn load(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self::default());
        }
        let file = fs::File::open(path)?;
        serde_json::from_reader(file)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn save(&self, path: impl Into<PathBuf>) -> io::Result<()> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(path)?;
        serde_json::to_writer_pretty(file, self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn upsert_source(&mut self, entry: SourceIndexManifestEntry) {
        if let Some(existing) = self.sources.iter_mut().find(|source| {
            source.source_path == entry.source_path && source.source_kind == entry.source_kind
        }) {
            *existing = entry;
            return;
        }
        self.sources.push(entry);
    }

    pub fn reusable_source(
        &self,
        source_path: impl Into<PathBuf>,
        source_kind: &str,
    ) -> io::Result<Option<SourceIndexManifestEntry>> {
        let source_path = source_path.into();
        let Some(entry) = self
            .sources
            .iter()
            .find(|entry| entry.source_path == source_path && entry.source_kind == source_kind)
        else {
            return Ok(None);
        };

        entry
            .matches_current_file()
            .map(|matches| matches.then(|| entry.clone()))
    }
}

impl ObservabilityIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_session(&mut self, entry: SessionIndexEntry) {
        self.sessions.push(entry);
        self.sessions.sort_by(|left, right| {
            right
                .updated_at_ms
                .cmp(&left.updated_at_ms)
                .then_with(|| left.session_id.cmp(&right.session_id))
        });
    }

    pub fn insert_event(&mut self, entry: EventIndexEntry) {
        self.events.push(entry);
    }

    pub fn insert_capability(&mut self, entry: CapabilityIndexEntry) {
        self.capabilities.push(entry);
        self.capabilities.sort_by(|left, right| {
            left.observed_at_ms
                .cmp(&right.observed_at_ms)
                .then_with(|| left.capability_type.cmp(&right.capability_type))
                .then_with(|| left.capability_name.cmp(&right.capability_name))
                .then_with(|| left.capability_id.cmp(&right.capability_id))
        });
    }

    pub fn session_summaries(&self, limit: usize) -> Vec<SessionIndexEntry> {
        self.sessions.iter().take(limit).cloned().collect()
    }

    pub fn event_detail(&self, event_id: &str) -> Option<EventIndexEntry> {
        self.events
            .iter()
            .find(|event| event.event_id == event_id)
            .cloned()
    }

    pub fn session_capabilities(&self, session_id: &str) -> Vec<CapabilityIndexEntry> {
        self.capabilities
            .iter()
            .filter(|capability| capability.session_id == session_id)
            .cloned()
            .collect()
    }

    pub fn replace_source_projection(
        &mut self,
        source_path: impl Into<PathBuf>,
        source_kind: &str,
        sessions: Vec<SessionIndexEntry>,
        events: Vec<EventIndexEntry>,
        capabilities: Vec<CapabilityIndexEntry>,
    ) {
        let source_path = source_path.into();
        self.sessions.retain(|session| {
            !(session.source_kind == source_kind && session.artifact.path == source_path)
        });
        self.events.retain(|event| {
            !(event.source_kind == source_kind && event.artifact.path == source_path)
        });
        self.capabilities.retain(|capability| {
            !(capability.source_kind == source_kind && capability.artifact.path == source_path)
        });
        for session in sessions {
            self.insert_session(session);
        }
        for event in events {
            self.insert_event(event);
        }
        for capability in capabilities {
            self.insert_capability(capability);
        }
    }

    pub fn retain_source_projections(&mut self, source_keys: &[(PathBuf, String)]) {
        self.sessions.retain(|session| {
            source_keys
                .iter()
                .any(|(path, kind)| session.artifact.path == *path && session.source_kind == *kind)
        });
        self.events.retain(|event| {
            source_keys
                .iter()
                .any(|(path, kind)| event.artifact.path == *path && event.source_kind == *kind)
        });
        self.capabilities.retain(|capability| {
            source_keys.iter().any(|(path, kind)| {
                capability.artifact.path == *path && capability.source_kind == *kind
            })
        });
    }

    pub fn save_jsonl(
        &self,
        sessions_path: impl Into<PathBuf>,
        events_path: impl Into<PathBuf>,
        capabilities_path: impl Into<PathBuf>,
    ) -> io::Result<()> {
        write_jsonl(sessions_path, &self.sessions)?;
        write_jsonl(events_path, &self.events)?;
        write_jsonl(capabilities_path, &self.capabilities)
    }

    pub fn load_jsonl(
        sessions_path: impl Into<PathBuf>,
        events_path: impl Into<PathBuf>,
        capabilities_path: impl Into<PathBuf>,
    ) -> io::Result<Self> {
        let mut index = Self::new();
        for session in read_jsonl::<SessionIndexEntry>(sessions_path)? {
            index.insert_session(session);
        }
        for event in read_jsonl::<EventIndexEntry>(events_path)? {
            index.insert_event(event);
        }
        for capability in read_jsonl::<CapabilityIndexEntry>(capabilities_path)? {
            index.insert_capability(capability);
        }
        Ok(index)
    }
}

fn write_jsonl<T: serde::Serialize>(path: impl Into<PathBuf>, items: &[T]) -> io::Result<()> {
    let path = path.into();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = String::new();
    for item in items {
        let line = serde_json::to_string(item)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        text.push_str(&line);
        text.push('\n');
    }
    fs::write(path, text)
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: impl Into<PathBuf>) -> io::Result<Vec<T>> {
    let path = path.into();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactRef, CapabilityIndexEntry, EventIndexEntry, ObservabilityIndex,
        ObservabilityIndexManifest, SessionIndexEntry, SourceIndexManifestEntry,
    };
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static UNIQUE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn observability_index_projection_round_trips_as_jsonl() -> io::Result<()> {
        let root = unique_test_dir();
        let sessions_path = root.join("index/sessions.jsonl");
        let events_path = root.join("index/events.jsonl");
        let capabilities_path = root.join("index/capabilities.jsonl");
        let mut index = ObservabilityIndex::new();
        index.insert_session(SessionIndexEntry {
            session_id: "session-1".into(),
            source_kind: "observer_event".into(),
            updated_at_ms: 20,
            artifact: ArtifactRef {
                path: root.join("session.jsonl"),
                line_index: None,
            },
        });
        index.insert_event(EventIndexEntry {
            event_id: "event-1".into(),
            session_id: "session-1".into(),
            source_kind: "observer_event".into(),
            occurred_at_ms: 21,
            artifact: ArtifactRef {
                path: root.join("session.jsonl"),
                line_index: Some(1),
            },
        });
        index.insert_capability(CapabilityIndexEntry {
            capability_id: "cap-1".into(),
            session_id: "session-1".into(),
            event_id: "event-1".into(),
            source_kind: "observer_event".into(),
            capability_type: "mcp".into(),
            capability_name: "github".into(),
            visibility_stage: "capability-snapshot".into(),
            observed_at_ms: 21,
            artifact: ArtifactRef {
                path: root.join("session.jsonl"),
                line_index: Some(1),
            },
        });

        index.save_jsonl(&sessions_path, &events_path, &capabilities_path)?;
        let loaded =
            ObservabilityIndex::load_jsonl(&sessions_path, &events_path, &capabilities_path)?;

        assert_eq!(loaded.session_summaries(10)[0].session_id, "session-1");
        assert_eq!(
            loaded
                .event_detail("event-1")
                .expect("event should round-trip")
                .artifact
                .line_index,
            Some(1)
        );
        assert_eq!(
            loaded.session_capabilities("session-1")[0].capability_name,
            "github"
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn observability_index_replaces_projection_for_one_source_only() {
        let source_a = PathBuf::from("/tmp/source-a.jsonl");
        let source_b = PathBuf::from("/tmp/source-b.jsonl");
        let mut index = ObservabilityIndex::new();
        index.insert_session(SessionIndexEntry {
            session_id: "old-a".into(),
            source_kind: "observer_event".into(),
            updated_at_ms: 10,
            artifact: ArtifactRef {
                path: source_a.clone(),
                line_index: None,
            },
        });
        index.insert_session(SessionIndexEntry {
            session_id: "keep-b".into(),
            source_kind: "observer_event".into(),
            updated_at_ms: 20,
            artifact: ArtifactRef {
                path: source_b.clone(),
                line_index: None,
            },
        });
        index.insert_event(EventIndexEntry {
            event_id: "old-a-event".into(),
            session_id: "old-a".into(),
            source_kind: "observer_event".into(),
            occurred_at_ms: 11,
            artifact: ArtifactRef {
                path: source_a.clone(),
                line_index: Some(1),
            },
        });
        index.insert_event(EventIndexEntry {
            event_id: "keep-b-event".into(),
            session_id: "keep-b".into(),
            source_kind: "observer_event".into(),
            occurred_at_ms: 21,
            artifact: ArtifactRef {
                path: source_b.clone(),
                line_index: Some(1),
            },
        });
        index.insert_capability(CapabilityIndexEntry {
            capability_id: "old-a-cap".into(),
            session_id: "old-a".into(),
            event_id: "old-a-event".into(),
            source_kind: "observer_event".into(),
            capability_type: "skill".into(),
            capability_name: "old-review".into(),
            visibility_stage: "capability-snapshot".into(),
            observed_at_ms: 11,
            artifact: ArtifactRef {
                path: source_a.clone(),
                line_index: Some(1),
            },
        });
        index.insert_capability(CapabilityIndexEntry {
            capability_id: "keep-b-cap".into(),
            session_id: "keep-b".into(),
            event_id: "keep-b-event".into(),
            source_kind: "observer_event".into(),
            capability_type: "mcp".into(),
            capability_name: "github".into(),
            visibility_stage: "capability-snapshot".into(),
            observed_at_ms: 21,
            artifact: ArtifactRef {
                path: source_b.clone(),
                line_index: Some(1),
            },
        });

        index.replace_source_projection(
            &source_a,
            "observer_event",
            vec![SessionIndexEntry {
                session_id: "new-a".into(),
                source_kind: "observer_event".into(),
                updated_at_ms: 30,
                artifact: ArtifactRef {
                    path: source_a.clone(),
                    line_index: None,
                },
            }],
            vec![EventIndexEntry {
                event_id: "new-a-event".into(),
                session_id: "new-a".into(),
                source_kind: "observer_event".into(),
                occurred_at_ms: 31,
                artifact: ArtifactRef {
                    path: source_a.clone(),
                    line_index: Some(2),
                },
            }],
            vec![CapabilityIndexEntry {
                capability_id: "new-a-cap".into(),
                session_id: "new-a".into(),
                event_id: "new-a-event".into(),
                source_kind: "observer_event".into(),
                capability_type: "skill".into(),
                capability_name: "review".into(),
                visibility_stage: "capability-snapshot".into(),
                observed_at_ms: 31,
                artifact: ArtifactRef {
                    path: source_a.clone(),
                    line_index: Some(2),
                },
            }],
        );

        let sessions = index.session_summaries(10);
        assert!(sessions.iter().any(|session| session.session_id == "new-a"));
        assert!(
            sessions
                .iter()
                .any(|session| session.session_id == "keep-b")
        );
        assert!(!sessions.iter().any(|session| session.session_id == "old-a"));
        assert!(index.event_detail("new-a-event").is_some());
        assert!(index.event_detail("keep-b-event").is_some());
        assert!(index.event_detail("old-a-event").is_none());
        assert_eq!(
            index.session_capabilities("new-a")[0].capability_name,
            "review"
        );
        assert_eq!(
            index.session_capabilities("keep-b")[0].capability_name,
            "github"
        );
        assert!(index.session_capabilities("old-a").is_empty());
    }

    #[test]
    fn index_manifest_detects_reusable_unchanged_source_file() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let source_path = root.join("session.jsonl");
        fs::write(&source_path, "{\"record_type\":\"event\"}\n")?;

        let mut manifest = ObservabilityIndexManifest::default();
        manifest.upsert_source(SourceIndexManifestEntry::from_file(
            &source_path,
            "observer_event",
            100,
        )?);

        let reusable = manifest.reusable_source(&source_path, "observer_event")?;
        assert!(reusable.is_some());
        assert_eq!(
            reusable.expect("source should be reusable").indexed_at_ms,
            100
        );

        fs::write(
            &source_path,
            "{\"record_type\":\"event\"}\n{\"record_type\":\"event\"}\n",
        )?;

        assert!(
            manifest
                .reusable_source(&source_path, "observer_event")?
                .is_none()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn index_manifest_round_trips_to_disk() -> io::Result<()> {
        let root = unique_test_dir();
        fs::create_dir_all(&root)?;
        let source_path = root.join("session.jsonl");
        let manifest_path = root.join("index/manifest.json");
        fs::write(&source_path, "{}\n")?;

        let mut manifest = ObservabilityIndexManifest::default();
        manifest.upsert_source(SourceIndexManifestEntry::from_file(
            &source_path,
            "codex_rollout",
            200,
        )?);
        manifest.save(&manifest_path)?;

        let loaded = ObservabilityIndexManifest::load(&manifest_path)?;

        assert_eq!(loaded.sources.len(), 1);
        assert_eq!(loaded.sources[0].source_path, source_path);
        assert_eq!(loaded.sources[0].source_kind, "codex_rollout");
        assert_eq!(loaded.sources[0].indexed_at_ms, 200);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn observability_index_orders_sessions_by_update_time() {
        let mut index = ObservabilityIndex::new();
        index.insert_session(SessionIndexEntry {
            session_id: "older".into(),
            source_kind: "observer_event".into(),
            updated_at_ms: 10,
            artifact: ArtifactRef {
                path: PathBuf::from("/tmp/older.jsonl"),
                line_index: None,
            },
        });
        index.insert_session(SessionIndexEntry {
            session_id: "newer".into(),
            source_kind: "codex_rollout".into(),
            updated_at_ms: 20,
            artifact: ArtifactRef {
                path: PathBuf::from("/tmp/newer.jsonl"),
                line_index: None,
            },
        });

        let sessions = index.session_summaries(1);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "newer");
    }

    #[test]
    fn observability_index_resolves_event_detail_reference() {
        let mut index = ObservabilityIndex::new();
        index.insert_event(EventIndexEntry {
            event_id: "event-1".into(),
            session_id: "session-1".into(),
            source_kind: "observer_event".into(),
            occurred_at_ms: 10,
            artifact: ArtifactRef {
                path: PathBuf::from("/tmp/session.jsonl"),
                line_index: Some(2),
            },
        });

        let event = index.event_detail("event-1").expect("event should resolve");

        assert_eq!(event.session_id, "session-1");
        assert_eq!(event.artifact.line_index, Some(2));
        assert!(index.event_detail("missing").is_none());
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let seq = UNIQUE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);

        std::env::temp_dir().join(format!(
            "prismtrace-index-test-{}-{}-{}",
            process::id(),
            nanos,
            seq
        ))
    }
}
