## ADDED Requirements

### Requirement: Host MUST support opencode through an official observer route

`PrismTrace host` MUST 通过 `opencode` 的官方 observer 路线接入 `opencode`，而不是要求 `opencode` 必须通过进程 attach 才能被观测。

#### Scenario: opencode is observed through its official server

- **WHEN** 用户希望观测 `opencode`
- **THEN** host 提供基于 `opencode` 官方 server 能力的 observer 入口
- **AND** 不要求用户先通过 `--attach <pid>` 将探针注入 `opencode`

### Requirement: Host MUST expose minimal opencode session and event data

host MUST 在第一版暴露最小可用的 `opencode` 高层观测数据，包括 health、session、export/message 和事件流中的至少一部分结构化数据。

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

### Requirement: Host MUST persist opencode observer handshakes and events as artifacts

`PrismTrace host` MUST 将 `opencode` observer 的握手结果和归一化事件以结构化 artifact 的形式落盘，供后续控制台和时间线消费。

#### Scenario: handshake and events are written to opencode observer artifacts

- **WHEN** host 成功建立 `opencode` observer 会话并读取到握手或事件
- **THEN** host 将这些记录写入 `.prismtrace/artifacts/observer_events/opencode/`
- **AND** 每条记录保留事件语义字段以及 `raw_json`

### Requirement: Host MUST connect to an existing opencode server in v1

第一版 `opencode` observer MUST 以连接现有 `opencode` server 为前提，而不是由 `PrismTrace` 自动拉起新的 server 进程。

#### Scenario: v1 observer uses an existing server endpoint

- **WHEN** 用户通过 `--opencode-observe` 或 `--opencode-url <url>` 启动观察
- **THEN** host 尝试连接指定或默认的现有 server endpoint
- **AND** 不自动启动新的 `opencode serve` 进程
