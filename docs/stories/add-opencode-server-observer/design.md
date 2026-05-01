# Opencode Server Observer 设计稿

日期：2026-04-26  
状态：已收敛，待进入实施计划

## 1. 背景

`opencode` 的实机验证已经表明：

- 现有 `SIGUSR1 + inspector` attach 路线会把 live `opencode` 打死
- `opencode` 已提供官方 `server + attach(url)` 观测路径
- `opencode serve` 的 HTTP server、session list、session export 已验证可用
- 官方全局事件面可为实时观测补充高层运行时信息

因此，这一轮不再把 `opencode` 视为“等待 Bun attach 兼容”的目标，而是明确把它定义为一条并行的官方 observer source。

本 story 要收敛的问题是：

`PrismTrace host 如何按 observer 的方式接入 opencode，并先把底层资源采集链路做实。`

## 2. 本轮目标

本轮只聚焦底层采集闭环，不扩散到控制台展示：

1. 将 `opencode` 接入为和 `Codex` 并列的正式 observer source
2. 固定第一版数据面：`health + session list + session export/message + global event`
3. 将 `opencode` 输出收敛到统一 `observer.rs` 抽象，而不是保留私有原型结构
4. 对齐 `Codex` 的 artifact 落盘模式，稳定保存握手与高层事件
5. 通过最小 CLI 入口完成本地可验证的采集闭环

## 3. 非目标

这一版明确不做：

- 不做 Bun runtime attach
- 不再尝试 `SIGUSR1` / inspector 路线
- 不承诺获取 `opencode -> 模型后端` 的原始 HTTP 报文
- 不为 `opencode` 接控制台 observer 视图
- 不顺手重构 `Codex` 和 `opencode` 的更上层统一 runtime
- 不改现有 Node / Electron attach 主链行为
- 不由 `PrismTrace` 负责自动拉起 `opencode` server

## 4. 选定方案

本轮采用 `observer-parity` 方案，而不是继续保留 snapshot 原型。

也就是说：

- 保留 `observer.rs` 作为统一接口层
- 参照 `codex_observer.rs` 的组织方式，把 `opencode_observer.rs` 补成正式的 `ObserverSource / ObserverSession / artifact writer / CLI` 链路
- 第一版优先做“连接现有 server 并采集高层资源”，不引入额外控制台或自动拉起逻辑

这样做的原因是：

- 能把“资源采集做实”与“控制台展示”解耦
- 能让 `opencode` 从一开始就和 `Codex` 共用 observer 主壳
- 后续接统一 timeline / inspector 时不需要再拆一次底层实现

## 5. 接入边界

### 5.1 不复用 attach 语义

`opencode` 官方接入不应经过：

- `AttachController`
- `LiveAttachBackend`
- `InstrumentationRuntime`
- probe bootstrap
- `HttpRequestObserved / HttpResponseObserved`

原因是这些抽象都围绕“把探针注入目标进程”设计，而 `opencode` 的优势恰恰在于它已经提供官方 server 观测能力。

### 5.2 在 host 层与 Codex 并列

host 内的采集层继续向 observer source 收口，第一版至少包含：

1. `CodexAppServerSource`
2. `OpencodeServerSource`

两者共享：

- `ObserverSource`
- `ObserverSession`
- `ObservedEvent`
- observer artifact 持久化方向

但仍允许各自保留面向协议差异的内部解析逻辑。

## 6. 第一版数据面

### 6.1 握手面

`GET /global/health` 只用于建立 `ObserverHandshake`，不作为普通事件输出。

握手阶段至少要产出：

- `channel = opencode-server`
- `transport = <base_url>`
- `server_label`
- `raw_json = health response`

### 6.2 快照面

启动观测后，先主动拉一轮高信息密度快照，来源包括：

- `session list`
- `session export`
- 必要时 `session message`

用途是：

- 在实时事件还不够密时，先建立可消费的 thread / item / tool 基线
- 为后续控制台和 timeline 预留稳定 artifact 输入

### 6.3 实时面

`global event` 用于补实时事件，不承担单独完成整条时间线重建的责任。

第一版策略是：

- 能稳定识别的事件才做归类
- 无法稳定归类的事件保留为 `unknown`
- 所有事件都保留 `raw_json`

## 7. 事件归一化规则

第一版不做激进语义投影，只做保守映射：

- session -> `ObservedEventKind::Thread`
- message / text / reasoning / 普通 part -> `ObservedEventKind::Item`
- tool part / tool result -> `ObservedEventKind::Tool`
- permission / approval -> `ObservedEventKind::Approval`
- agent 相关事件 -> `ObservedEventKind::Agent`
- MCP server 相关事件 -> `ObservedEventKind::Mcp`
- provider 相关事件 -> `ObservedEventKind::Provider`
- plugin 相关事件 -> `ObservedEventKind::Plugin`
- command 相关事件 -> `ObservedEventKind::Command`
- app 相关事件 -> `ObservedEventKind::App`
- 无法稳定判断的实时事件 -> `ObservedEventKind::Unknown`

注意：opencode 的 agent / MCP server / provider 不等价于 Codex 的 skill / plugin / app，底层 observer 必须保留真实协议域。

每条 `ObservedEvent` 至少保留：

- `channel_kind = OpencodeServer`
- `event_kind`
- `summary`
- `method`
- `thread_id`
- `turn_id`
- `item_id`
- `timestamp`
- `raw_json`

这里 `raw_json` 是第一版的强约束，不能省略。

## 8. Artifact 持久化

artifact 策略直接对齐 `Codex`：

- 路径：`.prismtrace/artifacts/observer_events/opencode/*.jsonl`
- 第一行写 `record_type = handshake`
- 后续逐行追加 `record_type = event`

每条记录至少包含：

- `record_type`
- `channel`
- `event_kind`
- `summary`
- `method`
- `thread_id`
- `turn_id`
- `item_id`
- `timestamp`
- `recorded_at_ms`
- `raw_json`

这样后续控制台无需依赖 live observer 才能消费 `opencode` 数据。

## 9. CLI 行为

第一版只提供最小采集入口：

- `--opencode-observe`
- `--opencode-url <url>`

默认行为是：

1. 输出 host startup summary
2. 连接现有 `opencode` server
3. 输出一条 handshake 记录
4. 拉取一轮 snapshot 事件
5. 再读取最小实时事件
6. 同步将 handshake / event 全部写入 artifact

本轮明确不做：

- 自动拉起新的 `opencode serve`
- 独立 `--opencode-export <session_id>` 调试命令
- 面向 UI 的额外交互协议

## 10. 风险与降级

### 风险 1：实时事件流信息密度不足

应对：

- 第一版不依赖纯实时流
- 用 `session list + export/message` 先建立快照基线

### 风险 2：`opencode` 事件语义在版本间变化

应对：

- 事件投影保持保守
- 统一保留 `raw_json`
- 无法识别时回退到 `unknown`

### 风险 3：server 连接参数未来可能扩展

应对：

- 第一版只支持连接现有本地 server
- 后续再补认证、自动发现、自动拉起等能力

## 11. 验证策略

本轮验证分三层：

1. 协议层测试
   - 覆盖 health、空 session、未知事件、错误响应
2. host 集成测试
   - 覆盖 CLI 参数解析、observer 事件输出、artifact 落盘
3. 本地基线
   - `cargo fmt --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
   - `cargo run -p prismtrace-host -- --discover`
