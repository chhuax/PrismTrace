use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    pub root: PathBuf,
    pub state_dir: PathBuf,
    pub db_path: PathBuf,
    pub artifacts_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl StorageLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let state_dir = root.join("state");

        Self {
            root,
            db_path: state_dir.join("observability.db"),
            artifacts_dir: state_dir.join("artifacts"),
            tmp_dir: state_dir.join("tmp"),
            logs_dir: state_dir.join("logs"),
            state_dir,
        }
    }

    pub fn initialize(&self) -> io::Result<()> {
        fs::create_dir_all(&self.artifacts_dir)?;
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
        assert!(layout.tmp_dir.is_dir());
        assert!(layout.logs_dir.is_dir());
        assert!(!layout.db_path.exists());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn unique_test_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!(
            "prismtrace-storage-test-{}-{}",
            process::id(),
            nanos
        ))
    }
}
