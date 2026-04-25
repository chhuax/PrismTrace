# add-request-inspector 设计

日期：2026-04-25
状态：草稿

## 概览

`add-request-inspector` 的目标，是把当前本地控制台里“基础 request 详情”推进成真正可用的单次 exchange 检查器。用户不应只看到 request summary、target 和 artifact 路径，而应该能够直接在控制台里检查：

- request 的方法、URL、headers、payload 正文与截断状态
- 与该 request 共享 `exchange_id` 的 response 状态码、时延、headers 与正文
- request / response artifact 的引用位置

本轮不解决完整会话重建，也不做复杂搜索、对比或归因分析。它只解决一个问题：

`对一条已捕获 request，PrismTrace 控制台能不能直接把这次 exchange 的关键事实展示清楚。`

## 背景

当前仓库已经完成：

- request capture 第一版
- response capture 第一版
- local console 第一版

这意味着 PrismTrace 已经有足够的事实层：

- request artifact
- response artifact
- `exchange_id` 作为单次 request-response 的最小关联键

但控制台当前仍停留在“浅详情”阶段。`/api/requests/:id` 返回的还是：

- request summary
- provider / model
- target
- artifact path
- probe context 占位

这不足以支撑用户真正检查一次 exchange，也会直接限制后续 session reconstruction 和分析层的价值验证。

## 目标 / 非目标

**目标：**
- 让控制台详情区直接展示 request payload 的关键字段与正文
- 让控制台能按 `exchange_id` 自动关联并展示对应 response detail
- 为 request / response 正文提供安全、明确的只读展示
- 保持 artifact 仍然是事实源，而不是引入第二套临时数据通路

**非目标：**
- 不做多 request 会话重建
- 不做 tool / skill visibility
- 不做 prompt diff、failure attribution 或 diagnostics
- 不做复杂全文搜索、时间范围筛选或跨 target 聚合分析

## 当前现状

从代码上看，request inspector 已有三个基础但还未真正收口：

- `load_request_detail()` 已能读取 request artifact 并返回基础 detail 结构
- response capture 已把 response artifact 写入 `.prismtrace/state/artifacts/responses/`
- 控制台 UI 已经有 request detail panel 和 `/api/requests/:id` 路由

缺口在于：

- detail 结构没有承载 request payload 正文、headers、truncated 信息
- detail 路径还不会自动查找匹配 response
- UI 还没有针对 request body / response body 的可读展示
- detail API 在带过滤上下文时缺少“不可绕过当前过滤范围”的明确收口

## 方案概览

### 1. 扩展控制台读模型

将 `ConsoleRequestDetail` 从“基础摘要模型”扩展为“单次 exchange inspector 模型”，至少包含：

- request metadata
  - `request_id`
  - `exchange_id`
  - `captured_at_ms`
  - `provider`
  - `model`
  - `target_display_name`
  - `artifact_path`
  - `request_summary`
- request fact detail
  - `method`
  - `url`
  - `headers`
  - `body_text`
  - `body_size_bytes`
  - `truncated`
  - `hook_name`
- response detail
  - `artifact_path`
  - `status_code`
  - `headers`
  - `body_text`
  - `body_size_bytes`
  - `truncated`
  - `started_at_ms`
  - `completed_at_ms`
  - `duration_ms`

### 2. 继续使用 artifact 作为唯一事实源

request detail 继续从 request artifact 读取。response detail 不新增新的状态通道，而是：

- 读取 `.prismtrace/state/artifacts/responses/`
- 根据 request 的 `exchange_id` 找到匹配 response
- 若存在多条匹配 response，则选择时间上最新的一条 terminal response

这样可以保持：

- request inspector 与 response capture 的契约一致
- 后续 session reconstruction 仍能围绕同一批 artifact 扩展

### 3. UI 展示深度升级，但仍保持只读与轻量

详情区升级为三段：

- Request Overview
- Request Payload
- Response Detail

展示原则：

- headers 以列表形式展示
- `body_text` 以 `<pre>` 形式展示，保留原始文本
- 当正文为空时给出明确空态
- 当正文已截断时，明确展示截断标记

### 4. detail API 必须遵守过滤范围

若控制台当前运行在 target filter 范围内，则 `/api/requests/:id` 不应绕过过滤限制直接暴露未匹配目标的 detail。

收口规则：

- 先读 request detail
- 再基于 detail 对应的 target 执行当前 filter 判定
- 若不匹配，则返回 `not_found` 语义而不是泄漏 detail 内容

## 关键设计决策

### 决策 1：本轮只交付单次 exchange inspector

- 背景：最自然的扩展方向是立刻进入 session reconstruction
- 方案：只对单个 request + matching response 做 detail 展示
- 取舍：这样能把当前控制台最短板补齐，同时避免范围膨胀到 timeline / correlation 层

### 决策 2：正文不做 provider-specific 富渲染

- 背景：不同 provider 的 payload 结构不同，如果本轮做 provider-aware inspector，会迅速放大复杂度
- 方案：正文以安全文本展示为主，保留原始 `body_text`
- 取舍：先保证可读和可检查，再考虑后续 provider-aware 增强

### 决策 3：detail API 的过滤语义优先于直接读 storage

- 背景：当前列表层已经支持过滤，如果 detail 层不做同样约束，就会留下绕过入口
- 方案：detail API 在 live path 上增加 filter 校验
- 取舍：增加一点路由内逻辑，但能保证首页与 detail 的监控范围一致

## 正确性与需求映射

### 属性 1：用户能直接看到 request payload
- 验证：detail 中可见 request method、url、headers、body_text 和 truncated

### 属性 2：用户能直接看到 matching response
- 验证：同一 `exchange_id` 下，response status / duration / body_text 可在 detail 中展示

### 属性 3：detail 不绕过当前过滤范围
- 验证：带过滤上下文时，未匹配 request 的 detail 返回 `not_found`

## 风险 / 取舍

- [风险] response artifact 可能缺失或尚未到达
  - 处理：request inspector 允许 `response = null`，但 request detail 仍可展示

- [风险] body_text 可能很长
  - 处理：继续依赖 capture 阶段的截断事实，并在 UI 中明确标记 `truncated`

- [风险] 将来 session reconstruction 会重用 detail 模型
  - 处理：本轮 detail 模型按单次 exchange 设计，不提前承诺跨请求结构
