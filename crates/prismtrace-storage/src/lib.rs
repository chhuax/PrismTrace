use std::fs;
use std::io;
use std::path::PathBuf;

pub use prismtrace_index::{
    ArtifactRef, CapabilityIndexEntry, EventIndexEntry, ObservabilityIndex,
    ObservabilityIndexManifest, SessionIndexEntry, SourceIndexManifestEntry,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    pub root: PathBuf,
    pub state_dir: PathBuf,
    pub db_path: PathBuf,
    pub artifacts_dir: PathBuf,
    pub index_dir: PathBuf,
    pub index_manifest_path: PathBuf,
    pub sessions_index_path: PathBuf,
    pub events_index_path: PathBuf,
    pub capabilities_index_path: PathBuf,
    pub tmp_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl StorageLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let state_dir = root.join("state");

        let index_dir = state_dir.join("index");

        Self {
            root,
            db_path: state_dir.join("observability.db"),
            artifacts_dir: state_dir.join("artifacts"),
            index_manifest_path: index_dir.join("manifest.json"),
            sessions_index_path: index_dir.join("sessions.jsonl"),
            events_index_path: index_dir.join("events.jsonl"),
            capabilities_index_path: index_dir.join("capabilities.jsonl"),
            index_dir,
            tmp_dir: state_dir.join("tmp"),
            logs_dir: state_dir.join("logs"),
            state_dir,
        }
    }

    pub fn initialize(&self) -> io::Result<()> {
        fs::create_dir_all(&self.artifacts_dir)?;
        fs::create_dir_all(&self.index_dir)?;
        fs::create_dir_all(&self.tmp_dir)?;
        fs::create_dir_all(&self.logs_dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::StorageLayout;
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static UNIQUE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn layout_uses_expected_state_paths() {
        let layout = StorageLayout::new("/tmp/prismtrace");

        assert_eq!(layout.root, PathBuf::from("/tmp/prismtrace"));
        assert_eq!(layout.state_dir, PathBuf::from("/tmp/prismtrace/state"));
        assert_eq!(
            layout.db_path,
            PathBuf::from("/tmp/prismtrace/state/observability.db")
        );
        assert_eq!(
            layout.artifacts_dir,
            PathBuf::from("/tmp/prismtrace/state/artifacts")
        );
        assert_eq!(
            layout.index_dir,
            PathBuf::from("/tmp/prismtrace/state/index")
        );
        assert_eq!(
            layout.index_manifest_path,
            PathBuf::from("/tmp/prismtrace/state/index/manifest.json")
        );
        assert_eq!(
            layout.sessions_index_path,
            PathBuf::from("/tmp/prismtrace/state/index/sessions.jsonl")
        );
        assert_eq!(
            layout.events_index_path,
            PathBuf::from("/tmp/prismtrace/state/index/events.jsonl")
        );
        assert_eq!(
            layout.capabilities_index_path,
            PathBuf::from("/tmp/prismtrace/state/index/capabilities.jsonl")
        );
        assert_eq!(layout.tmp_dir, PathBuf::from("/tmp/prismtrace/state/tmp"));
        assert_eq!(layout.logs_dir, PathBuf::from("/tmp/prismtrace/state/logs"));
    }

    #[test]
    fn initialize_creates_the_state_directory_tree() -> io::Result<()> {
        let root = unique_test_dir();
        let layout = StorageLayout::new(&root);

        layout.initialize()?;

        assert!(layout.state_dir.is_dir());
        assert!(layout.artifacts_dir.is_dir());
        assert!(layout.index_dir.is_dir());
        assert!(layout.tmp_dir.is_dir());
        assert!(layout.logs_dir.is_dir());
        assert!(!layout.db_path.exists());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn index_projection_types_live_in_prismtrace_index_crate() -> io::Result<()> {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("storage crate should have a parent")
            .parent()
            .expect("crates directory should have a parent")
            .to_path_buf();
        let index_lib = fs::read_to_string(workspace.join("crates/prismtrace-index/src/lib.rs"))?;
        let storage_lib =
            fs::read_to_string(workspace.join("crates/prismtrace-storage/src/lib.rs"))?;
        let storage_production_lib = storage_lib
            .split("#[cfg(test)]")
            .next()
            .expect("storage production module should be present");

        assert!(index_lib.contains("pub struct ObservabilityIndex"));
        assert!(!storage_production_lib.contains(concat!("pub struct ", "ObservabilityIndex")));

        Ok(())
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let seq = UNIQUE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);

        std::env::temp_dir().join(format!(
            "prismtrace-storage-test-{}-{}-{}",
            process::id(),
            nanos,
            seq
        ))
    }
}
