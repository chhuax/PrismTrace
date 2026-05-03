use crate::observability_read_model::{
    EventDetail, EventSummary, SessionDetail, SessionSummary, load_event_detail,
    read_codex_rollout_session, read_observer_artifact_session,
};
use prismtrace_analysis::{CapabilityProjection, CapabilityRawRef};
use prismtrace_index::{
    CapabilityIndexEntry as StorageCapabilityIndexEntry, ObservabilityIndex,
    SessionIndexEntry as StorageSessionIndexEntry,
};
use prismtrace_storage::StorageLayout;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) struct IndexReadStore {
    index: ObservabilityIndex,
    workspace_root: Option<PathBuf>,
}

impl IndexReadStore {
    pub(crate) fn load(storage: &StorageLayout, workspace_root: Option<&Path>) -> io::Result<Self> {
        if !storage.sessions_index_path.exists()
            || !storage.events_index_path.exists()
            || !storage.capabilities_index_path.exists()
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "observability index projection files are missing",
            ));
        }

        Ok(Self {
            index: ObservabilityIndex::load_jsonl(
                &storage.sessions_index_path,
                &storage.events_index_path,
                &storage.capabilities_index_path,
            )?,
            workspace_root: workspace_root.map(Path::to_path_buf),
        })
    }

    pub(crate) fn session_summaries(&self, limit: usize) -> io::Result<Vec<SessionSummary>> {
        let mut summaries = Vec::new();
        for entry in self.index.session_summaries(limit) {
            if let Some(session) = self.session_detail_for_entry(&entry)? {
                summaries.push(session.summary);
            }
        }
        Ok(summaries)
    }

    pub(crate) fn event_summaries(&self, limit: usize) -> io::Result<Vec<EventSummary>> {
        let mut events = Vec::new();
        for session in self.index.session_summaries(usize::MAX) {
            let Some(session) = self.session_detail_for_entry(&session)? else {
                continue;
            };
            events.extend(session.events);
        }
        events.sort_by(|left, right| {
            right
                .occurred_at_ms
                .cmp(&left.occurred_at_ms)
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        events.truncate(limit);
        Ok(events)
    }

    pub(crate) fn session_detail(&self, session_id: &str) -> io::Result<Option<SessionDetail>> {
        let Some(entry) = self
            .index
            .session_summaries(usize::MAX)
            .into_iter()
            .find(|entry| entry.session_id == session_id)
        else {
            return Ok(None);
        };
        self.session_detail_for_entry(&entry)
    }

    pub(crate) fn event_detail(&self, event_id: &str) -> io::Result<Option<EventDetail>> {
        let Some(entry) = self.index.event_detail(event_id) else {
            return Ok(None);
        };
        let Some(session) = self.session_detail(&entry.session_id)? else {
            return Ok(None);
        };
        Ok(session
            .events
            .iter()
            .find(|event| event.event_id == event_id)
            .and_then(|event| load_event_detail(&session, event)))
    }

    pub(crate) fn session_capabilities(&self, session_id: &str) -> Vec<CapabilityProjection> {
        self.index
            .session_capabilities(session_id)
            .into_iter()
            .map(capability_projection_from_index_entry)
            .collect()
    }

    fn session_detail_for_entry(
        &self,
        entry: &StorageSessionIndexEntry,
    ) -> io::Result<Option<SessionDetail>> {
        match entry.source_kind.as_str() {
            "observer_event" => {
                let channel_dir = observer_channel_dir_from_session_id(&entry.session_id)
                    .or_else(|| observer_channel_dir_from_artifact_path(&entry.artifact.path))
                    .unwrap_or("observer");
                read_observer_artifact_session(&entry.artifact.path, channel_dir)
            }
            "codex_rollout" => {
                read_codex_rollout_session(&entry.artifact.path, self.workspace_root.as_deref())
            }
            _ => Ok(None),
        }
    }
}

fn capability_projection_from_index_entry(
    entry: StorageCapabilityIndexEntry,
) -> CapabilityProjection {
    CapabilityProjection {
        capability_id: entry.capability_id,
        session_id: entry.session_id,
        event_id: entry.event_id,
        source_kind: entry.source_kind,
        capability_type: entry.capability_type,
        capability_name: entry.capability_name,
        visibility_stage: entry.visibility_stage,
        observed_at_ms: entry.observed_at_ms,
        raw_ref: CapabilityRawRef {
            path: entry.artifact.path,
            line_index: entry.artifact.line_index,
        },
    }
}

fn observer_channel_dir_from_session_id(session_id: &str) -> Option<&str> {
    let rest = session_id.strip_prefix("observer:")?;
    rest.split(':').next().filter(|channel| !channel.is_empty())
}

fn observer_channel_dir_from_artifact_path(path: &Path) -> Option<&str> {
    path.parent()?.file_name()?.to_str()
}
