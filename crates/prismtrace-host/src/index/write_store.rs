use crate::observability_read_model::SessionDetail;
use prismtrace_analysis::CapabilityProjection;
use prismtrace_index::{
    ArtifactRef as StorageArtifactRef, CapabilityIndexEntry as StorageCapabilityIndexEntry,
    EventIndexEntry as StorageEventIndexEntry, ObservabilityIndex, ObservabilityIndexManifest,
    SessionIndexEntry as StorageSessionIndexEntry, SourceIndexManifestEntry,
};
use prismtrace_storage::StorageLayout;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct IndexWritePlan {
    source_keys: Vec<(PathBuf, String)>,
    changed_source_keys: HashSet<(PathBuf, String)>,
}

pub(crate) struct IndexWriteStore;

impl IndexWriteStore {
    pub(crate) fn prepare(
        storage: &StorageLayout,
        sessions: &[SessionDetail],
    ) -> io::Result<IndexWritePlan> {
        let source_keys = collect_source_keys(sessions);
        let mut changed_source_keys = persist_index_manifest(storage, sessions)?;
        if !storage.sessions_index_path.exists()
            || !storage.events_index_path.exists()
            || !storage.capabilities_index_path.exists()
        {
            changed_source_keys.extend(source_keys.iter().cloned());
        }

        Ok(IndexWritePlan {
            source_keys,
            changed_source_keys,
        })
    }

    pub(crate) fn persist_changed_projection(
        storage: &StorageLayout,
        sessions: &[SessionDetail],
        capabilities: &[CapabilityProjection],
        plan: &IndexWritePlan,
    ) -> io::Result<()> {
        let mut persisted_index = ObservabilityIndex::load_jsonl(
            &storage.sessions_index_path,
            &storage.events_index_path,
            &storage.capabilities_index_path,
        )?;
        persisted_index.retain_source_projections(&plan.source_keys);

        for (source_path, source_kind) in &plan.changed_source_keys {
            let mut session_entries = Vec::new();
            let mut event_entries = Vec::new();
            let capability_entries = capabilities
                .iter()
                .filter(|capability| {
                    capability.raw_ref.path == *source_path
                        && capability.source_kind == *source_kind
                })
                .map(Self::capability_index_entry)
                .collect::<Vec<_>>();
            for session in sessions.iter().filter(|session| {
                session.summary.artifact.path == *source_path
                    && session.summary.source.kind == *source_kind
            }) {
                session_entries.push(StorageSessionIndexEntry {
                    session_id: session.summary.session_id.clone(),
                    source_kind: session.summary.source.kind.clone(),
                    updated_at_ms: session.summary.completed_at_ms,
                    artifact: StorageArtifactRef {
                        path: session.summary.artifact.path.clone(),
                        line_index: session.summary.artifact.line_index,
                    },
                });
                for event in &session.events {
                    event_entries.push(StorageEventIndexEntry {
                        event_id: event.event_id.clone(),
                        session_id: session.summary.session_id.clone(),
                        source_kind: event.source.kind.clone(),
                        occurred_at_ms: event.occurred_at_ms,
                        artifact: StorageArtifactRef {
                            path: event.artifact.path.clone(),
                            line_index: event.artifact.line_index,
                        },
                    });
                }
            }
            persisted_index.replace_source_projection(
                source_path,
                source_kind,
                session_entries,
                event_entries,
                capability_entries,
            );
        }

        persisted_index.save_jsonl(
            &storage.sessions_index_path,
            &storage.events_index_path,
            &storage.capabilities_index_path,
        )
    }

    pub(crate) fn capability_index_entry(
        capability: &CapabilityProjection,
    ) -> StorageCapabilityIndexEntry {
        StorageCapabilityIndexEntry {
            capability_id: capability.capability_id.clone(),
            session_id: capability.session_id.clone(),
            event_id: capability.event_id.clone(),
            source_kind: capability.source_kind.clone(),
            capability_type: capability.capability_type.clone(),
            capability_name: capability.capability_name.clone(),
            visibility_stage: capability.visibility_stage.clone(),
            observed_at_ms: capability.observed_at_ms,
            artifact: StorageArtifactRef {
                path: capability.raw_ref.path.clone(),
                line_index: capability.raw_ref.line_index,
            },
        }
    }
}

fn persist_index_manifest(
    storage: &StorageLayout,
    sessions: &[SessionDetail],
) -> io::Result<HashSet<(PathBuf, String)>> {
    let indexed_at_ms = current_time_ms();
    let previous_manifest = ObservabilityIndexManifest::load(&storage.index_manifest_path)?;
    let mut manifest = ObservabilityIndexManifest::default();
    let mut seen_sources = HashSet::new();
    let mut changed_sources = HashSet::new();

    for session in sessions {
        let source_path = session.summary.artifact.path.clone();
        let source_kind = session.summary.source.kind.clone();
        if !seen_sources.insert((source_path.clone(), source_kind.clone())) {
            continue;
        }
        let entry = if let Some(reusable_entry) =
            previous_manifest.reusable_source(&source_path, &source_kind)?
        {
            reusable_entry
        } else {
            changed_sources.insert((source_path.clone(), source_kind.clone()));
            SourceIndexManifestEntry::from_file(source_path, source_kind, indexed_at_ms)?
        };
        manifest.upsert_source(entry);
    }

    manifest.save(&storage.index_manifest_path)?;
    Ok(changed_sources)
}

fn collect_source_keys(sessions: &[SessionDetail]) -> Vec<(PathBuf, String)> {
    let mut source_keys = Vec::new();
    for session in sessions {
        let source_key = (
            session.summary.artifact.path.clone(),
            session.summary.source.kind.clone(),
        );
        if !source_keys.contains(&source_key) {
            source_keys.push(source_key);
        }
    }
    source_keys
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
