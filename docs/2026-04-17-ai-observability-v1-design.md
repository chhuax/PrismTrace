# AI Application Observability Tool - V1 Design

Date: 2026-04-17
Status: Draft

## 1. Product Definition

The project is defined as an AI application observability tool.

It is not just a prompt sniffer, not only an attach debugger, and not a single-purpose reverse-engineering utility for one AI CLI. The long-term direction is to build a product around observation, analysis, and explanation for AI applications.

The first step is narrower:

- collect the high-value facts we actually need
- do it without restarting the observed application
- preserve the current session instead of forcing a new one

The short version of phase 1 is:

`Capture the real model-facing information from running AI applications without interrupting the current session, so later analysis capabilities can be built on top of that fact layer.`

## 2. Product Roadmap Layers

The product should evolve in three layers:

### Layer 1: Information Collection

Answer: what can we see?

Goals:

- capture real request payloads
- capture visible tool or skill registration information when available
- capture response stream data and useful metadata
- avoid requiring SDK integration or application restart

### Layer 2: Session Reconstruction

Answer: what happened?

Goals:

- reconstruct a usable session timeline
- connect requests, responses, and local observable events
- support replay, comparison, and inspection of sessions

### Layer 3: Analysis and Explanation

Answer: why did it happen?

Goals:

- explain why a skill or tool did not trigger when possible
- explain prompt growth and context composition
- identify patterns such as fallback, filtering, repeated retries, or routing shifts

## 3. Phase 1 Scope

Phase 1 is only about collecting the right facts for future analysis.

We do not optimize for maximum capture breadth. We optimize for the information that is most valuable for analysis later.

### Core Data To Capture

#### Request Metadata

- timestamp
- provider
- model
- endpoint
- streaming or non-streaming mode
- latency metrics
- token or size estimates when available

#### Request Payload

- system prompt
- messages
- tools or functions definitions
- tool choice
- response format
- sampling parameters
- visible attachment descriptions or transformed attachment content metadata

#### Response Data

- first token time
- completion time
- stream chunk summary
- final assistant message
- tool calls
- finish reason
- usage data when available

#### Session Association

- process id
- application name
- command or window context when available
- session id or request grouping id

#### Local Tool Visibility

If visible from the local process, record:

- candidate tool or skill list
- final registered tool or skill list sent toward model invocation

### Optional Enhancements

- pre-assembly and post-assembly prompt snapshots
- local orchestration filter logs
- file reads and writes
- command execution events
- working directory changes
- user interaction events
- retries, fallback, and provider switching

### Explicit Non-Goals For V1

- complete explanation of all local decision paths
- guaranteed support for every Node-based application
- full multi-language runtime coverage
- perfect reconstruction of every multi-turn session tree

## 4. V1 User Experience

V1 should be a Rust backend with a local web UI.

The product should feel like a local observability console rather than a heavy desktop shell.

### User Flow

1. Launch the observability tool locally.
2. See a list of attachable target processes.
3. Choose a target and click Attach.
4. Start capturing new requests without interrupting the running session.
5. Inspect a session timeline in real time.
6. Open a request to inspect payload, response, tools, and metadata.

### Request Detail View

Each captured request should show:

- raw request payload
- response summary and stream information
- visible tools or skills
- metadata and timeline placement

### Why Local Web UI

- better for large payload inspection than a CLI
- faster to ship than a heavy Electron shell
- clean split between Rust systems backend and UI rendering
- easier to grow into filtering, diffing, and future analysis views

## 5. Technical Architecture

The system should be split into five layers with clear boundaries.

### 5.1 Attach Layer

Responsibilities:

- discover candidate processes
- attach to target process
- inject or activate probe
- manage connection lifecycle
- recover from detach or failure

### 5.2 Probe Layer

This runs closest to the target process.

Responsibilities:

- hook critical Node or Electron networking paths
- capture request payload before encryption and transmission
- capture response and stream chunks
- capture visible tool or skill information when possible

This layer should only collect facts. It should not own complex analysis.

### 5.3 Event Pipeline

Responsibilities:

- normalize raw probe events
- pair requests and responses
- associate events into sessions
- order events on a timeline
- apply truncation or redaction policy

Output:

- a stable observability event schema

### 5.4 Storage Layer

V1 storage can be simple and local.

Recommended starting point:

- SQLite for metadata, indexes, and session relationships
- SQLite or blob files for large payload bodies

Required capabilities:

- filter by app, time, provider, model, or session
- search payload content
- provide stable data access for future analysis modules

### 5.5 Analysis and UI Layer

V1 should start with browsing and search only, but the architecture must leave room for future analysis.

Planned future capabilities:

- prompt diff
- tool visibility diff
- failure pattern attribution
- skill non-trigger diagnostics
- token, latency, and cost analysis

### Architectural Rule

`Probe collects facts. Analysis explains facts.`

This boundary is important because it lets phase 1 stay focused on reliable collection without overcommitting on interpretation.

## 6. Current Product Boundary

Current agreed boundary:

- macOS only for the first version
- focus on Node or Electron based AI CLI and desktop applications
- do not require restarting the observed application
- start with payload visibility as the primary value
- leave room for later analysis and explanation features

