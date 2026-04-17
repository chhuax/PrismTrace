## 1. Readiness 领域模型

- [x] 1.1 在 `prismtrace-core` 中增加 attach readiness 结果模型和状态枚举
- [x] 1.2 为 readiness 状态、原因说明和保守 unknown 行为补充测试

## 2. Host readiness service

- [x] 2.1 在 host 中增加 readiness 模块，将候选 `ProcessTarget` 转化为 readiness 结果
- [x] 2.2 增加一个返回 readiness 结果集合的 host service 入口
- [x] 2.3 为 supported / unsupported / unknown 等判断路径增加确定性测试

## 3. 本地演示与回归验证

- [x] 3.1 将 readiness service 接到一个可本地运行验证的最小 host 入口上
- [x] 3.2 验证 readiness 引入后，现有 bootstrap 和 discovery 能力仍然通过
- [x] 3.3 如果本地开发入口变化，则同步更新 README 与相关 codemap

## 4. writing-plans 追补计划与执行映射

### 4.1 Readiness 领域模型

- [x] 4.1.1 补充 `crates/prismtrace-core/src/lib.rs` 的执行映射，明确该文件承载 `AttachReadinessStatus`、`AttachReadiness` 与人类可读摘要
- [x] 4.1.2 补充该层测试映射，确认 readiness 状态标签与摘要输出由 `prismtrace-core` 的单元测试覆盖
- [x] 4.1.3 记录这一层本应按“TDD 先测状态表达、再补最小实现、最后回归验证”的顺序推进
- [x] 4.1.4 将领域模型完成情况与当前代码状态对齐，作为本 change 归档前的可追踪执行记录

### 4.2 Host readiness service

- [x] 4.2.1 补充 `crates/prismtrace-host/src/readiness.rs` 的执行映射，明确该文件负责将 `ProcessTarget` 转化为保守的 readiness 结果
- [x] 4.2.2 补充 host readiness 测试映射，覆盖 `supported`、`permission_denied`、`unknown` 三条判断路径
- [x] 4.2.3 补充 `crates/prismtrace-host/src/lib.rs` 的执行映射，明确 snapshot/report 属于 host 聚合层职责
- [x] 4.2.4 记录这一层本应按“受控样本测试 -> 最小判断逻辑 -> host 集成测试”的顺序推进
- [x] 4.2.5 将 service 与 host 集成完成情况写回当前 change，作为归档前的追补执行记录

### 4.3 本地入口与回归验证

- [x] 4.3.1 在 `crates/prismtrace-host/src/main.rs` 中新增 `--readiness` 入口，让本地 host 可以直接输出 readiness 报告
- [x] 4.3.2 更新 `README.md`、`README.zh-CN.md`、`docs/surfaces.yaml` 与 codemap，保持本地演示入口可发现
- [x] 4.3.3 运行 `cargo test`，确认 readiness 引入后 bootstrap、discovery、readiness 全量测试都通过
- [x] 4.3.4 运行 `cargo run -p prismtrace-host -- --readiness`，确认本地演示入口可用并输出结构化 readiness 报告
