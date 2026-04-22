## Why

当前本地控制台默认会枚举并展示本机上大量 Node / Electron 目标，这在真实使用中会让 `Targets` 区域变得嘈杂，也会让活动和 request 视图混入大量与当前观测目标无关的信息。对于只想观察特定 AI 应用（例如 `opencode`、`codex`）的用户来说，现有控制台缺少一个“限定监控范围”的入口。

在本地控制台已经具备最小闭环之后，现在补上启动时目标过滤能力，可以显著提升控制台的可用性，并为后续更细粒度的监控范围控制与产品化筛选能力打下基础。

## What Changes

- 为 `prismtrace-host -- --console` 增加启动时目标过滤能力，允许用户通过参数指定一个或多个要监控的目标关键字
- 控制台首页及其相关聚合结果只展示匹配过滤条件的 targets / activity / requests / observability health
- 在控制台界面中展示当前过滤条件，避免用户误以为自己看到的是“全局视图”
- 当过滤条件下没有匹配目标时，控制台仍正常启动，但给出明确空态说明，而不是展示全局扫描结果或模糊空白状态

## Capabilities

### New Capabilities
- `console-target-filter`: 为本地控制台提供启动时目标过滤能力，包括过滤参数表达、匹配语义、过滤后视图聚合，以及无匹配时的空态说明

### Modified Capabilities
- `local-console`: 控制台入口与首页视图从默认全局枚举扩展为支持“带过滤上下文的局部监控视图”，相关 requirement 需要补充过滤条件显示、过滤后聚合范围和无匹配空态行为

## Impact

- 影响代码：`crates/prismtrace-host` 中的命令行参数解析、控制台快照构建、target/activity/request/health 聚合逻辑，以及控制台静态页面渲染
- 影响接口：控制台启动方式将新增目标过滤参数；如有需要，控制台首页或相关 API payload 可能增加当前过滤条件字段
- 影响系统行为：本地控制台从“全局 discover 视图”扩展为“可限定监控范围的观测视图”
- 文档影响：README / 使用说明需要补充带过滤参数的控制台启动示例与行为说明