## 7. Open Design Topics

The next major topic to design is:

- which concrete Node or Electron hook points should V1 target first

That decision will determine whether the first implementation is practical and how broad the initial compatibility can be.

## 8. Design Five: First Node and Electron Hook Targets

V1 should prioritize hook points that maximize payload visibility across Node-based AI applications while keeping the implementation realistic.

The goal is not to hook every abstraction layer. The goal is to capture model-facing payloads as reliably as possible in the smallest number of high-leverage places.

### 8.1 Hooking Strategy Principle

For V1, the best default rule is:

`Hook as high as possible to preserve semantic meaning, and as low as necessary to preserve compatibility.`

This means:

- prefer interception points that still expose structured request objects
- fall back to lower HTTP-layer interception only when higher-level SDK hooks are not present
- avoid starting at the TLS or socket layer because payload meaning becomes harder to reconstruct

### 8.2 Priority Order For V1

The first version should target hook points in this order:

1. global `fetch`
2. `undici`
3. Node `http` and `https`
4. selective SDK-level hooks where low-cost and high-value
5. optional Electron renderer and main-process bridging later

This order gives good coverage for modern Node and Electron AI tools without over-specializing too early.

### 8.3 Tier 1: Global Fetch

Many modern Node applications and Electron apps route model requests through `fetch`, either directly or through wrappers.

Why it matters:

- broad coverage
- request bodies are still semantically structured nearby
- lower implementation complexity than deeper transport hooks
- a single interception point can catch multiple providers

What to capture:

- request URL
- method
- headers after local assembly but before transmission
- request body before encryption
- response status and headers
- streamed response body chunks when present

Main limitation:

- if an application bypasses `fetch` entirely, this hook sees nothing

### 8.4 Tier 2: Undici

`undici` is an especially important target because many Node runtimes and modern libraries use it directly or indirectly.

Why it matters:

- strong coverage in current Node ecosystems
- closer to the actual request path than generic wrappers
- useful fallback when application code does not expose a simple global `fetch` path

Likely useful interception points:

- `fetch`
- `request`
- `stream`
- dispatcher-level request execution

What to capture:

- normalized request payload
- stream lifecycle events
- timing markers such as request start, first byte, and end

### 8.5 Tier 3: Node HTTP and HTTPS

This is the compatibility layer for tools that do not use `fetch` or `undici` in an easily hookable way.

Why it matters:

- broad fallback coverage
- catches older SDKs and hand-rolled clients
- increases the probability of provider visibility across heterogeneous tools

What to capture:

- hostname and path
- headers
- body writes before flush
- response chunks

Main tradeoff:

- semantics are weaker here than higher-level hooks
- reconstructing a clean JSON payload may require buffering and reassembly

### 8.6 Tier 4: Selective SDK Hooks

V1 should not depend on SDK-specific hooks, but selective support can add value when it is cheap.

Examples:

- OpenAI client wrappers
- Anthropic client wrappers
- internal provider abstractions in target applications

Why this is useful:

- preserves more intent-rich information
- may expose tool registration or pre-flight transformations
- may reveal request-building stages before final serialization

Why this is not the default foundation:

- brittle across versions
- high maintenance cost
- easy to overfit to one tool

Recommended V1 rule:

- use SDK hooks only as targeted enrichments after transport-level hooks already work

### 8.7 Electron-Specific Considerations

Electron apps may send model requests from:

- the main process
- a renderer process
- a Node-enabled preload bridge

V1 should assume payload generation may live in any of these places.

Recommended approach:

- focus first on the main process and any Node-capable runtime path
- treat renderer-only browser paths as secondary
- capture process identity so events can later be tied back to the correct app component

This means V1 should prioritize attach reliability over complete renderer introspection.

### 8.8 Capturing Tool and Skill Visibility

Prompt payload capture is the primary objective, but V1 should opportunistically capture local tool or skill visibility when it can do so cheaply.

Good candidate hook categories:

- functions that assemble tool definitions into a model request
- registries that produce callable tool descriptors
- wrapper functions that translate internal tools into provider-specific schema

What to record if visible:

- candidate tool count and names
- filtered tool count and names
- final serialized tool schema included in the request

Important boundary:

- V1 does not need to prove why a tool or skill was excluded
- V1 only needs to preserve enough evidence for future analysis

### 8.9 What V1 Should Not Start With

V1 should explicitly avoid starting at these layers:

- raw TLS interception
- packet capture as the primary method
- JavaScript source rewriting on disk
- per-app custom reverse engineering as the default path

These approaches may help later in specific cases, but they are not the right default foundation for a maintainable observability product.

### 8.10 Recommended V1 Hook Bundle

The recommended first implementation bundle is:

- primary: global `fetch`
- primary: `undici`
- fallback: `http` and `https`
- enrichment: request-body JSON reconstruction and response stream capture
- opportunistic enrichment: local tool or skill assembly visibility when naturally exposed

This bundle best matches the current product boundary:

- macOS only
- Node and Electron targets
- no restart requirement
- payload visibility first

## 9. Next Design Topic

The next design topic after hook targets should be:

- how attach and injection work on macOS for already-running Node and Electron processes

