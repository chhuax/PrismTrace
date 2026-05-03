# observability-read-model Delta

## Modified Requirements

### Requirement: 系统必须提供 observer-first read model
PrismTrace MUST provide a read model for observer-first console/API consumption so that the console does not parse source artifacts directly.

#### Scenario: Console lists local-machine sessions through the read model
- **WHEN** the local console handles `/api/sessions`
- **THEN** it queries session summaries from the user-level local-machine state and read model
- **AND** session `cwd` is treated as metadata, not as the default data boundary

### Requirement: 系统必须建立 session/event index
PrismTrace MUST build a queryable session/event index from local artifacts so repeated API requests do not require each route to rediscover every source file independently.

#### Scenario: Index rebuild includes local-machine Codex sessions by default
- **WHEN** the read model rebuilds the session index without an explicit project filter
- **THEN** unarchived interactive Codex sessions from the current user are eligible for indexing regardless of their `cwd`
- **AND** each session preserves its `cwd` field for display and later filtering
