# Proposal: add-observer-source-abstraction

## Why

`PrismTrace` 最早把 live `attach + probe` 当成主要接入思路，但后续对真实目标的验证已经收敛出一个更明确的事实：

- `Codex` 需要走官方 `App Server + IPC socket`
- `opencode` 需要走 `server + event/export`
- `Claude Code` 需要走 transcript / event / export 一类官方或准官方观测面

这意味着统一抽象层不应该再围绕 attach 建模，而应该围绕“可持续、可验证、不会打崩 live runtime 的观测 source”建模。

## What Changes

- 在 host 架构层面引入统一的 `ObserverSource` / `ObserverEvent` 抽象
- 让 `Codex` 的 `App Server` observer 路线作为正式 source backend 接入
- 为 `opencode` 的 `server + event/export` 路线预留并行接入位
- 为 `Claude Code` 的 transcript / event / export 路线预留并行接入位
- 明确高层产品面以 `session / turn / item / tool / approval / hook / capability / error` 为统一事件语义

## Impact

受影响 spec：

- 新增：`observer-source-abstraction`

受影响模块：

- `prismtrace-host`
- 后续 `Codex observer`、`opencode observer`、`Claude Code transcript observer` 实现

## Out of Scope

- 本 change 不要求立即重写现有 request/response artifact 体系
- 不要求这一轮就完成 `opencode` 或 `Claude Code` 实现
- 不要求这一轮统一控制台 UI
- 不重新引入或恢复 live attach 方案
