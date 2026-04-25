## ADDED Requirements

### Requirement: Host MUST support opencode through an official observer route

`PrismTrace host` MUST 通过 `opencode` 的官方 observer 路线接入 `opencode`，而不是要求 `opencode` 必须通过进程 attach 才能被观测。

#### Scenario: opencode is observed through its official server

- **WHEN** 用户希望观测 `opencode`
- **THEN** host 提供基于 `opencode` 官方 server 能力的 observer 入口
- **AND** 不要求用户先通过 `--attach <pid>` 将探针注入 `opencode`

### Requirement: Host MUST expose minimal opencode session and event data

host MUST 在第一版暴露最小可用的 `opencode` 高层观测数据，包括 health、session、export 或事件流中的至少一部分结构化数据。

#### Scenario: opencode observer returns structured session data

- **WHEN** host 成功连接到 `opencode` observer source
- **THEN** 它至少可以返回结构化 session 或 event 数据
- **AND** 数据中包含足够支持时间线、工具链分析或高层结果检查的字段

### Requirement: opencode observer events MUST integrate with the unified observer event layer

`opencode` source 的输出 MUST 能映射到统一 observer 事件层，而不是停留在完全私有的单独格式。

#### Scenario: opencode source emits normalized observer events

- **WHEN** `opencode` source 读取到 session、message、tool 或事件流数据
- **THEN** host 将其映射到统一 observer 事件语义
- **AND** 必要时保留原始数据以支持后续演进
