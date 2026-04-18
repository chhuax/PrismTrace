# Implementation Plan: add-probe-bootstrap

## Overview

用真实的 `LiveAttachBackend` 替换 `ScriptedAttachBackend`，实现 bootstrap probe 注入、IPC heartbeat 通道、probe 健康状态可见性，以及 `--detach` / `--attach-status` CLI 命令面。

整体分为五个阶段：
1. 定义 IPC 消息协议与 `InstrumentationRuntime` 适配器接口
2. 实现 bootstrap probe（JavaScript）与 host 端 IPC 监听器
3. 实现 `LiveAttachBackend`，替换 `ScriptedAttachBackend`
4. 实现 `--detach` 与 `--attach-status` CLI 命令面
5. 集成收尾与端到端验证

## Tasks

- [x] 1. 定义 IPC 消息协议与插桩运行时适配器接口
  - [x] 1.1 在 `prismtrace-core` 中新增 IPC 消息类型
    - 新增 `IpcMessage` enum，包含 `Heartbeat { timestamp_ms: u64 }`、`BootstrapReport { installed_hooks: Vec<String>, failed_hooks: Vec<String>, timestamp_ms: u64 }`、`DetachAck { timestamp_ms: u64 }` 三个变体
    - 为 `IpcMessage` 实现面向行的 JSON 序列化（`to_json_line() -> String`）与反序列化（`from_json_line(s: &str) -> Result<Self, ...>`）
    - _Requirements: 3.4_

  - [x] 1.2 为 `IpcMessage` 编写单元测试
    - 验证每种消息变体的序列化 / 反序列化往返一致性
    - 验证格式错误输入返回结构化错误而非 panic
    - _Requirements: 3.4, 7.1_

  - [x] 1.3 在 `prismtrace-host` 中定义 `InstrumentationRuntime` trait
    - 定义 `InstrumentationRuntime` trait，包含 `inject_probe(pid: u32, probe_script: &str) -> Result<(), AttachFailure>` 和 `send_detach_signal(pid: u32) -> Result<(), AttachFailure>` 两个方法
    - 定义 `ScriptedInstrumentationRuntime`，允许测试注入受控的插桩结果（成功 / 失败）
    - _Requirements: 7.1, 7.2_

  - [x] 1.4 为 `InstrumentationRuntime` 编写单元测试
    - 验证 `ScriptedInstrumentationRuntime` 能正确返回预设的成功 / 失败结果
    - _Requirements: 7.1_

- [x] 2. 实现 bootstrap probe（JavaScript）
  - [x] 2.1 创建 probe 脚本文件结构
    - 在 `crates/prismtrace-host/probe/` 目录下创建 `bootstrap.js`
    - probe 脚本将在构建时通过 `include_str!` 嵌入到 host 二进制中
    - _Requirements: 2.6_

  - [x] 2.2 实现运行时检测逻辑
    - 在 `bootstrap.js` 中检测 `fetch`、`undici`、`http`、`https` 的可用性
    - 将检测结果收集为 `{ available: string[], unavailable: string[] }` 结构
    - _Requirements: 2.1_

  - [x] 2.3 实现 hook 骨架安装逻辑
    - 对每个可用的运行时 API，安装占位 hook（V1 不捕获 payload，仅占位）
    - 每个 hook 安装操作包裹在 try/catch 中，失败时跳过并记录到 `failed_hooks`
    - 保证幂等性：安装前检查是否已安装，避免重复安装
    - _Requirements: 2.2, 2.3, 2.4, 2.5_

  - [x] 2.4 实现 IPC 通道与 heartbeat 发射逻辑
    - probe 通过 `process.send()` 向 host 发送 `BootstrapReport` 消息（包含 `installed_hooks` 和 `failed_hooks`）
    - bootstrap 完成后以固定间隔（默认 5000ms）持续发送 `Heartbeat` 消息
    - 收到 detach 信号时发送 `DetachAck`，移除已安装的 hook，停止 heartbeat
    - _Requirements: 3.1, 3.2, 3.4, 5.5_

  - [x] 2.5 为 probe 逻辑编写单元测试
    - 在受控的 mock 运行时环境中验证 hook 检测和安装决策
    - 验证 hook 安装幂等性：重复调用不产生副作用
    - 验证单个 hook 安装失败不中止整个 bootstrap
    - _Requirements: 2.4, 2.5, 7.3_

