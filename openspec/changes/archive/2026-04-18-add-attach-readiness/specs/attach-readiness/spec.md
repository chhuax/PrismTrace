## ADDED Requirements

### Requirement: Host 提供 attach readiness 结果
PrismTrace host MUST 在候选进程发现结果之上提供 attach readiness 结果，用于表示某个目标当前是否适合进入 attach 流程。

#### Scenario: Readiness 返回结构化结果
- **WHEN** host 对一个候选进程执行 attach readiness 判断
- **THEN** 返回的是结构化 readiness 结果，而不是只有布尔值

### Requirement: Readiness 结果包含状态与原因
每个 attach readiness 结果 MUST 至少包含目标进程、readiness 状态和人类可读原因说明。

#### Scenario: Readiness 结果包含可读解释
- **WHEN** 一个候选目标被评估
- **THEN** 返回结果中包含状态字段和一段说明为什么是该状态的解释信息

### Requirement: Readiness 状态必须保守表达不确定性
当 host 无法仅凭当前可见元数据可靠判断目标是否适合 attach 时，MUST 返回 `unknown` 或等价的不确定状态，而不是强行标记为可附着。

#### Scenario: 不确定时返回 unknown
- **WHEN** 一个候选目标没有满足已知支持条件，也没有命中明确不支持条件
- **THEN** readiness 结果返回 `unknown`

### Requirement: Readiness 逻辑在不依赖真实 attach 的情况下可测试
Host MUST 将 attach readiness 判断逻辑设计为可以基于受控输入数据进行确定性测试，而不要求真实执行 attach。

#### Scenario: Readiness 可以用受控样本验证
- **WHEN** 对 readiness 实现进行测试
- **THEN** 测试可以使用受控的候选进程样本验证状态判断和原因说明，而不需要真正附着进程
