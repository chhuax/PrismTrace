# local-console Specification

## Purpose

`local-console` 能力把 PrismTrace 从主要依赖 CLI 输出的观测原型，推进到一个可在浏览器中使用的本地可观测性控制台。它的重点不是一次性做完整分析，而是先把已经存在的 attach、probe health 和 request capture 事实，稳定地呈现在一个统一的本地产品界面中。

本次能力面向“看见现在正在发生什么”和“快速回看最近捕获了什么”。因此规格优先约束：控制台入口、本地 target/activity 列表、基础 request 浏览和 attach/probe 状态可见性；不要求在这一轮完成复杂 request inspector、response 深度解析或完整会话重建。

术语说明：

- 本地控制台：PrismTrace host 在本机暴露的 Web UI 与其最小 API surface。
- 活动时间线：按时间顺序展示的最近 attach、probe 或 request 相关活动列表。
- request 摘要：用于列表和快速浏览的轻量信息集合，不等价于完整 payload 检查器。

## Requirements

### Requirement: Host 必须提供可本地访问的控制台入口
PrismTrace host MUST 提供一个可在本机访问的控制台入口，使用户无需直接查看 CLI dump 或手工打开 artifact 文件，也能进入本地可观测性界面。

#### Scenario: 启动 host 后可以打开本地控制台
- **WHEN** 用户启动带有控制台能力的 PrismTrace host
- **THEN** host 提供一个明确的本地访问入口，用户可以通过浏览器进入控制台

#### Scenario: 控制台入口不可用时返回结构化失败
- **WHEN** 控制台入口因为绑定失败、初始化失败或静态资源不可用而无法启动
- **THEN** host 返回结构化失败信息，而不是静默退化为无反馈状态

### Requirement: 控制台必须展示 attach target 与其当前状态
本地控制台 MUST 展示可观测目标列表，并为每个目标暴露足够的状态信息，让用户判断该目标是否已 attach、是否可继续操作以及当前 probe/attach 状态是否健康。

#### Scenario: 控制台显示目标列表与 attach 状态
- **WHEN** 用户打开本地控制台首页
- **THEN** 控制台展示本地 target 列表，并为每个 target 显示至少名称或显示名、pid、runtime 类型以及当前 attach 相关状态

#### Scenario: 没有活跃目标时仍保留空态解释
- **WHEN** 当前没有发现任何 target 或没有 active attach session
- **THEN** 控制台返回明确的空态说明，而不是只显示空白区域

#### Scenario: 带过滤条件时目标列表受过滤约束
- **WHEN** 用户以目标过滤条件启动本地控制台首页
- **THEN** 控制台展示的 target 列表只包含匹配过滤条件的目标，并保持与该范围一致的 attach 状态视图

### Requirement: 控制台必须展示最近观测活动时间线
本地控制台 MUST 提供一个最近活动视图，用于按时间顺序呈现 attach、probe health 和 request capture 等已经持久化或已知的观测活动，帮助用户快速理解“刚刚发生了什么”。

#### Scenario: 最近活动按时间顺序展示
- **WHEN** 系统中已经存在 attach、probe 或 request 相关活动
- **THEN** 控制台按稳定顺序展示最近活动项，并包含可识别的类型、时间和关联对象摘要

#### Scenario: 没有活动时展示空时间线
- **WHEN** 当前尚未捕获任何可展示活动
- **THEN** 控制台展示“尚无观测活动”的空态说明

#### Scenario: 带过滤条件时活动时间线受过滤约束
- **WHEN** 用户以目标过滤条件启动本地控制台，且系统同时存在匹配目标与非匹配目标的活动
- **THEN** 控制台只展示与匹配目标相关的活动时间线

### Requirement: 控制台必须提供 request 摘要列表与基础详情跳转能力
本地控制台 MUST 提供 request 摘要列表，使用户可以浏览已捕获请求的核心元数据，并进入单条 request 的基础详情视图；该详情视图至少能呈现该请求的关键摘要与关联元数据。

#### Scenario: 用户可以浏览 request 摘要列表
- **WHEN** 系统中已经持久化至少一条 request capture 结果
- **THEN** 控制台展示 request 列表，并为每条 request 提供至少 provider、model、时间、目标或会话关联摘要

#### Scenario: 用户可以进入单条 request 的基础详情
- **WHEN** 用户在控制台中选择一条 request
- **THEN** 控制台展示该 request 的基础详情信息，而不是只停留在列表摘要层

#### Scenario: 带过滤条件时 request 列表与详情受过滤约束
- **WHEN** 用户以目标过滤条件启动本地控制台，且系统中存在匹配目标与非匹配目标的 request
- **THEN** 控制台只展示与匹配目标相关的 request，且详情视图不暴露未匹配目标的 request 内容

### Requirement: 控制台必须暴露 probe 健康与基础错误可见性
本地控制台 MUST 展示 active session 的 probe 健康摘要和基础错误可见性，使用户能够判断当前观测链路是否工作，以及失败发生在哪一层。

#### Scenario: 控制台显示 probe 健康摘要
- **WHEN** 当前存在 active attach session 或最近 probe 状态信息
- **THEN** 控制台展示 probe 状态、已安装 hook 摘要或失败 hook 摘要中的至少一种可观察健康信息

#### Scenario: 控制台显示基础错误说明
- **WHEN** attach、probe 或 request capture 路径出现已知失败
- **THEN** 控制台向用户展示基础错误说明或失败摘要，而不是要求用户仅从日志中排查

#### Scenario: 带过滤条件时健康信息反映过滤后的监控范围
- **WHEN** 用户以目标过滤条件启动本地控制台
- **THEN** 控制台展示的 observability health 摘要只反映匹配目标范围内的 probe 与错误信息

## Non-Goals / Boundaries

- 本次不要求完成完整 request inspector；深度 payload 检查可在后续 `add-request-inspector` change 中展开。
- 本次不要求完成 response / stream capture 的可视化闭环。
- 本次不要求完成复杂全文搜索、跨 session 重建或分析解释能力。
