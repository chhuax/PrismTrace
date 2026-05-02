# PrismTrace 技术架构改进方案 TODO

日期：2026-04-30
状态：执行计划

## 目标

把 `docs/技术方案.md` 中的全局架构方案拆成可连续执行的工程任务。总体方向保持：

```text
sources -> ingest -> index -> analysis -> api -> console
```

当前原则：

- 先在 `prismtrace-host` 内拆清边界，不急着拆 crate
- 每个阶段都要能独立验证、独立回退
- 保持现有 console 可用，不做一次性重写
- raw artifacts 继续保留，projection / index 只作为查询和分析入口

## 底层接入提示约定

后续执行时，如果任务进入以下任一范围，必须先明确标注“这是底层接入工作”，再继续设计或实现：

- 新增或修改 Codex / Claude / opencode 的真实采集协议
- 从 snapshot observer 升级为 live stream / polling 增量 observer
- 接入 socket、HTTP server、SSE、websocket、CLI export、attach、注入、抓包等 source-specific 通道
- 采集 raw model request / response 或 provider backend payload
- 扩展某个 source 的 prompt / tool / approval / hook / plugin / skill / agent / MCP / provider 原始语义解析
- 引入 source capability 检测，例如 `supports_live_events`、`supports_raw_model_request`

不属于底层接入的任务：

- read model / index / cache / API / console 的消费路径改造
- artifact projection、分页、详情 API、legacy adapter
- UI 展示、筛选、虚拟列表、diagnostics 面板的只读消费

如果一个阶段需要底层能力才能继续，TODO 中要明确写出：

- 当前中间层可以先做什么
- 哪些部分会被底层接入阻塞
- 需要哪一个 source 的具体接入能力

### 当前底层接入状态

- Codex：已有 app-server observer 能采集 skills / MCP servers / plugins / apps capability snapshot，也能通过 Codex rollout transcript 重建 prompt / tool / tool_result 时间线；MCP 作为独立 `mcp` capability，不并入 `plugin`。
- opencode：已进入底层接入工作，当前已打通 `health + session list + message + tool part + global/event + artifact` 的 observer-first 链路；`global/event` 已支持 permission / agent / MCP / provider / plugin / command / app / tool / session / message 的保守映射与 `unknown` 回退；`/agent`、`/mcp`、`/provider` 主动 snapshot 会分别进入 `agent`、`mcp`、`provider`，不会伪装成 Codex 的 `skill`、`plugin`、`app`。
- 仍未 parity：Codex 与 opencode 都有 MCP facts，但 opencode 的 `agent` / `provider` 不等价于 Codex 的 `skill` / `app`，Codex 的 `plugin` 也不等价于 MCP；如果产品要求“采集同样的内容”，下一步必须继续在 source 协议层定义并补齐对应事实，不能靠中间层命名强行抹平。raw model request / response 级采集也仍未完成。

## 阶段 0：当前基线与已完成事项

- [x] 建立 OpenSpec change：`split-observability-read-model`
- [x] 增加 `ObservabilityReadModel`
- [x] 增加最小 `ObservabilityIndex`
- [x] 将 `/api/sessions`、`/api/sessions/{id}`、`/api/requests/{id}` 背后迁移到 read model adapter
- [x] 覆盖 malformed observer JSONL 不应导致 session 消失
- [x] 本地基线通过：
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `cargo run -p prismtrace-host -- --discover`
  - `npx openspec validate split-observability-read-model --strict`

## 阶段 1：Host 内部模块边界收敛

目标：先降低 `prismtrace-host` 和 `console/mod.rs` 的职责密度，为后续并发 API、分页、analysis 做准备。

- [x] 1.1 新增 `api` 模块，承接本地 HTTP route 与 JSON payload adapter
  - 当前问题：`console/mod.rs` 同时包含 HTTP route、静态资源、legacy request artifacts、read model adapter
  - 结果：新增 `crates/prismtrace-host/src/console/api.rs`，先承接 read model API adapter
  - 验证：现有 `/api/*` 测试保持通过；完整 workspace tests 通过

