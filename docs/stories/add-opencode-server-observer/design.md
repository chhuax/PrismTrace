# Opencode Server Observer 设计稿

日期：2026-04-26  
状态：草案，待进入实现

## 1. 背景

`opencode` 的实机验证已经表明：

- 现有 `SIGUSR1 + inspector` attach 路线会把 live `opencode` 打死
- `opencode` 自带官方 `server + attach(url)` 路线
- `opencode serve` 的官方 HTTP server、OpenAPI、session list、session export 都已验证可用
- 官方 plugin 事件面明确存在

因此，`opencode` 不应再被视为“等 Bun attach 再修一修”的目标，而应被视为一条新的官方观测后端。

这一轮的目标是把下面这件事收敛到可实施：

`PrismTrace host 如何把 opencode server 当成新的官方观测后端接入，并在第一版交付稳定的高层运行时事件。`

## 2. 目标

本 story 第一版要解决四件事：

1. 明确 `opencode` 观测后端的接入方式
2. 明确第一版事件面和统一读模型
3. 明确它与现有 attach/probe 路线如何并存
4. 明确最小实现入口先做 CLI/host 验证，而不是先做复杂 UI

## 3. 非目标

这一版明确不做：

- 不做 Bun runtime attach
- 不再尝试 `SIGUSR1` / inspector 路线
- 不承诺获取 `opencode -> 模型后端` 的原始 HTTP 报文
- 不先做本地控制台复杂展示
- 不研究 `Codex` live transport 细节
- 不改现有 Node/Electron attach 主链行为

## 4. 用户价值

如果这条路线接通，`PrismTrace` 对 `opencode` 的第一版价值会从“容易把目标打崩”变成：

- 能看到 `opencode` 一轮任务是怎么推进的
- 能看到 session / message / part / reasoning / tool 的结构化时间线
- 能看到 model / provider / token / finish reason
- 能看到 permission / command / plugin / server 事件

这意味着我们先把 `PrismTrace` 做成一个：

`opencode 专用高层运行时观测器`

而不是继续误当成“Bun 进程注入抓包器”。

## 5. 方案总览

### 5.1 新增一条并行的官方观测后端

现有 host 主线是：

- 发现目标
- readiness 判断
- attach controller
- probe bootstrap
- request / response / tool visibility capture

这条链路继续服务 Node / Electron attach 路线，不为 `opencode` 改语义。

对 `opencode`，新增一条并行后端：

- 连接运行中的 `opencode` server，或启动最小 headless server
- 读取高层运行时事件与结构化 session 数据
- 将其归一化成 host 内统一的 observability event
- 第一版先走 CLI/host 输出，不强依赖控制台 UI

### 5.2 接入形态

建议在 `prismtrace-host` 中新增独立入口，而不是复用 `attach`。

推荐的最小入口形态：

- `--opencode-observe`
  - 优先连接现有 `opencode` server
  - 必要时允许指定 URL
  - 输出初始化结果和后续高层事件摘要

可选调试入口：

- `--opencode-url <url>`
  - 直接指定 server URL
- `--opencode-export <session_id>`
  - 直接读取结构化 session 导出，做离线验证

这样做的原因是：

- 避免把 `opencode` 混进 `--attach <pid>`
- 避免用户误以为 `opencode` 仍然走 inspector attach
- 让 CLI 行为直接表达“这是官方 server 观测，不是进程注入”

## 6. 第一版最小事件面

第一版只接这几类高价值事件面：

### 6.1 session

表示一段会话的创建、更新、结束或导出结果。

产品用途：

- 会话时间线
- 会话筛选和索引

### 6.2 message

表示 user / assistant 消息。

产品用途：

- 会话回放
- 高层输入输出观察

### 6.3 part / item

表示消息中的分段内容，如：

- text
- reasoning
- step-start
- step-finish
- tool

产品用途：

- “这一轮到底做了哪些步骤”
- “消息内部是怎么组成的”

### 6.4 tool

表示工具调用及其结果。

产品用途：

- 工具链分析
- 故障定位

### 6.5 approval / permission

表示需要确认、审批或等待确认的状态变化。

产品用途：

- 定位“为什么停住”
- 权限链路解释

### 6.6 plugin / command / server event

表示插件事件、命令事件和 server 生命周期事件。

产品用途：

- 运行时联动分析
- 事件驱动观测

### 6.7 model summary

表示：

- provider
- model
- token
- finish reason

产品用途：

- 模型与成本分析

## 7. 统一数据模型建议

`opencode` 不应被强行塞进现有 `HttpRequestObserved` / `HttpResponseObserved`。

建议直接挂到新的统一 observer 抽象上：

- `ObserverSourceKind::OpencodeServer`
- `ObservedEventKind`
- `ObservedEvent`

第一版 `ObservedEvent` 最少应包含：

- `timestamp_ms` 或原始时间字段
- `source = opencode_server`
- `event_kind`
- `external_session_id`
- `external_message_id`（可见时）
- `external_part_id`（可见时）
- `summary`
- `raw_json`

这里保留 `raw_json` 很重要，因为第一版还在探索 `opencode` 官方事件与导出结构的实际信息密度。

## 8. 与现有 attach 路线如何并存

### 8.1 不复用 attach controller

`opencode` 官方接入不应经过：

- `AttachController`
- `LiveAttachBackend`
- `InstrumentationRuntime`
- probe bootstrap

原因：

- 这些抽象都是围绕“把探针注入目标进程”设计的
- `opencode` 的优势恰恰是它已经有官方 server / export / plugin 面

### 8.2 在 host 层并列多个 source

建议 host 在采集面上形成至少三条 source：

1. `AttachProbeSource`
   - 现有 Node / Electron attach 路线

2. `CodexAppServerSource`
   - `Codex` 官方接入路线

3. `OpencodeServerSource`
   - `opencode` 官方 server 路线

它们最终都可以汇聚到：

- 统一 observer 事件层
- 统一 artifact 存储策略
- 统一 timeline / inspector 演进方向

### 8.3 readiness 维持保守

当前 `readiness` 中对 `opencode` 继续标记为不适合 attach 是合理的。

后续如果控制台里要给用户“如何观测 opencode”的引导，应显示为：

- 不支持 attach
- 但支持官方 observer/server 路线

## 9. 最小实现建议

### 9.1 第一版功能

只实现：

1. 连接指定 `opencode` server URL
2. 打印基本握手 / 健康结果
3. 拉取最小 session 列表或导出结果
4. 订阅最小事件流，打印结构化摘要

### 9.2 第一版优先级

推荐顺序：

1. `health + session list`
2. `export session`
3. `global/event`
4. 如有必要再补 plugin event

原因：

- 这样最容易形成最小可演示闭环
- 也最不依赖对 `opencode` 私有协议的过度猜测

## 10. 当前最稳的产品判断

如果 `PrismTrace` 走 `opencode` 官方接入路线，当前最现实的产品定位是：

`一个通过官方 server / export / event 面接入的高层运行时观测器`

它最适合做：

- 会话时间线
- session / message / reasoning / tool 结构化观察
- 模型与成本分析
- plugin / permission / command / server 事件观察

它当前不适合直接做：

- Bun attach
- 原始 HTTP 抓包器
- 线包级响应流还原

## 11. 下一步建议

下一步建议按最小风险顺序推进：

1. 新增 `OpencodeServerSource` 模块
2. 先打通 CLI 与 health / session list / export
3. 再接 `global/event`
4. 最后再决定是否补 plugin 事件或控制台展示
