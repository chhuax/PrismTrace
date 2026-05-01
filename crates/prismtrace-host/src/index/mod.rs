mod read_store;
mod write_store;

use crate::observability_read_model::SessionDetail;
use prismtrace_analysis::CapabilityProjection;
use prismtrace_index::CapabilityIndexEntry as StorageCapabilityIndexEntry;
use prismtrace_storage::StorageLayout;
use std::io;
use std::path::Path;

pub(crate) use read_store::IndexReadStore;
use write_store::{IndexWritePlan, IndexWriteStore};

pub(crate) struct ObservabilityIndexStore;

impl ObservabilityIndexStore {
    pub(crate) fn load_read_store(
        storage: &StorageLayout,
        workspace_root: Option<&Path>,
    ) -> io::Result<IndexReadStore> {
        IndexReadStore::load(storage, workspace_root)
    }

    pub(crate) fn prepare_write(
        storage: &StorageLayout,
        sessions: &[SessionDetail],
    ) -> io::Result<IndexWritePlan> {
        IndexWriteStore::prepare(storage, sessions)
    }

    pub(crate) fn persist_changed_projection(
        storage: &StorageLayout,
        sessions: &[SessionDetail],
        capabilities: &[CapabilityProjection],
        plan: &IndexWritePlan,
    ) -> io::Result<()> {
        IndexWriteStore::persist_changed_projection(storage, sessions, capabilities, plan)
    }

    pub(crate) fn capability_index_entry(
        capability: &CapabilityProjection,
    ) -> StorageCapabilityIndexEntry {
        IndexWriteStore::capability_index_entry(capability)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn host_callers_depend_on_index_facade_not_split_read_write_stores() {
        let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let console_api =
            fs::read_to_string(src_dir.join("console/api.rs")).expect("console api should exist");
        let read_model = fs::read_to_string(src_dir.join("observability_read_model.rs"))
            .expect("read model should exist");
        let direct_read_store = ["Index", "ReadStore::"].concat();
        let direct_write_store = ["Index", "WriteStore::"].concat();
        let facade_store = ["Observability", "IndexStore"].concat();

        assert!(
            !console_api.contains(&direct_read_store),
            "console API should use the index facade"
        );
        assert!(
            !read_model.contains(&direct_write_store),
            "read model should use the index facade"
        );
        assert!(
            console_api.contains(&facade_store) && read_model.contains(&facade_store),
            "host callers should depend on the index facade"
        );
    }
}