## 10. Design Six: Attach and Injection On macOS

V1 needs a practical way to attach to an already-running Node or Electron process on macOS, inject a probe, and stream captured events back to the local observability console without restarting the target.

This is the technical hinge of the whole product.

### 10.1 Design Goal

The attach system must satisfy these requirements:

- attach to an already-running target process
- avoid restarting the target application
- inject a probe into the live runtime
- capture future requests after attach
- stream events back to the Rust backend
- detach cleanly without crashing or permanently modifying the target

Important boundary:

- V1 only guarantees observation of activity that happens after attach
- V1 does not promise retroactive recovery of requests already sent before the probe was injected

### 10.2 Candidate Approaches

There are three broad implementation approaches.

#### Approach A: Build A Custom Native Injector

This means implementing the attach and injection machinery ourselves using lower-level macOS debugging or task control primitives.

Pros:

- maximum control
- no dependency on a third-party instrumentation runtime
- possible long-term performance and packaging advantages

Cons:

- highest implementation risk
- more fragile across macOS versions
- more work before first usable payload capture
- likely to pull the project into OS internals too early

V1 recommendation:

- do not start here

#### Approach B: Use An Existing Dynamic Instrumentation Runtime

This means relying on a mature runtime-instrumentation backend and building our product-specific probe and event pipeline on top.

Pros:

- fastest path to a usable prototype
- already aligned with live attach workflows
- lowers the risk of writing unstable injection code first

Cons:

- adds an external dependency to the architecture
- may impose limitations on packaging or supported environments
- may require adaptation for long-term product hardening

V1 recommendation:

- start here

#### Approach C: Debugger-Driven Attach

This means using debugger-style workflows to suspend, inspect, and possibly inject behavior into the running process.

Pros:

- conceptually available on macOS developer machines
- useful for diagnostics and experiments

Cons:

- not a good product foundation for continuous runtime observation
- higher operational friction
- harder to turn into a smooth always-on user experience

V1 recommendation:

- use only as a development aid, not as the core product path

### 10.3 Recommended Direction For V1

V1 should use an existing dynamic instrumentation runtime as the attach backend, while keeping the rest of the architecture under our control.

That means:

- Rust owns process discovery, probe lifecycle, event normalization, storage, and UI
- the instrumentation backend owns the low-level live attach and in-process execution
- the injected probe logic stays product-specific and focused on Node and Electron payload capture

This gives the project a practical first step:

- we validate that prompt payload capture is truly possible
- we learn which Node and Electron targets are compatible
- we avoid spending the first milestone building custom OS injection machinery

### 10.4 Attach Flow

The recommended attach flow for V1 is:

1. Discover candidate processes.
2. Identify likely Node or Electron targets.
3. Attempt live attach through the instrumentation backend.
4. Inject a lightweight bootstrap probe.
5. Bootstrap installs the selected hook bundle.
6. Probe opens a messaging channel back to the Rust host.
7. Captured events stream into the event pipeline.
8. User can detach, after which hooks are removed or the probe is disabled.

This keeps the attach pipeline explicit and debuggable.

### 10.5 Probe Bootstrap Responsibilities

The bootstrap running inside the target process should be very small.

Responsibilities:

- detect runtime shape
- detect whether `fetch`, `undici`, or `http/https` are available
- install the correct hook set
- avoid duplicate installation
- expose health and version information to the host
- stream events through a stable bridge

The bootstrap should not perform heavy analysis or local persistence.

### 10.6 Host And Probe Communication

The host and probe need a communication channel for:

- health state
- captured events
- hook installation status
- detach or shutdown commands

V1 should keep this simple:

- line-oriented or framed JSON messages are enough
- event ordering should be timestamped inside the probe and normalized by the host
- the bridge should tolerate bursty streaming output

The host should treat the probe as an untrusted event producer and validate message shape before storage.

### 10.7 Target Eligibility On macOS

Not every process on macOS will be attachable in practice.

V1 should assume:

- userland Node and Electron applications are the primary supported targets
- system-protected processes may be unavailable
- some applications may resist attach because of runtime hardening, signing, or environment-specific restrictions
- the user may need normal macOS developer-style permissions for live instrumentation workflows

This means attach support should be presented as a compatibility matrix, not as a blanket promise.

### 10.8 Failure Model

Attach is not binary in a useful product. V1 should classify failures clearly.

Recommended failure classes:

- process not supported
- permission denied
- runtime detected but hook point unavailable
- probe injected but event channel failed
- hook installed but payload reconstruction incomplete

This is important because the product is observability software. It should explain what it could and could not observe.

### 10.9 Safety Rules For Injection

The probe must behave conservatively.

Rules:

- never patch on-disk application files
- avoid permanent runtime mutation
- keep hook installation idempotent
- keep captured payload size bounded
- fail closed if the runtime shape is unknown
- allow fast detach and probe disable

The probe should prefer losing visibility over destabilizing the target application.

### 10.10 Packaging Implication

Because attach is the hardest part, V1 packaging should separate product value from attach backend complexity.

Recommended structure:

- Rust core binary
- local web UI
- pluggable instrumentation backend adapter
- versioned injected probe bundle for Node and Electron targets

