# add-session-reconstruction 设计

日期：2026-04-25
状态：草稿

## 概览

`add-session-reconstruction` 的目标，是把当前已经具备的 request / response / tool visibility 事实，从“单次 exchange 可检查”推进到“同一目标进程内的一段连续调用可理解”。

本轮只做一个保守、可验收的第一版：

- 在同一 `pid` 内重建 session
- 用时间窗口把连续 exchange 切成一段 session
- 在本地控制台展示 session 列表与 session timeline
- timeline 中的每个条目继续复用已有 request inspector

它不尝试推断“用户的一次完整工作流”，也不做跨进程合并。它先回答一个更基础的问题：

`同一个被观测目标里，刚才连续发生了哪些模型调用，它们前后顺序是什么。`

## 背景

当前仓库已经完成：

- request capture 第一版
- response capture 第一版
- local console 第一版
- request inspector 第一版
- request-embedded tool visibility 第一版

这意味着 PrismTrace 已经能回答：

- 发了什么 request
- 收到了什么 response
- 这次 request 里最终带了哪些 tools / functions

但这些事实仍然主要以“单条 request 详情”的形式存在。用户能够检查某一条 exchange，却还不能自然回答：

- 这是一个连续会话里的第几次调用
- 同一个目标刚才连续发生了哪些调用
- 哪些调用属于同一段观察上下文

当前的产品短板已经从“事实采集不够”转成“事实没有被组织成更高一层的会话视图”。

## 目标 / 非目标

**目标：**
- 在 host 侧基于现有 artifacts 重建 session
- 在本地控制台展示 session 列表与单个 session 的 timeline
- 让每个 timeline item 呈现 request / response / tool visibility 的聚合摘要
- 保持 request inspector 仍是单条 exchange 的深度入口

**非目标：**
- 不做跨 `pid` 的 session 合并
- 不做 attach 生命周期持久化驱动的 session 边界
- 不做 provider-specific conversation / thread 推断
- 不做全文搜索、时间范围筛选或复杂聚合分析
- 不做 prompt diff、tool visibility diff、failure attribution
- 不做 stream replay

## 方案概览

### 1. 先构建 exchange，再构建 session

本轮不直接对 request / response / tool visibility 三类原始事实做扁平时间线，而是先把它们聚合成更稳定的 `exchange`，再按时间把多个 exchange 组织成 `session`。

一个 exchange 以 request artifact 为主记录，并尽可能挂接：

- matching response
- matching tool visibility

这样做的原因是：

- request 仍是当前最稳定、最完整的事实入口
- 控制台的主要阅读对象是“这次模型调用”，而不是底层原始事件
- session timeline 更适合展示连续调用列表，而不是 probe 级事件流

### 2. session 只在同一 pid 内重建

第一版 session reconstruction 的边界固定为：

- 同一 `pid`
- exchange 按起始时间排序
- 相邻 exchange 的时间差超过阈值时切新 session

推荐阈值先固定为 `5 min`，以 host 内常量实现，不在本轮暴露成控制台配置。

采用这个边界，是因为它完全建立在当前已有事实层上：

- request / response / tool visibility artifacts 均有稳定时间戳
- request artifact 已包含 `pid`
- 不需要先引入 daemon 或跨命令 attach session store

它的语义是“同一目标进程内的一段连续观测窗口”，而不是“用户任务级 session”。

### 3. 控制台新增 session 列表与 session timeline

控制台只补最小的两个新面：

- `Sessions` 列表
- `Session Timeline` 详情区

每个 session 展示摘要：

- `session_id`
- `pid`
- `target_display_name`
- `started_at_ms`
- `completed_at_ms`
- `exchange_count`
- `request_count`
- `response_count`

每个 timeline item 展示 exchange 聚合摘要：

- `request_id`
- `exchange_id`
- `provider`
- `model`
- `started_at_ms`
- `completed_at_ms`
- `duration_ms`
- `request_summary`
- `response_status`
- `tool_count_final`
- `has_response`
- `has_tool_visibility`

timeline item 不重新承载全文详情，继续跳到已有 `/api/requests/:id`。

### 4. 过滤语义继续与控制台保持一致

当控制台运行在 target filter 范围内时：

- `sessions` 列表只包含匹配目标的 session
- `session timeline` 只返回匹配目标范围内的 session
- 不允许通过 session detail 绕过当前过滤范围

这保证了：

- 首页 target / request 视图与 session 视图属于同一监控范围
- 已有 `console-target-filter` 能力可以直接复用到新 API

## 数据模型

### Session Summary

第一版 session summary 至少包含：

- `session_id`
- `pid`
- `target_display_name`
- `started_at_ms`
- `completed_at_ms`
- `exchange_count`
- `request_count`
- `response_count`

