## Why

PrismTrace 现在已经能发现候选目标并输出 attach readiness，但用户还不能真正对某个目标发起连接，因此产品仍停留在“告诉你值得 attach”而没有进入“已经 attach 上去”的阶段。下一步需要先把 attach / detach / session state 这个最小闭环做出来，让用户可以看到连接状态和失败原因，再为后续 payload 采集打底。

## What Changes

- 为 PrismTrace 增加第一版 attach controller，允许 host 对单个 readiness 通过的目标发起 attach 与 detach
- 定义 attach session 生命周期、状态流转和失败分类
- 增加 probe bootstrap 的最小握手骨架，但本次不承诺 request / response 采集
- 提供一个可本地运行验证的 attach 演示入口，用于查看 attach state 与错误反馈

## Capabilities

### New Capabilities
- `attach-controller`: 管理 attach / detach 请求、attach session 生命周期、probe bootstrap 最小握手和失败可见性

### Modified Capabilities

## Impact

- 影响代码：`crates/prismtrace-core`、`crates/prismtrace-host`
- 影响系统：host 本地目标选择流程，从 readiness 判断提升到可执行 attach 的连接控制层
- 依赖影响：需要选定第一版 instrumentation backend 接口形态，但本次尽量不引入真正的 payload capture 依赖

## Docs Impact

- 在 `openspec/changes/add-attach-controller/specs/` 下新增 `attach-controller` capability spec
- 若本地演示入口变化，则更新 README 与 codemap
