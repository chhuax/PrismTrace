## 概览

`add-response-capture` 的目标是把当前仓库里已经存在的 response 事件骨架，收敛成一个可以被正式验收和后续功能复用的稳定能力。整体方案继续沿用现有 request capture 的结构：probe 负责发事实，host 负责判断、落盘和摘要输出，artifact 作为后续控制台与会话重建的共享事实源。

本次设计刻意避免同时进入三条会明显放大范围的路线：

- stream chunk 时间线回放
- 完整 response inspector UI
- session reconstruction / failure attribution

本轮只解决一个问题：

`对单次 exchange，PrismTrace 能不能稳定回答“它收到了什么 response”。`

## 背景

当前代码已经具备以下基础：

- `prismtrace-core` 已有 `IpcMessage::HttpResponseObserved`
- `probe/bootstrap.js` 已能发出 `http_response_observed`
- `request_capture.rs` 已在前台消费循环中接收 response 事件
- `response_capture.rs` 已能按 provider hint 写入 response artifact 并输出摘要

但当前缺口也很明确：

- 没有独立的 OpenSpec change 来定义 response capture 的边界
- 没有稳定 spec 来声明用户可见能力与错误行为
- 没有黑盒验收口径证明它已经是一个正式交付能力
- 没有明确说明流式响应、截断与脱敏策略

所以本轮不是从零实现，而是从“内部可运行”推进到“外部可依赖”。

## 目标 / 非目标

**目标：**
- 定义 response capture V1 的最小事实集
- 用 `exchange_id` 形成 request-response 的稳定配对
- 稳定落盘 response artifact，供 CLI、console 和后续 reconstruction 复用
- 覆盖错误响应、空响应、不可文本化 body 和截断行为

**非目标：**
- 完整流式 chunk 回放
- response 深度详情页渲染
- usage / finish reason / tool calls 的 provider-specific 语义归一化
- session grouping、timeline reconstruction 和 failure attribution

## 架构与方案概览

### 1. probe 负责发送 response 事实

probe 在请求生命周期结束时发出一条 `http_response_observed`，至少包含：

- `exchange_id`
- `hook_name`
- `method`
- `url`
- `status_code`
- `headers`
- `body_text`
- `body_truncated`
- `started_at_ms`
- `completed_at_ms`

probe 的约束与 request capture 一致：

- 观察逻辑不得改变原始 response 语义
- 无法安全文本化的正文允许降级为 `null`
- 大 body 允许截断，但必须明确标记

### 2. host 负责最小关联与 artifact 落盘

前台消费循环继续在 `request_capture.rs` 中维护一个最小的 `exchange_id -> provider_hint` 映射：

- request 被识别后缓存 provider hint
- response 到达时优先复用同 exchange 的 provider hint
- 若没有缓存，则允许 response 路径做一次本地推断

`response_capture.rs` 负责：

- 生成 `event_id`
- 计算 `duration_ms`
- 脱敏 headers
- 写入 `.prismtrace/state/artifacts/responses/`
- 输出单行 CLI 摘要

### 3. artifact 作为后续产品面的稳定事实源

本轮明确把 response artifact 视为后续控制台和详情页的事实源，而不是临时调试输出。这样后续 `add-request-inspector` 或 `add-session-reconstruction` 可以围绕同一批 artifact 继续扩展，而不需要重新定义另一条 response 数据通路。

## 关键设计决策

### 决策 1：继续以 `exchange_id` 作为本轮唯一配对键

- 背景：最自然的诱惑是直接引入 session 级关联模型
- 方案：本轮只承诺 exchange 级配对
- 取舍：这足以形成 request-response 闭环，又不会提前锁死未来 reconstruction 的设计

### 决策 2：交付 terminal response artifact，不交付 stream replay

- 背景：stream 是高复杂度区域，但不是当前最短产品路径的阻塞点
- 方案：只保证最终 response 摘要和 artifact
- 取舍：先把“看见收到了什么”做稳，再在后续 change 中扩展到“看见每个 chunk 怎么到达”

### 决策 3：安全策略优先于正文完整性

- 背景：response 侧更容易出现大包体、敏感头和不可文本化正文
- 方案：允许截断、允许丢正文、必须脱敏、不得因为异常正文破坏主链路
- 取舍：牺牲部分保真度，换取更稳定的观测能力

## 数据模型

本轮 response artifact 至少包含：

- `event_id`
- `exchange_id`
- `pid`
- `target_display_name`
- `provider_hint`
- `hook_name`
- `method`
- `url`
- `status_code`
- `headers`
- `body_text`
- `body_size_bytes`
- `truncated`
- `started_at_ms`
- `completed_at_ms`
- `duration_ms`

这与当前 `response_capture.rs` 的输出表面基本一致，因此重点是把它正式化，而不是再引入第二套 schema。

## 正确性与需求映射

### 属性 1：response 必须能与 request 稳定配对
- 覆盖需求：response capture 必须支持单次 exchange 闭环
- 验证：同一 exchange 的 request / response artifact 使用同一 `exchange_id`

### 属性 2：错误路径不能破坏前台持续采集主链路
- 覆盖需求：response capture 在大 body、不可文本化 body、错误响应下仍可安全降级
- 验证：focused tests 覆盖异常正文和错误状态码

### 属性 3：artifact 必须可供后续 UI 与 reconstruction 复用
- 覆盖需求：response capture 不是一次性 CLI 文本输出，而是稳定事实层
- 验证：artifact 字段足以支持后续列表、详情和基础时序展示

## 风险 / 取舍

- [风险] 当前实现已经先行，change 文档如果范围过大，会和代码事实脱节
  - 处理：本次只正式化已存在的主路径能力，把更深的 stream / provider 语义后移

- [风险] response capture 容易和 request inspector 范围缠在一起
  - 处理：本 change 只交付 artifact 和 CLI summary，不承诺完整 UI

- [风险] local console 很快会希望读取 response 列表与详情
  - 处理：通过稳定 artifact schema 先解决数据契约，UI 接入后置