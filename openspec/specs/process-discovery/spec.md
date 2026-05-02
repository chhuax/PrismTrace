# process-discovery Specification

## Purpose
定义 PrismTrace 在 macOS 上发现候选 Node / Electron 进程的能力边界，确保 host 能返回结构化的进程目标集合，为后续 observer source 与本地观测流程提供稳定输入。
## Requirements
### Requirement: Host 可以枚举候选进程
PrismTrace host MUST 提供一个进程发现操作，用于返回当前 macOS 上正在运行的候选进程集合。

#### Scenario: Discovery 返回零个或多个 process target
- **WHEN** host 执行 process discovery
- **THEN** 它返回的是结构化的 process target 集合，而不是原始文本输出

### Requirement: Process target 包含 PrismTrace 需要的运行时元数据
每个被 discovery 返回的 process target MUST 包含 PrismTrace 后续观测工作需要的元数据：process id、可展示的应用名称、可执行路径和 runtime kind。

#### Scenario: Discovery 结果包含必需字段
- **WHEN** 一个运行中的进程被 discovery 返回
- **THEN** 该 process target 包含 process id、可展示名称、可执行路径和 runtime kind

### Requirement: Runtime 分类必须容忍不确定性
Process discovery MUST 将候选进程分类为 `node`、`electron` 或 `unknown`，并且当仅凭当前元数据无法可靠判断 runtime 时，MUST 保留 `unknown`。

#### Scenario: 无法确定 runtime 时显式保留 unknown
- **WHEN** 一个被发现的进程没有命中 host 的 Node 或 Electron 启发式规则
- **THEN** 返回的 process target 使用 `unknown` 作为 runtime kind，而不是强行分类为 Node 或 Electron

### Requirement: Discovery 逻辑在不依赖 live attach 的情况下可测试
Host MUST 将 process discovery 设计为可以在不依赖 probe injection 或 live attach session 的前提下，对 runtime 分类和 target 标准化逻辑进行测试。

#### Scenario: Discovery 行为可以通过确定性测试验证
- **WHEN** 对 discovery 实现进行测试
- **THEN** 测试可以使用可控输入数据验证 target 标准化和 runtime 分类，而不需要附着到另一个进程
