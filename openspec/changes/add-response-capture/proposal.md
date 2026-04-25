## Why

PrismTrace 目前已经完成真实 attach、前台持续采集、request capture 第一版，以及 local console 第一版。这让系统可以回答“现在能 attach 到谁”和“它发出了什么 request”，但还不能把一次 exchange 收口成完整的 request-response 事实闭环。

当前仓库其实已经有 response capture 的基础实现：`HttpResponseObserved` 协议、probe 发 response 事件、host 写入 response artifact、CLI 输出 response 摘要。但这条能力仍缺正式的 change 边界、稳定规格和黑盒验收标准，因此还不能被当作已交付能力来依赖。

现在单独开 `add-response-capture`，是为了把这条已经萌芽的链路收敛成一个稳定产品切片：用户在 attach 到真实目标后，不只知道“它发了什么”，也能稳定知道“它收到了什么”。

## What Changes

- 正式定义 response capture V1 的能力边界、artifact schema、request-response 关联语义、脱敏与截断策略
- 让 `prismtrace-host` 在前台 attach 持续采集路径上稳定处理 `HttpResponseObserved`，并输出最小 response 摘要与 artifact
- 为 response capture 增加 focused tests 与黑盒验收口径，覆盖 happy path 和关键错误路径
- 为后续 local console、request inspector 和 session reconstruction 提供可复用的 response facts 基础

## Capabilities

### New Capabilities
- `response-capture`: 为已 attach 的目标稳定捕获模型相关 HTTP response 的核心事实，包括状态码、响应摘要、时延与持久化 artifact

### Modified Capabilities
- `request-capture`: 从“只保证 request facts”扩展为“在单次 exchange 上提供 request-response 的最小关联基础”，但本 change 不扩展到完整 session reconstruction

## Impact

- 影响代码：`crates/prismtrace-core` 中的 IPC 协议保持为稳定依赖；`crates/prismtrace-host` 中的 probe、前台消费循环与 response artifact 落盘逻辑会成为本 change 的主要实现面
- 影响系统行为：`--attach <pid>` 前台持续采集路径将不只打印 request 摘要，也会稳定打印 response 摘要并写入 `.prismtrace/state/artifacts/responses/`
- 影响文档：新增 `response-capture` change 文档；后续 stable spec 与控制台相关文档将以本 change 的输出为事实源
- 边界说明：本次不交付 stream replay、完整 response inspector、tool visibility 或 session reconstruction