- [x] 1.2 将 read model 到 console 兼容 payload 的转换移入 `api`
  - 当前问题：`console/mod.rs` 已经接入 read model，但 adapter 仍在 console 模块内部
  - 结果：`load_read_model_*` 与 read model detail payload 渲染已移入 `console/api.rs`
  - 验证：observer requests/session/detail 相关测试保持通过

- [x] 1.3 将 observer source 文件按 `sources` 边界移动或重新导出
  - 候选文件：`codex_observer.rs`、`claude_observer.rs`、`opencode_observer.rs`
  - 结果：新增 `sources/mod.rs`，通过 `sources::codex`、`sources::claude`、`sources::opencode` re-export 现有 observer
  - 验证：host run 方法和 CLI 参数类型已开始使用 `sources::*` 边界；observer CLI 相关测试保持通过

- [x] 1.4 将 artifact writer / normalization 归入 `ingest` 边界
  - 当前问题：source 模块里同时包含连接 source、normalize event、写 artifacts
  - 结果：新增 `crates/prismtrace-host/src/ingest.rs`，统一 `ObserverArtifactWriter` 与 handshake/event artifact record shape；Codex / Claude / opencode source 只选择 artifact source 类型并追加事件
  - 验证：`cargo test -p prismtrace-host observer` 通过，覆盖 Codex / Claude / opencode artifact writer 与 observer source 路径

- [x] 1.5 删除或降级 `console/observer.rs` 的旧详情读链
  - 当前状态：sessions/detail API 已经迁移到 read model，但 target/activity 仍使用旧 helper
  - 结果：`console/observer.rs` 只保留 observer target/activity 兼容 reader；旧 observer request/session detail adapter 与 Codex rollout parser 已移除，详情统一由 read model API adapter 提供
  - 验证：新增 `console_observer_module_no_longer_owns_legacy_detail_adapters` 架构回归；`cargo test -p prismtrace-host observer`、`cargo clippy --workspace --all-targets -- -D warnings` 通过

## 阶段 2：Index / Cache 产品化

目标：从“每次构建内存 read model”升级为“可复用、可失效、可增量重建”的本地 index。

- [x] 2.1 定义 index manifest
  - 字段：source file path、mtime、size、indexed_at_ms、source_kind
  - 结果：`prismtrace-storage` 增加 `ObservabilityIndexManifest` / `SourceIndexManifestEntry`，`StorageLayout` 增加 `state/index/manifest.json`
  - 验证：源文件未变化时可复用 index manifest；read model build 会写入 parsed source manifest

- [x] 2.2 将 session/event projection 写入 `state/index`
  - 建议路径：
    - `.prismtrace/state/index/sessions.jsonl`
    - `.prismtrace/state/index/events.jsonl`
    - `.prismtrace/state/index/capabilities.jsonl`
  - 结果：`ObservabilityIndex` 支持 JSONL save/load；read model build 写出 session/event/capability projection
  - 验证：projection JSONL 可 round-trip；read model build 后可从 `state/index` 读取 session/event/capability reference

- [x] 2.3 支持按 source file 增量重建
  - 目标：只重建变化文件对应的 sessions/events/capabilities
  - 结果：`ObservabilityIndex` 支持按 source path / source kind 替换 projection；read model 写索引时保留未变化 source 的 manifest entry，只替换新增或变化 source 的 session/event/capability projection
  - 验证：新增一个 JSONL 文件时，未变化 source 的 `indexed_at_ms` 保持不变；source A projection 替换不影响 source B

- [x] 2.4 保留 SQLite 迁移口
  - 不立即引入 SQLite schema
  - 当分页、跨 session 查询、diagnostics 历史样本成为瓶颈时再切换
  - 结果：当前 index/cache 继续使用 manifest + JSONL projection，未绑定 SQLite schema；`observability.db` 路径继续作为后续迁移口保留

- [x] 2.5 增加 index-backed read store
  - 目标：让 API/read model 查询路径能优先消费 `state/index/*.jsonl`，不再每次都从 artifact 目录扫描入口开始
  - 结果：新增 `index::IndexReadStore`，从 persisted index 读取 session/event/capability 引用，再按引用读取单个 artifact；console read-model API 路径优先使用 index store，index 不存在或读取失败时回退到原 `ObservabilityReadModel::build`
  - 模块边界：`IndexReadStore` 已移出 `observability_read_model.rs`，放入 `crates/prismtrace-host/src/index/read_store.rs`，作为后续拆 `prismtrace-index` crate 的前置边界
  - 验证：新增 `index_read_store_uses_persisted_index_without_rescanning_artifact_dirs`，证明 index store 不会把 index 写入后新增但尚未索引的 artifact 误读进来；新增 `index_read_store_lives_outside_observability_read_model_module` 防止职责回流