This lets the team evolve the attach backend later without rewriting storage, event models, or UI.

### 10.11 V1 Recommendation Summary

For V1:

- use a dynamic instrumentation backend rather than building a custom injector first
- support only already-running macOS userland Node and Electron targets
- capture only post-attach activity
- keep the bootstrap probe thin
- keep the host in charge of event shaping and persistence
- present compatibility honestly and explicitly

## 11. Next Design Topic

The next design topic after attach and injection should be:

- the concrete event schema for requests, responses, tool visibility, and session association

## 12. Design Seven: Event Schema

The event schema is the foundation of the entire product.

If this layer is too narrow, later analysis will be impossible. If it is too loose, the system will become hard to query, compare, and reason about.

V1 should define a stable fact-oriented schema that captures what happened without forcing premature interpretation.

### 12.1 Schema Design Principles

The schema should follow these rules:

- store facts before conclusions
- keep transport details and semantic payload details both available
- separate raw captured data from normalized fields
- support partial capture when a hook sees only part of the picture
- preserve enough linkage to reconstruct a session timeline

Important rule:

- the schema must tolerate incomplete visibility without breaking the whole event model

### 12.2 Top-Level Object Model

V1 should use four primary entities:

- `process_instance`
- `session`
- `event`
- `artifact`

These are enough for V1 without over-modeling the system too early.

### 12.3 Process Instance

`process_instance` represents a live observed process during some period of time.

Suggested fields:

- `process_instance_id`
- `pid`
- `parent_pid`
- `app_name`
- `executable_path`
- `runtime_kind`
- `host_machine_id`
- `attach_started_at`
- `attach_ended_at`
- `attach_status`
- `probe_version`

Purpose:

- identify which running target produced a set of events
- distinguish multiple attaches to the same app over time

### 12.4 Session

`session` represents a logical grouping of related AI activity.

A session may map to:

- one CLI conversation
- one desktop chat tab
- one request chain discovered from context
- or a synthetic grouping when only partial signals are available

Suggested fields:

- `session_id`
- `process_instance_id`
- `session_key`
- `session_source`
- `title`
- `created_at`
- `last_seen_at`
- `status`

Notes:

- `session_key` may come from app-visible identifiers if available
- `session_source` should record whether the grouping came from native app metadata, request correlation, or heuristic reconstruction

### 12.5 Event

`event` is the main fact record in the system.

Every captured observation should enter the system as an event.

Suggested common fields:

- `event_id`
- `session_id`
- `process_instance_id`
- `event_type`
- `captured_at`
- `observed_at`
- `sequence_no`
- `source_layer`
- `capture_confidence`
- `partial`
- `raw_ref`
- `normalized_ref`

Field intent:

- `captured_at` is when the probe emitted the event
- `observed_at` is when the underlying runtime action happened if known
- `sequence_no` helps maintain local ordering
- `source_layer` indicates whether the event came from `fetch`, `undici`, `http`, SDK hook, or another source
- `capture_confidence` allows later analysis to rank evidence quality
- `partial` tells the system not to assume a full picture

### 12.6 Event Types

V1 should support these initial event types:

- `request_started`
- `request_payload_captured`
- `response_headers_received`
- `response_stream_chunk`
- `response_completed`
- `tool_visibility_snapshot`
- `session_metadata_updated`
- `probe_status`
- `error_observed`

This list is intentionally small. It is enough to reconstruct most useful request lifecycles without overcomplicating the pipeline.

### 12.7 Request Event Payload

For request-related events, the normalized payload should support these fields:

- `request_id`
- `provider`
- `endpoint`
- `model`
- `method`
- `stream`
- `headers_visible`
- `payload_format`
- `system_text`
- `messages_json`
- `tools_json`
- `tool_choice`
- `response_format`
- `sampling_json`
- `attachment_summary_json`
- `body_size_bytes`

Notes:

- `messages_json` and `tools_json` should preserve raw structure as closely as possible
- normalized helper fields may be added later for analysis, but raw semantics matter most in V1

### 12.8 Response Event Payload

For response-related events, the normalized payload should support these fields:

- `request_id`
- `status_code`
- `response_headers_visible`
- `first_byte_at`
- `completed_at`
- `finish_reason`
- `usage_json`
- `assistant_message_json`
- `tool_calls_json`
- `stream_summary_json`
- `body_size_bytes`

The schema should allow response events to arrive incrementally and be merged later by the event pipeline.

### 12.9 Tool Visibility Snapshot

This event captures what tools or skills were visible locally when observable.

Suggested fields:

- `request_id`
- `visibility_stage`
- `candidate_tools_json`
- `filtered_tools_json`
- `final_tools_json`
- `tool_count_candidate`
- `tool_count_filtered`
- `tool_count_final`

`visibility_stage` should describe where the snapshot came from, such as:

- pre-filter
- post-filter
- pre-serialization
- request-embedded

This is important because the product will later analyze gaps between candidate tools and final model-visible tools.

### 12.10 Probe Status Event

Probe health should also be modeled as events rather than hidden logs.

Suggested fields:

- `probe_state`
- `hook_bundle_version`
- `available_hooks_json`
- `installed_hooks_json`
- `failed_hooks_json`
- `error_message`

