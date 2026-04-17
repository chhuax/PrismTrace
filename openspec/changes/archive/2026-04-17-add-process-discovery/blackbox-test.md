# 黑盒测试说明

## 测试目标

- 验证 PrismTrace host 能从当前 macOS 环境返回结构化的候选进程列表，而不是原始命令输出
- 验证进程发现能力即使遇到无法明确分类的进程，也不会错误地强行标记为 Node 或 Electron

## 测试范围

- 覆盖 host 启动后的本地进程发现入口
- 覆盖发现结果中的核心字段输出：pid、显示名称、可执行路径、runtime kind
- 覆盖 `unknown` 分类的外部可观察行为

## 前置条件

- 在 macOS 本机运行 PrismTrace workspace
- 可以执行 `cargo test` 与 `cargo run -p prismtrace-host`
- 准备一组可控输入用于单元测试或 host 内部 discovery 演示

## 操作约束

- 本次验证只关注“发现候选进程”，不验证 attach、注入、hook 或 payload 采集
- 不依赖未来的 Web UI 或 HTTP API；只验证当前 host 能否返回稳定结果

## 核心场景

### 1. 返回结构化候选进程列表

- 场景类型：成功
- 输入：执行 host 侧的进程发现入口
- 关注点：
  - 返回值是结构化集合而不是原始文本
  - 每个候选项都包含 pid、显示名称、可执行路径、runtime kind
  - 空结果也以合法空集合表示
- 预期：
  - 不应出现未解析的命令行文本泄漏到最终返回结构
  - 不应因为当前没有候选目标而报错

### 2. 无法识别的进程保持 unknown

- 场景类型：阻断
- 输入：提供一个无法命中 Node / Electron 启发式规则的进程样本
- 关注点：
  - 返回结果仍然是合法的结构化 process target
  - runtime kind 明确为 `unknown`
- 预期：
  - 不应为了“看起来更完整”而错误标记为 `node` 或 `electron`

### 3. 结果可以被 host 启动流程消费

- 场景类型：回归
- 输入：运行 host 的进程发现相关测试或演示入口
- 关注点：
  - process discovery 结果能被 host 启动逻辑消费
  - 本地状态目录初始化行为未回退
- 预期：
  - 不应破坏现有 bootstrap 和本地状态目录初始化

## 通过标准

- Host 的 process discovery 入口能返回结构化候选进程集合
- 候选项包含规定字段且 runtime kind 只出现 `node`、`electron`、`unknown`
- `unknown` 分类显式保留，不出现误分类
- 现有 workspace skeleton 的启动和测试能力未回退

## 回归重点

- host bootstrap 是否仍然可以初始化 `.prismtrace/state` 目录结构
- 新增 discovery 能力后，核心领域类型是否仍保持稳定、可测试

## 自动化验证对应

- `crates/prismtrace-host/src/discovery.rs`
  - 覆盖 process discovery 结果结构化输出
  - 覆盖 runtime kind 分类与 unknown 保留
- `crates/prismtrace-host/src/lib.rs`
  - 覆盖 host bootstrap 未回退

## 测试环境待补充项

- 真实 macOS 运行进程上的集成验证尚待补充
- 后续接入 CLI 或 HTTP 入口后，需要补充端到端可见性验证
