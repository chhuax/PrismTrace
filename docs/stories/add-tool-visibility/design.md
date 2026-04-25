# add-tool-visibility 设计

日期：2026-04-25
状态：草稿

## 概览

`add-tool-visibility` 的目标，是让 PrismTrace 在已经具备 request / response 事实的基础上，再补上一层“当次 request 最终暴露给模型的 tool / skill 集合”事实。

这一轮只做一个很窄的切片：

- 从已捕获的 request payload 中提取 request-embedded tool visibility
- 将这份 visibility 作为独立 artifact 落盘
- 在 request inspector 中展示该次 request 的工具可见性摘要

它不尝试解释为什么某个 tool 被过滤，也不做跨 request 的 diff。它只回答：

`这次真正发给模型的请求里，带了哪些工具，以及 tool choice 是什么。`

## 背景

当前仓库已经完成：

- request capture 第一版
- response capture 第一版
- local console 第一版
- request inspector 第一版

这意味着控制台已经能回答：

- 发了什么 request
- 收到了什么 response

但还不能回答：

- 这次 request 里，模型到底看到了哪些 tools / functions

而这恰好是后续 session reconstruction、tool visibility diff 和 skill diagnostics 的前置事实层。

## 目标 / 非目标

**目标：**
- 在 request capture 阶段机会式提取 request-embedded tool visibility
- 将 tool visibility 作为独立 artifact 持久化
- 在 request inspector 中展示 final tools、tool count 与 tool choice

**非目标：**
- 不做 pre-filter / post-filter 多阶段 visibility
- 不做为什么某个 tool 没出现的解释
- 不做跨 request 的 visibility diff
- 不做 session timeline 重建

## 方案概览

### 1. 以 request payload 作为唯一事实源

本轮不新增 probe 侧复杂 hook，也不尝试直接感知本地编排器的内部筛选过程。

实现方式是：

- 在 host 已拿到 `HttpRequestObserved` 后，解析 `body_text`
- 若 payload 中存在 `tools` 或 `functions` 字段，则提取为 request-embedded visibility
- 将这份 visibility 写入 `.prismtrace/state/artifacts/tool_visibility/`

这样可以直接复用当前 request capture 链路，不引入新的跨进程协议复杂度。

### 2. visibility artifact 结构

本轮 artifact 至少包含：

- `request_id`
- `exchange_id`
- `pid`
- `target_display_name`
- `provider_hint`
- `captured_at_ms`
- `visibility_stage`
- `tool_choice`
- `final_tools_json`
- `tool_count_final`

其中：

- `visibility_stage` 固定为 `request-embedded`
- `final_tools_json` 保留 payload 中的原始结构
- `tool_choice` 若不是字符串，则保留其 JSON 文本表示

### 3. 控制台展示方式

request inspector 增加一个 `Tool Visibility` 区块，展示：

- `visibility_stage`
- `tool_count_final`
- `tool_choice`
- 最终工具列表摘要
- 原始 `final_tools_json`

展示策略保持轻量：

- 优先展示名称与类型摘要
- 同时保留原始 JSON 文本，避免过早做 provider-specific 富渲染

### 4. 关联规则

request detail 加载 visibility 时：

- 优先按 `request_id` 精确匹配
- 若未来出现仅有 `exchange_id` 的 visibility artifact，再允许回退到 `exchange_id`

这样可以保证 request inspector 的详情和当前 request 一一对应。

## 关键设计决策

### 决策 1：先做 request-embedded，而不是多阶段 visibility

- 背景：设计文档中的理想形态包含 candidate / filtered / final 多阶段 visibility
- 方案：本轮只交付 `request-embedded`
- 取舍：先补齐最可靠、最容易验证的 final fact，再把更高成本的阶段化采集留到后续

### 决策 2：artifact 独立落盘，而不是把 visibility 直接塞回 request artifact

- 背景：最省事的做法是把 tools 直接嵌回 request artifact
- 方案：独立写入 `tool_visibility` artifact
- 取舍：这样更接近未来事件管线与多类事实并存的形态，也更方便后续扩展成更多 visibility stage

### 决策 3：UI 只做可读展示，不做解释

- 背景：tool visibility 很容易自然扩散到“为什么没出现”
- 方案：只展示事实，不给归因
- 取舍：保持这一轮仍属于事实采集与检查层

## 验证策略

- 单元测试覆盖 request payload 提取 tools / functions 与 tool choice
- request capture 测试覆盖 tool visibility artifact 落盘
- request inspector 测试覆盖 detail 中的 tool visibility 读取与 API payload
- 本地 CI 基线覆盖 `fmt`、`clippy`、`test` 和 `--discover`
