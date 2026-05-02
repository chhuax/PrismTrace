# opencode 官方接入能力验证计划

日期：2026-04-25  
状态：草案

## 1. 目标

在不修改 `PrismTrace` 主实现的前提下，先把 `opencode` 的安全官方接入路线验证清楚，避免继续沿用会打崩目标进程的 attach 思路。

## 2. 已完成

- [x] 确认本机 `opencode` 存在官方 CLI 接入命令
- [x] 确认 `opencode serve` 可启动 headless server
- [x] 确认 `GET /global/health` 可用
- [x] 确认 `GET /doc` 返回 OpenAPI 文档
- [x] 确认 `opencode attach <url>` 可连接运行中的 server
- [x] 确认 `opencode session list --format json` 可列出真实 session
- [x] 确认 `opencode export <sessionID>` 可导出结构化 session JSON
- [x] 确认官方插件文档存在丰富的运行时事件
- [x] 确认现有 `SIGUSR1` attach 路线会把 live `opencode` 打死，不再适合作为主方案

## 3. 下一步最小验证

### 3.1 实时事件流

- [ ] 验证 `/global/event` SSE 中实时会返回哪些事件
- [ ] 记录 event 类型、字段和稳定性
- [ ] 判断这些事件是否足够支撑实时时间线

### 3.2 session 数据密度

- [ ] 再抽样导出 1-2 个真实 session
- [ ] 判断 message / part / tool / reasoning / tokens / finish reason 是否稳定存在
- [ ] 判断这些字段是否足以支撑第一版离线分析

### 3.3 插件事件价值

- [ ] 用最小插件验证 `session.*` / `tool.execute.*` / `permission.*` 是否真的能稳定触发
- [ ] 判断插件路线更适合做“实时镜像”还是“补充埋点”

## 4. 验收标准

这轮验证结束时，至少要明确三件事：

1. `PrismTrace` 是否可以把 `opencode` 当成官方 server source 接入
2. 官方 server / export / plugin 这几条线，哪条最适合做第一版产品
3. 第一版 `opencode` 观测台是否已经可以完全绕开危险 attach

## 5. 暂不做

- 不再尝试用 `SIGUSR1` 方式 attach live `opencode`
- 不再把 Bun attach 作为主路线
- 不把“原始后端 HTTP 报文抓取”当作当前阶段的默认可交付项
