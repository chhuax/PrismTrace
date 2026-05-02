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

## 14. 控制台状态页映射与缺页清单

### 14.1 信息架构硬约束

后续控制台页面必须遵守以下分工，避免再次出现“导航切了，但主体内容还停在原页”的错误：

- 左侧只负责：
  - 一级导航
  - source 摘要
  - sessions 列表等入口型列表
- 中间必须是主内容画布：
  - 总览 feed
  - empty state
  - no-selection guidance
  - session timeline drilldown
  - source unavailable 主态
- 右侧只负责辅助面板：
  - inspector
  - timeline 辅助态
  - health / source diagnosis

也就是说：

- 左侧导航切换的对象是“中间主画布页面”
- 右侧只跟随当前 selection 改变为空态或详情态
- 不能再把 `activity`、`sessions` 的主体内容塞回左侧栏

### 14.1.1 应用壳层必须统一

除了上面的分工约束，这一轮还必须增加一条更硬的设计约束：

`所有状态页必须共用同一套应用壳层，不能每张稿各自发明顶部和侧栏。`

当前壳层唯一基准页明确指定为：

- [prismtrace_活动首页_中文版_白色版/code.html](/Volumes/MacData/workspace/PrismTrace/html/stitch_prismtrace_console_redesign_explorations/prismtrace_活动首页_中文版_白色版/code.html)

后续所有 Stitch 稿和实现稿，都要以这张页面的壳层为母版：

- 顶部栏以它为准
- 左栏以它为准
- 右栏以它为准
- 页面变化只允许发生在中间主画布

具体来说，下面这些内容在不同页面之间必须保持一致：

- 顶部左侧品牌区
- 顶部右侧全局 `Theme / Language` 控件
- 左栏一级导航：`Activity / Sources / Sessions`
- 右栏三块辅助模块的骨架：`Timeline / Inspector / Observability Health`

允许变化的只有：

- 中间主画布内容
- 左栏对象列表的数据内容
- 右栏模块在同一骨架下的空态 / 摘要态 / 详情态

不允许再出现：

- 页面 A 的右上角是一套控件，页面 B 的右上角换成另一套控件
- 页面 A 的顶部是 observer console，页面 B 的顶部又像另一款产品
- 页面 A 的左栏是 `Activity / Sources / Sessions`，页面 B 又变成 `Dashboard / Metrics / Logs`
- 页面 A 的右栏是 `Inspector / Health / Timeline`，页面 B 右栏顺序、命名、结构全变

这条约束的实现意义是：

- 程序员只需要实现一个固定 `App Shell`
- 真正切换的是中间主画布页面与右栏子态
- 设计稿必须服务这个实现模型，而不是制造多个互不兼容的页面模板

如果后续某张设计稿与这张基准页在顶部左侧、顶部右侧、左栏导航或右栏模块骨架上不一致，应优先修改设计稿，而不是要求程序员为该页面单独兼容另一套框架。

### 14.2 当前权威设计稿清单

当前这组 Stitch 页面应视为 observer-first 控制台的权威评审稿集合。后续实现必须以这组页面为准，不再把旧稿、试验稿、模块页和首页稿混用。

- 首页总览：
  - `html/prismtrace_observer_console_overview_dark_with_global_theme_and_language`
  - `html/prismtrace_observer_console_overview_light_with_global_theme_and_language`
  - `html/prismtrace_activity_global_feed_state`
  - `html/prismtrace_activity_event_selected_state`
  - 说明：`Activity` 是默认首页；这组稿同时定义了首页壳层、全局 `Theme / Language` 控件，以及“点选 event 后中间主画布不切页”的主交互
- Sessions：
  - `html/prismtrace_timeline_no_selection_state_design_review`
  - `html/prismtrace_session_timeline_drilldown`
- Sources：
  - `html/prismtrace_source_feed_available_state`
  - `html/prismtrace_sources_unavailable_state`
  - 说明：当前已明确有 `healthy / degraded / unavailable` 三类 sources 主态语义
- Events：
  - `html/prismtrace_events_empty_state_design_review`
- 通用中间主画布：
  - `html/prismtrace_no_selection_guidance_state`
- Inspector：
  - `html/prismtrace_inspector_no_selection_state_design_review`
  - `html/prismtrace_inspector_event_detail_state`
  - `html/prismtrace_inspector_event_detail_state_light`