This helps the UI explain what the system could observe and what it missed.

### 12.11 Error Observed Event

Errors should be first-class facts because later analysis will need them.

Suggested fields:

- `error_class`
- `error_scope`
- `request_id`
- `message`
- `details_json`
- `recoverable`

Possible scopes:

- attach
- hook_installation
- request_capture
- response_capture
- session_correlation

### 12.12 Artifact

Large raw payloads and stream fragments should not always live inline on every event row.

`artifact` provides an object for larger bodies or raw captures.

Suggested fields:

- `artifact_id`
- `artifact_type`
- `storage_kind`
- `storage_path`
- `content_type`
- `size_bytes`
- `sha256`
- `created_at`

Examples:

- raw request body
- raw response chunk stream
- redacted payload copy
- hook diagnostic dump

### 12.13 Raw Versus Normalized Storage

Each important capture should preserve both:

- a raw representation
- a normalized representation

Why both matter:

- raw is needed for forensic trust and future reprocessing
- normalized is needed for filtering, UI, and analysis

V1 should not force the raw and normalized forms to be identical.

### 12.14 Correlation Keys

The pipeline will need stable ways to connect related events.

V1 should support these correlation keys when available:

- `request_id`
- `session_id`
- `process_instance_id`
- `provider request fingerprint`
- `hook-local sequence id`

The host should accept that some events may correlate only probabilistically at first.

### 12.15 Redaction Readiness

Even if V1 does not fully implement redaction policy, the schema should prepare for it.

Suggested support fields:

- `contains_sensitive_data`
- `redaction_status`
- `redaction_policy_version`

This avoids having to redesign the storage layer later when privacy controls are added.

### 12.16 Query Priorities For The Schema

The schema should make these V1 queries easy:

- show all requests from one process
- show all events in one session ordered by time
- open one request and inspect request plus response side by side
- compare final tools embedded across multiple requests
- search for specific prompt fragments or model names
- inspect failures by source layer or hook type

If the schema supports those queries well, it is doing its job.

## 13. Next Design Topic

The next design topic after the event schema should be:

- storage layout and local database design for events, artifacts, indexes, and search

## 14. Design Eight: Storage Layout And Local Database Design

V1 should use a local-first storage design that is simple enough to ship quickly and strong enough to support later analysis features.

The storage design should optimize for:

- append-heavy event ingestion
- timeline reconstruction
- inspection of large request and response bodies
- filtering and search
- later addition of analysis outputs without rewriting the raw fact layer

### 14.1 Storage Model

V1 should use a hybrid local storage model:

- SQLite for structured metadata, indexes, relationships, and queryable normalized fields
- file-backed artifacts for large raw payloads and chunk-heavy bodies when needed

This gives a practical balance:

- SQLite remains the source of truth for queries and UI views
- large payload bodies do not bloat every hot query path

### 14.2 Recommended Local Data Layout

V1 should keep all local data under one workspace root for the product, for example:

- `state/observability.db`
- `state/artifacts/<artifact_id>`
- `state/tmp/`
- `state/logs/`

The exact path can be decided later, but the structure should separate:

- durable queryable data
- large immutable artifacts
- temporary ingestion files
- operator logs

### 14.3 SQLite As Metadata Backbone

SQLite should hold:

- process instances
- sessions
- events
- artifact metadata
- search indexes
- analysis outputs later

Why SQLite is a good V1 fit:

- easy local deployment
- excellent support for relational lookup and filtering
- enough performance for early versions
- simple packaging for a macOS local tool

V1 should avoid introducing a separate local service just for storage.

### 14.4 Proposed Core Tables

The first database shape should include these tables:

- `process_instances`
- `sessions`
- `events`
- `artifacts`
- `event_artifacts`
- `session_labels`

Optional early tables if useful:

- `probe_health_snapshots`
- `ingestion_failures`

### 14.5 Events Table Strategy

The `events` table should be optimized for timeline and filter queries.

Recommended column groups:

- identity: `event_id`, `session_id`, `process_instance_id`
- ordering: `captured_at`, `observed_at`, `sequence_no`
- classification: `event_type`, `source_layer`
- correlation: `request_id`, `provider`, `model`
- state: `partial`, `capture_confidence`
- compact normalized fields for fast filtering
- `raw_ref` and `normalized_ref` pointers where appropriate

Important design rule:

- do not force the entire normalized payload into one opaque JSON column if the UI needs to filter by it often

Instead:

- keep high-value filter keys in columns
- keep rich nested structures in JSON

### 14.6 Artifacts Table Strategy

The `artifacts` table should index large or raw captured bodies stored separately.

Recommended uses:

- raw request payloads
- raw streamed responses
- redacted copies
- debug capture fragments

Recommended metadata:

- `artifact_id`
- `artifact_type`
- `storage_kind`
- `storage_path`
- `content_type`
- `size_bytes`
- `sha256`
- `compression`
- `created_at`

V1 should prefer immutable artifact files once written.

### 14.7 When To Inline Versus Externalize Payloads

V1 should apply a simple policy:

- small normalized payloads can remain inline in SQLite
- large raw bodies should move to artifact storage
- stream-heavy captures should almost always go to artifacts

