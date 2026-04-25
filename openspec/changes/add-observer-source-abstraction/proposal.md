# Proposal: add-observer-source-abstraction

## Why

`PrismTrace` 现有架构默认把 `attach + probe` 当成主要接入方式，但最新的真实目标验证已经表明：

- `Codex` 更适合通过 `App Server + IPC socket` 官方协议接入
- `opencode` 更适合通过 `server + attach(url) + export + plugin/event` 官方能力接入

如果不补一个统一的上层观测协议，后续每接一个新产品都要重新发明 host 接入边界，也会让 UI 和分析层长期被 `HTTP request/response` 模型绑死。

因此需要正式引入：

- 统一上层观测协议
- 多 source backend 抽象

## What Changes

- 在 host 架构层面引入统一的 `ObserverSource` / `ObserverEvent` 抽象
- 将现有 attach/probe 路线重新定位为 `AttachProbeSource`
- 让 `Codex` 的 `App Server` observer 路线能够作为新的 source backend 并列接入
- 为后续 `opencode` 的 `server` 路线预留并行接入位
- 明确高层产品面以 `session / turn / item / tool / approval / hook / capability / error` 为统一事件语义

## Impact

受影响 spec：

- 新增：`observer-source-abstraction`

受影响模块：

- `prismtrace-host`
- 后续 `Codex observer` 与 `opencode observer` 实现

## Out of Scope

- 本 change 不要求立即重写现有 request/response artifact 体系
- 不要求这一轮就完成 `opencode` 实现
- 不要求这一轮统一控制台 UI
