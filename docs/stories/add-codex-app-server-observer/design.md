# Codex App Server Observer 设计稿

日期：2026-04-25  
更新：2026-04-26  
状态：进行中，已补充统一 observer UI 约束

## 1. 背景

`Codex.app` 已经确认存在官方接入面：

- `Codex App Server`
- 本地 IPC socket
- `codex app-server proxy`

同时也已经确认，当前 `PrismTrace` 的 `SIGUSR1 + inspector` attach 路线对 live `Codex` 不安全，会导致运行中的 `Codex` 崩溃。因此，`Codex` 不能继续被当作“再修一修 attach 就能接”的目标，而应被视为一条新的、官方支持的数据源。

这一轮的目标不是继续争论“能不能抓到原始后端报文”，而是把下面这件事收敛到可实施：

`PrismTrace host 如何把 Codex App Server 当成新的官方观测后端接入，并在第一版交付稳定的高层运行时事件。`

## 2. 目标

本 story 第一版要解决四件事：

1. 明确 `Codex` 观测后端的接入方式
2. 明确第一版事件面和统一读模型
3. 明确它与现有 attach/probe 路线如何并存
4. 明确最小实现入口先做 CLI/host 验证，再接入 Stitch 控制台的最小 observer 展示

## 3. 非目标

这一版明确不做：

- 不做 `Codex` live attach
- 不再尝试 `SIGUSR1` / inspector 路线
- 不承诺获取 `Codex -> 模型后端` 的原始 HTTP 报文
- 不重做 Stitch 已产出的本地控制台视觉稿
- 不为 `Codex` 和 `opencode` 分别做两套独立 UI
- 不研究 `claude code`
- 不改现有 Node/Electron attach 主链的行为

## 4. 用户价值

如果这条路线接通，`PrismTrace` 对 `Codex` 的第一版价值会从“几乎不可用”变成：

- 能看到 `Codex` 一轮任务是怎么推进的
- 能看到什么时候开始、结束、卡住、报错
- 能看到用了哪些工具、skills、plugins、apps
- 能看到高层结果项和步骤时间线

这意味着我们先把 `PrismTrace` 做成一个：

`Codex 专用高层运行时观测器`

而不是继续把它误当成“Codex 原始报文抓包器”。

同时，既然 `opencode` 也已经转向 observer 方式，这一轮的产品收敛目标应进一步明确为：

`先把 Codex 打通，再让 Codex 和 opencode 共用同一套 observer 控制台语义。`

## 5. 方案总览

### 5.1 新增一条并行的官方观测后端

现有 host 主线是：

- 发现目标
- readiness 判断
- attach controller
- probe bootstrap
- request / response / tool visibility capture

这条链路继续服务 Node / Electron attach 路线，不为 `Codex` 改语义。

对 `Codex`，新增一条并行后端：

- 发现 live `Codex` IPC socket
- 作为 `Codex App Server client` 建立连接
- 读取高层运行时事件
- 将其归一化成 host 内统一的 observability event
- 第一版先走 CLI/host 输出，再接到既有 Stitch 控制台的最小 observer 视图

### 5.2 接入形态

建议在 `prismtrace-host` 中新增 `codex` 观测入口，而不是复用 `attach`。

推荐的最小入口形态：

- `--codex-observe`
  - 自动发现 live `Codex` IPC socket
  - 尝试建立最小 client 会话
  - 输出初始化结果和后续高层事件摘要

必要时允许第二种更底层入口，供调试用：

- `--codex-socket <path>`
  - 直接指定 Unix socket

这样做的原因是：

- 避免把 `Codex` 混进 `--attach <pid>`
- 避免用户以为 `Codex` 仍然走 inspector attach
- 让 CLI 行为直接表达“这是官方接入，不是进程注入”

### 5.3 底层分层

为了让后续 `opencode` 等通道接入时不影响 `Codex`，host 内部应拆成三层：

- 工厂层
  - 负责根据输入和环境构造候选 source
  - 例如：`proxy socket` 优先，`standalone app-server` 作为回退
- 接口层
  - 对上暴露统一的 `observer source / observer session / observed event`
  - 不让上层关心底层到底是 IPC socket、stdio 还是其他 transport
- 通道实现层
  - `Codex` 自己的 transport、握手和事件投影逻辑
  - 后续其他通道新增实现时，只实现同一组接口

这样 `Codex` 只是第一条实现通道，不会变成上层协议本身。

## 6. 第一版最小事件面

第一版只接这七类高价值事件面：

### 6.1 thread

表示一段会话或工作线程的开始、恢复、结束、归档等生命周期。

产品用途：

