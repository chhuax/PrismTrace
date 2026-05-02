# Observer Source Abstraction 设计稿

日期：2026-04-26  
更新：2026-04-26  
状态：草案

## 1. 背景

`PrismTrace` 早期一度把 live attach 当成主要突破口，但对真实目标的连续验证已经给出更明确的结论：

- `Codex` 只能稳定走 `App Server + IPC socket`
- `opencode` 只能稳定走 `server + event/export`
- `Claude Code` 也不应再走 live attach，而应走 transcript / event / export

因此当前真正要统一的，不是“怎样继续保留 attach”，而是“怎样让多种可行的 observer source 进入同一套上层模型”。

## 2. 目标

这份设计要解决四件事：

1. 重新定义 `PrismTrace` 的上层统一观测模型
2. 定义 source backend 抽象，让不同产品可以走不同接入路线
3. 明确 legacy attach 已经不再属于当前 host 产品面
4. 为 `Codex`、`opencode`、`Claude Code` 这几条已验证路线提供统一承载层

## 3. 非目标

这一版明确不做：

- 不重写现有 request/response artifact 体系
- 不在这一轮统一所有控制台 UI
- 不承诺所有目标最终都能拿到原始后端 HTTP 报文
- 不把 `Codex` / `opencode` / `Claude Code` 强行投影成 HTTP request/response 模型
- 不重新引入 live attach 方案

## 4. 设计判断

### 4.1 上层不该统一成 HTTP 抓包模型

官方接入路线更多给的是：

- session
- turn
- item / step
- tool call
- approval
- hook
- capability snapshot
- transcript / export record

如果继续要求所有 source 都先还原成“模型后端 HTTP 包”，会导致：

- `Codex` 官方协议价值被压扁
- `opencode` 的 session / plugin / event 能力无法自然表达
- `Claude Code` transcript / approval / tool 事实无法自然表达

因此统一层应当统一为：

`AI 运行时观测协议`

而不是：

`模型网络报文协议`

### 4.2 下层允许多种实现方式，但不再包含 live attach

不同产品的最佳接入路线已经明显分化：

- `Codex`
  - `App Server + IPC socket`
- `opencode`
  - `server + event/export`
- `Claude Code`
  - `transcript / event / export`

因此底层必须允许：

- 官方协议型 source
- 离线导出型 source
- 事件订阅型 source

这些都属于 source backend，不应再强行合并成一个 attach 体系。

## 5. 新的分层建议

建议把 `PrismTrace` 的采集架构拆成三层：

### 5.1 Source 层

负责和具体产品或运行时打交道。

建议的 source backend：

- `CodexAppServerSource`
- `OpencodeServerSource`
- `ClaudeCodeTranscriptSource`
- `ExportReplaySource`

### 5.2 Normalize 层

负责把不同 source 的原始事件映射成统一的 `PrismTrace` 观测事件。

### 5.3 Product 层

这一层为：

- artifact 落盘
- timeline
- inspector
- session reconstruction
- diff / analysis

提供统一消费面。

## 6. 统一上层观测协议

建议统一成以下核心对象：

- `observer_session`
- `observer_event`
- `observer_capability_snapshot`

第一版统一事件种类建议至少包含：

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

## 7. 与 legacy attach 的关系

当前结论很明确：

- attach 是历史探索路线
- attach 已经不再属于 `prismtrace-host` 当前产品面
- 新的 source abstraction 不再以 attach 是否存在为前提

也就是说，这份设计不是“给 attach 找新位置”，而是“在 attach 退出主线后，为可行 source 建统一承载层”。

## 8. 三条已验证路线如何挂进去

### 8.1 Codex

- `CodexAppServerSource`
- 通过 `App Server + IPC socket`
- 第一版重点产出：
  - handshake
  - capability snapshot
  - thread / turn / item / tool / approval / hook

### 8.2 opencode

- `OpencodeServerSource`
- 通过 `server + event/export`
- 第一版重点产出：
  - session
  - message / part
  - reasoning
  - tool call
  - plugin / permission / command / server 事件

### 8.3 Claude Code

- `ClaudeCodeTranscriptSource`
- 通过 transcript / event / export
- 第一版重点产出：
  - transcript item
  - tool call
  - approval
  - error / completion summary

## 9. 第一版最小落地建议

1. 先补统一接口层
2. 先让 `Codex` 挂进去
3. 再给 `opencode` 和 `Claude Code` 预留接入位

## 10. 当前最稳的产品定义

补完这一层后，`PrismTrace` 的产品定义会从：

`一个不断尝试 attach 的实验工具`

升级成：

`一个支持多种官方或准官方 source backend 的 AI 运行时观测平台`
