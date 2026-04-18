## Context

PrismTrace 当前已经具备候选目标发现和 attach readiness 判断能力，但产品还停留在“告诉用户哪些目标值得 attach”的阶段。下一步需要把这条链路推进到真正可执行的连接控制层：用户可以选择一个 readiness 通过的目标，发起 attach，看到 attach 是否成功，以及在失败时得到可理解的反馈。

这一迭代的核心价值不是“已经采集到 payload”，而是“已经接通到目标并拥有可见的连接生命周期”。如果这一步不单独稳定下来，后续 request capture、session timeline 和分析能力都会建立在不稳定的 attach 基础之上。

当前约束：

- 第一版仍然是 macOS only
- 目标仍然聚焦 Node / Electron 系 AI CLI 或桌面应用
- 不要求重启目标应用
- 本次不承诺任何 request / response capture
- 当前 host 仍然是本地 CLI 演示入口，而不是常驻 daemon

## Goals / Non-Goals

**Goals:**
- 在 host 中建立 attach controller，允许对单个 readiness 通过的目标发起 attach 和 detach
- 定义 attach session 的生命周期状态、失败分类和最小可见元数据
- 定义 probe bootstrap 的最小握手骨架，使“已 attach”有明确语义边界
- 提供可本地演示和可测试的 attach control path

**Non-Goals:**
- 实现真实的 request / response payload 采集
- 支持多个目标并发 attach
- 支持所有 Node / Electron 进程的真实 live attach 成功率
- 建立长期驻留的后台服务或完整本地 Web UI

## Decisions

### Decision: V1 只允许一个 active attach session

V1 的 host 在任意时刻只维护一个 active attach session。

Why:
- attach controller 的第一目标是把“能 attach 上去”这件事变得稳定，而不是提前解决并发调度
- 单 session 模式更容易测试状态流转和失败处理
- 后续即使要支持多目标 attach，也可以在 controller 接口稳定后扩展

Alternative considered:
- 一开始就支持多 session。拒绝原因是会显著增加 session 管理、冲突处理和 detach 语义复杂度，超出当前产品切片

### Decision: attach success 必须以后端握手成功为边界

本次设计不把“已发起 attach 尝试”视为 attach 成功，而是要求 attach backend 返回成功并完成最小 probe bootstrap 握手后，session 才进入 `attached`。

Why:
- 否则“连接成功”会变成没有语义保证的假阳性
- 后续 request capture 需要依赖“probe 已经在线”这一事实
- 这能把 attach backend 成功和 probe 在线明确区分出来

Alternative considered:
- 只要 backend 返回 attach 尝试成功就标记为 attached。拒绝原因是会让后续采集层无法判断 probe 是否真实可用

### Decision: attach backend 以抽象接口接入，第一版可以先用 fake backend 验证控制流

attach controller 不直接绑定 конкрет instrumentation runtime，而是先定义一层 attach backend 接口。实现阶段可以先提供 fake backend 或受控 backend 验证控制路径与状态机。

Why:
- 当前阶段的重点是产品闭环和控制面，而不是底层注入细节
- 这样能在没有真实 live attach backend 的情况下先完成 host 侧设计和测试
- 后续替换成真实 backend 时，不需要推倒 controller、session 或错误模型

Alternative considered:
- 在这一迭代里就绑定某个真实 instrumentation runtime。拒绝原因是会把本次切片和底层注入选型绑死，增加交付风险

### Decision: CLI 演示入口先服务控制流验证，不追求最终产品命令面

第一版 attach controller 可以通过本地 CLI 入口暴露 attach / detach / status 行为，但这些参数形态只服务当前阶段的验证，不视为最终稳定 surface。

Why:
- 当前仓库还没有本地控制台或常驻 API
- 先把 attach control path 跑通，比过早设计最终命令面更重要
- README 和 codemap 只需要把它当作开发/演示入口说明

Alternative considered:
- 等 Web UI 或 HTTP API 再引入 attach。拒绝原因是会让“电动自行车阶段”失去独立可验证价值

## Risks / Trade-offs

- [真实 backend 迟迟未定，attach 只能停留在模拟层] → 先把 backend interface、session 状态机和错误模型稳定下来，后续再替换真实 backend
- [用户误以为 attach 成功就意味着已经开始采集] → 明确把 `attached` 与“probe 已握手但尚未采集”区分开，在报告中避免误导
- [单 active session 限制过早暴露产品局限] → 在设计里明确这是 V1 约束，保证接口可扩展但不提前做多 session
- [attach 失败语义不清导致后续调试困难] → 先定义有限且稳定的失败分类，保证错误可观察、可测试、可渲染

## Migration Plan

本次变更不涉及数据迁移。

实现顺序：
- 在 core 中增加 attach session 和 attach state 领域模型
- 在 host 中增加 attach controller 与 backend interface
- 增加 probe bootstrap 最小握手结果模型
- 提供本地 attach / detach / status 演示入口
- 更新 README 与 docs contract

回滚策略：
- 删除 attach controller 入口和 session 模型，保留既有 discovery / readiness 能力

## Open Questions

- 第一版 CLI 演示入口是否收敛为 `--attach <pid>` / `--detach` / `--attach-status`，还是改成子命令形态
- 真实 instrumentation backend 在下一步实现时优先接 fake backend + adapter，还是直接绑定现成 runtime
- attach session 是否需要在第一版持久化到本地 storage，还是先保持进程内状态

## Docs Impact

- 本设计将由 `attach-controller` capability spec 约束
- 若新增本地 attach 演示入口，需要同步 README 与 codemap
