## Why

PrismTrace 现在已经有可运行的本地 CLI、observer-first console、read model 和 macOS CI，但还没有面向用户的交付方式。当前 README 仍要求用户通过 `cargo run -p prismtrace-host ...` 使用工具，这只适合开发者验证，不适合作为本地 AI observability 产品的安装体验。

第一版 release kit 的目标是把 PrismTrace 从“源码工程”推进到“可下载、可安装、可验证”的 alpha 工具：GitHub Release 能产出 macOS 二进制包，用户能通过安装脚本得到 `prismtrace` 命令，并能用公开命令启动 discovery、console 和 observer。

## What Changes

- 新增面向 GitHub Release 的 macOS 打包 workflow
- 产出压缩包、校验和与安装脚本，而不是要求用户自己编译
- 将用户入口命令收口为 `prismtrace`，保留内部 crate/package 名称 `prismtrace-host`
- 更新 README / README.zh-CN，加入 alpha 安装与升级说明
- 增加最小本地验证，确保 release archive 包含预期文件且安装脚本可解析

## Capabilities

### New Capabilities

- `release-kit`：PrismTrace repository 能为 macOS 用户提供可下载的 alpha release artifact，包含二进制、安装脚本、校验和和基础使用说明。

### Modified Capabilities

- `macos-ci-workflow`：保留现有 CI baseline，不在普通 PR CI 中强制执行 release 构建；release workflow 只在 tag / manual dispatch 时运行。

## Impact

- 影响代码：新增 release workflow、打包脚本、安装脚本和 README 安装说明；可能新增一个 `prismtrace` bin alias 指向现有 host CLI。
- 影响系统：GitHub Release / workflow_dispatch 可以生成 macOS alpha artifact。
- 依赖影响：使用 GitHub Actions、Rust stable toolchain 和 shell 脚本；首版不引入 cargo-dist、Homebrew tap、codesign 或 notarization。

## Docs Impact

- 新增 `release-kit` capability spec、design 和 tasks。
- README / README.zh-CN 从“开发者运行命令”补充为“用户安装命令 + 开发者运行命令”双路径。