- [x] 2.6 增加 index-backed write store
  - 目标：让 read model 不再直接管理 index manifest、changed source 判断和 JSONL projection 替换
  - 结果：新增 `index::IndexWriteStore`，负责 `prepare` manifest/changed-source plan、`persist_changed_projection` 以及 capability projection 到 storage index entry 的转换；`ObservabilityReadModel::build` 只负责解析 artifacts、构建内存 read model，并调用 write store 持久化
  - 模块边界：index 写入逻辑已移出 `observability_read_model.rs`，放入 `crates/prismtrace-host/src/index/write_store.rs`，继续作为后续拆 `prismtrace-index` crate 的前置边界
  - 验证：新增 `index_write_store_lives_outside_observability_read_model_module` 防止职责回流；`writes_session_event_and_capability_index_projection_files` 继续覆盖 projection 写入

- [x] 2.7 增加 index facade
  - 目标：让 host 调用方只依赖一个 index 门面，而不是分别依赖 read/write store
  - 结果：新增 `index::ObservabilityIndexStore` facade，统一提供 `load_read_store`、`prepare_write`、`persist_changed_projection`、`capability_index_entry`
  - 模块边界：`console/api.rs` 与 `observability_read_model.rs` 已切换为依赖 facade；`IndexReadStore` / `IndexWriteStore` 仍留在 index 模块内部作为实现细节
  - 验证：新增 `host_callers_depend_on_index_facade_not_split_read_write_stores`，防止 host 调用方重新直接依赖 split stores

## 阶段 3：Local API 产品化

目标：把本地 API 从“返回全部”改为分页和按需读取。

- [x] 3.1 固定 API response envelope
  - 建议字段：`items`、`next_cursor`、`empty_state`、`active_filters`
  - 结果：targets / activity / requests / sessions 保留旧字段，同时新增兼容 `items` 与 `next_cursor`
  - 验证：前端可继续读取旧字段，新 API consumer 可读取统一 envelope

- [x] 3.2 实现 session list 分页
  - API：`GET /api/sessions?source=codex&limit=30&cursor=...`
  - 结果：`GET /api/sessions?limit=N&cursor=OFFSET` 支持 offset cursor；响应返回 `next_cursor`
  - 验证：大量 session 下只返回 limit 条

- [x] 3.3 实现 session events 分页
  - API：`GET /api/sessions/:session_id/events?limit=100&cursor=...`
  - 结果：新增 `/api/sessions/:session_id/events?limit=N&cursor=OFFSET`，响应返回 `items`、`timeline_items`、`next_cursor`、`empty_state`；支持 snapshot/legacy session 与 read model session
  - 验证：timeline 不需要一次性加载全部事件

- [x] 3.4 新增 event detail API
  - API：`GET /api/events/:event_id`
  - 结果：新增 `/api/events/:event_id`，直接返回 `event` 语义 payload，包含 source、artifact、raw_json、detail，不再套用 legacy request envelope
  - 验证：`cargo test -p prismtrace-host console_server_returns_observer` 通过，覆盖 `/api/events/:event_id` 与旧 `/api/requests/:id` 兼容路径

- [x] 3.5 保留 legacy route adapter
  - `/api/requests/:id` 短期继续可用
  - 结果：console JS 对 `observer:` / `codex-thread:` read model event id 优先请求 `/api/events/:event_id`，其他 legacy request id 继续请求 `/api/requests/:id`
  - 验证：`cargo test -p prismtrace-host console_script_prefers_event_detail_api_for_read_model_event_ids` 通过；`cargo test -p prismtrace-host console_server_returns_observer` 继续覆盖旧详情路由

## 阶段 4：并发化本地 HTTP server

目标：避免慢 API 阻塞静态资源和其他 API 请求。

