# Observer Source Abstraction 设计稿

日期：2026-04-26  
状态：草案

## 1. 背景

`PrismTrace` 最初的主线几乎完全围绕 `attach + probe + request/response capture` 展开。

这条路线对纯 Node / Electron 目标仍然成立，但最近两条真实目标线已经证明：

- `Codex.app` 存在官方 `App Server + IPC socket` 接入面，继续走 `SIGUSR1 + inspector` 会把 live `Codex` 打崩
- `opencode` 存在官方 `server + attach(url) + SDK + plugin + export` 接入面，继续走 `SIGUSR1` 也会把 live `opencode` 打死

这说明：

`PrismTrace` 不能再把“运行中 attach 注入”当成唯一接入方式。

如果继续沿用旧思路，后面每接一个目标产品都要先问：

- 能不能 attach？
- 会不会被信号打崩？
- runtime 是 Node、Electron 还是 Bun？

这会把产品架构长期锁死在“底层注入工具”的心智里。

因此需要补一层更高的抽象：

`统一上层观测协议，允许底层按不同产品实现不同 source backend。`

## 2. 目标

这份设计要解决四件事：

1. 重新定义 `PrismTrace` 的上层统一观测模型
2. 定义 source backend 抽象，让不同产品可以走不同接入路线
3. 明确现有 `attach/probe` 路线在新架构中的位置
4. 为 `Codex` 和 `opencode` 这两条已验证路线提供统一承载层

## 3. 非目标

这一版明确不做：

- 不重写现有 request/response artifact 体系
- 不在这一轮统一所有控制台 UI
- 不承诺所有目标最终都能拿到原始后端 HTTP 报文
- 不把 `Codex` / `opencode` 强行投影成 HTTP request/response 模型
- 不改现有 attach 主链的成功语义

## 4. 设计判断

### 4.1 上层不该统一成 HTTP 抓包模型

现有 `attach/probe` 路线天然偏向：

- request
- response
- tool visibility

但官方接入路线更多给的是：

- session
- turn
- item / step
- tool call
- approval
- hook
- capability snapshot

如果继续要求所有 source 都先还原成“模型后端 HTTP 包”，会导致：

- `Codex` 官方协议价值被压扁
- `opencode` 的 session / plugin / event 能力无法自然表达
- 高层产品面始终被低层抓包视角绑住

因此统一层应当统一为：

`AI 运行时观测协议`

而不是：

`模型网络报文协议`

### 4.2 下层允许多种实现方式

不同产品的最佳接入路线已经明显分化：

- Node / Electron 类目标
  - `attach + probe`
- `Codex`
  - `App Server + IPC socket`
- `opencode`
  - `server + attach(url) + SDK + plugin + export`

因此底层必须允许：

- 官方协议型 source
- 进程注入型 source
- 离线导出型 source
- 事件订阅型 source

这些都属于 source backend，不应再强行合并成一个 attach 体系。

## 5. 新的分层建议

建议把 `PrismTrace` 的采集架构拆成三层：

### 5.1 Source 层

负责和具体产品或运行时打交道。

每个 source 只关心：

- 如何连上目标
- 如何读取原始事件
- 如何处理目标特有协议

建议的 source backend：

- `AttachProbeSource`
- `CodexAppServerSource`
- `OpencodeServerSource`

未来还可以继续加：

- `ClaudeCodeSource`
- `ExportReplaySource`

### 5.2 Normalize 层

负责把不同 source 的原始事件映射成统一的 `PrismTrace` 观测事件。

这一层不关心：

- 是 IPC 过来的
- 是 HTTP server 过来的
- 是 attach probe 过来的
- 还是离线 export 过来的

它只关心统一语义。

### 5.3 Product 层

这一层为：

- artifact 落盘
- timeline
- inspector
- session reconstruction
- diff / analysis

提供统一消费面。

Product 层不应直接读取各 source 的私有协议。

## 6. 统一上层观测协议

建议新增一套高层统一事件模型。

### 6.1 顶层概念

建议统一成以下核心对象：

- `observer_session`
- `observer_event`
- `observer_capability_snapshot`

### 6.2 建议事件种类

第一版统一种类建议至少包含：

- `session_started`
- `session_updated`
- `session_completed`
- `turn_started`
- `turn_completed`
- `item_observed`
- `tool_call_observed`
- `approval_observed`
- `hook_observed`
- `capability_snapshot_observed`
- `observer_error_observed`

