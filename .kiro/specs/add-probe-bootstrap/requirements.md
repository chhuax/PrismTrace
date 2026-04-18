# Requirements Document

## Introduction

`add-probe-bootstrap` 是迭代 3（电动自行车 - 可连接但暂不采集）的收尾部分。

当前 `attach-controller` 已经建立了 attach session 生命周期和结构化失败结果，但 attach backend 仍然是 `ScriptedAttachBackend`——一个只返回预设结果的占位实现。本 feature 的目标是用真实的 live instrumentation backend 替换它，完成从"结构化 attach 控制流"到"真正连接到目标进程"的闭环。

具体来说，本 feature 需要交付：

1. 一个真实的 live attach backend，能够通过动态插桩运行时附加到运行中的 Node / Electron 进程
2. 一个轻量级 bootstrap probe，注入后能检测运行时形态、安装 hook 骨架、建立与 host 的 heartbeat 通道
3. 对外稳定的 `detach` 和 `attach-status` 命令面
4. 完整的 probe 健康状态可见性

完成后，用户可以输入一个 readiness 通过的目标 PID，看到真实的 attach / detach / attach-status / 失败原因，即使此时还没有 payload capture。

## Glossary

- **Host**: `prismtrace-host` crate，运行在用户机器上的 Rust 进程，负责进程发现、attach 控制、事件规范化和存储
- **Probe**: 注入到目标进程内部的轻量级 JavaScript 模块，负责 hook 安装和事件发射
- **Bootstrap**: Probe 的初始化阶段，包括运行时检测、hook 安装和通道建立
- **Live_Backend**: 真实的 instrumentation backend，替换 `ScriptedAttachBackend`，能够对运行中的进程执行真实 attach
- **Attach_Session**: 已在 `prismtrace-core` 中定义的结构体，表示一次 attach 的生命周期状态
- **ProbeBootstrap**: 已在 `prismtrace-core` 中定义的结构体，表示 probe 初始化结果（`Pending` / `Ready` / `Failed`）
- **ProbeHealth**: 已在 `prismtrace-core` 中定义的结构体，表示 probe 运行时健康状态，包含已安装 hook 列表和失败 hook 列表
- **Heartbeat**: Host 与 probe 之间的周期性存活信号，用于检测 probe 是否仍在运行
- **IPC_Channel**: Host 与 probe 之间的进程间通信通道，V1 使用面向行的 JSON 消息
- **AttachController**: 已在 `prismtrace-host` 中定义的控制器，管理 attach / detach 生命周期
- **AttachBackend**: 已在 `prismtrace-host` 中定义的 trait，`Live_Backend` 是其真实实现
- **Detach_Command**: 用户通过 CLI 发起的 detach 操作，对应 `--detach` 标志
- **Attach_Status_Command**: 用户通过 CLI 查询当前 attach 状态的操作，对应 `--attach-status` 标志

---

## Requirements

### Requirement 1: Live Backend 能够附加到运行中的 Node / Electron 进程

**User Story:** 作为 PrismTrace 用户，我希望能够真正附加到一个正在运行的 Node 或 Electron 进程，而不只是得到一个预设的脚本化结果，以便我能看到真实的连接状态。

#### Acceptance Criteria

1. WHEN 用户对一个 readiness 状态为 `supported` 的目标发起 attach，THE Live_Backend SHALL 通过动态插桩运行时对目标进程执行真实 attach，而不是返回预设结果
2. WHEN Live_Backend 成功附加到目标进程，THE Live_Backend SHALL 向目标进程注入 bootstrap probe
3. WHEN Live_Backend 完成 probe 注入，THE Live_Backend SHALL 返回包含 `ProbeBootstrapState::Ready` 的 `BackendAttachOutcome`，使 `AttachController` 能将 session 标记为 `attached`
4. IF Live_Backend 无法附加到目标进程（权限拒绝、进程不存在、运行时不兼容），THEN THE Live_Backend SHALL 返回包含具体失败原因的 `AttachFailure`，而不是让进程崩溃
5. THE Live_Backend SHALL 实现 `AttachBackend` trait，使 `AttachController` 无需感知底层插桩机制

---

### Requirement 2: Bootstrap Probe 完成运行时检测与 Hook 骨架安装

**User Story:** 作为 PrismTrace 用户，我希望注入的 probe 能够自动检测目标进程的运行时形态并安装正确的 hook 骨架，以便系统能够为后续 payload capture 做好准备。

#### Acceptance Criteria

1. WHEN bootstrap probe 被注入到目标进程，THE Probe SHALL 检测目标进程中 `fetch`、`undici`、`http`、`https` 的可用性
2. WHEN bootstrap probe 完成运行时检测，THE Probe SHALL 安装与检测结果对应的 hook 骨架（V1 hook 骨架只需占位，不需要捕获 payload）
3. WHEN bootstrap probe 完成 hook 安装，THE Probe SHALL 向 Host 报告已安装的 hook 列表和失败的 hook 列表
4. IF bootstrap probe 在安装某个 hook 时遇到错误，THEN THE Probe SHALL 跳过该 hook 并继续安装其余 hook，而不是中止整个 bootstrap
5. THE Probe SHALL 保证 hook 安装幂等——对同一个 hook 点重复安装不产生副作用
6. THE Probe SHALL 不修改目标进程的磁盘文件，所有操作仅限于运行时内存

