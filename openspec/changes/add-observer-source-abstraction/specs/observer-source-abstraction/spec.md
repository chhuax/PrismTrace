## ADDED Requirements

### Requirement: Host MUST support multiple official or source-safe observer backends

`PrismTrace host` MUST 支持多种 observer source backend，并允许不同目标产品通过各自稳定、不会破坏 live runtime 的接入方式进入统一观测体系。

#### Scenario: Official app server, server event, and transcript sources coexist

- **WHEN** host 同时存在基于官方协议、server event/export 或 transcript/export 的 source
- **THEN** 它们都可以被视为合法 observer source backend
- **AND** host 不得要求所有 source 都先转换成 live attach 或底层注入流程

### Requirement: Host MUST normalize source-specific events into high-level observer events

host MUST 将不同 source 的原始事件归一化成统一的高层 observer event，而不是强制统一到底层网络报文模型。

#### Scenario: Codex, opencode, and Claude Code events are normalized into shared semantics

- **WHEN** `Codex` source 提供 thread / turn / item 类事件，`opencode` source 提供 session / message / tool 类事件，`Claude Code` source 提供 transcript / tool / approval 类事件
- **THEN** host 将其映射到共享的高层语义，例如 session、turn、item、tool、approval、hook、capability snapshot、error
- **AND** 保留必要的原始 source 数据以支持后续演进

### Requirement: Source abstraction MUST not depend on legacy live attach support

统一 source abstraction MUST 不依赖 legacy live attach 方案是否存在或是否可用。

#### Scenario: Legacy attach path is removed from host product surface

- **WHEN** host 清理历史 attach 控制链
- **THEN** 统一 observer source abstraction 仍然成立
- **AND** 后续 source backend 设计不需要为 attach 兼容性保留额外架构负担
