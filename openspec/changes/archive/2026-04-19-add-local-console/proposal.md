## Why

PrismTrace 目前已经能在 CLI 路径上完成真实 attach，并捕获运行中 Node 目标发给模型的 request，但用户仍然主要依赖命令行输出和 artifacts 才能理解一次观测结果。这让系统更像“调试转储工具”，还不是一个真正可操作的本地可观测性产品。

进入迭代 5 的第一步，需要先把已经采集到的事实接到一个本地控制台上，让用户能在统一界面里查看 attach target、最近活动和基础 request 摘要，为后续 request inspector、response 展示和搜索过滤能力打下产品 surface。

## What Changes

- 为 PrismTrace 增加第一版本地可观测性控制台能力，提供本地 Web UI 和最小 host API surface
- 在控制台中展示 attach target 列表、基础会话/活动时间线和 request 摘要列表
- 提供从列表进入单条 request 基础详情入口所需的摘要级数据结构，但本次不追求深度 inspector 体验
- 暴露 probe 健康状态、attach 状态和基础错误可见性，减少用户对 CLI dump 的依赖

## Capabilities

### New Capabilities
- `local-console`: 提供本地可观测性控制台的最小产品闭环，包括 host API、target/activity 列表和基础 request 浏览能力

### Modified Capabilities

## Impact

- 影响代码：`crates/prismtrace-host`、`crates/prismtrace-storage`，以及新的本地 UI/静态资源承载位置
- 影响系统：PrismTrace 从 CLI-first 的调试入口，扩展到 local-first 的控制台产品 surface
- 依赖影响：需要确定第一版本地 HTTP/UI 交付形态，但不要求本次完成复杂前端框架或完整 request inspector
- 文档影响：需要新增 `local-console` capability spec，并在 README / 使用说明中补充本地控制台入口