- 会话时间线
- 会话状态解释

### 6.2 turn

表示某一轮用户请求或任务轮次的开始、完成、中断等事件。

产品用途：

- 单轮任务边界
- 一轮任务耗时分析

### 6.3 item

表示 `Codex` 在 turn 中产出的各类高层步骤项，例如 message、reasoning summary、tool call、command output 等。

产品用途：

- “这一轮到底做了哪些步骤”
- “最后输出了什么”

### 6.4 tool

表示工具调用及其结果，包括本地命令、搜索、函数调用、MCP tool 等高层可观察执行。

产品用途：

- 工具链分析
- 故障定位

### 6.5 approval

表示需要用户确认、权限审批或等待确认的状态变化。

产品用途：

- 定位“为什么停住”
- 审批链路解释

### 6.6 hook

表示 hook 的开始 / 完成等生命周期事件。

产品用途：

- 理解本地自动化联动
- 判断 hook 是否参与当前任务

### 6.7 plugin / skill / app

表示当前 `Codex` 可见的扩展能力快照。

产品用途：

- 能力可见性分析
- 行为差异解释

## 7. 统一数据模型建议

第一版不要急着把 `Codex` 事件硬塞进现有 `HttpRequestObserved` / `HttpResponseObserved` 模型。建议新增一套更通用的高层事件读模型，先在 host 内独立落地。

建议新增的内部读模型概念：

- `CodexObserverSession`
- `CodexObserverEvent`
- `CodexEventKind`
- `CodexCapabilitySnapshot`

其中：

- `CodexObserverSession`
  表示一次观测连接，不等价于原有 attach session
- `CodexObserverEvent`
  表示一个归一化后的高层事件
- `CodexEventKind`
  至少包含：`thread`、`turn`、`item`、`tool`、`approval`、`hook`、`capability_snapshot`
- `CodexCapabilitySnapshot`
  表示 plugins / skills / apps 的可见性快照

第一版事件最少应包含：

- `timestamp_ms`
- `source = codex_app_server`
- `event_kind`
- `thread_id`（可见时）
- `turn_id`（可见时）
- `item_id`（可见时）
- `summary`
- `raw_json`

这里保留 `raw_json` 很重要，因为第一版我们还在探索 `Codex` 协议的实际信息密度，先把原始响应保留住，后续再稳定投影。

## 8. 与 legacy attach 路线的边界

这是这次设计最关键的边界。

### 8.1 不复用 attach controller

`Codex` 官方接入不应经过：

- `AttachController`
- `LiveAttachBackend`
- `InstrumentationRuntime`
- probe bootstrap

原因：

- 这些抽象都是围绕“把探针注入目标进程”设计的
- `Codex App Server` 是官方协议客户端模型，不是注入模型

### 8.2 在 host 层以 observer source 为主

当前 host 的产品面已经清理旧 attach 控制链，建议采集面直接以 observer source 组织：

1. `CodexAppServerSource`
   - `Codex` 官方接入路线

2. `OpencodeObserverSource`
   - `opencode` 的 observer / snapshot 读取路线

3. 后续 `ClaudeCodeTranscriptSource`
   - `Claude Code` transcript / export 路线

这些 source 最终都可以汇聚到：

- 统一 artifact 存储策略
- 统一 timeline / inspector 演进方向
- 统一控制台信息架构

但第一版先不要强行统一到一个大而全的 event schema。先在 host 内保证：

- source 分层清晰
- 存储结构清晰
- CLI 可验证

### 8.3 不再给 `Codex` 暴露 attach 心智

后续如果要在控制台中给用户“如何观测 Codex”的引导，也应直接显示：

- 使用官方 observer 路线
- 不再提供 attach 入口或 attach-ready 暗示

## 9. 统一 Observer UI 原则

Stitch 已经产出了一版本地控制台设计稿，这一版视觉方向继续保留，不重新推翻。需要调整的是信息架构和交互语义，让它从旧的 attach 心智切换到新的 observer 心智。

### 9.1 UI 调整目标

控制台需要从：

- “目标进程 attach 监控台”

调整为：

- “统一 observer 运行时观测台”

这里的“统一”有两个硬约束：

1. `Codex` 和 `opencode` 必须共用一套 UI 壳
2. 右侧 inspector、会话列表、时间线都按统一 observer 事件来组织，而不是按接入方式硬分页面

### 9.2 保留什么，不保留什么

建议保留：

- Stitch 设计稿的整体三栏布局
- 顶部导航和筛选带
- 左侧导航骨架
- 右侧 inspector 区域
- 既有视觉语言、卡片样式和信息密度

建议调整：

