## Why

PrismTrace 当前已经完成 request capture、response capture、local console、request inspector 和第一版 tool visibility。系统已经能够回答：

- 它发了什么 request
- 它收到了什么 response
- 这次 request 最终暴露了哪些 tools / functions

但这些事实仍主要停留在“单条 exchange 可检查”的层次。用户还不能直接在控制台里理解：

- 同一目标进程刚才连续发生了哪些调用
- 哪些调用属于同一段连续观察上下文
- 一段会话内调用的时间顺序与基本关联关系

现在单独开 `add-session-reconstruction`，是为了把现有事实层组织成第一版 session timeline，让控制台从“能检查一条请求”推进到“能看懂一段连续调用”。

## What Changes

- 基于现有 request / response / tool visibility artifacts 聚合 exchange
- 采用 `pid + 时间窗口` 的保守规则重建第一版 session
- 在本地控制台增加 session 列表与 session timeline API / UI
- 保持 request inspector 作为 timeline item 的深度详情入口

## Capabilities

### New Capabilities
- `session-reconstruction`: 在本地控制台中查看同一目标进程内、按时间组织的 session 与 session timeline

### Modified Capabilities
- `local-console`: 从“request 列表 + 单条详情”扩展为“可浏览最近 session 与 session timeline 的控制台”

## Impact

- 影响代码：主要集中在 `crates/prismtrace-host/src/console.rs`
- 影响系统行为：控制台新增 `/api/sessions` 与 `/api/sessions/:id`，首页新增 session 相关视图
- 影响文档：新增 `add-session-reconstruction` story 与 OpenSpec 变更文档
- 边界说明：本次不交付跨 pid 合并、attach 生命周期持久化、provider-specific thread 推断、stream replay 或分析解释层