- 壳层布局交互：
  - `html/prismtrace_activity_left_sidebar_collapsed_state`
  - `html/prismtrace_activity_right_sidebar_collapsed_state`

这意味着：

- 首页壳层不是只有深色稿，而是必须同时兼容深色、浅色和全局 `Theme / Language`
- `Activity / Sources / Sessions / Inspector` 的核心主态、空态、异常态和部分交互态已经基本成套
- 后续如果还有“页面缺失”，应该以这套清单为基线去判断，而不是重新怀疑首页 / sessions / inspector 是否已有稿

### 14.3 `Codex / opencode` 真实可采信息与展示维度

在页面收口前，必须先把“后端到底采到了什么”与“前端应该按什么维度组织”分开。否则就会出现把 capability snapshot、运行时事件、source 健康、session timeline 全堆到同一块 feed 里的问题。

#### 14.3.1 当前后端真实可采到的信息

按当前 observer 实现，后端已经能稳定采到下面这几层信息：

- Source 层：
  - channel kind：`codex-app-server` / `opencode-server`
  - transport label
  - server label
  - source display name
  - source last seen / session 数 / event 数
- Session 层：
  - observer session id
  - started / completed 时间
  - session 下 event 总数
  - tool event 数
  - artifact 路径
  - transport / server label
- Event 层：
  - event kind：`thread / turn / item / tool / approval / hook / plugin / skill / app / unknown`
  - method
  - summary
  - occurred_at_ms
  - raw_json
- Context 关联层：
  - `thread_id`
  - `turn_id`
  - `item_id`
  - `timestamp`

其中，`Codex` 额外会在初始化阶段主动探 capability snapshot：

- `skills/list`
- `mcpServer/listStatus`
- `plugin/list`
- `app/list`

这些 snapshot 当前会被归为：

- `skill`
- `mcp`
- `plugin`
- `app`

它们本质上是“能力目录信息”，不是运行过程中的主事件流。

#### 14.3.2 这批信息应该分成 4 类，而不是混成一种 feed

从产品信息架构上，`Codex / opencode` 的观测信息至少要拆成 4 类：

##### A. 观测对象是谁

这是 Source 维度，回答：

- 现在到底在看谁
- 是 `Codex` 还是 `opencode`
- 连接是否活着
- 最近有没有 session

适合展示为：

- 左侧 `Active Sources`
- 首页 source 状态摘要
- source unavailable / degraded 状态页

不适合展示为：

- 中间主 feed 的逐条事件卡片

##### B. 运行时发生了什么

这是 Runtime Event 维度，回答：

- 一个 thread/turn/item 内发生了哪些动作
- 什么时候发起 tool call / approval / hook
- 哪个 session 正在流动

对应数据就是：

- `thread`
- `turn`
- `item`
- `tool`
- `approval`
- `hook`
- `unknown`

适合展示为：

- 首页中间主 feed
- session drilldown 中间主画布
- 右侧 inspector detail

这才是中间主画布最应该承载的“主剧情”。

##### C. 当前系统拥有哪些能力

这是 Capability Catalog 维度，回答：

- 这个 `Codex` 当前暴露了哪些 skills
- 哪些 plugins / apps 可用
- observer 启动时能力目录是什么

对应数据就是：

- `skill`
- `plugin`
- `app`

适合展示为：

- source inspector 中的 capability 摘要
- source detail / auxiliary panel
- 次级 overview 模块

不适合展示为：

- 首页主 feed 的高频主内容

因为它们更像“环境清单”或“启动快照”，不是用户最关心的执行时间线。

##### D. 过程上下文如何串起来

这是 Correlation 维度，回答：

- 这一条事件属于哪个 thread
- 属于哪个 turn
- 与哪个 item 关联
- 同一 session 内前后关系是什么

对应字段就是：

- `thread_id`
- `turn_id`
- `item_id`
- `session_id`
- 时间戳

适合展示为：

- 右侧 inspector 的上下文区
- session drilldown 的时间线辅助信息
- 过滤 / 搜索 / 跳转锚点

它不应该单独成为一级导航，但应该成为所有详情页的核心结构。

#### 14.3.3 首页、中间、右侧各自应该看什么

