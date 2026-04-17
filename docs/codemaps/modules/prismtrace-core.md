# Module Identity

`prismtrace-core`

## Why It Exists

承载 PrismTrace 的共享领域模型，避免 host 和后续其他运行时模块重复定义进程样本、归一化后的目标类型和 runtime 分类逻辑。

## Main Entrypoints

- `RuntimeKind`
- `ProcessTarget`
- `ProcessSample`
- `ProbeHealth`

## Core Flows

- `ProcessSample::runtime_kind()`：根据进程名和可执行路径做启发式 runtime 分类
- `ProcessSample::normalized_app_name()`：对通用运行时名做应用名标准化
- `ProcessSample::into_target()`：把原始样本转换为结构化 `ProcessTarget`

## Internal And External Dependencies

- 当前只依赖 Rust 标准库
- 被 `prismtrace-host` 消费

## Edit Hazards And Debugging Notes

- 这里的逻辑应该保持可测试和无副作用
- 如果要引入更复杂的 runtime 判定，不要直接耦合 macOS 系统调用；优先保留 sample -> normalized target 的纯逻辑边界
