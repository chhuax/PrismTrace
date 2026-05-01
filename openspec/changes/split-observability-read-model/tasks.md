## 1. 收敛 read model V1 边界

- [x] 1.1 固定本轮只拆 console 读链，不改 observer 采集协议
  - 验证：proposal/design 明确 source/ingest 不在本轮重写范围

- [x] 1.2 固定第一版 read model 对象
  - 验证：文档明确 `SessionSummary`、`SessionDetail`、`EventSummary`、`EventDetail`、`ArtifactRef`、`SourceRef`

- [x] 1.3 固定兼容优先策略
  - 验证：现有 `/api/sessions`、`/api/sessions/{id}`、`/api/requests/{id}` 路径保持不变

## 2. 拆分 reader / projector

- [x] 2.1 新增 observer artifact reader
  - 验证：可以从 `state/artifacts/observer_events/**.jsonl` 读取 handshake/event 并保留 raw JSON

- [x] 2.2 新增 Codex rollout transcript reader
  - 验证：可以从 `~/.codex/sessions/**.jsonl` 读取 active transcript，并排除 `archived_sessions`

- [x] 2.3 新增 read model projector
  - 验证：reader 输出可以被投影为 session/event summary/detail，而不依赖 console HTML 或 payload renderer

## 3. 建立最小 index/query 层

- [x] 3.1 增加 session index
  - 验证：支持按更新时间倒序列出 session summaries，并限制返回数量

- [x] 3.2 增加 event index
  - 验证：支持通过 event/request id 找到 source kind、artifact path、line index 与 session id

- [x] 3.3 增加 detail query
  - 验证：支持通过 session id 查询 session detail，通过 event/request id 查询 event detail

## 4. 迁移 console API

- [x] 4.1 将 `/api/sessions` 背后迁移为 read model query
  - 验证：前端 session 列表字段保持兼容

- [x] 4.2 将 `/api/sessions/{id}` 背后迁移为 read model query
  - 验证：前端 timeline/detail 字段保持兼容

- [x] 4.3 将 `/api/requests/{id}` 背后迁移为 read model query
  - 验证：前端展开详情仍能展示 raw payload / observer event / Codex rollout event

## 5. 清理旧 console 读链

- [x] 5.1 删除或收缩 `console/observer.rs` 中已迁移的扫描逻辑
  - 验证：sessions / requests / detail API 已迁移到 read model；`console/observer.rs` 仅保留 target/activity 兼容读链和待后续删除的旧 helper

- [x] 5.2 保留必要 adapter 以降低一次性迁移风险
  - 验证：adapter 只做旧字段兼容，不再读取源 artifacts

## 6. 验证与收尾

- [x] 6.1 增加聚焦测试
  - 验证：覆盖 reader、projector、index query、console payload 兼容性

- [x] 6.2 运行本地 CI 基线
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`

## 7. 阶段 6 Analysis 模块

- [x] 7.1 新增 prompt projection 与 prompt diff
  - 验证：同一 session 相邻 prompt 可以生成 added/removed diff

- [x] 7.2 新增 capability visibility diff
  - 验证：tool / skill capability snapshot 可识别 added、removed、hidden、rename candidate

- [x] 7.3 新增 skill diagnostics
  - 验证：facts 充足时返回 available/unavailable；缺少 snapshot 时返回 partial

## 8. 阶段 7 Console API / UI 消费

- [x] 8.1 移除 console 外部静态资源依赖
  - 验证：Tailwind / Google Fonts / Material Symbols 不再通过 CDN 加载，console 静态资源由 host 本地提供

- [x] 8.2 前端消费分页 session / timeline API
  - 验证：session list 使用 `GET /api/sessions?limit=...`，timeline 使用 `GET /api/sessions/:id/events?limit=...`，并提供加载更多控制

- [x] 8.3 新增 session diagnostics API 与面板
  - 验证：`GET /api/sessions/:session_id/diagnostics` 返回 prompt/tool/skill diagnostics payload，console 右侧面板只消费该 API

## 9. 阶段 8 Crate 边界收敛

- [x] 9.1 拆出 `prismtrace-index`
  - 验证：index projection / manifest / JSONL persistence 迁入 `crates/prismtrace-index`；`prismtrace-storage` 只保留目录布局并兼容 re-export；host 直接依赖 `prismtrace_index` 的 index 类型

- [x] 9.2 拆出 `prismtrace-analysis`
  - 验证：prompt diff、tool/skill visibility diff、skill diagnostics、capability projection 迁入 `crates/prismtrace-analysis`；host 只保留兼容 re-export 并直接消费 `prismtrace_analysis`

- [x] 9.3 拆出 `prismtrace-api`
  - 验证：capability projection payload、session diagnostics payload 与空态 payload 迁入 `crates/prismtrace-api`；host console 仅负责数据加载与 API 输入适配

- [x] 9.4 收缩 host lifecycle root
  - 验证：启动配置、bootstrap、discover/report、observer session orchestration 迁入 `crates/prismtrace-host/src/lifecycle.rs`；`lib.rs` 只保留模块声明与兼容 re-export
