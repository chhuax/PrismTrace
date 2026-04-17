## Context

PrismTrace 当前是一个早期 Rust workspace，主开发入口集中在 `cargo fmt --check`、`cargo clippy`、`cargo test` 和 `cargo run -p prismtrace-host -- --discover`。本地命令已经基本可用，但仓库缺少一个在 GitHub 上自动执行这些检查的最小 CI 关卡，因此格式回归或 host 启动回归只能靠本地手工发现。

这次 change 只需要给仓库补上一条稳定、可理解的 macOS 校验流水线，并把当前唯一的红灯项 `cargo fmt --check` 修复到通过。实现应保持轻量，避免在项目还处于 bootstrap 阶段时就把 CI 设计成复杂矩阵。

## Goals / Non-Goals

**Goals:**
- 为仓库增加一个可在 GitHub 上运行的 macOS CI workflow
- 让 workflow 在单个 job 中顺序执行格式检查、静态检查、测试和 `--discover` smoke test
- 让当前仓库在本地与 CI 中都能通过同一组命令
- 保持首版 CI 简洁，便于后续再扩展 Linux、Windows 或 release 流程

**Non-Goals:**
- 多平台矩阵构建
- 二进制打包、签名或发布
- 覆盖未来 attach、payload capture 或 Web UI 的更复杂集成测试
- 通过 CI 引入新的产品行为或对外接口

## Decisions

### Decision: 采用单个 `macos-latest` job 串行执行首版检查

首版 CI 使用一个 job，按固定顺序执行 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`。

Why:
- 当前仓库规模很小，单 job 配置最容易读懂和维护
- 顺序执行可以让失败点更贴近本地开发流程
- 用户已经明确选择先用最小可行方案跑通 macOS

Alternative considered:
- 拆成静态检查和测试两个 job。拒绝原因是当前收益不大，反而会增加 workflow 噪音

### Decision: smoke test 直接复用现有 host CLI 入口

workflow 中的运行级验证直接使用 `cargo run -p prismtrace-host -- --discover`，不额外新增专门的 CI-only binary 或脚本。

Why:
- 该入口已经是 README 中公开的本地验证方式
- 它同时覆盖 host bootstrap、本地状态目录初始化和 discovery 报告生成
- 避免为 CI 维护一条与开发者入口脱节的平行路径

Alternative considered:
- 只运行 `cargo test --workspace`。拒绝原因是这样无法覆盖最基本的可执行入口是否还能启动

### Decision: 在引入 workflow 时同步修复现有格式红灯

CI 会把 `cargo fmt --check` 作为第一个阻断项，因此实现时需要同步整理现有 Rust 文件格式，让仓库基线先处于 green。

Why:
- 当前本地基线里唯一失败项就是格式检查
- 如果不先修复格式，workflow 一落地就会持续红灯，没有保护价值
- 格式整理是低风险、可机械验证的变更

Alternative considered:
- 暂时不启用 fmt 检查。拒绝原因是这会放弃当前最便宜、最稳定的回归保护

## Risks / Trade-offs

- [macOS runner 环境与本机存在差异] → 使用标准 Rust toolchain 安装和当前已存在的 CLI 入口，减少环境假设
- [smoke test 输出较长] → 首版只关心命令成功退出，不在 workflow 中额外解析长输出
- [单 job 后续变慢] → 当前先换取配置简单；当检查项扩展后再按阶段拆分 job

## Migration Plan

本次变更不涉及运行时数据迁移。

实施步骤：
- 新增 OpenSpec artifacts，明确 CI 的能力边界与验证方式
- 修复当前 `cargo fmt --check` 红灯
- 新增 macOS GitHub Actions workflow
- 在本地重新执行与 workflow 对齐的命令确认通过

回滚策略：
- 删除新增 workflow，并回退本次为了 CI 基线进行的必要格式整理

## Open Questions

- 是否在下一轮把 Linux 校验一并加入矩阵
- 是否在后续为 CI 增加缓存优化或 README 中的开发者提示

## Docs Impact

- `macos-ci-workflow` 的行为约束由当前 change 下的 capability spec 定义
- 当前 change 不要求 README 变更，除非实现后需要显式记录 GitHub CI 状态或开发者校验顺序
