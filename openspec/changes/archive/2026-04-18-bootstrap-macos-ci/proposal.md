## Why

PrismTrace 现在已经有可运行的 Rust workspace、单元测试和本地 discovery 入口，但仓库还没有任何持续集成保护。现在补上第一版 macOS CI，可以把当前已经成立的本地验证路径搬到 GitHub 上，尽早阻断格式、静态检查、测试或启动回归。

## What Changes

- 为仓库新增第一版 GitHub Actions macOS workflow
- 在同一个 `macos-latest` job 中串行执行 Rust 格式检查、静态检查、测试和 host discovery smoke test
- 将当前仓库中导致 `cargo fmt --check` 失败的格式问题整理到通过状态
- 保持首版 CI 轻量，不引入多平台矩阵、发布流程或额外部署步骤

## Capabilities

### New Capabilities
- `macos-ci-workflow`：在 GitHub 上为 PrismTrace 提供可重复执行的 macOS 持续集成校验，覆盖格式、lint、测试和 host 启动 smoke test

### Modified Capabilities

## Impact

- 影响代码：新增 `.github/workflows/` 下的 workflow 配置，并整理现有 Rust 源文件格式
- 影响系统：GitHub pull request / push 阶段新增 macOS 自动校验关卡
- 依赖影响：使用 GitHub Actions 的标准 checkout 与 Rust toolchain 安装步骤；不引入新的运行时依赖

## Docs Impact

- 在当前 change 中新增 `macos-ci-workflow` capability spec、design、blackbox test 和 tasks
- 当前 README 的本地开发命令无需变化；只有当 CI 入口与开发者工作流说明需要显式暴露时再更新 README