- [x] 4.1 抽象 route handler
  - 先将 request path -> response body 的逻辑从同步 socket loop 中分离
  - 结果：新增 `ConsoleRouteResponse` 与 `render_console_route_response`，socket 写入只负责序列化响应
  - 验证：新增 `console_route_handler_renders_health_without_tcp`；`cargo test -p prismtrace-host console_` 通过

- [x] 4.2 引入轻量并发处理
  - 第一版可用 thread-per-connection 或小型 thread pool
  - 暂不强制引入 async runtime
  - 结果：`serve_forever` 改为 thread-per-connection；保留 `serve_once` 的同步测试路径
  - 验证：新增 `console_server_handles_static_asset_while_previous_connection_is_idle`，空闲连接不阻塞后续 `/assets/console.js` 响应；`cargo test -p prismtrace-host console_`、`cargo clippy --workspace --all-targets -- -D warnings` 通过

- [x] 4.3 增加超时和错误响应
  - API 超时返回明确 JSON error
  - 静态资源继续快速返回
  - 结果：未知 `/api/*` 返回 JSON 404；连接读请求超时返回 JSON 408；非 API 页面路径继续保留 HTML 404
  - 验证：新增 `console_server_returns_json_error_for_unknown_api_path`、`console_server_returns_json_error_when_request_read_times_out`；`cargo test -p prismtrace-host console_`、`cargo clippy --workspace --all-targets -- -D warnings` 通过

## 阶段 5：Capability Projection

目标：把 skill / MCP / tool / plugin / app visibility 从事件详情里提升为一等 projection。

- [x] 5.1 定义 `CapabilityProjection`
  - 字段：`capability_id`、`session_id`、`event_id`、`source_kind`、`capability_type`、`capability_name`、`visibility_stage`、`observed_at_ms`、`raw_ref`
  - 结果：新增 `capability_projection` 模块，定义 projection / raw ref，并提供 observer event 与 tool visibility artifact 的 projection helper

- [x] 5.2 从 Codex observer capability snapshot 构建 projection
  - 覆盖 apps / MCP servers / plugins / skills
  - 保留 raw_json 引用
  - 结果：read model build 时从 `app` / `mcp` / `plugin` / `skill` observer event 中提取 capability projection，`raw_ref` 指向原始 artifact 行
  - 验证：新增 `projects_capabilities_from_observer_snapshots_and_tool_events`

- [x] 5.3 从 request-embedded tool visibility 构建 projection
  - 覆盖 tools / functions
  - 与 existing tool visibility artifacts 兼容
  - 结果：`/api/sessions/:session_id/capabilities` 对 legacy request session 会读取 existing `tool_visibility/*.json`，将 `final_tools_json` 投影为 `tool` / `function` capability
  - 说明：这是中间层兼容 existing artifact，不是底层统一接入；真正让 opencode 与 Codex 同级采集仍需要后续 source/observer 层工作

- [x] 5.4 增加 capability API
  - API：`GET /api/sessions/:session_id/capabilities`
  - 验证：console 可展示某个 session 可见能力清单
  - 结果：新增本地 API route，console session detail 加载时并行请求 capability projection，并在 timeline 顶部展示可见能力清单
  - 验证：新增 `console_server_returns_observer_session_capabilities_api_payload`、`console_server_returns_request_embedded_tool_capabilities_api_payload`、`console_script_fetches_and_renders_session_capabilities`

## 阶段 6：Analysis 模块

目标：prompt / tool / skill diagnostics 不再散落在 console renderer 中，而是统一从 projection 读取。

- [x] 6.1 新增 `analysis` 模块
  - 第一批能力：prompt projection、prompt diff
  - 验证：同一 session 相邻事件可以生成 diff
  - 结果：新增 `analysis` 模块，支持从 read model event detail 提取 `PromptProjection`，并生成相邻 prompt 的 added/removed line diff
  - 验证：新增 `analysis_projects_prompt_diff_between_adjacent_events`

- [x] 6.2 新增 tool visibility diff
  - 比较相邻 turn / request 的 tool 集合变化
  - 验证：新增、删除、隐藏、重命名可被识别
  - 结果：新增 `tool_visibility_diffs`，基于 capability projection 比较相邻 tool/function snapshot，输出 added / removed / hidden / rename candidate
  - 验证：新增 `analysis_diffs_tool_visibility_snapshots`

