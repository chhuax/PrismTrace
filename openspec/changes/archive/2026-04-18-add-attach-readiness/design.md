## Context

PrismTrace 当前已经具备候选进程发现能力，但 discovery 的输出仍然偏底层：它能告诉用户“系统里有哪些可能相关的进程”，却还不能回答“现在到底该 attach 哪个目标”。在真正进入 attach controller 和 probe 注入之前，产品需要先交付一个更有产品价值的中间层：attach readiness。

这个中间层的作用，是把 discovery 结果转成更接近操作决策的结果。用户不应该只看到 pid、路径和 runtime kind，还应该看到该目标是否值得 attach、为什么值得或不值得，以及如果不能 attach，属于哪一类失败或不确定状态。

## Goals / Non-Goals

**Goals:**
- 在候选进程之上建立 attach readiness 结果模型
- 输出可附着性状态、原因分类和人类可读的解释信息
- 提供 host 侧的本地 readiness 入口，便于演示和验证
- 保持 readiness 逻辑可测试，不依赖真实 probe 注入

**Non-Goals:**
- 真正执行 attach
- 注入 probe 或建立运行时 hook
- 判断所有 macOS 进程的真实可注入性
- 完整实现复杂权限检查或系统完整性保护绕过逻辑

## Decisions

### Decision: 将 readiness 作为独立领域模型处理

attach readiness 不应仅仅是 `ProcessTarget` 上多挂一个布尔值，而应有自己的结构化结果模型。

Why:
- readiness 是一个独立的产品语义层，不只是内部计算过程
- 后续 UI、CLI、API 都需要消费这一层结果
- readiness 结果会承载状态、原因、提示语和后续动作建议

Alternative considered:
- 直接在 `ProcessTarget` 上增加 `is_attachable: bool`。拒绝原因是表达力太弱，无法承载失败分类和人类可读解释

### Decision: 第一版 readiness 使用保守状态集

第一版建议只使用少量、语义清晰的状态：

- `supported`
- `unsupported`
- `permission_denied`
- `unknown`

Why:
- 当前阶段还没有真正 attach，因此不能承诺过细的系统级判断
- 保守状态更容易稳定测试，也更不容易误导用户
- 后续 attach controller 迭代可以继续细化状态

Alternative considered:
- 一开始就细分大量 attach 错误类别。拒绝原因是当前证据不足，过早细分会制造假的精确性

### Decision: readiness 规则先基于现有可见元数据

第一版判断应优先依赖 discovery 已有字段和小幅补充的规则，而不是引入真实 attach backend。

Why:
- 该迭代的产品目标是“值得不值得 attach”，而不是“已经 attach 成功”
- 可以在不引入复杂 instrumentation 依赖的情况下快速形成有价值的产品切片
- 让 attach controller 迭代可以建立在稳定的 readiness 接口之上

Alternative considered:
- 直接把 readiness 和 attach backend 绑定。拒绝原因是会让“自行车阶段”跳成“造发动机”，失去当前切片的独立价值

## Risks / Trade-offs

- [readiness 误判过于乐观] → 第一版默认保守，宁可输出 `unknown` 也不假装“可附着”
- [用户误以为 readiness=一定 attach 成功] → 在结果中明确区分“适合进入 attach 流程”和“已验证可 attach”
- [范围滑向真正 attach] → 本次 change 明确不包含 probe、注入和 attach controller

## Migration Plan

本次变更不涉及迁移。

实现顺序：
- 在 core 中增加 readiness 领域模型
- 在 host 中增加 readiness service
- 增加本地 readiness 报告入口
- 在 README 中更新本地演示命令（如果入口发生变化）

回滚策略：
- 删除 readiness 结果模型和 host 入口，保留既有 discovery 流程

## Open Questions

- 第一版是否需要单独拆 `candidate-filtering`，还是直接在 readiness 中体现基础过滤
- 后续 attach controller 是否复用同一套状态枚举，还是只复用部分 readiness 结果

## Docs Impact

- 本设计将由 `attach-readiness` capability spec 约束
- 如果本地入口新增 `--readiness` 或等价参数，需要同步 README 与 codemap
