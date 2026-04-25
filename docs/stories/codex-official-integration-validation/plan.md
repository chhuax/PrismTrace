# Codex 官方接入能力验证计划

日期：2026-04-25  
状态：草案

## 1. 目标

在不修改 `PrismTrace` 主实现的前提下，把 `Codex` 官方接入路线的能力边界验证清楚，避免继续基于错误假设推进。

## 2. 已完成

- [x] 确认 `Codex.app` 自带 `codex app-server`
- [x] 确认 `codex app-server proxy` 存在
- [x] 确认 live `Codex` 打开了本地 IPC socket
- [x] 跑通独立 `app-server` 的最小 `initialize`
- [x] 导出官方 schema
- [x] 确认公开 schema 以 thread / turn / item / hook / plugin / skill / app 为主
- [x] 初步确认公开 schema 中未暴露原始 inference request / response payload

## 3. 待验证

### 3.1 运行中 Codex 的 proxy 接入

- [ ] 验证 `proxy --sock <running-codex-socket>` 的最小握手行为
- [ ] 确认是协议格式问题、时序问题，还是 live app 对 proxy 有额外前提

### 3.2 高层 item 的实际信息密度

- [ ] 用最小 thread / turn 样本验证实际能返回哪些 item
- [ ] 判断 item 是否足够支撑时间线、工具链路和错误定位

### 3.3 官方 hooks 的实际价值

- [ ] 验证 hooks 能覆盖哪些生命周期
- [ ] 判断 hooks 更适合做“自动化联动”还是“行为观测”

## 4. 验收标准

这轮验证结束时，至少要明确下面三件事：

1. `PrismTrace` 是否能稳定作为 `Codex app-server` client 接入 live `Codex`
2. 通过官方面到底能拿到多少高层运行时信息
3. 官方路线是否足够支撑第一版 `Codex` 专用观测台

## 5. 暂不做

- 不进入 live `Codex` 的危险 attach 路线
- 不再用 `SIGUSR1` 尝试唤醒 inspector
- 不把“原始后端请求抓包”当作这条路线的默认可交付项

## 6. 下一阶段承接

这轮验证已经足够支撑进入新的收敛 story：

- `docs/stories/add-codex-app-server-observer/`
- `openspec/changes/add-codex-app-server-observer/`

下一阶段重点不再是“继续证明官方接入存在”，而是把它收敛成可实施的 host 接入方案与最小 CLI slice。