### Session Timeline Item

第一版 timeline item 至少包含：

- `request_id`
- `exchange_id`
- `provider`
- `model`
- `started_at_ms`
- `completed_at_ms`
- `duration_ms`
- `request_summary`
- `response_status`
- `tool_count_final`
- `has_response`
- `has_tool_visibility`

### Session Detail

单个 session detail 返回：

- session summary
- 按时间升序排列的 timeline items

本轮不增加 session 级原始事件数组，也不在 detail 中内嵌完整 request / response body。

## 关联与切分规则

### 1. exchange 聚合规则

- request artifact 是 exchange 主记录
- response 通过 `exchange_id` 关联
- tool visibility 优先按 `request_id` 精确匹配
- 若未来存在仅有 `exchange_id` 的 visibility artifact，允许回退到 `exchange_id`
- 缺 response 或缺 tool visibility 时，exchange 仍然成立

### 2. 时间选择规则

exchange 的 session 排序时间采用 request 的 `captured_at_ms`。

原因：

- request 是每个 exchange 最稳定的起点
- response 可能缺失或延迟到达
- tool visibility 本轮是 request-embedded，时间与 request 天然靠近

### 3. session 切分规则

对同一 `pid` 下按时间排序的 exchange 序列：

- 第一条 exchange 开启一个新 session
- 若当前 exchange 与上一条 exchange 的 `captured_at_ms` 差值大于 `5 min`
  - 则切新 session
- 否则归入当前 session

### 4. session_id 规则

第一版 `session_id` 只要求稳定可序列化，不要求成为跨版本永久标识。

建议格式：

- `<pid>-<started_at_ms>-<ordinal>`

其中 `ordinal` 用于避免极端情况下同 pid、同起始时间产生冲突。

## 控制台与 API 设计

### 新增 API

本轮新增两个只读 API：

- `/api/sessions`
- `/api/sessions/:id`

#### `/api/sessions`

返回最近 session 摘要列表，并带上当前过滤上下文。

#### `/api/sessions/:id`

返回单个 session 的 timeline detail。

若 session 不存在或不匹配当前过滤范围，则返回 `not_found` 语义。

### UI 结构

控制台在现有页面上新增：

- `Sessions` 区域：列出最近 session
- `Session Timeline` 区域：展示当前选中 session 的 exchange timeline

读取流程建议保持与现有 console 风格一致：

- 首页加载后请求 `/api/sessions`
- 默认选中第一条 session
- 点击 session 后请求 `/api/sessions/:id`
- 点击 timeline item 后继续请求已有 `/api/requests/:id`

## 关键设计决策

### 决策 1：选择 `pid + 时间窗口`，而不是 attach 生命周期

- 背景：attach 生命周期从产品语义上更直观
- 方案：第一版不依赖 attach lifecycle store，只使用 `pid + 时间窗口`
- 取舍：放弃更强语义，换取当前架构下最小实现面和最稳定验收路径

### 决策 2：timeline 主对象是 exchange，而不是原始事件

- 背景：直接展示原始 request / response / tool visibility 事件实现更省事
- 方案：先聚合成 exchange，再按时间组织 timeline
- 取舍：增加少量读模型聚合逻辑，但显著提升用户理解力

### 决策 3：request inspector 继续作为深度详情入口

- 背景：可以把 request / response / tool visibility 详情再次塞进 session timeline detail
- 方案：timeline 只展示摘要，深度详情仍走 `/api/requests/:id`
- 取舍：避免重复承载 detail 结构，控制本轮 UI 与 API 复杂度

## 风险与处理

- [风险] 时间窗口切分不一定等价于真实业务会话
  - 处理：明确这只是第一版 observability session，而不是用户任务级 session

- [风险] response 或 tool visibility 缺失时，timeline 可能看起来不完整
  - 处理：允许不完整 exchange 存在，并用 `has_response` / `has_tool_visibility` 显式表达

- [风险] artifact 读取逻辑进一步集中到 `console.rs`，文件可能继续膨胀
  - 处理：本轮允许继续沿用当前控制台单文件模式，但应把 session/exchange 读模型保持成内部清晰分段，必要时在后续 change 中拆分

## 验证策略

- 聚焦测试覆盖 exchange 聚合逻辑
- 聚焦测试覆盖同 pid、时间连续的 exchange 被归为同一 session
- 聚焦测试覆盖时间窗口触发 session 切分
- 控制台 API 测试覆盖 `/api/sessions` 与 `/api/sessions/:id`
- 过滤测试覆盖 session 列表与 session detail 不绕过 target filter
- 本地 CI 基线继续覆盖：
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `cargo run -p prismtrace-host -- --discover`
