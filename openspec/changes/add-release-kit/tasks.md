## 1. OpenSpec 与范围收敛

- [x] 1.1 建立 `add-release-kit` proposal、design、tasks 和 capability spec
- [x] 1.2 运行 `npx openspec validate add-release-kit --strict`

## 2. CLI 用户入口

- [x] 2.1 新增 `prismtrace` bin alias，指向现有 `crates/prismtrace-host/src/main.rs`
- [x] 2.2 保留 `prismtrace-host` bin，确保现有 CI 和开发命令不破坏
- [x] 2.3 增加测试或 smoke command，确认 `cargo run -p prismtrace-host --bin prismtrace -- --discover` 可运行

## 3. Release 打包脚本

- [x] 3.1 新增 `scripts/package-release.sh`，构建 release binary 并生成 tarball 目录结构
- [x] 3.2 新增 `scripts/install-prismtrace.sh`，支持默认 `/usr/local` 与 `--prefix` / `PREFIX`
- [x] 3.3 生成 `SHA256SUMS`，覆盖 archive 内 binary 和安装脚本
- [x] 3.4 增加本地 package smoke test，验证 tarball 中包含 `bin/prismtrace`、`install.sh`、`SHA256SUMS`、`README.md`、`LICENSE`

## 4. GitHub Release workflow

- [x] 4.1 新增 `.github/workflows/release.yml`
- [x] 4.2 workflow 支持 `workflow_dispatch` 与 `v*` tag 触发
- [x] 4.3 workflow 执行 baseline checks、release build、package script、artifact upload
- [x] 4.4 tag 触发时创建 GitHub Release 并上传 tarball / checksum

## 5. 用户文档

- [x] 5.1 更新 `README.md`，加入 alpha release 安装、升级和 smoke test 命令
- [x] 5.2 更新 `README.zh-CN.md`，加入中文安装说明
- [x] 5.3 明确 alpha 限制：macOS Apple Silicon、未签名、Homebrew/pkg/dmg 暂未提供

## 6. 验证与收尾

- [x] 6.1 运行 `cargo fmt --check`
- [x] 6.2 运行 `cargo clippy --workspace --all-targets -- -D warnings`
- [x] 6.3 运行 `cargo test --workspace`
- [x] 6.4 运行 `cargo run -p prismtrace-host -- --discover`
- [x] 6.5 运行 `cargo run -p prismtrace-host --bin prismtrace -- --discover`
- [x] 6.6 运行 release package smoke test
- [x] 6.7 运行 `npx openspec validate add-release-kit --strict`