基于上面的 4 类信息，页面分工应固定如下：

- 左侧：
  - 看“观测对象是谁”
  - 展示 source、session 入口、状态灯、活跃摘要
- 中间：
  - 看“运行时发生了什么”
  - 主画布承载 runtime events、empty state、no-selection、session drilldown
- 右侧：
  - 看“上下文怎么串”和“附属能力有什么”
  - 承载 inspector detail、timeline 辅助态、source diagnosis、capability 摘要

一句话概括就是：

- 左侧看对象
- 中间看过程
- 右侧看解释

#### 14.3.4 `Codex` 与 `opencode` 应该用同一套展示骨架

`Codex` 和 `opencode` 不应该做成两张不同的产品脑图，而应该共用同一套 observer console 骨架：

- source 是谁
- session 在哪里
- event 正在怎么流动
- inspector 如何解释当前选中对象

区别只体现在：

- source label
- capability catalog 内容
- runtime event 密度与 method 命名
- 某些 channel 特有字段

也就是说：

- 页面维度应统一
- 具体内容按 channel 类型适配

#### 14.3.5 当前实现最大的认知错误

当前页面之所以容易“看起来乱”，根本原因不是样式，而是把不同维度的信息混排了：

- 把 `skill/plugin/app` 当成主 feed 事件
- 把 sessions 当成右侧小组件，而不是中间主画布切换态
- 把 source 健康和 runtime event 堆在同一视图层

后续实现必须先按维度拆开，再谈具体卡片长什么样。

#### 14.3.6 展示内容关键词体系（中英双语）

为了支持 `中文 / EN` 切换，必须先把“页面上会出现的词”按层级梳理清楚。这里的目标不是做逐字翻译，而是建立一套稳定的术语体系，避免同一个概念在不同页面里出现多种叫法。

##### A. 一级导航词

这些词出现在全局导航，决定用户“从哪个角度看系统”。

| 语义层级 | 英文 | 中文 | 说明 |
| --- | --- | --- | --- |
| 一级导航 | `Activity` | `活动` | 全局运行态总览；默认 landing page |
| 一级导航 | `Sources` | `来源` | 按 observer source 查看 |
| 一级导航 | `Sessions` | `会话` | 按 session 查看 |

约束：

- `Activity / Sources / Sessions` 是“看问题的角度”，不是数据类型本身
- `Events / Timeline / Inspector / Health` 不是一级导航词，而是页面内部区域词

##### B. 页面区域词

这些词用来描述页面区域和固定模块。

| 语义层级 | 英文 | 中文 | 说明 |
| --- | --- | --- | --- |
| 主区域 | `Unified Telemetry Feed` | `统一遥测事件流` | 中间主画布的主 feed 名称 |
| 左侧模块 | `Active Sources` | `活跃来源` | 当前可观察 source 列表 |
| 中间模块 | `Session Events` | `会话事件流` | `Sessions` drilldown 中间主区 |
| 右侧模块 | `Session Timeline` | `会话时间线` | 右侧时间线辅助块 |
| 右侧模块 | `Temporal Timeline` | `时序时间线` | 首页右侧时间摘要块 |
| 右侧模块 | `Inspector` | `检查器` | 当前选中对象详情 |
| 右侧模块 | `Observability Health` | `观测健康度` | source / observer 健康摘要 |
| 过滤上下文 | `Filter Context` | `筛选上下文` | 当前筛选条件摘要 |

约束：

- `Feed` 偏主流程，不要翻成“日志”
- `Inspector` 建议统一为 `检查器`，不要一会儿叫“详情”，一会儿叫“检视器”
- `Timeline` 是结构化时间关系，不要和 `History`、`Log` 混用

##### C. 观测对象词

这些词回答“我们到底在看谁”。

| 语义层级 | 英文 | 中文 | 说明 |
| --- | --- | --- | --- |
| 观测对象 | `Source` | `来源` | observer source，不等于 attach target |
| 观测对象 | `Channel` | `通道` | `codex-app-server / opencode-server` 这类协议通道 |
| 观测对象 | `Server Label` | `服务标签` | 人类可读的 source 标识 |
| 观测对象 | `Transport` | `传输方式` | socket / stdio 等 |
| 观测对象 | `Session` | `会话` | 一次 observer session |
| 观测对象 | `Event` | `事件` | session 内的单个观测事件 |

