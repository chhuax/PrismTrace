## 概览

`split-observability-read-model` 的目标，是把 PrismTrace 当前已经过载的 console 读链拆成更清晰的职责层：

- source / ingest 继续负责采集事实并保留 raw JSON
- storage / index 负责从 artifacts 构建可查询的 session/event 索引
- read model 负责投影 console 和本地 API 需要的 summary/detail
- console 只负责本地 HTTP API 和 UI 展示

本轮不是一次“大重写”，而是先把最影响后续维护的边界拆出来，尤其是 `console/observer.rs` 中混合的 artifact reader、Codex transcript parser、session indexer 和 payload renderer。

## 背景

当前文档中的目标架构已经倾向：

```text
sources/
ingest/
index/
analysis/
api/
console/
```

但代码实现仍主要集中在 `prismtrace-host`：

- observer source 实现已经有独立模块，但读取 artifacts 的逻辑仍长在 console 下
- Codex rollout transcript 扫描直接服务 console API，而不是先进入稳定 read model
- storage crate 只创建目录，没有 session/event index、查询或缓存职责
- console API 目前通过同步单线程 HTTP server 提供，但真正风险不是同步本身，而是每次请求触发重复扫描和投影

因此这一轮应优先解决“数据从哪里读、由谁投影、console 消费什么模型”的边界问题。

## 目标 / 非目标

**目标：**
- 定义 observer-first 的 session/event read model
- 将 observer artifact 读取与 Codex rollout transcript 解析从 console renderer 中分离
- 建立最小 session index / event index 职责，让 storage 不再只是目录布局
- 让 `/api/sessions`、`/api/sessions/{id}`、`/api/requests/{id}` 背后消费 read model
- 保持现有 UI 和 API 字段兼容优先，降低迁移风险

**非目标：**
- 不重写 Web console 为 React / Vue / Svelte
- 不把本地 HTTP server 一次性迁移到 async runtime
- 不引入云端同步或远程 SaaS
- 不在 read model 拆分阶段交付 prompt diff、tool diff、skill diagnostics；用户要求继续推进后，阶段 6 将以独立 `analysis` 模块纳入同一 change
- 不强制把所有历史 request/response artifacts 改写为新格式

## 方案

### 1. Read model 边界

新增一组内部读模型，表达 console/API 需要的稳定查询面：

- `SessionSummary`
- `SessionDetail`
- `EventSummary`
- `EventDetail`
- `ArtifactRef`
- `SourceRef`

这些模型不是 source 原始协议，也不是 UI HTML。它们只描述“本地 API 和 console 需要展示的事实”。

第一版应覆盖：

- observer session
- observer event
- Codex rollout transcript session
- Codex rollout event
- 现有 request/response/tool visibility artifacts 的兼容引用

### 2. Parser / projector 分离

将当前 `console/observer.rs` 中的职责拆成三个方向：

- artifact reader：读取 `state/artifacts/observer_events/**.jsonl`
- transcript reader：读取 `~/.codex/sessions/**.jsonl`，排除 archived sessions
- projector：把 reader 输出投影为 read model summary/detail

reader 保留 raw JSON，projector 只生成 read model 字段。console payload renderer 不再直接解析源文件。

### 3. 最小索引与缓存

storage/read-model 层提供第一版全量 rebuild：

- 扫描 artifacts / transcript 文件
- 生成 session index
- 生成 event index
- 记录 artifact path、line index、source kind、timestamp、session id、event id

第一版可以是内存索引或轻量文件缓存，不要求立即启用 SQLite schema。已有 `db_path` 可保留给后续持久化索引。

索引必须支持：

- 列出 session summaries
- 根据 session id 查询 session detail
- 根据 event/request id 查询 event detail
- 限制返回数量，为后续分页预留接口

### 4. Console API 迁移

保持现有前端请求路径优先不变：

- `/api/sessions`
- `/api/sessions/{id}`
- `/api/requests/{id}`

但背后改为：

```text
HTTP route -> read model query -> JSON payload renderer
```

而不是：

```text
HTTP route -> console observer helpers -> scan files -> JSON payload
```

迁移完成后，console 层允许保留少量兼容 adapter，但不再拥有 source parser。

### 5. 兼容策略

为降低风险，本 change 不要求一次删除旧 helper。建议采用迁移栅栏：

1. 新增 read model modules 与测试
2. 将 `/api/sessions` 切到新 read model
3. 将 `/api/sessions/{id}` 切到新 read model
4. 将 `/api/requests/{id}` 切到新 read model
5. 删除或降级旧 `console/observer.rs` 中已迁移的扫描逻辑

## 数据流

```text
Source observers
  -> raw observer artifacts / transcript files
  -> artifact readers
  -> session/event index
  -> read model query
  -> local HTTP API
  -> console UI
```

长期上，analysis 层也应消费 read model 或 index，而不是重新扫描 artifacts。

## 阶段 6 扩展：Analysis projection

阶段 5 已经将 skill / plugin / app / tool visibility 提升为 capability projection。阶段 6 在同一 change 内继续补上只读 analysis 模块，但仍不改变 source / observer 底层采集协议。

Analysis 模块第一版只消费 read model event detail 与 capability projection：

- prompt projection：从 message / prompt-like event detail 提取 prompt 文本事实
- prompt diff：比较同一 session 内相邻 prompt projection
- tool / skill visibility diff：比较相邻 capability snapshot 的新增、移除和 rename candidate
- skill diagnostics：当 facts 足够时返回 available / unavailable；当没有 capability snapshot 时返回 partial，避免猜测

## 阶段 7 扩展：Console API / UI 消费

阶段 7 继续保持“不改底层 observer 协议”的边界，只把 console 前端从文件结构和外部静态资源依赖中解耦出来：

- console 静态资源本地化：移除 Tailwind / Google Fonts / Material Symbols CDN 依赖，改由 host 提供本地 CSS 与 icon fallback
- session list 消费分页 API：前端使用 `/api/sessions?limit=...` 与 `next_cursor`，避免一次性渲染全部 session
- timeline 消费分页 API：session detail 不再依赖 `/api/sessions/:id` 中的全量 timeline，而是并行读取 `/api/sessions/:id/events?limit=...`
- diagnostics 明确视图：新增 `/api/sessions/:session_id/diagnostics`，由 API 聚合 prompt diff、tool / skill visibility diff 与 skill diagnostics，UI 只负责展示

## 错误处理

- 单个 artifact 损坏时，不应导致整个 console API 失败
- reader 应记录可诊断的 skipped/error event，并继续处理其他文件
- detail 查询命中不存在的 session/event 时返回明确 not found
- transcript 文件缺少时间戳或 cwd 时，应保留 session 但降级字段为空

## 验证策略

- 聚焦测试覆盖 observer artifact reader
- 聚焦测试覆盖 Codex transcript reader 排除 archived sessions
- 聚焦测试覆盖 session/event index 查询
- 聚焦测试覆盖 console API payload 与旧字段的兼容性
- 最终运行本地 CI 基线：
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `cargo run -p prismtrace-host -- --discover`