- [x] 3. 实现 host 端 IPC 监听器与 probe 健康状态管理
  - [x] 3.1 在 `prismtrace-host` 中实现 `IpcListener`
    - 实现 `IpcListener` 结构体，通过子进程 stdout 读取面向行的 JSON 消息
    - 提供 `poll_message(&mut self) -> Option<IpcMessage>` 方法（非阻塞）
    - 提供 `last_heartbeat_at(&self) -> Option<Instant>` 方法
    - _Requirements: 3.4, 3.5_

  - [x] 3.2 在 `prismtrace-host` 中实现 heartbeat 超时检测
    - 在 `AttachController` 中新增 `check_heartbeat_timeout(&mut self, timeout: Duration)` 方法
    - 当超时时将 active session 标记为失联状态，并通过 `StorageLayout::logs_dir` 写入结构化失联事件 JSON 文件
    - _Requirements: 3.3, 4.3_

  - [x] 3.3 为 `IpcListener` 编写单元测试
    - 验证正常消息解析路径
    - 验证 IPC 通道断开时返回结构化错误而非 panic
    - _Requirements: 3.5, 7.2_

  - [x] 3.4 在 `prismtrace-host` 中实现 `ProbeHealth` 状态管理
    - 在 `AttachController` 中新增 `probe_health: Option<ProbeHealth>` 字段
    - 收到 `BootstrapReport` 消息时，构造 `ProbeHealth` 并存储到 controller
    - 将 `ProbeHealth` 事件序列化为 JSON 写入 `StorageLayout::logs_dir`
    - _Requirements: 4.1, 4.3_

  - [x] 3.5 为 probe 健康状态管理编写单元测试
    - 验证 `BootstrapReport` 消息正确填充 `ProbeHealth`
    - 验证失败 hook 列表被正确记录
    - _Requirements: 4.1, 4.4_

- [x] 4. 实现 `LiveAttachBackend`
  - [x] 4.1 创建 `LiveAttachBackend` 结构体
    - 在 `crates/prismtrace-host/src/attach.rs` 中新增 `LiveAttachBackend<R: InstrumentationRuntime>` 结构体
    - 持有 `runtime: R`、`ipc_listener: Option<IpcListener>`、`probe_script: &'static str`（通过 `include_str!` 嵌入）
    - _Requirements: 1.5, 7.1_

  - [x] 4.2 实现 `AttachBackend::attach` for `LiveAttachBackend`
    - 调用 `runtime.inject_probe(pid, probe_script)` 执行真实注入
    - 等待 `IpcListener` 收到 `BootstrapReport` 消息（带超时，默认 10s）
    - 根据 `BootstrapReport` 构造 `BackendAttachOutcome`（`installed_hooks` 非空则 `Ready`，否则 `Failed`）
    - 注入失败时返回包含具体原因的 `AttachFailure`（`BackendRejected`）
    - _Requirements: 1.1, 1.2, 1.3, 1.4_

  - [x] 4.3 实现 `AttachBackend::detach` for `LiveAttachBackend`
    - 调用 `runtime.send_detach_signal(pid)` 向目标进程发送 detach 信号
    - 等待 `IpcListener` 收到 `DetachAck` 消息（带超时，默认 5s）
    - 超时或失败时返回 `AttachFailure { kind: DetachFailed, ... }`
    - _Requirements: 5.1, 5.2_

  - [x] 4.4 为 `LiveAttachBackend` 编写单元测试（使用 `ScriptedInstrumentationRuntime`）
    - 验证成功 attach 路径：注入成功 + `BootstrapReport` 到达 → `BackendAttachOutcome::ready`
    - 验证注入失败路径：`runtime.inject_probe` 返回错误 → `AttachFailure::BackendRejected`
    - 验证 bootstrap 超时路径：`BootstrapReport` 未到达 → `AttachFailure::HandshakeFailed`
    - 验证 detach 路径：`DetachAck` 到达 → 成功
    - _Requirements: 1.4, 7.1, 7.2_

  - [x] 4.5 为 `AttachController` + `LiveAttachBackend` 编写状态机一致性测试
    - 验证 attach → heartbeat → detach 完整序列下，session 状态转换与 `ScriptedAttachBackend` 产生相同结果
    - _Requirements: 7.4_

