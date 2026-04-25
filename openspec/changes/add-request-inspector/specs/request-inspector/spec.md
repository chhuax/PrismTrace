## 变更概述

`request-inspector` 能力为 PrismTrace 本地控制台增加单次 exchange 的深度检查能力，使用户在查看一条已捕获 request 时，不只看到摘要，还能直接检查 request payload 与 matching response 的关键事实。

本次变更聚焦于 request detail、response detail 与过滤收口，不要求完成 session reconstruction 或分析解释。

## ADDED Requirements

### Requirement: 控制台必须展示 request payload 的关键事实
PrismTrace local console MUST 在单条 request 详情中展示该 request 的方法、URL、headers、正文与截断状态，而不只停留在摘要层。

#### Scenario: 用户查看单条 request 的 payload detail
- **WHEN** 用户在控制台中打开一条已捕获 request 的详情
- **THEN** 控制台展示该 request 的 request metadata、headers、正文与截断状态

### Requirement: 控制台必须展示 matching response 的关键事实
PrismTrace local console MUST 在可能时展示与该 request 共享 `exchange_id` 的 response detail，使用户能检查单次 exchange 的闭环。

#### Scenario: request 存在 matching response
- **WHEN** 某条 request 对应的 response artifact 已存在
- **THEN** 控制台详情中展示该 response 的状态码、时延、headers 和正文摘要

#### Scenario: request 暂无 matching response
- **WHEN** 某条 request 尚未匹配到 response artifact
- **THEN** 控制台仍展示 request detail，并对 response 区域给出明确空态

### Requirement: detail API 必须遵守当前过滤范围
PrismTrace local console MUST 保证 `/api/requests/:id` 的 detail 返回与当前过滤范围一致，而不是绕过首页过滤直接暴露未匹配 request。

#### Scenario: 过滤视图下 detail 仅暴露匹配 request
- **WHEN** 用户在带 target filter 的控制台中请求某条未匹配 request 的 detail
- **THEN** 控制台返回 `not_found` 语义，而不是返回该 request 的 detail 内容

## 非目标与边界

- 本次不要求完成 session reconstruction。
- 本次不要求完成 response stream replay。
- 本次不要求完成 provider-specific 的富渲染与归因分析。