This keeps interactive queries fast while preserving complete evidence.

### 14.8 Search Strategy

V1 should support useful local search from the start.

Recommended search layers:

- exact filter columns in SQLite for provider, model, app, source layer, status, and time
- full-text search for prompt fragments, message content, and tool names

Practical V1 approach:

- use SQLite FTS for selected textual fields
- index normalized prompt-visible content rather than every raw byte

This gives real product value without introducing a separate search engine.

### 14.9 Write Path

The write path should be append-oriented and resilient.

Recommended ingestion flow:

1. host receives probe event
2. event is validated
3. large bodies are written to artifact storage if needed
4. normalized event row is inserted
5. search indexes are updated
6. UI notification is emitted

Key property:

- ingestion should not block on heavy analysis

That means V1 should separate:

- event persistence
- derived analysis jobs

### 14.10 Read Path

The read path should support these product views efficiently:

- attach target activity feed
- per-session timeline
- per-request detail panel
- filtered request list
- prompt search results

The UI should not need to reconstruct every response body from scratch for common list views.

This means the storage layer should maintain:

- lightweight summary fields
- direct pointers to full payload bodies

### 14.11 Summary Fields

V1 should compute and store summary fields at ingestion time for fast browsing.

Useful examples:

- request preview text
- response preview text
- tool count
- stream flag
- body size
- finish reason
- request duration
- app name

These are not replacements for raw data. They exist to keep the UI responsive.

### 14.12 Index Priorities

The first index set should optimize for actual product usage.

Recommended indexes:

- events by `session_id` and time
- events by `process_instance_id` and time
- events by `request_id`
- events by `provider`, `model`, and time
- sessions by `last_seen_at`
- artifacts by `artifact_id`

FTS should cover:

- request prompt text
- response text
- tool names
- model name and provider labels where useful

### 14.13 Retention And Pruning

Even in V1, storage growth needs a policy.

Recommended defaults:

- keep metadata longer than raw large artifacts
- allow manual purge by session or time range
- support future policies for redacted retention tiers

V1 does not need automatic aggressive cleanup, but it should not assume infinite local disk.

### 14.14 Migration Strategy

The schema will evolve. V1 should plan for migrations from the start.

Requirements:

- explicit database schema version
- forward-only migrations
- probe version recorded alongside events
- storage compatibility checks at startup

This matters because event models and normalization logic will change as compatibility improves.

### 14.15 Privacy And Locality

Because this product captures sensitive AI payloads, storage design should assume privacy matters.

V1 storage stance:

- local by default
- no cloud dependency
- no hidden background export
- clear distinction between raw captured content and redacted or derived content

This will become a trust advantage for the product if implemented clearly.

### 14.16 Recommended V1 Storage Summary

V1 should use:

- SQLite for structured event and session data
- SQLite FTS for prompt and response search
- external artifact files for large raw payloads
- append-oriented ingestion with precomputed summaries
- explicit schema versioning and migration support

This is enough to support the observability console, future analysis modules, and local-first trust.

## 15. Next Design Topic

The next design topic after storage should be:

- redaction, privacy controls, and safe handling of sensitive captured payloads

## 16. Design Ten: Minimum Viable Implementation Plan

The purpose of V1 is not to prove every theoretical capability of the product.

The purpose of V1 is to get from zero to a real, local observability workflow where a user can:

- see a running Node or Electron AI target
- attach without restarting it
- capture new model-facing payloads
- inspect those payloads in a local console

This implementation plan should therefore optimize for shortest path to trustworthy signal.

### 16.1 V1 Success Criteria

V1 is successful if a user can do the following on macOS:

1. launch the tool
2. identify a supported running AI process
3. attach to it successfully
4. capture at least one real post-attach LLM request
5. inspect request payload, response summary, and metadata in the UI

Everything else is valuable, but secondary.

### 16.2 Milestone Structure

The recommended implementation should be split into six milestones:

1. skeleton platform
2. live attach prototype
3. payload capture pipeline
4. local observability console
5. session correlation and search
6. hardening and compatibility pass

This keeps the team focused on shipping working slices instead of building too much infrastructure up front.

### 16.3 Milestone 1: Skeleton Platform

Goal:

- establish the host application shape without yet solving deep instrumentation

Deliverables:

- Rust workspace with clear crates or modules for host, storage, event model, and UI server
- configuration model for local state paths
- SQLite schema bootstrap and migration runner
- process discovery on macOS
- placeholder attach states and health reporting
- local web UI shell with empty views for process list, sessions, and requests

Why this milestone matters:

- it defines the durable product skeleton
- later probe work plugs into something real rather than a throwaway prototype

### 16.4 Milestone 2: Live Attach Prototype

Goal:

- prove that a running Node or Electron target can be attached to without restart

Deliverables:

- instrumentation backend adapter wired into Rust host
- target selection and attach action from the UI or CLI control path
- bootstrap probe injection into a supported target
- host and probe heartbeat channel
- probe status events stored in SQLite

Exit criteria:

- at least one known Node target can be attached to reliably on the developer machine
- attach, detach, and failure states are visible in the product

This milestone is the make-or-break technical validation point.

### 16.5 Milestone 3: Payload Capture Pipeline