约束：

- `Source` 统一译为 `来源`，不译成“目标”
- `Target` 只能保留在旧兼容字段中，不再作为产品主词
- `Session` 用 `会话`，不要与 `连接`、`任务` 混用

##### D. 运行时事件词

这些词回答“运行时发生了什么”，是中间主画布的主词汇表。

| 英文枚举 | 中文建议 | 展示说明 |
| --- | --- | --- |
| `thread` | `线程` | 上层工作链路 |
| `turn` | `轮次` | 一次交互轮次 |
| `item` | `条目` | 轮次中的内容单元 |
| `tool` | `工具调用` | 运行时工具行为 |
| `approval` | `授权确认` | 权限/审批相关动作 |
| `hook` | `钩子事件` | 生命周期或扩展钩子 |
| `unknown` | `未分类事件` | 无法归类的运行时消息 |

约束：

- 这些是“事件种类词”，应该主要出现于中间主画布和右侧 detail
- `tool` 不要翻成“工具”单独出现，推荐完整写成 `工具调用`
- `approval` 不要翻成“批准”，更接近用户语义的是 `授权确认`

##### E. 能力目录词

这些词回答“当前系统有什么能力”，不属于主事件流。

| 英文枚举 | 中文建议 | 展示说明 |
| --- | --- | --- |
| `skill` | `技能` | Codex skill catalog |
| `plugin` | `插件` | plugin marketplace / local plugin |
| `app` | `应用` | app capability catalog |
| `capability` | `能力` | 泛称 |
| `capability snapshot` | `能力快照` | 启动或刷新时的目录状态 |

约束：

- `skill / plugin / app` 是“能力目录词”，优先进入右侧摘要或 source detail
- 不应把它们当成首页中间主剧情的高频标签

##### F. 上下文关联词

这些词回答“这一条事件挂在哪个链路上”。

| 英文 | 中文 | 说明 |
| --- | --- | --- |
| `thread id` | `线程 ID` | thread 关联键 |
| `turn id` | `轮次 ID` | turn 关联键 |
| `item id` | `条目 ID` | item 关联键 |
| `session id` | `会话 ID` | session 关联键 |
| `timestamp` | `时间戳` | 事件时间字段 |
| `artifact path` | `归档路径` | 原始 artifact 存储位置 |
| `raw payload` | `原始载荷` | 原始 JSON / 原始消息内容 |

约束：

- 这些词主要属于 detail / inspector，不该大量占用导航区
- `ID` 字段保持英文缩写 `ID`，不翻成“编号”

##### G. 状态词

这些词回答“当前页面/对象处于什么状态”。

| 英文 | 中文 | 说明 |
| --- | --- | --- |
| `active` | `活跃` | source/session 正在工作 |
| `idle` | `空闲` | 当前无活跃流动 |
| `degraded` | `降级` | source 异常但未完全失联 |
| `unavailable` | `不可用` | source 不可访问 |
| `live` | `实时` | 正在流式更新 |
| `observed` | `已观测` | 已被 observer 捕获 |
| `selected` | `已选中` | 当前焦点对象 |
| `no selection` | `未选中` | 需要先选对象 |
| `empty` | `空态` | 当前维度没有可展示内容 |

约束：

- `degraded` 和 `unavailable` 不能混用
- `empty` 是页面态，不是 source 健康态
- `no selection` 是交互态，不是数据缺失态

##### H. 动作词

这些词会出现在按钮、链接和交互提示中。

| 英文 | 中文 | 说明 |
| --- | --- | --- |
| `View Inspector` | `查看检查器` | 跳到右侧详情 |
| `Retry Connection` | `重试连接` | source 恢复动作 |
| `View Connection Logs` | `查看连接日志` | source 异常诊断 |
| `Modify Filters` | `调整筛选条件` | 空态引导动作 |
| `Run Test Ping` | `发送测试探测` | 空态/连接验证动作 |
| `Jump to Session` | `跳转到会话` | 从 event detail 跳 session |
| `Locate in Timeline` | `在时间线中定位` | 从 event detail 反查时间点 |

约束：

- 动作词优先使用“动词 + 对象”的短语结构
- 不要把按钮文案写成名词，例如单独 `Inspector`、`Timeline`