- `Targets` 的语义，改成 `Sources` 或 `Runtimes`
- `Requests` 的语义，扩成 `Events`
- 列表主轴从 HTTP request stream 扩成 observer event stream
- attach 状态从主叙事降级为 source health 的一部分

### 9.3 统一展示模型

控制台第一版应统一展示这几类东西：

- source
  - 例如 `codex-app-server`、`opencode`
- session
  - 一次 observer 连接或一次上层会话
- event
  - `thread / turn / item / tool / approval / hook / capability / message`

中间主列表建议统一成一套事件流表壳，最少包含：

- `Source`
- `Kind`
- `Summary`
- `Status`
- `Time`

其中：

- `Codex` 主要落到 `thread / turn / item / tool / approval / hook / capability`
- `opencode` 主要落到 `session / message / tool / snapshot`

字段不要求完全相同，但都必须能投影到同一套列表框架里。

### 9.4 Inspector 原则

右侧 inspector 继续保留一个统一容器，但内容应按事件类型自适应：

- 点 `Codex tool`，显示 tool detail
- 点 `Codex approval`，显示 approval detail
- 点 `Codex item`，显示 item / raw payload
- 点 `opencode message`，显示 message detail
- 点 `opencode tool`，显示 tool / result detail

也就是说：

- UI 壳统一
- detail 面板按事件类型切换

而不是给不同来源做两套完全独立的 detail 页面。

### 9.5 第一版展示边界

这一轮只做最小 observer 可见性，不做控制台大改版：

- 要能看到 `Codex` 和 `opencode` 的统一入口
- 要能看到 session / timeline / event 摘要
- 要能在 inspector 里看最小 detail
- 不做复杂聚合分析
- 不做深度 request payload 解释
- 不做 attach 与 observer 的全量对账界面

## 10. 最小实现建议

如果进入实现，建议只做一个最小 CLI/host slice。

### 10.1 第一版功能

只实现：

1. 自动发现 live `Codex` IPC socket
2. 建立最小 observer client
3. 完成初始化握手
4. 读取并打印高层事件摘要
5. 以结构化 JSON artifact 落盘
6. 将 `Codex` / `opencode` observer artifact 以最小方式接到统一控制台壳中

### 10.2 第一版不做

- 不重做 Stitch 视觉稿
- 不做复杂本地控制台 UI 重构
- 不做高级会话分析
- 不做深度 request inspector 融合
- 不做原始 payload 解释

## 11. 文件边界建议

如果进入实现，建议修改范围控制在这些地方：

### 必改

- `crates/prismtrace-host/src/main.rs`
  - 增加 `--codex-observe` / `--codex-socket` CLI 入口

- `crates/prismtrace-host/src/lib.rs`
  - 暴露 `Codex` observer 入口函数

- `crates/prismtrace-host/src/observer.rs`
  - 新增：统一 observer 接口层与最小事件协议

- `crates/prismtrace-host/src/codex_observer.rs`
  - 新增：observer 主逻辑、socket 发现、握手、事件读取、摘要输出

### 按需新增

- `crates/prismtrace-host/src/codex_protocol.rs`
  - 新增：最小 client message / server event 结构和解析

- `crates/prismtrace-host/src/codex_storage.rs`
  - 新增：Codex observer artifact 落盘辅助

### 暂不改

- `crates/prismtrace-host/src/runtime.rs`
- `crates/prismtrace-host/src/request_capture.rs`
- `crates/prismtrace-host/src/response_capture.rs`

## 12. 验证策略

实现前和实现后都应围绕三层验证：

### 12.1 协议层

- 最小 initialize 是否成功
- live socket 是否能建立读取循环

### 12.2 事件层

- thread / turn / item / tool / approval / hook / capability snapshot 是否能被归一化
- 未知事件是否会降级保留 raw JSON，而不是直接丢弃

### 12.3 产品层

- 用户是否能通过 CLI 直接看到 `Codex` 在做什么
- 用户是否能在统一控制台中同时看到 `Codex` 和 `opencode` 的 observer 事件
- 是否已经可以回答“它刚才做了哪些步骤、停在哪、用了什么能力”

## 13. 当前建议

当前已经足够明确进入下一阶段：

- 先开 `add-codex-app-server-observer`
- 先做 CLI/host 最小验证入口
- 再把 `Codex` 与 `opencode` 以统一 observer 语义接入 Stitch 控制台
- 始终避免回到旧 attach 心智去扩页面

这能确保 `Codex` 路线终于从“不断修 attach 失败”切换成“沿官方接入面稳定推进”，同时也让 `opencode` 在产品面上不再成为另一套平行世界。
