## 1. Attach 领域模型

- [x] 1.1 在 `prismtrace-core` 中增加 attach session、attach state 和 attach failure 结果模型
- [x] 1.2 为 attach 成功边界、单 active session 约束和结构化失败结果补充测试

## 2. Host attach controller

- [x] 2.1 在 host 中增加 attach backend interface 和受控 backend 测试替身
- [x] 2.2 增加 attach controller，将 readiness 通过的目标转化为 attach session 状态流转
- [x] 2.3 增加 detach 入口和 active session 管理
- [x] 2.4 为 attach、握手失败、重复 attach 和 detach 路径补充确定性测试

## 3. 本地演示与回归验证

- [x] 3.1 将 attach controller 接到一个可本地运行验证的最小 host 入口上
- [x] 3.2 验证 attach 引入后，现有 bootstrap、discovery 和 readiness 能力仍然通过
- [x] 3.3 如果本地开发入口变化，则同步更新 README 与相关 codemap

## 4. writing-plans 细化执行记录

### 4.1 Attach 领域模型

- [x] 4.1.1 先在 `crates/prismtrace-core/src/lib.rs` 中补充 attach session、attach state、probe bootstrap 和结构化 failure 的失败测试
- [x] 4.1.2 运行 `cargo test -p prismtrace-core attach`，确认新增测试先失败，再进入最小实现
- [x] 4.1.3 在 `crates/prismtrace-core/src/lib.rs` 中实现 attach 领域模型与摘要输出
- [x] 4.1.4 再次运行 `cargo test -p prismtrace-core attach`，确认领域模型测试转绿

### 4.2 Host attach controller

- [x] 4.2.1 先在 `crates/prismtrace-host/src/attach.rs` 中补充受控 backend、单 active session、握手失败和 detach 路径的失败测试
- [x] 4.2.2 运行 `cargo test -p prismtrace-host attach::tests`，确认控制流测试先失败，再进入实现
- [x] 4.2.3 在 `crates/prismtrace-host/src/attach.rs` 中实现 attach backend interface、受控 backend 和 attach controller
- [x] 4.2.4 在 `crates/prismtrace-host/src/lib.rs` 中接入 attach snapshot/report 和 host 侧集成测试
- [x] 4.2.5 再次运行 `cargo test -p prismtrace-host attach::tests`，确认 attach controller 测试转绿

### 4.3 本地入口与回归验证

- [x] 4.3.1 在 `crates/prismtrace-host/src/main.rs` 中增加最小 attach 演示入口，优先覆盖 supported target 的 attach 成功路径
- [x] 4.3.2 更新 `README.md`、`README.zh-CN.md`、`docs/surfaces.yaml` 与 codemap，使新的本地演示入口可发现
- [x] 4.3.3 运行 `cargo test`，确认 bootstrap、discovery、readiness 和 attach 全量测试都通过
- [x] 4.3.4 运行 `cargo run -p prismtrace-host -- --attach <pid>` 的本地演示命令，确认 attach 报告可用