- [x] 6.3 新增 skill visibility diff
  - 比较 skills 在不同 session / turn 的可见性变化
  - 验证：能定位“用户以为 skill 在，实际未出现在 capability snapshot”的情况
  - 结果：新增 `skill_visibility_diffs`，复用 capability projection 比较相邻 skill snapshot
  - 验证：新增 `analysis_diffs_skill_visibility_and_diagnoses_missing_facts`

- [x] 6.4 新增 skill diagnostics
  - 输入：session/event/capability projection
  - 输出：`available` / `partial` / `unavailable` 诊断结果
  - 验证：事实不足时明确返回 partial/unavailable，而不是猜测
  - 结果：新增 `diagnose_skill_visibility`；有 skill snapshot 且命中返回 `available`，有 snapshot 但未命中返回 `unavailable`，没有 snapshot 返回 `partial`
  - 说明：这是 analysis 层能力，不改变底层 observer 接入；若某 runtime 没有 capability snapshot，诊断会返回 `partial`，这就是需要底层接入补事实的信号

## 阶段 7：Console 体验优化

目标：UI 只消费 API，不绑定本地文件结构，并提升大 session 下的交互流畅度。

- [x] 7.1 移除 Tailwind CDN
  - 改为本地 CSS 或构建期 CSS
  - 验证：离线环境可以加载 console
  - 结果：新增 `/assets/console-utilities.css`，`console.html` 不再加载 Tailwind CDN
  - 验证：`rg "cdn.tailwindcss.com|fonts.googleapis.com|fonts.gstatic.com" crates/prismtrace-host/assets` 无匹配

- [x] 7.2 字体和 icon 本地化或降级
  - 避免 Google Fonts / Material Symbols 网络依赖
  - 验证：无网络时 UI 不出现明显破损
  - 结果：移除 Google Fonts / Material Symbols 外链，新增本地 font fallback 与 icon 文本 fallback hydrate 逻辑
  - 验证：`console.js` 初始化本地 icon fallback，静态资源由 host 本地提供

- [x] 7.3 session list 分页或虚拟列表
  - 依赖阶段 3 API
  - 验证：大量 session 下滚动不卡顿
  - 结果：前端 session 初始加载改为 `GET /api/sessions?limit=30`，支持 `next_cursor` 和显式加载更多
  - 验证：`cargo test -p prismtrace-host console_script_uses_paginated_session_and_timeline_apis`

- [x] 7.4 timeline 分页或虚拟列表
  - 依赖 `/api/sessions/:id/events`
  - 验证：大 timeline 不全量渲染
  - 结果：session detail 并行读取 `/api/sessions/:id/events?limit=100`，timeline 支持加载更多
  - 验证：`cargo test -p prismtrace-host console_script_uses_paginated_session_and_timeline_apis`

- [x] 7.5 diagnostics panel
  - 将 capability / diagnostics 做成明确视图
  - 不再混在 request detail 的 raw payload 展示里
  - 结果：新增 `/api/sessions/:session_id/diagnostics`，右侧 diagnostics panel 展示 prompt/tool/skill diff summary、visible skills，以及按 `agent` / `app` / `mcp` / `plugin` / `provider` / `skill` 分组的 capability inventory；其中 MCP 是独立 capability，不并入 plugin
  - 验证：`cargo test -p prismtrace-host console_server_returns_observer_session_diagnostics_api_payload -- --nocapture`、`cargo test -p prismtrace-host console_script_fetches_and_renders_session_diagnostics -- --nocapture`

## 阶段 8：拆 crate

目标：当 host 内模块边界稳定后，再拆成独立 crate。

- [x] 8.1 拆 `prismtrace-sources`
  - 范围：先拆 source/observer 共享 contract 与 observer artifact writer，不移动 Codex / Claude / opencode 的具体 observer 实现
  - 结果：新增 `crates/prismtrace-sources`，承载 `ObserverSource` / `ObserverSession` / `ObservedEvent` / `ObserverArtifactWriter` 等类型；host 内 `observer.rs` / `ingest.rs` 缩为兼容 re-export，Codex / Claude / opencode observer 直接消费 `prismtrace_sources`
  - 验证：`cargo test -p prismtrace-sources observer_ -- --nocapture`、`cargo test -p prismtrace-host source_contracts_live_in_prismtrace_sources_crate -- --nocapture`、`cargo test -p prismtrace-host lifecycle::tests::run_opencode_observer_session_passes_storage_to_artifact_writer -- --nocapture`
  - 底层接入说明：这一步不是 opencode / Codex 底层采集统一；它只是把已有 observer-first source 边界从 host 拆出。底层 parity 仍需要在 source 协议层单独定义并补齐 facts
