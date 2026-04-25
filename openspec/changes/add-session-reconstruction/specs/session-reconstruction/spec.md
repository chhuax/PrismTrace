## ADDED Requirements

### Requirement: 控制台必须提供最近 session 列表
PrismTrace local console MUST 提供最近 session 列表，使用户能够查看同一目标进程内最近重建出的连续观测会话，而不必手工从 request 列表中自行拼装时间顺序。

#### Scenario: 用户可以浏览最近 session 摘要
- **WHEN** 系统中已经存在可用于重建 session 的 request artifacts
- **THEN** 控制台返回最近 session 摘要列表，并为每个 session 显示至少目标名、pid、开始时间、结束时间和 exchange 数量

#### Scenario: 没有可重建 session 时显示空态
- **WHEN** 当前没有任何 request artifacts
- **THEN** 控制台返回明确的空态说明，而不是只显示空白列表

### Requirement: Session 必须在同一 pid 内按时间窗口重建
PrismTrace host MUST 在同一 `pid` 内按时间顺序重建 session，并使用固定时间窗口切分连续 exchange，以提供稳定、可解释的第一版 session 边界。

#### Scenario: 同一 pid 且时间连续的 exchange 被归为同一 session
- **WHEN** 两条或多条 exchange 属于同一 `pid` 且时间间隔不超过当前固定窗口阈值
- **THEN** host 将它们归入同一个 session

#### Scenario: 超过时间窗口时切出新 session
- **WHEN** 同一 `pid` 下当前 exchange 与上一条 exchange 的时间间隔超过固定窗口阈值
- **THEN** host 为当前 exchange 创建新的 session

#### Scenario: 不跨 pid 合并 session
- **WHEN** 两条 exchange 分别来自不同 `pid`
- **THEN** host 不得把它们归并到同一个 session

### Requirement: Session timeline 必须以 exchange 聚合项展示
PrismTrace local console MUST 以 exchange 聚合项而不是原始事件流展示 session timeline，使用户能够直接理解一段连续调用中每次模型调用的核心事实。

#### Scenario: timeline item 聚合 request / response / tool visibility 摘要
- **WHEN** 用户打开单个 session 的 timeline
- **THEN** 每个 timeline item 至少展示 request 摘要，以及 matching response 和 tool visibility 的可用摘要

#### Scenario: response 或 tool visibility 缺失时 timeline 仍可展示
- **WHEN** 某条 exchange 没有关联到 matching response 或 tool visibility
- **THEN** 该 exchange 仍然出现在 timeline 中，并明确表示缺失状态

### Requirement: Session 视图必须遵守当前 target filter
PrismTrace local console MUST 在 session 列表和 session detail 路径上继续遵守当前 target filter，使过滤视图下不会泄漏未匹配目标的 session 内容。

#### Scenario: 过滤视图下 session 列表只返回匹配目标
- **WHEN** 控制台带有 target filter 启动，且系统中同时存在匹配与未匹配目标的 session
- **THEN** session 列表只返回匹配目标的 session

#### Scenario: 过滤视图下 session detail 不泄漏未匹配目标
- **WHEN** 用户请求一个属于未匹配目标的 session detail
- **THEN** 控制台返回 `not_found` 语义，而不是暴露该 session timeline
