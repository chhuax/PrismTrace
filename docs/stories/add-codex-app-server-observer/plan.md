# Codex App Server Observer 实施计划

日期：2026-04-25  
更新：2026-04-26  
状态：完成

## 1. 目标

把 `Codex App Server + IPC socket` 路线推进到可实施状态，并在第一版通过最小 CLI/host slice 验证：

- `PrismTrace` 能把 `Codex` 当成一个新的官方观测后端接入
- 能稳定拿到高层运行时事件
- 不需要再依赖危险的 attach 路线

## 2. 分阶段推进

### 阶段 A：设计收敛

- [x] A.1 固定 `Codex` 走官方 observer 路线，不再走 attach
  - 验证：story 与 change 文档明确 `Codex` 不复用 `AttachController`

- [x] A.2 固定第一版事件面
  - 验证：明确只收 `thread / turn / item / tool / approval / hook / plugin / skill / app`

- [x] A.3 固定第一版产品入口
  - 验证：明确先做 CLI/host 验证入口，不直接扩散到控制台

### 阶段 B：最小 host 实现

- [x] B.1 新增 `Codex` observer 模块
  - 验证：存在独立 `codex_observer.rs`，不改 attach 主链语义

- [x] B.2 新增 CLI 入口
  - 验证：可通过 `--codex-observe` 或 `--codex-socket <path>` 启动最小观察流程

- [x] B.3 完成最小握手与事件读取
  - 验证：CLI 能输出 initialize 成功和后续高层事件摘要

- [x] B.4 事件落盘
  - 验证：高层事件可以按结构化 artifact 保存，供后续 UI/分析复用

### 阶段 C：聚焦验证

- [x] C.1 协议层测试
  - 验证：受控输入可覆盖初始化、未知事件、错误回包

- [x] C.2 集成层验证
  - 验证：live `Codex` 环境下能稳定读取至少一组高层事件

- [x] C.3 基线验证
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`

## 3. 当前实现状态

当前分支上的 `Codex observer` 已经具备最小可运行主干：

- 已有独立入口：`--codex-observe` / `--codex-socket <path>`
- 已有 source / session / event 基础抽象
- 已支持 socket 发现、`initialize`、capability 拉取和最小事件归一化
- `prismtrace-host` 已移除旧 attach/readiness 控制链，避免产品面对外继续暴露失效方案

当前主干已经形成第一版产品闭环：

- 已有 `observer_events/codex` 结构化 artifact 落盘
- 控制台已能读取 observer artifact，并以统一 `Sources / Events / Sessions / Timeline / Inspector` 语义展示 `Codex`
- 已完成 live `Codex` 验证和本地 CI 基线

当前剩余收尾项：

- 协议层测试已覆盖初始化、错误回包和未知事件保留；后续如果继续深挖，可再补更多稀有协议分支
- 已确认此前自动发现命中的 `codex-ipc` socket 实际属于 VS Code `openai.chatgpt` 扩展宿主，而不是桌面版 `Codex.app`；默认发现现已收紧为仅接受桌面版 Codex owner，因此当前默认路径会直接走稳定的 `standalone-app-server`
- attach 历史实现与只为其存在的 host 测试链已从 `prismtrace-host` 清理，当前产品与代码主线都收口为 observer-first

## 4. 当前 TODO（按优先级）

### TODO-1：Codex observer artifact 落盘

- [x] 设计 `Codex observer` 的最小 artifact 结构
  - 建议先独立落到 `observer-events/codex` 或等价目录，不强行复用 request / response schema
- [x] 为每条 observer event 保留稳定字段
  - 至少包含 `channel`、`event_kind`、`summary`、`thread_id`、`turn_id`、`item_id`、`timestamp`、`raw_json`
- [x] 把 `run_codex_observer` 输出同步写入 artifact
  - 验证：运行一次 `--codex-observe` 后，`.prismtrace` 下可看到结构化事件文件

### TODO-2：控制台最小可见性

- [x] 让 Stitch 控制台能识别 observer artifact
  - 验证：首页可出现统一的 observer source / session 入口，而不是为 `Codex` 单独造一套页面
- [x] 统一 `Codex` 与 `opencode` 展示语义
  - 验证：两者共用 `Sources / Events / Sessions / Timeline / Inspector` 壳，至少能投影到同一套列表与详情结构
- [x] 增加最小 timeline 与 inspector 视图
  - 验证：可看到 `thread / turn / item / tool / approval / hook / capability / message` 摘要，并查看最小 detail
- [x] 明确第一版展示边界
  - 保持 Stitch 视觉稿，只做 observer 语义调整，不在这一轮扩成复杂分析台

### TODO-3：聚焦验证

- [x] 为 artifact 落盘补测试
  - 验证：受控输入下能生成结构化 observer artifact
- [x] 为控制台读取 observer artifact 补测试
  - 验证：最小页面或 API payload 能返回 `Codex observer` 事件
- [x] 做一次 live `Codex` 验证
  - 验证：运行中的 `Codex.app` 至少能读到一组真实高层事件

### TODO-4：基线收尾

- [x] 跑本地 CI 基线
- [x] 回填 `openspec/changes/add-codex-app-server-observer/tasks.md`
- [x] 回写本文件状态，标记本轮完成项
## 5. 建议的最小实现顺序

1. 先建 `Codex` observer 独立模块
2. 先打通 CLI 与最小握手
3. 再做事件归一化和 artifact 落盘
4. 最后再决定是否需要把结果接到控制台

当前建议的实际执行顺序调整为：

1. 先做 `Codex observer artifact 落盘`
2. 再做 `控制台最小可见性`
3. 再做 `live 验证 + 聚焦测试`
4. 最后跑 CI 并回填 tasks

## 6. 建议文件边界

### 文档

- `docs/stories/add-codex-app-server-observer/design.md`
- `docs/stories/add-codex-app-server-observer/plan.md`
- `openspec/changes/add-codex-app-server-observer/*`

### 最小实现

- `crates/prismtrace-host/src/main.rs`
- `crates/prismtrace-host/src/lib.rs`
- `crates/prismtrace-host/src/observer.rs`
- `crates/prismtrace-host/src/codex_observer.rs`

### 可能的后续扩展

- `crates/prismtrace-host/src/codex_protocol.rs`
- `crates/prismtrace-host/src/codex_storage.rs`

## 7. 当前建议

当前这一轮已经把 `Codex` 从“CLI 主干已通”推进到了“artifact、控制台、live 验证和本地 CI 基线都已打通”。下一步如果继续深挖，重点应转到协议层补测和 `proxy socket` 直连稳定性，而不是重新回到 attach 路线。
