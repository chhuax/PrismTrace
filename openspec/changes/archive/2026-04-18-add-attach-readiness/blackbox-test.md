# 黑盒测试说明

## 测试目标

- 验证 PrismTrace host 能把候选进程进一步转化为 attach readiness 结果，而不是只输出底层 discovery 列表
- 验证 readiness 结果能明确表达“值得 attach / 不值得 attach / 目前无法判断”的状态与原因

## 测试范围

- 覆盖 host 侧的 attach readiness 入口
- 覆盖 readiness 结果中的核心字段：目标进程、readiness 状态、原因说明
- 覆盖不确定状态和保守判断策略

## 前置条件

- 在 macOS 本机运行 PrismTrace workspace
- 已具备 `process-discovery` 能力
- 可以执行 `cargo test` 与 `cargo run -p prismtrace-host`

## 操作约束

- 本次验证只关注 attach readiness 判断，不验证真正 attach、probe 注入或 payload 采集
- 不依赖未来的 Web UI；只验证当前 host 能否返回稳定的 readiness 结果

## 核心场景

### 1. 候选进程被转化为 readiness 结果

- 场景类型：成功
- 输入：执行 host 的 attach readiness 入口
- 关注点：
  - 返回值是 readiness 结果集合，而不是 discovery 原始集合
  - 每个结果都绑定一个候选目标
  - 每个结果都包含状态和原因说明
- 预期：
  - 不应退化成只有 `true/false` 的判断
  - 不应丢失目标进程上下文

### 2. 不确定目标保持 unknown

- 场景类型：阻断
- 输入：提供一个当前无法可靠判断是否值得 attach 的候选目标
- 关注点：
  - readiness 明确返回不确定状态
  - 原因说明能让用户理解为什么当前不能贸然 attach
- 预期：
  - 不应为了看起来更完整而强行标记为 supported

### 3. readiness 不破坏现有 discovery 和 bootstrap

- 场景类型：回归
- 输入：运行 readiness 相关测试和本地演示入口
- 关注点：
  - 现有 `.prismtrace/state` 初始化不回退
  - 现有 discovery 能力仍然可用
- 预期：
  - 不应因为引入 readiness 而破坏 discovery 或 host 启动流程

## 通过标准

- Host 能输出结构化 attach readiness 结果集合
- 每个 readiness 结果都有状态和原因说明
- 无法可靠判断时显式返回 `unknown` 或等价状态
- 现有 discovery 和 bootstrap 能力未回退

## 回归重点

- `process-discovery` 输出是否仍然稳定
- host bootstrap 和本地状态目录初始化是否仍然通过

## 自动化验证对应

- `crates/prismtrace-core/src/lib.rs`
  - 覆盖 readiness 领域模型与状态表达
- `crates/prismtrace-host/src/readiness.rs`
  - 覆盖 readiness 结果生成和原因说明
- `crates/prismtrace-host/src/lib.rs`
  - 覆盖 host bootstrap 与 readiness 集成未回退

## 测试环境待补充项

- 真实 macOS 进程下的 readiness 策略仍需进一步验证
- 后续接入 attach controller 后，需要补充 readiness 到 attach 的端到端验证