- [x] 8.2 拆 `prismtrace-index`
  - 范围：先拆 index projection / manifest / JSONL persistence 类型与逻辑，不移动 host 内依赖 read model 的 read/write store orchestration
  - 结果：新增 `crates/prismtrace-index`，`prismtrace-storage` 收缩为目录布局并通过 re-export 保持兼容；host 内直接使用 `prismtrace_index` 的 index 类型
  - 验证：`cargo test -p prismtrace-storage index_projection_types_live_in_prismtrace_index_crate -- --nocapture`、`cargo test -p prismtrace-index observability_index -- --nocapture`、`cargo test -p prismtrace-host index_write_store -- --nocapture`
  - 底层接入说明：这一步不是 opencode / Codex 底层采集统一；底层接入仍需在 sources/observer 阶段单独推进
- [x] 8.3 拆 `prismtrace-analysis`
  - 范围：拆 prompt diff、tool/skill visibility diff、skill diagnostics，以及 capability projection 类型与投影函数
  - 结果：新增 `crates/prismtrace-analysis`；host 内 `analysis.rs` / `capability_projection.rs` 缩为兼容 re-export，host 读链和 console API 直接消费 `prismtrace_analysis`
  - 验证：`cargo test -p prismtrace-analysis analysis_ -- --nocapture`、`cargo test -p prismtrace-host analysis_projection_types_live_in_prismtrace_analysis_crate -- --nocapture`、`cargo test -p prismtrace-host console_server_returns_observer_session_diagnostics_api_payload -- --nocapture`
  - 底层接入说明：这一步不是 opencode / Codex 底层采集统一；它只消费已有 artifacts / capability facts
- [x] 8.4 拆 `prismtrace-api`
  - 范围：先拆 read-only console API 的纯 JSON payload renderer，不移动 host 内 storage/read model 查询编排
  - 结果：新增 `crates/prismtrace-api`，承载 `ApiFilterContext`、capability projection payload、session diagnostics payload 及空态 payload；host console 只负责把 legacy/read model 数据适配成 API 输入
  - 验证：`cargo test -p prismtrace-api api_ -- --nocapture`、`cargo test -p prismtrace-host api_payload_renderers_live_in_prismtrace_api_crate -- --nocapture`、`cargo test -p prismtrace-host console_server_returns_observer_session_diagnostics_api_payload -- --nocapture`、`cargo test -p prismtrace-host console_server_returns_observer_session_capabilities_api_payload -- --nocapture`
  - 底层接入说明：这一步不是 opencode / Codex 底层采集统一；它只拆本地 HTTP API payload 层
- [x] 8.5 收缩 `prismtrace-host` 为 CLI / lifecycle orchestration
  - 范围：先收缩 crate root，把启动配置、bootstrap、discover/report、observer session orchestration 从 `lib.rs` 迁入独立 lifecycle 模块；不移动底层 observer 实现
  - 结果：新增 `crates/prismtrace-host/src/lifecycle.rs`；`lib.rs` 只保留模块声明与兼容 re-export，host crate 根不再承载 lifecycle 实现细节
  - 验证：`cargo test -p prismtrace-host lifecycle_orchestration_lives_outside_lib_module -- --nocapture`、`cargo test -p prismtrace-host lifecycle::tests -- --nocapture`、`cargo test -p prismtrace-host collect_console_snapshot_exposes_local_console_url -- --nocapture`
  - 底层接入说明：这一步不是 opencode / Codex 底层采集统一；它只是 host orchestration 边界收口

## 每阶段完成标准

每个阶段完成前至少运行：

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p prismtrace-host -- --discover
```

涉及 OpenSpec change 时额外运行：

```bash
npx openspec validate <change-id> --strict
```
