## 概览

`add-session-reconstruction` 的目标，是把当前已经存在的 request / response / tool visibility 事实，从单条 exchange 检查能力推进到第一版 session timeline 能力。

本轮只做一个保守、可验证的版本：

- session 只在同一 `pid` 内重建
- exchange 按 request 时间排序
- 相邻 exchange 超过固定时间窗口就切新 session
- 控制台展示 session 列表与 session timeline

不交付：

- 跨 `pid` 的 session 合并
- attach 生命周期持久化驱动的 session 划分
- provider-specific conversation / thread 推断
- prompt diff、tool visibility diff、failure attribution
- stream replay

## 背景

当前仓库已经具备：

- request artifact
- response artifact
- request-embedded tool visibility artifact
- request inspector

这说明事实层已经足够支撑“连续调用”的重建，只是这些事实还没有被组织成更高层的 session 视图。

如果继续停留在单条 request 详情，PrismTrace 会只能回答“其中一次调用发生了什么”，而无法回答“刚才这一段连续调用整体发生了什么”。

## 目标 / 非目标

**目标：**
- 让控制台列出最近 session
- 让用户进入单个 session 查看连续 timeline
- 让 timeline item 展示 request / response / tool visibility 的聚合摘要

**非目标：**
- 不做任务级 session 语义
- 不做全文搜索
- 不做跨 session diff
- 不做 attach 生命周期持久化

## 方案

### 1. 先聚合 exchange

session reconstruction 以 request artifact 为主记录构建 exchange：

- request 作为主记录
- response 通过 `exchange_id` 关联
- tool visibility 优先按 `request_id` 关联，必要时回退 `exchange_id`

exchange 是 timeline 的基本单元，而不是原始 request / response / tool 事件。

### 2. 再按 `pid + 时间窗口` 聚合 session

session 切分规则：

- 仅在同一 `pid` 内考虑聚合
- exchange 以 request 的 `captured_at_ms` 升序排序
- 第一条 exchange 启动一个新 session
- 若当前 exchange 与上一条 exchange 的时间差大于 `5 min`，则切新 session
- 否则归入当前 session

### 3. 控制台新增 session 读模型与路由

新增：

- `/api/sessions`
- `/api/sessions/:id`

控制台首页新增：

- `Sessions` 区域
- `Session Timeline` 区域

timeline item 继续链接已有 request inspector，而不是再次内嵌完整 request / response body。

### 4. 过滤语义保持一致

当控制台当前运行在 target filter 范围内时：

- 只展示匹配目标的 session
- session detail 不允许绕过过滤范围

## 验证策略

- 聚焦测试覆盖 exchange 聚合
- 聚焦测试覆盖 session 切分
- 控制台 API 测试覆盖 session 列表与 detail
- 过滤测试覆盖 session API 不绕过 target filter
