## 概览

`add-request-inspector` 的目标，是把当前 local console 中的 request 基础详情升级成真正的单次 exchange inspector。系统已经有 request artifact、response artifact 和 `exchange_id`，因此本轮不需要再发明新链路，而是围绕现有 artifacts 构建更完整的 detail 读模型和 UI 展示。

本轮只交付：

- request payload detail
- matching response detail
- detail API 的过滤收口

不交付：

- session reconstruction
- timeline replay
- tool visibility
- prompt diff / failure attribution

## 背景

当前 `ConsoleRequestDetail` 仅包含基础字段：

- request summary
- provider / model
- target
- artifact path
- probe context

这足以支撑 local console 第一版，但不足以支撑真正的 request inspector。与此同时，仓库已经具备 response capture 第一版，说明 request inspector 再只展示 request 半边，已经不符合当前阶段的产品目标。

## 目标 / 非目标

**目标：**
- 让 detail API 返回 request payload 关键事实
- 让 detail API 返回 matching response 的核心事实
- 让控制台详情区可直接检查单次 exchange

**非目标：**
- 不做多请求时间线
- 不做跨请求 diff
- 不做 provider-specific 富渲染

## 方案

### 1. 扩展 request detail 读模型

`ConsoleRequestDetail` 扩展为三部分：

- request overview
- request payload detail
- optional response detail

request 仍从 request artifact 读取；response 从 response artifact 读取，并通过 `exchange_id` 做最小关联。

### 2. response 匹配规则

- 以 request artifact 中的 `exchange_id` 为主键
- 从 `.prismtrace/state/artifacts/responses/` 读取匹配 response
- 若有多条匹配，选择时间上最新的 terminal response
- 若无匹配，response detail 为 `null`

### 3. detail API 过滤规则

当控制台运行在 target filter 范围内时：

- detail 读模型先正常读取
- 再基于 detail 对应 target 执行 filter 匹配
- 不匹配则返回 `not_found`

## 验证策略

- 聚焦测试覆盖 request detail 字段读取
- 聚焦测试覆盖 response 按 `exchange_id` 关联
- 控制台 API 测试覆盖 detail payload
- 过滤测试覆盖 detail 不绕过当前视图范围
