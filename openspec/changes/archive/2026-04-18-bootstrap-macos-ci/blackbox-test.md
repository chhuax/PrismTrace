# 黑盒测试说明

## 测试目标

- 验证仓库已经具备一个可在 GitHub 上执行的 macOS CI 入口
- 验证首版 CI 覆盖当前最关键的四类校验：格式、lint、测试、host 启动 smoke test

## 测试范围

- 覆盖 `.github/workflows/` 下的 macOS workflow 是否存在且可读
- 覆盖 workflow 中对 Rust toolchain 和四个基线命令的串行执行定义
- 覆盖本地执行同组命令时是否与 CI 预期一致

## 前置条件

- 仓库包含本次 change 对应的 workflow 文件
- 本地或 runner 环境可执行 Rust stable toolchain、`cargo fmt`、`cargo clippy`、`cargo test`
- `cargo run -p prismtrace-host -- --discover` 可以访问系统 `ps` 命令

## 操作约束

- 本次验证只关注第一版 macOS CI，不验证发布、打包或多平台矩阵
- 本次验证只检查命令是否按预期被执行和阻断，不对 discovery 输出内容做逐行断言

## 核心场景

### 1. macOS workflow 覆盖完整基线

- 场景类型：成功
- 输入：查看仓库中的 macOS GitHub Actions workflow 定义
- 关注点：
  - 存在 `macos-latest` runner
  - workflow 中按顺序执行 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`
- 预期：
  - 不应缺失任一基线命令
  - 不应把 smoke test 替换成仅测试编译通过的空步骤

### 2. 任一基线命令失败时 CI 被阻断

- 场景类型：失败
- 输入：让某一项基线命令返回非零退出码，例如制造格式违规
- 关注点：
  - workflow 在失败命令处终止
  - 该次 CI 结果标记为失败
- 预期：
  - 不应在失败后继续报告整条 workflow 通过

### 3. 本地与 CI 的校验路径保持一致

- 场景类型：回归
- 输入：在本地按 workflow 顺序执行同组 cargo 命令
- 关注点：
  - 本地执行结果与 CI 预期一致
  - `--discover` smoke test 仍然可以完成 host bootstrap 和 discovery 报告输出
- 预期：
  - 不应出现“CI 能跑、本地不能跑”或反之的分叉验证路径

## 通过标准

- 仓库存在首版 macOS workflow，且步骤顺序与设计一致
- 四个基线命令都被纳入自动校验
- 本地执行同组命令可以全部通过
- 当任一命令失败时，workflow 理论上会被阻断

## 回归重点

- 现有 Rust workspace 的格式是否全部整理到 `cargo fmt --check` 通过
- `prismtrace-host -- --discover` 是否仍然能作为公开的 smoke test 入口

## 自动化验证对应

- `.github/workflows/macos-ci.yml`
  - 覆盖 macOS runner 与四个基线步骤的编排
- `cargo fmt --check`
  - 覆盖 Rust 源码格式回归
- `cargo clippy --workspace --all-targets -- -D warnings`
  - 覆盖静态检查回归
- `cargo test --workspace`
  - 覆盖工作区单元测试回归
- `cargo run -p prismtrace-host -- --discover`
  - 覆盖 host 启动与 discovery smoke test

## 测试环境待补充项

- GitHub 实际 runner 上的首次执行结果需要在 workflow 合并后补充确认
- 后续如果引入多平台矩阵，需要补充不同 runner 间的兼容性验证
