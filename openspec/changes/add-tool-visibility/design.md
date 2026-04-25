## 概览

`add-tool-visibility` 的目标，是在不扩展复杂 probe 协议的前提下，把“request 最终携带了哪些 tools / functions”这一层事实补进 PrismTrace。

本轮只交付 request-embedded visibility：

- 数据来源是已经捕获到的 request body
- artifact 单独落盘
- request inspector 提供详情展示

## 背景

当前系统已经具备 request artifact 与 response artifact。request inspector 第一版也已经能展示单次 exchange 的 request / response detail。

但“工具可见性”仍然缺席，导致控制台里虽然能看到 request body 原文，却不能以结构化方式回答：

- final tools 有多少个
- tool choice 是什么
- 最终发给模型的是哪些工具

## 目标 / 非目标

**目标：**
- 机会式提取 request-embedded tools / functions
- 将其持久化为独立 artifact
- 在控制台详情区结构化展示

**非目标：**
- 不做 candidate / filtered / final 多阶段 visibility
- 不做 tool visibility diff
- 不做 session reconstruction
- 不做“为什么没带某个 tool”的解释

## 方案

### 1. request payload 提取规则

对 `HttpRequestObserved.body_text` 执行 JSON 解析：

- 若存在 `tools` 数组，则将其视为 `final_tools_json`
- 否则若存在 `functions` 数组，则将其视为 `final_tools_json`
- 读取 `tool_choice`
- 若 `tools` / `functions` / `tool_choice` 都不存在，则不生成 visibility artifact

本轮不尝试做 provider-specific 规范化，只保留原始结构，并额外生成轻量的名称 / 类型摘要用于 UI 展示。

### 2. artifact 持久化

artifact 目录：

- `.prismtrace/state/artifacts/tool_visibility/`

artifact 至少包含：

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

其中 `visibility_stage = request-embedded`。

### 3. 控制台 detail 读取

`load_request_detail()` 在读取 request 与 matching response 后，继续查找对应 visibility artifact：

- 优先按 `request_id`
- 如无结果，再允许按 `exchange_id` 回退

读取成功后，将其装入 `ConsoleRequestDetail.tool_visibility`。

### 4. 控制台展示

request inspector 新增 `Tool Visibility` 区块，展示：

- stage
- tool count
- tool choice
- 最终工具摘要列表
- 原始 JSON

若当前 request 没有 visibility artifact，则展示明确空态。

## 验证策略

- 测试覆盖 `tools` 数组提取
- 测试覆盖 `functions` 数组提取
- 测试覆盖无 visibility 字段时不落盘
- 测试覆盖 request detail / API payload 中的 visibility 展示
