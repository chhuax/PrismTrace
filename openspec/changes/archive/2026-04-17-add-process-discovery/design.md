## Context

PrismTrace 现在已经有了 Rust workspace 骨架，包括 host 二进制、共享领域类型和本地状态初始化能力。下一步产品要解决的问题，是在 macOS 上发现真实存在的可附着候选进程，这样后续 attach 和观测能力才能从真实目标列表出发，而不是依赖手写 pid 或临时输入。

这个 change 会同时涉及核心领域类型和 host crate，但整体仍然应该保持轻量。它的目标只是建立 discovery 流程，并定义稳定的候选进程数据结构。

## Goals / Non-Goals

**Goals:**
- 为 macOS 增加 host 侧的进程发现入口
- 返回结构化的 `ProcessTarget`，用于表示看起来像 Node / Electron AI 应用的运行进程
- 保持分类逻辑可预测、可测试、便于后续扩展
- 让第一版 discovery 可以在不依赖 live attach 的情况下完成测试

**Non-Goals:**
- live attach、probe injection、网络拦截
- 对所有 macOS 进程做到完整而精确的 AI 应用识别
- Electron renderer 进程级别的深入分析
- 超出当前 host 本地测试需要的 HTTP 或 UI 暴露能力

## Decisions

### Decision: 先做 host 本地 discovery service

第一版实现应放在 `prismtrace-host` 中，并返回定义在 `prismtrace-core` 里的领域类型。

Why:
- discovery 属于 host 行为，而不是 storage 行为
- 后续 host 需要通过 CLI 或 HTTP 边界暴露 discovery 结果
- 把共享 target 类型放在 `prismtrace-core` 中，可以避免重复定义进程元数据模型

Alternative considered:
- 直接把 discovery 写进二进制入口。拒绝原因是这会让后续 API 化和测试都更难推进

### Decision: 第一版先使用收窄后的进程元数据模型

第一版只采集当前路线图真正需要的字段：pid、应用名、可执行路径、runtime kind。

Why:
- 这些字段已经和现有的 `ProcessTarget` 骨架对齐
- 第一阶段 discovery 的用途是候选目标筛选，而不是做进程取证分析
- 收窄模型可以在 attach 方案还没定完前保持 spec 稳定

Alternative considered:
- 一开始就加入命令行参数、环境变量、父进程链、窗口标题、代码签名信息。拒绝原因是这会在产品真正需要之前就扩大表面积

### Decision: 用显式启发式规则做 runtime 分类

第一版应通过简单、可解释的启发式规则，从进程名和可执行路径推断 `RuntimeKind`。

Why:
- 行为可预测，测试容易写
- 可以避免过早依赖更深层的 macOS 专有 API
- 为后续兼容性增强留出清晰扩展缝隙

Alternative considered:
- 一开始就依赖更丰富的 macOS 进程检查能力。拒绝原因是当前 change 的目标是建立流程，而不是在第一步就追求识别精度最大化

## Risks / Trade-offs

- [启发式规则误分类] → 第一版分类器保持保守，并明确允许返回 `Unknown`
- [macOS 进程可见性差异] → 让测试围绕可控输入和标准化逻辑展开，把真实进程枚举封装在很小的 host 抽象之后
- [范围膨胀到 attach 工作] → 明确将本次 change 限定在 discovery 和结构化输出

## Migration Plan

本次变更不涉及运行时迁移。

实现顺序：
- 视需要补充 discovery 相关共享进程类型
- 增加 host discovery service 及测试
- 增加一个可在本地触发 discovery 的最小 host 入口

回滚策略：
- 删除 host discovery 模块，保留现有 workspace skeleton 不变

## Open Questions

- 第一版真实实现是仅使用标准库加 `ps`，还是引入专门的进程检查 crate
- 第一版 host 暴露 discovery 能力时，是先走 CLI 入口，还是直接接到后续 HTTP handler

## Docs Impact

- 本设计由当前 change 中的 `process-discovery` capability spec 约束
- 只有当本地开发入口在实现后发生变化时，才需要更新仓库 README