#### 14.3.7 中英文切换规则

为了避免语言切换把数据和 UI 混在一起，必须遵守这些规则：

##### 只翻 UI，不翻原始数据

应该翻译的：

- 导航文案
- 区块标题
- 状态文案
- 按钮文案
- 空态 / 错误提示
- 事件种类的人类可读标签

不应该翻译的：

- `raw_json`
- 原始 `method`
- source 真实名称
- server label 原值
- `thread_id / turn_id / item_id / session_id`
- artifact path

##### UI 词和数据词分层

- UI 层可以显示：
  - `工具调用`
  - `授权确认`
  - `活跃来源`
- 数据层保留：
  - `tool`
  - `approval`
  - `Codex Desktop/...`

也就是：

- 页面标题翻译
- 原始 payload 不翻译
- 枚举值通过 label map 转成人类可读文案

##### 英文优先作为内部 canonical key

后续前端实现时应以英文作为稳定 key，例如：

- `activity`
- `sources`
- `sessions`
- `thread`
- `turn`
- `tool`
- `degraded`
- `unavailable`

中文只作为展示 label，不反向进入状态机或业务判断。

##### 推荐的文案分层

建议把文案资源至少拆成这 4 组：

- `nav.*`
  - `nav.activity`
  - `nav.sources`
  - `nav.sessions`
- `panel.*`
  - `panel.feed`
  - `panel.inspector`
  - `panel.timeline`
  - `panel.health`
- `state.*`
  - `state.no_session_selected`
  - `state.no_event_selected`
  - `state.awaiting_telemetry`
  - `state.source_unavailable`
- `action.*`
  - `action.view_inspector`
  - `action.retry_connection`
  - `action.modify_filters`

这样后面再考虑页面展示时，讨论的是：

- 哪个 panel 用哪些词
- 哪种 state 该触发哪组文案

而不是一边改页面，一边临时想中文怎么写。

### 14.4 已有设计稿的页面职责

#### A. 首页总览

设计稿：

- `html/prismtrace_observer_console_overview_dark_with_global_theme_and_language`
- `html/prismtrace_observer_console_overview_light_with_global_theme_and_language`
- `html/prismtrace_activity_global_feed_state`

出现条件：

- 默认进入 observer console
- 当前处于 `Activity` 默认 landing page
- 已存在 source，且可展示 `Unified Telemetry Feed`

想要的效果：

- 左侧显示 `Activity / Sources / Sessions` 一级导航，以及跨 source 摘要
- 中间显示 `Unified Telemetry Feed`
- 右侧显示 `Temporal Timeline`、`Observability Health` 摘要与 `Inspector` 空态

#### B. Activity Event Selected State

设计稿：

- `html/prismtrace_activity_event_selected_state`

出现条件：

- 当前位于 `Activity`
- 用户点中中间主画布中的某条 event

想要的效果：

- 中间主画布继续保留 `Unified Telemetry Feed`
- 当前 event 行高亮，但主画布不切页
- 右侧 `Inspector` 切到结构化 detail，`Timeline` 高亮对应时间点

#### C. Source Feed Available State

设计稿：

- `html/prismtrace_source_feed_available_state`

出现条件：

- 当前一级导航切到 `Sources`
- 用户已选中可用 source
- 当前 source 存在可展示事件

想要的效果：

- 左侧保留 source 列表，并突出当前 source
- 中间主画布显示该 source 的 scoped feed，而不是全局 feed
- 右侧保持 `Source Health`、压缩时间线与 `Inspector` 空态

#### D. Events Empty State

设计稿：

- `html/prismtrace_events_empty_state_design_review`

出现条件：

- 当前主画布语义仍是 `Unified Telemetry Feed`
- 但当前 source / filter / time window 下没有事件可显示

想要的效果：

- 中间主画布显示大面积空态和引导动作
- 左侧仍保留来源导航
- 右侧保持 inspector 空态与 timeline 空态

#### E. Sources Unavailable State

设计稿：

- `html/prismtrace_sources_unavailable_state`

出现条件：

- 当前选中 source 处于 unavailable / degraded / disconnected
- 无法继续实时展示该 source 的 telemetry

想要的效果：

