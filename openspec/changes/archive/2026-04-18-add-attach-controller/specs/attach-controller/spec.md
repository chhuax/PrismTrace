## ADDED Requirements

### Requirement: Host 可以对 readiness 通过的目标发起 attach
PrismTrace host MUST 提供一个 attach 操作，用于对 readiness 通过的目标发起连接尝试，并返回结构化的 attach session 结果。

#### Scenario: Supported target 可以进入 attach 流程
- **WHEN** 用户对一个 readiness 状态为 `supported` 的目标发起 attach
- **THEN** host 返回结构化 attach session，而不是只有成功或失败的布尔值

### Requirement: Attach session 必须暴露生命周期状态
每个 attach session MUST 至少暴露目标进程、当前 attach 状态以及与该状态对应的人类可读说明。

#### Scenario: Attach session 返回状态与解释
- **WHEN** 一个 attach session 被创建或状态发生变化
- **THEN** host 返回结果中包含状态字段和一段说明当前 attach 进展或失败原因的解释信息

### Requirement: Attach success 必须以后端握手完成为边界
Host MUST 仅在 attach backend 返回成功并完成最小 probe bootstrap 握手后，将 attach session 标记为 `attached` 或等价成功状态。

#### Scenario: 握手完成后才进入 attached
- **WHEN** attach backend 已连接目标进程且 probe bootstrap 握手成功
- **THEN** attach session 进入 `attached` 状态

### Requirement: Host 必须支持 detach active session
PrismTrace host MUST 提供一个 detach 操作，用于结束当前 active attach session，并返回结构化的结束结果。

#### Scenario: Active session 可以被主动结束
- **WHEN** 当前存在一个 active attach session 且用户发起 detach
- **THEN** host 结束该 session，并返回表示 detach 已完成的结构化结果

### Requirement: V1 同一时刻只允许一个 active attach session
在 V1 中，host MUST 在任意时刻最多维护一个 active attach session；当已有 active session 时，不得再接受第二个 attach。

#### Scenario: 第二次 attach 被拒绝
- **WHEN** 当前已经存在一个 active attach session 且用户再次对另一个目标发起 attach
- **THEN** host 拒绝该请求，并返回说明“当前已存在 active session”的结构化错误

### Requirement: Attach 失败必须以结构化方式暴露
当 attach 因为权限、backend 拒绝、握手失败或其他已知原因未能成功时，host MUST 返回结构化失败结果，而不是仅打印原始错误文本或让进程崩溃。

#### Scenario: Attach 失败返回结构化错误
- **WHEN** attach 尝试未能完成
- **THEN** host 返回包含失败状态和人类可读原因说明的结构化结果

### Requirement: Attach controller 在不依赖真实 live attach 的情况下可测试
Host MUST 将 attach controller 设计为可以基于受控 backend 或受控输入数据进行确定性测试，而不要求真实附着到另一个进程。

#### Scenario: Attach 控制流可以通过受控 backend 验证
- **WHEN** 对 attach controller 进行测试
- **THEN** 测试可以使用受控 backend 验证 attach、handshake、detach 和失败路径，而不需要真实 live attach