### 6.3 每个统一事件的最小字段

建议至少包含：

- `timestamp_ms`
- `source_kind`
- `source_session_id`
- `event_kind`
- `external_session_id`（可见时）
- `external_turn_id`（可见时）
- `external_item_id`（可见时）
- `summary`
- `raw_json`

字段说明：

- `source_kind`
  - 例如 `attach_probe`、`codex_app_server`、`opencode_server`
- `source_session_id`
  - source backend 自己的连接或观测会话 ID
- `external_*`
  - 目标产品自己的 session / turn / item 标识
- `raw_json`
  - 保留原始协议包，便于后续演进投影

## 7. 与现有 attach 路线的关系

### 7.1 attach 不再是唯一入口

现有：

- `AttachController`
- `LiveAttachBackend`
- `InstrumentationRuntime`
- probe bootstrap

这些继续存在，但它们只代表：

`AttachProbeSource`

它们不再代表整个 `PrismTrace` 的总入口。

### 7.2 现有 request/response capture 仍然有价值

现有 request / response / tool visibility 路线仍然是：

- Node / Electron 目标的重要 source
- “原始模型报文”层的主要来源

但它应该被定位为：

- 一类 source 的高价值能力

而不是：

- 所有产品都必须符合的统一上层模型

### 7.3 控制台与分析层以后应吃统一事件

后续控制台和分析层不应直接依赖：

- 只有 request/response 才能展示

而应允许展示：

- HTTP request/response 时间线
- `Codex` item / approval / hook 时间线
- `opencode` session / tool / reasoning 时间线

这正是统一上层协议的意义。

## 8. 两条已验证路线如何挂进去

### 8.1 Codex

`Codex` 目前已经收敛为：

- `CodexAppServerSource`
- 通过 `App Server + IPC socket`
- 第一版重点产出：
  - handshake
  - skill / plugin / app 快照
  - 后续 thread / turn / item / tool / approval / hook 事件

### 8.2 opencode

`opencode` 目前已经验证到：

- `opencode serve`
- `opencode attach <url>`
- `session list`
- `export`
- `plugin` 事件
- `/global/event`

因此其第一版可挂成：

- `OpencodeServerSource`
- 通过 `server + event/export`
- 第一版重点产出：
  - session
  - message / part
  - reasoning
  - tool call
  - model/provider
  - token / finish reason
  - plugin / permission / command / server 事件

## 9. 第一版最小落地建议

这一层设计不应该一上来就大改代码库，而应按最小风险顺序推进。

### 9.1 先补统一接口层

先在 host 中引入 source backend 抽象，例如：

- `ObserverSource`
- `ObserverEvent`
- `ObserverSourceKind`

### 9.2 先接 Codex，再接 opencode

原因：

- `Codex` 的第一版 observer slice 已经开始落代码
- `opencode` 的官方面已验证成熟，但还没开始进入 host 实现

建议顺序：

1. 让 `Codex` 的实现先挂到统一接口层
2. 再让 `opencode` 复用同一接口层接入

### 9.3 不急于把所有旧模型重构掉

现有 request / response / tool visibility 先保留。

新的统一事件层可以先平行存在，等：

- `Codex`
- `opencode`

这两条线都跑通后，再决定如何把控制台和分析层统一迁移。

## 10. 当前最稳的产品定义

补完这一层后，`PrismTrace` 的产品定义会从：

`一个 attach + prompt capture 工具`

升级成：

`一个支持多种官方或注入式 source backend 的 AI 运行时观测平台`

它的长期能力边界会更清楚：

- 有些 source 擅长拿原始报文
- 有些 source 擅长拿高层运行时事件
- 上层产品统一关心“发生了什么”，而不是强迫所有 source 都变成抓包器

## 11. 下一步建议

下一步建议分成两条并行线：

1. `Codex`
   继续把已落地的 `Codex observer` slice 挂稳到统一接口层，并优先解决 live transport 超时
2. `opencode`
   新开 `OpencodeServerSource` 设计与实现，优先从 `session export + global/event` 入手

从此之后，`PrismTrace` 的主架构就不再是：

- “哪个目标能 attach”

而是：

- “这个目标最适合接成哪一种 source”
