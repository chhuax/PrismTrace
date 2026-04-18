# 黑盒测试说明

## 测试目标

- 验证 PrismTrace host 已经从“只做 readiness 判断”推进到“可以对目标发起 attach / detach 并返回结构化连接状态”
- 验证 attach 成功、attach 失败和主动 detach 都能以用户可见的结构化结果表达，而不是退化成裸日志或崩溃

## 测试范围

- 覆盖 host 侧 attach controller 入口
- 覆盖 attach session 生命周期状态、失败分类和最小 probe bootstrap 握手边界
- 覆盖 attach 引入后对 bootstrap、discovery、readiness 的非回退要求

## 前置条件

- 在 macOS 本机运行 PrismTrace workspace
- 已具备 `process-discovery` 和 `attach-readiness` 能力
- 可以执行 `cargo test` 与 `cargo run -p prismtrace-host`
- 当前阶段允许使用受控 backend 或 fake backend 验证 attach 控制流

## 操作约束

- 本次验证只关注 attach control path，不验证 request / response payload capture
- 不依赖未来的 Web UI；只验证当前 host 能否返回稳定的 attach / detach / status 结果
- 即使 attach backend 是受控实现，也必须从外部可见行为上验证状态流转和错误反馈

## 核心场景

### 1. readiness 通过的目标可以进入 attached

- 场景类型：成功
- 输入：对一个 readiness 为 `supported` 的目标发起 attach
- 关注点：
  - host 返回结构化 attach session 结果
  - attach session 包含目标、状态和人类可读说明
  - 只有在 backend 和最小握手都完成后，状态才进入 `attached`
- 预期：
  - 不应把“仅发起 attach 尝试”误报成 `attached`
  - 不应丢失 attach 目标上下文

### 2. 第二个 attach 在已有 active session 时被拒绝

- 场景类型：阻断
- 输入：先 attach 成功一个目标，再对第二个目标发起 attach
- 关注点：
  - host 明确拒绝第二次 attach
  - 错误结果是结构化的，并明确说明已有 active session
- 预期：
  - 不应静默覆盖当前 active session
  - 不应出现两个同时处于 active 状态的 session

### 3. attach 失败返回结构化失败结果

- 场景类型：失败
- 输入：对一个在 backend 或握手阶段会失败的目标发起 attach
- 关注点：
  - host 返回失败状态和人类可读原因
  - 失败后 session 不会被误保留为 active
- 预期：
  - 不应只打印原始错误文本
  - 不应让 host 进程因 attach 失败而崩溃

### 4. active session 可以被主动 detach

- 场景类型：成功
- 输入：对当前 active attach session 发起 detach
- 关注点：
  - host 返回表示 detach 已完成的结构化结果
  - detach 之后 host 不再报告 active session
- 预期：
  - 不应在 detach 后继续保留过期 active 状态
  - 不应要求重启 host 才能清除 session

### 5. attach controller 不破坏现有基础能力

- 场景类型：回归
- 输入：运行 attach 相关测试和本地演示入口
- 关注点：
  - 现有 `.prismtrace/state` 初始化不回退
  - 现有 discovery 和 readiness 能力仍然可用
- 预期：
  - 不应因为引入 attach controller 而破坏 bootstrap、discovery 或 readiness

## 通过标准

- Host 能输出结构化 attach session 结果，而不是只有布尔值或裸日志
- attach 仅在 backend 与最小握手完成后才进入成功状态
- 已有 active session 时，第二次 attach 被结构化拒绝
- active session 可以被 detach，且 detach 后不再保持 active
- attach 引入后，bootstrap、discovery、readiness 未回退

## 回归重点

- `process-discovery` 输出是否仍然稳定
- `attach-readiness` 判断是否仍然保守稳定
- host bootstrap 和本地状态目录初始化是否仍然通过

## 自动化验证对应

- `crates/prismtrace-core/src/lib.rs`
  - 覆盖 attach session、attach state 与失败分类的领域模型
- `crates/prismtrace-host/src/attach.rs`
  - 覆盖 attach / detach 控制流、单 active session 限制和结构化失败结果
- `crates/prismtrace-host/src/lib.rs`
  - 覆盖 host bootstrap、discovery、readiness 与 attach 集成未回退

## 测试环境待补充项

- 真实 macOS live attach backend 接入后的联调验证仍需补充
- attach 成功后与后续 request capture 的端到端验证将在下一迭代补充
