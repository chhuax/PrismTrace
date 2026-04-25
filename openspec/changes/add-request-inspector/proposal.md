## Why

PrismTrace 当前已经完成 request capture、response capture 和 local console 第一版，系统已经能够回答：

- 可以 attach 到谁
- 它发了什么 request
- 它收到了什么 response

但控制台里的 request detail 仍然停留在“基础摘要”层，用户还不能直接在控制台里检查 request payload 和 matching response 的关键事实。这让本地控制台还不能真正承担“单次 exchange 检查器”的角色。

现在单独开 `add-request-inspector`，是为了把控制台从“能列出请求”推进到“能检查一次完整 exchange”。

## What Changes

- 扩展控制台 request detail 读模型，展示 request payload 的方法、URL、headers、正文和截断状态
- 基于 `exchange_id` 关联 response artifact，在 detail 中展示 response 状态码、时延、headers 和正文
- 收口 `/api/requests/:id` 的过滤约束，避免 detail 绕过当前 target filter
- 升级本地控制台详情区 UI，使其成为真正可用的 request inspector

## Capabilities

### New Capabilities
- `request-inspector`: 在本地控制台中检查单次 exchange 的 request payload 与 matching response detail

### Modified Capabilities
- `local-console`: 从“基础 request detail”扩展为“可检查 request / response 事实的 inspector detail”

## Impact

- 影响代码：主要集中在 `crates/prismtrace-host/src/console.rs`
- 影响系统行为：`/api/requests/:id` 将返回更完整的 request / response detail 结构，控制台详情区将展示更深的 exchange 信息
- 影响文档：新增 `add-request-inspector` 设计、计划与 OpenSpec 变更文档
- 边界说明：本次不交付 session reconstruction、timeline replay、tool visibility 或分析解释层