- [x] 5. Checkpoint — 确保所有测试通过
  - 运行 `cargo test --workspace`，确保所有测试通过，如有问题请向用户说明。

- [x] 6. 实现 `--detach` 与 `--attach-status` CLI 命令面
  - [x] 6.1 在 `prismtrace-host/src/lib.rs` 中新增 `detach_snapshot` 与 `attach_status_snapshot` 函数
    - `collect_detach_snapshot(result, backend) -> io::Result<DetachSnapshot>`：调用 `AttachController::detach()`，返回结构化结果
    - `collect_attach_status_snapshot(result) -> io::Result<AttachStatusSnapshot>`：只读查询 active session 与 `ProbeHealth`，不修改状态
    - 定义对应的 `DetachSnapshot` 和 `AttachStatusSnapshot` 结构体
    - _Requirements: 5.2, 5.3, 5.4, 6.1, 6.2, 6.4_

  - [x] 6.2 实现 `detach_report` 与 `attach_status_report` 格式化函数
    - `detach_report(snapshot: &DetachSnapshot) -> String`：包含目标进程信息和 detach 状态
    - `attach_status_report(snapshot: &AttachStatusSnapshot) -> String`：包含 session 状态、目标信息、`ProbeHealth` 摘要（已安装 hook 数量、失败 hook 数量）
    - 无 active session 时输出明确说明，而非报错
    - _Requirements: 5.2, 5.4, 6.1, 6.2, 6.3_

  - [x] 6.3 在 `main.rs` 中接入 `--detach` 与 `--attach-status` 命令
    - 解析 `--detach` 标志，调用 `collect_detach_snapshot` 并打印 `detach_report`
    - 解析 `--attach-status` 标志，调用 `collect_attach_status_snapshot` 并打印 `attach_status_report`
    - _Requirements: 5.1, 5.3, 5.4, 6.1, 6.2_

  - [x] 6.4 为 `--detach` 与 `--attach-status` 编写单元测试
    - 验证有 active session 时 detach 返回结构化成功结果
    - 验证无 active session 时 detach 返回 `NoActiveSession` 错误
    - 验证 `--attach-status` 在有 / 无 session 时均返回正确报告，且不修改 session 状态
    - 验证 `--attach-status` 报告包含 `ProbeHealth` 摘要
    - _Requirements: 5.3, 5.4, 6.1, 6.2, 6.3, 6.4_

- [x] 7. 在 `main.rs` 中将 `ScriptedAttachBackend` 替换为 `LiveAttachBackend`
  - 将 `--attach` 命令的 backend 从 `ScriptedAttachBackend::ready()` 替换为 `LiveAttachBackend::new(NodeInstrumentationRuntime)`
  - 确保 `ScriptedAttachBackend` 仍保留供测试使用，不删除
  - _Requirements: 1.1, 1.5_

- [x] 8. Final Checkpoint — 确保所有测试通过
  - 运行 `cargo test --workspace`，确保所有测试通过，如有问题请向用户说明。

## Notes

- 标有 `*` 的子任务为可选测试任务，可跳过以加快 MVP 进度
- `ScriptedAttachBackend` 和 `ScriptedInstrumentationRuntime` 保留供测试使用
- probe 脚本通过 `include_str!` 嵌入二进制，不依赖运行时文件系统
- V1 hook 骨架只需占位，不捕获 payload
- IPC 通道使用面向行的 JSON，每行一条消息
