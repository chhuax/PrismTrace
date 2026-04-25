# codex-app-server-observer Specification

## Purpose

`codex-app-server-observer` 能力把 `Codex.app` 从当前不安全的 attach 目标，转换成一个通过官方 `App Server + IPC socket` 接入的高层运行时观测数据源。它的重点不是抓取 `Codex -> 模型后端` 的原始网络报文，而是先稳定暴露 thread、turn、item、tool、approval、hook 和能力可见性等高价值运行时事实。

术语说明：

- `Codex observer`：PrismTrace 作为 `Codex App Server` 客户端建立的只读观测连接。
- `capability snapshot`：`plugin / skill / app` 的当前可见性快照。
- `高层运行时事件`：面向会话、步骤、工具、审批和扩展能力的结构化事件，而非原始 HTTP 报文。

## Requirements

### Requirement: Host 必须提供独立于 attach 的 Codex observer 入口

PrismTrace host MUST 提供一个独立于现有 attach/probe 路线的 `Codex` 官方观测入口，使用户无需对 live `Codex` 进程执行 attach，也能开始观测 `Codex` 的高层运行时行为。

#### Scenario: 用户可以启动 Codex observer

- **WHEN** 用户选择使用 `Codex` 官方观测能力
- **THEN** host 提供一个明确入口启动 `Codex observer`

#### Scenario: 启动失败时返回结构化失败

- **WHEN** `Codex observer` 因 socket 不可达、协议初始化失败或其他已知错误无法建立
- **THEN** host 返回结构化失败信息，而不是静默退化或错误复用 attach 失败语义

### Requirement: Codex observer 必须读取高层运行时事件

PrismTrace host MUST 能从 `Codex App Server` 读取高层运行时事件，并以结构化形式暴露给 CLI 或后续 UI/分析层。

#### Scenario: 读取 thread / turn / item 事件

- **WHEN** 运行中的 `Codex` 会话产生 thread、turn 或 item 相关事件
- **THEN** observer 返回可识别的结构化事件，而不是仅保留不可解释的原始文本

#### Scenario: 读取 tool / approval / hook 事件

- **WHEN** 运行中的 `Codex` 会话发生工具调用、审批等待或 hook 生命周期变化
- **THEN** observer 返回对应的结构化事件摘要

### Requirement: Host 必须暴露能力可见性快照

PrismTrace host MUST 能通过 `Codex` 官方接口读取并暴露当前 `plugin / skill / app` 能力面，作为解释行为差异的基础上下文。

#### Scenario: observer 返回 capability snapshot

- **WHEN** host 成功建立 `Codex observer`
- **THEN** observer 能返回至少一类可见能力快照，帮助解释当前 `Codex` “看得到什么能力”

### Requirement: 未识别事件必须保守保留原始数据

当 host 暂时无法完全解释某个 `Codex` 事件时，MUST 保守保留其原始 JSON 或等价原始载荷，而不是静默丢弃。

#### Scenario: 未知事件仍然可追溯

- **WHEN** observer 收到一个当前版本尚未归一化的事件
- **THEN** host 保留原始事件内容，并以明确的未知或未投影状态向上层暴露

### Requirement: Codex observer 不得要求进入现有 attach 路线

`Codex` 官方观测能力 MUST 与现有 attach/probe 路线并行存在，不得要求用户通过 `--attach <pid>` 或等价 attach 入口才能使用。

#### Scenario: Codex observer 与 attach 并存但不混用

- **WHEN** 用户选择观测 `Codex`
- **THEN** host 使用独立 observer 路线，而不是进入 attach controller 或 inspector runtime

## Non-Goals / Boundaries

- 本次不要求抓取 `Codex -> 模型后端` 的原始 HTTP 请求 / 响应报文。
- 本次不要求将 `Codex` 事件立即融合进现有 request / response inspector。
- 本次不要求交付复杂控制台 UI；CLI/host 验证入口即可构成第一版产品切片。
