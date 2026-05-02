# observability-read-model Specification

## Purpose
TBD - created by archiving change split-observability-read-model. Update Purpose after archive.
## Requirements
### Requirement: 系统必须提供 observer-first read model
PrismTrace MUST provide a read model for observer-first console/API consumption so that the console does not parse source artifacts directly.

#### Scenario: Console lists sessions through the read model
- **WHEN** the local console handles `/api/sessions`
- **THEN** it queries session summaries from the observability read model
- **AND** it does not directly scan observer artifact or Codex transcript files from the route handler

#### Scenario: Console loads a session detail through the read model
- **WHEN** the local console handles `/api/sessions/{session_id}`
- **THEN** it queries the session detail from the observability read model
- **AND** the returned timeline items preserve the fields required by the existing Web console

### Requirement: Reader and projector responsibilities must be separated
PrismTrace MUST separate source file reading from console projection so source-specific parsing can evolve without being coupled to HTML or API payload rendering.

#### Scenario: Observer artifacts are read without console rendering
- **WHEN** PrismTrace reads `state/artifacts/observer_events/**.jsonl`
- **THEN** the reader returns structured records with source metadata, timestamps, artifact references, and raw JSON
- **AND** the reader does not render console JSON payloads directly

#### Scenario: Codex rollout transcripts are read without console rendering
- **WHEN** PrismTrace reads `~/.codex/sessions/**.jsonl`
- **THEN** the transcript reader returns structured records with thread/session metadata, event references, and raw JSON
- **AND** archived sessions under `archived_sessions` are excluded

### Requirement: 系统必须建立 session/event index
PrismTrace MUST build a queryable session/event index from local artifacts so repeated API requests do not require each route to rediscover every source file independently.

#### Scenario: Session index supports ordered summaries
- **WHEN** the read model builds a session index
- **THEN** sessions are ordered by latest completion/update time descending
- **AND** the query supports limiting the number of returned sessions

#### Scenario: Event index supports detail lookup
- **WHEN** the console asks for an event or request detail by id
- **THEN** the event index resolves the id to its source kind, session id, artifact path, and line/reference position
- **AND** the read model returns the detail without scanning unrelated sessions

### Requirement: Console API routes must remain compatible during migration
PrismTrace MUST preserve the existing local console route shape during this refactor so the Web console can migrate without a full frontend rewrite.

#### Scenario: Existing session routes remain available
- **WHEN** the Web console requests `/api/sessions` or `/api/sessions/{session_id}`
- **THEN** the route remains available
- **AND** the JSON contains the existing fields needed by the current frontend

#### Scenario: Existing request detail route remains available
- **WHEN** the Web console requests `/api/requests/{request_id}`
- **THEN** the route remains available
- **AND** observer event and Codex rollout event details remain inspectable

### Requirement: Damaged artifacts must degrade gracefully
PrismTrace MUST tolerate malformed or partially unreadable local artifacts so one damaged file does not make the entire console unusable.

#### Scenario: One artifact line is malformed
- **WHEN** a reader encounters one malformed JSONL line
- **THEN** it skips or records that line as a read error
- **AND** it continues indexing other readable events

#### Scenario: Detail id is missing
- **WHEN** the console asks for a session/event id that is not present in the index
- **THEN** the API returns a clear not-found response instead of falling back to an expensive full scan