Goal:

- capture actual model-facing request and response facts from a live target

Deliverables:

- hook support for `fetch`
- hook support for `undici`
- fallback support for `http` and `https`
- request event emission
- response event emission
- raw payload persistence into artifacts when needed
- normalized event insertion into SQLite

Exit criteria:

- at least one real provider request is visible in storage
- request and response can be correlated by request id or equivalent key
- the captured payload is inspectable after the target continues running

This is the first moment the product becomes meaningfully useful.

### 16.6 Milestone 4: Local Observability Console

Goal:

- make the captured data inspectable in a way that feels like a product, not a debug dump

Deliverables:

- process list page with attach state
- live session timeline
- request list view
- request detail view with side-by-side request and response panels
- basic metadata chips such as provider, model, duration, stream, and tool count
- probe health and error visibility

Exit criteria:

- a user can attach to a process and inspect captured requests without touching the database directly

This milestone turns capture infrastructure into an actual observability experience.

### 16.7 Milestone 5: Session Correlation And Search

Goal:

- make the product usable across more than one request

Deliverables:

- session grouping logic
- searchable request and response content
- filters for app, model, provider, and time range
- summary previews for fast browsing
- event ordering and timeline reconstruction improvements

Exit criteria:

- a user can find a specific request from prior captured activity without manually remembering timestamps

This is the step that moves the product from demo to tool.

### 16.8 Milestone 6: Hardening And Compatibility Pass

Goal:

- stabilize the first public-quality slice

Deliverables:

- compatibility matrix for tested targets
- clearer attach failure classification
- payload size bounds
- backpressure handling for bursty stream capture
- basic data retention controls
- packaging and local installation workflow

Exit criteria:

- the tool works reliably enough on a small tested set of real Node or Electron AI targets
- known unsupported cases are explicit

This milestone is about honesty and trust as much as engineering.

### 16.9 Recommended First Supported Target Set

V1 should not claim broad compatibility at the beginning.

A better strategy is:

- choose a very small set of representative Node or Electron AI tools
- make those work well first
- publish the compatibility boundary clearly

Good initial target categories:

- one Node-based AI CLI
- one Electron-based desktop AI app if feasible
- one controlled internal or demo target for repeatable testing

This avoids building blindly against a vague ecosystem.

### 16.10 Suggested Internal Development Order

Within the milestones above, the practical order of engineering work should be:

1. event model and storage bootstrap
2. process discovery and attach control path
3. instrumentation backend adapter
4. bootstrap probe and heartbeat
5. `fetch` capture
6. `undici` capture
7. request and response persistence
8. local UI for request inspection
9. session grouping and search
10. compatibility and hardening

This order maximizes the chance of seeing a real payload early.

### 16.11 What To Defer On Purpose

The team should explicitly defer these items until after V1 capture works:

- deep explanation of why a skill did not trigger
- automatic prompt diffing across sessions
- cross-machine sync
- cloud storage
- multi-user collaboration
- support for non-Node runtimes
- sophisticated dashboards

These are all attractive, but they should not compete with first-signal delivery.

### 16.12 MVP Demo Scenario

The MVP should have one repeatable demo scenario:

1. start the observability tool
2. choose a running supported AI app
3. attach successfully
4. send a new prompt in the target app
5. watch a request appear in the session timeline
6. open the request and inspect the exact payload sent to the model

If this works cleanly, the project has crossed from idea to product.

### 16.13 Recommended Team Mindset For V1

The first version should be evaluated by these questions:

- did we capture a real payload from a live running app
- did we do it without forcing restart
- can a user inspect the result comfortably
- can the stored facts support future analysis

If the answer is yes, V1 is on track even if many later observability features are still missing.

## 17. Current Document Status

This document now covers:

- product definition
- roadmap layers
- phase 1 scope
- V1 user experience
- technical architecture
- Node and Electron hook targets
- macOS attach and injection direction
- event schema
- storage design
- MVP implementation plan

## 18. Next Design Topics

Recommended next topics are:

- redaction and privacy controls
- security boundaries for local capture
- compatibility test strategy
- first repository structure and crate layout

## 19. Design Nine: Privacy, Redaction, And Security Boundaries

This product captures some of the most sensitive data in an AI workflow.

Depending on the observed application, captured payloads may include:

- proprietary system prompts
- internal tools or skill definitions
- source code
- customer data
- secrets copied into prompts
- file paths and local environment details

Because of that, privacy and security are not secondary concerns. They are core product design constraints.

### 19.1 Privacy Stance

V1 should adopt a strict local-first privacy stance.

Default rules:

- data stays on the local machine
- no cloud sync
- no hidden export
- no remote telemetry containing captured payload content
- no background sharing with third-party services

This is not only safer. It is also an important trust advantage for the product.

### 19.2 Trust Boundary

The system should define three explicit trust zones:

1. observed process
2. local observability host
3. local storage and UI

Important rule:

- the host may receive sensitive content from the observed process, but it should expose that content only through explicit local user actions

That means captured data should not be sprayed across logs, crash dumps, analytics, or uncontrolled debug output.

### 19.3 Data Classes

V1 should treat captured data as belonging to distinct sensitivity classes.

