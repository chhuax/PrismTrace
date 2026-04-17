## ADDED Requirements

### Requirement: Repository 提供 macOS 持续集成 workflow
PrismTrace repository MUST 提供一个可在 GitHub 上执行的 macOS CI workflow，用于在 pull request 和相关代码更新时自动校验当前 Rust workspace。

#### Scenario: GitHub 上存在可执行的 macOS workflow
- **WHEN** 维护者查看仓库的 CI 配置
- **THEN** 仓库中存在一个针对 macOS runner 的 GitHub Actions workflow 文件

### Requirement: macOS workflow 串行执行当前基线校验命令
The macOS CI workflow MUST 在同一个 job 中按顺序执行当前仓库约定的基线校验命令：`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`。

#### Scenario: workflow 依次执行格式、lint、测试和 smoke test
- **WHEN** macOS CI workflow 被触发
- **THEN** 它在单个 job 中按既定顺序运行格式检查、静态检查、工作区测试和 host discovery smoke test

### Requirement: 任一基线校验失败都必须阻断 workflow
The macOS CI workflow MUST 在任一校验命令失败时将该次运行标记为失败，而不是继续报告通过。

#### Scenario: 格式或启动校验失败时 workflow 失败
- **WHEN** `cargo fmt --check`、`cargo clippy`、`cargo test` 或 `cargo run -p prismtrace-host -- --discover` 中任一命令返回非零退出码
- **THEN** 该次 macOS CI workflow 运行失败

### Requirement: workflow 必须使用仓库当前公开的 host CLI 入口做 smoke test
PrismTrace MUST 使用现有的 `prismtrace-host` CLI `--discover` 入口作为首版 CI 的运行级 smoke test，而不是引入仅供 CI 使用的平行验证入口。

#### Scenario: smoke test 复用当前 host CLI
- **WHEN** macOS workflow 进入运行级校验步骤
- **THEN** 它执行的是 `cargo run -p prismtrace-host -- --discover`
