## 1. OpenSpec artifacts 与 CI 基线收敛

- [x] 1.1 完成 `bootstrap-macos-ci` change 的 proposal、design、spec 和 blackbox test，明确首版 macOS CI 的范围与阻断规则
- [x] 1.2 在本地确认目标基线命令当前状态，并记录需要先修复的红灯项

## 2. GitHub Actions workflow

- [x] 2.1 新增一个运行在 `macos-latest` 上的 GitHub Actions workflow 文件
- [x] 2.2 在 workflow 中安装 Rust toolchain，并串行执行 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`

## 3. 仓库修正与验证

- [x] 3.1 整理现有 Rust 文件格式，使 `cargo fmt --check` 通过
- [x] 3.2 重新执行与 workflow 对齐的本地验证命令，确认 macOS CI 基线已全部通过