Suggested classes:

- `public_metadata`
- `sensitive_prompt_content`
- `sensitive_response_content`
- `secret_like_material`
- `diagnostic_internal`

Examples:

- provider name is usually `public_metadata`
- system prompt text is often `sensitive_prompt_content`
- API keys or token-like strings are `secret_like_material`
- hook installation diagnostics are `diagnostic_internal`

This classification enables future policy without redesigning the product.

### 19.4 Raw Capture Policy

Raw capture is important for trust and reprocessing, but it must be handled carefully.

V1 raw capture policy should be:

- raw request and response bodies may be stored locally
- raw payloads should be marked clearly as sensitive by default
- raw payloads should not be rendered automatically everywhere in the UI
- raw payloads should be eligible for size bounds and later redaction passes

The product should assume raw capture is high-risk but necessary.

### 19.5 Redaction Modes

V1 does not need perfect redaction, but it should define modes early.

Recommended modes:

- `off`
- `display_only`
- `persist_redacted_copy`
- `persist_redacted_only`

Interpretation:

- `off`: keep and display full local data
- `display_only`: raw stays local, but UI hides likely sensitive fragments by default
- `persist_redacted_copy`: keep raw plus a derived redacted version
- `persist_redacted_only`: only persist a redacted representation for selected payload classes

V1 can begin with `off` and `display_only`, while preparing the storage model for stronger modes later.

### 19.6 Secret-Like Detection

The system should plan for lightweight secret-like detection during ingestion.

Examples:

- API key patterns
- bearer tokens
- private key headers
- obvious credential-shaped environment variable values

V1 rule:

- detection should annotate, not silently mutate, unless a user-selected redaction mode requires masking

This protects forensic usefulness while still preparing for safer defaults.

### 19.7 UI Display Safety

The UI should avoid accidental exposure of sensitive content.

Recommended V1 display safeguards:

- collapsed raw payload sections by default
- explicit reveal for large text bodies
- visual marking for suspected secret-like material
- copy actions that are deliberate, not automatic
- preview fields trimmed and bounded

This keeps the product useful without turning every screen into a potential leak surface.

### 19.8 Logging Rules

Application logs must never become a side channel for captured secrets.

Strict V1 rules:

- do not write full captured payloads to normal application logs
- do not include prompt bodies in crash summaries
- do not print sensitive content in debug logs by default
- use event ids and artifact ids for diagnostics instead of body dumps

This is one of the easiest places to accidentally break trust, so the policy should be strict.

### 19.9 Export Boundary

Even if V1 does not implement export immediately, it should define the rule now.

Any future export must be:

- explicit
- user-initiated
- previewable before completion
- scope-limited to selected sessions or events

The product should never assume that export is harmless just because data is already local.

### 19.10 Retention Safety

Sensitive local data should not be kept forever by accident.

Recommended V1 groundwork:

- surface storage usage clearly
- allow deletion by session, process, or time range
- distinguish metadata deletion from artifact deletion
- make full purge understandable and reliable

This does not require aggressive auto-deletion in V1, but it does require honest lifecycle control.

### 19.11 Probe Safety Boundary

The injected probe should follow a very narrow contract.

Rules:

- capture only what is necessary for the event model
- do not attempt privilege escalation
- do not read arbitrary unrelated memory regions
- do not enumerate unrelated app data unless tied to the defined hook points
- disable cleanly when the host detaches

This keeps the probe aligned with observability rather than drifting toward general process scraping.

### 19.12 Host Security Boundary

The Rust host should behave as though probe input may be malformed or hostile.

Host-side rules:

- validate probe messages
- bound payload sizes
- reject invalid schema versions
- treat attach-time metadata as untrusted input
- isolate artifact writing paths

Even if the user is observing their own process, the host should still protect itself from malformed capture streams.

### 19.13 Future Multi-User And Team Boundary

The product may later grow team workflows, but V1 should not design as though shared storage is already safe.

Future sharing should be treated as a separate trust model, not an extension of local mode.

This means V1 should avoid:

- implicit team sync assumptions
- server-oriented storage shortcuts
- schemas that assume all captured content is safe to replicate

### 19.14 Recommended V1 Privacy Defaults

The recommended V1 defaults are:

- local-only storage
- no payload content in logs
- collapsed raw content in the UI
- sensitivity markers in metadata
- deletion controls for sessions and artifacts
- preparation for optional redaction modes later

These defaults strike a good balance between observability usefulness and user trust.

### 19.15 Product Positioning Benefit

Privacy controls are not just defensive engineering.

They also sharpen the product story:

- this tool helps inspect real AI behavior
- it does so locally
- it does not force the user to surrender the captured prompts to another service

That positioning can become a real differentiator.

## 20. Current Document Status

This document now covers:

- product definition
- roadmap layers
- phase 1 scope
- V1 user experience
- technical architecture
- Node and Electron hook targets
- macOS attach and injection direction
- event schema
- storage design
- MVP implementation plan
- privacy, redaction, and security boundaries

## 21. Recommended Next Steps

The next most useful design topics are:

- compatibility testing strategy
- repository and crate layout
- first supported target selection
- implementation planning from this spec into concrete tasks