---

### Requirement 3: Host 与 Probe 建立 Heartbeat 通道

**User Story:** 作为 PrismTrace 用户，我希望系统能够持续感知 probe 是否仍在运行，以便在 probe 意外退出时能够看到明确的状态变化，而不是静默失联。

#### Acceptance Criteria

1. WHEN bootstrap probe 完成初始化，THE Probe SHALL 通过 IPC_Channel 向 Host 发送第一条 heartbeat 消息
2. WHILE attach session 处于 `attached` 状态，THE Probe SHALL 以固定间隔持续向 Host 发送 heartbeat 消息
3. WHEN Host 在预期时间窗口内未收到 heartbeat，THE Host SHALL 将 attach session 标记为失联状态，并记录结构化的失联事件
4. THE IPC_Channel SHALL 使用面向行的 JSON 消息格式，每条消息包含消息类型和时间戳
5. IF IPC_Channel 连接中断，THEN THE Host SHALL 记录结构化的通道断开事件，而不是让 host 进程崩溃

---

### Requirement 4: Probe 健康状态对 Host 可见

**User Story:** 作为 PrismTrace 用户，我希望能够看到 probe 当前的健康状态，包括已安装的 hook 和失败的 hook，以便我能理解系统实际能观察到什么。

#### Acceptance Criteria

1. WHEN bootstrap probe 完成初始化，THE Host SHALL 将 probe 健康状态存储为结构化的 `ProbeHealth` 对象，包含 `installed_hooks` 和 `failed_hooks`
2. WHEN 用户查询 attach 状态，THE Host SHALL 在响应中包含当前 `ProbeHealth` 摘要
3. THE Host SHALL 将 probe 健康状态事件记录到本地状态目录，以便后续诊断
4. IF probe 报告某个 hook 安装失败，THEN THE Host SHALL 在 attach 状态报告中明确列出失败的 hook 名称，而不是只报告"部分成功"

---

### Requirement 5: Host 提供稳定的 `--detach` 命令面

**User Story:** 作为 PrismTrace 用户，我希望能够通过 CLI 主动结束当前 attach session，以便我能干净地断开与目标进程的连接。

#### Acceptance Criteria

1. WHEN 用户执行 `--detach` 命令且存在 active attach session，THE Host SHALL 向目标进程发送 detach 信号，使 probe 禁用已安装的 hook
2. WHEN detach 完成，THE Host SHALL 输出结构化的 detach 结果，包含目标进程信息和 detach 状态
3. WHEN detach 完成，THE Host SHALL 清除 active attach session，使后续 attach 请求可以被接受
4. IF 用户执行 `--detach` 命令但不存在 active attach session，THEN THE Host SHALL 返回说明"当前没有 active session"的结构化错误，而不是静默退出
5. WHEN probe 收到 detach 信号，THE Probe SHALL 移除已安装的 hook，使目标进程恢复到 attach 前的运行时状态

---

### Requirement 6: Host 提供稳定的 `--attach-status` 命令面

**User Story:** 作为 PrismTrace 用户，我希望能够随时查询当前 attach 状态，以便我能了解是否有 active session、目标是谁、probe 健康状况如何。

#### Acceptance Criteria

1. WHEN 用户执行 `--attach-status` 命令且存在 active attach session，THE Host SHALL 输出包含目标进程信息、session 状态、probe 健康摘要的结构化报告
2. WHEN 用户执行 `--attach-status` 命令且不存在 active attach session，THE Host SHALL 输出明确说明"当前无 active session"的报告，而不是报错退出
3. THE Host SHALL 在 `--attach-status` 输出中包含 `ProbeHealth` 摘要，列出已安装 hook 数量和失败 hook 数量
4. THE `--attach-status` 命令 SHALL 不修改任何 session 状态，仅作只读查询

---

### Requirement 7: Live Backend 在不依赖真实目标进程的情况下可测试

**User Story:** 作为 PrismTrace 开发者，我希望 live backend 的核心逻辑能够通过确定性测试验证，以便 CI 不需要真实的 Node 或 Electron 进程也能运行。

#### Acceptance Criteria

1. THE Live_Backend SHALL 将插桩运行时的调用封装在可替换的适配器接口后面，使测试可以注入受控的插桩结果
2. WHEN 对 live backend 进行测试，THE AttachController SHALL 能够使用受控 backend 验证 attach、bootstrap、heartbeat 和 detach 路径，而不需要真实附着到另一个进程
3. THE Probe bootstrap 逻辑 SHALL 能够在受控的运行时环境中进行单元测试，验证 hook 检测和安装决策，而不需要真实的 Node 进程
4. FOR ALL valid attach sequences（attach → heartbeat → detach），THE AttachController SHALL 在受控 backend 下产生与真实 backend 相同的 session 状态转换序列（状态机一致性）
