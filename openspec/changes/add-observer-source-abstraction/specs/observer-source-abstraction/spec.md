## ADDED Requirements

### Requirement: Host MUST support multiple observer source backends

`PrismTrace host` MUST 支持多种 observer source backend，并允许不同目标产品通过不同接入方式进入统一观测体系。

#### Scenario: Attach probe and official app server sources coexist

- **WHEN** host 同时存在基于 attach/probe 的 source 与基于官方协议的 source
- **THEN** 它们都可以被视为合法 observer source backend
- **AND** host 不得要求所有 source 都先转换成同一种底层注入或抓包方式

### Requirement: Host MUST normalize source-specific events into high-level observer events

host MUST 将不同 source 的原始事件归一化成统一的高层 observer event，而不是强制统一到底层网络报文模型。

#### Scenario: Codex and opencode events are normalized into shared semantics

- **WHEN** `Codex` source 提供 thread / turn / item 类事件，`opencode` source 提供 session / message / tool 类事件
- **THEN** host 将其映射到共享的高层语义，例如 session、turn、item、tool、approval、hook、capability snapshot、error
- **AND** 保留必要的原始 source 数据以支持后续演进

### Requirement: Existing attach route MUST remain as one observer source, not the only source

现有 attach/probe 路线 MUST 继续可用，但其架构定位应是 observer source backend 之一，而不是整个 host 的唯一入口模型。

#### Scenario: Attach-based target remains supported after source abstraction is introduced

- **WHEN** host 引入统一 observer source abstraction
- **THEN** 现有 attach/probe 路线仍可继续产出可消费的观测事件
- **AND** 其能力不会因为引入其他 source 而被删除或语义混淆