- 中间主画布用大面积异常态解释当前 source 不可用
- 右侧显示该 source 的诊断信息与恢复动作
- 左侧 source 列表保留状态灯与错误摘要

#### F. Timeline No-Selection State

设计稿：

- `html/prismtrace_timeline_no_selection_state_design_review`

出现条件：

- 当前一级导航切到 `Sessions`
- 左侧已有 sessions 列表
- 但用户尚未选中任何 session

想要的效果：

- 左侧显示 sessions 列表
- 中间主画布显示 “No Session Selected” 的大面积引导态
- 右侧 inspector 保持空态

#### G. Session Timeline Drilldown

设计稿：

- `html/prismtrace_session_timeline_drilldown`

出现条件：

- 当前一级导航切到 `Sessions`
- 用户已选中具体 session

想要的效果：

- 左侧继续保留 sessions 列表，并突出当前选中 session
- 中间主画布展示该 session 的事件流与步骤卡片
- 右侧展示 session timeline 与当前 event inspector

#### H. No-Selection Guidance State

设计稿：

- `html/prismtrace_no_selection_guidance_state`

出现条件：

- 当前主模块需要用户先从左侧选择实体
- 但中间主画布还没有可渲染的具体对象

想要的效果：

- 用于中间主画布的通用未选中引导态
- 不能降级成只在右侧显示一句提示

#### I. Inspector No-Selection State

设计稿：

- `html/prismtrace_inspector_no_selection_state_design_review`

出现条件：

- 中间 feed 已有内容
- 但当前未点中具体 event

想要的效果：

- 右侧 inspector 显示 “No Event Selected”
- 中间主画布不应被空态覆盖

#### J. Inspector Event Detail State

设计稿：

- `html/prismtrace_inspector_event_detail_state`
- `html/prismtrace_inspector_event_detail_state_light`

出现条件：

- 用户点中某条 event

想要的效果：

- 右侧展示结构化 event detail
- 至少包含上下文标识、状态、时间戳、raw payload
- 中间主画布继续保留当前 feed 或 session 页面，不应被替换

#### K. Shell Collapse States

设计稿：

- `html/prismtrace_activity_left_sidebar_collapsed_state`
- `html/prismtrace_activity_right_sidebar_collapsed_state`

出现条件：

- 用户点击左栏或右栏的收起按钮

想要的效果：

- 只改变左右栏宽度分配
- 中间主画布扩展，但页面语义与 selection 不变
- 不能因为收起侧栏而清空当前上下文

### 14.5 当前实现对齐缺口

当前情况需要拆开描述：

- 设计稿层面：
  - `Activity` 首页总览、正常主态与 event selected 主态已经齐，并带 `Theme / Language`
  - `Sources` 的正常主态与异常态语义已明确
  - `Sessions` 的 `no-selection -> drilldown`
  - `Events empty`
  - `No-selection guidance`
  - `Inspector detail / no-selection`
  - 左栏 / 右栏收起壳层态
  这些 observer-first 核心评审页已经基本齐全
- 实现层面：
  - 当前 host console 已接入 observer 数据，但页面状态机与最新设计稿仍未完全对齐

当前主要实现缺口如下：

- `Sessions` 虽已有 `no-selection` 与 `drilldown` 设计稿，但实现还没有严格按这两态切换中间主画布
- `Sources` 虽已有 `healthy / degraded / unavailable` 设计语义，但实现还没有接入真实 source 异常态、degraded 态或 reconnect 诊断流
- `Events empty` 与通用 `No-selection guidance` 已有设计稿，但还没有真正落实成中间主画布状态机
- 右侧 inspector 仍偏 raw dump，尚未完整收口到 `no-selection -> structured detail` 两态
- 左右栏虽已有收起态设计稿，但实现还没有建立稳定的布局回流和状态持久化
- 首页虽已接入 `Theme / Language` 壳控件，但尚未真正实现全局主题切换和中英文切换的数据与文案同步

### 14.6 后续实现原则

实现顺序必须按“先页面状态机，后细节样式”推进：

1. 先让导航切换真正驱动中间主画布
2. 再让右侧 panel 只跟随 selection 变化
3. 最后再补每个状态页里的字段密度与视觉收口

如果缺新的设计态，应先补齐“页面目的、触发条件、想要效果”，再继续编码，避免再靠猜测拼页面。
