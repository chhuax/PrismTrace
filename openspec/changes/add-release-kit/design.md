## Context

PrismTrace 当前已经能作为 Rust binary 在本机运行，但仓库还没有用户可安装的 release artifact。README 仍展示 `cargo run -p prismtrace-host -- --console` 这类开发命令；binary 名称也暴露为 `prismtrace-host`，这会让用户感知到内部架构，而不是一个产品命令。

这次 change 只做 Alpha Release Kit：让维护者可以从 GitHub Actions 生成 macOS 二进制包，让用户下载后运行安装脚本得到 `prismtrace` 命令。更重的分发形态，例如 Homebrew tap、`.pkg`、`.dmg`、codesign、notarization 和自动更新，后续再独立推进。

## Goals / Non-Goals

**Goals:**
- 为 macOS 生成可下载 release archive。
- archive 内包含 `prismtrace` binary、`install.sh`、`SHA256SUMS`、README 摘要或 LICENSE。
- 用户安装后可以运行 `prismtrace --discover`、`prismtrace --console`、`prismtrace --codex-observe`、`prismtrace --claude-observe`、`prismtrace --opencode-observe`。
- release workflow 不污染普通 PR CI；只通过 tag 或 manual dispatch 触发。
- 本地或 CI 中能验证 release archive 的文件结构。

**Non-Goals:**
- Homebrew tap。
- macOS codesign / notarization。
- `.pkg` / `.dmg` 图形安装包。
- launch agent / background daemon。
- 自动更新。
- Windows / Linux release。
- 改变 observer 底层采集能力。

## Decisions

### Decision: 首版使用 shell script + GitHub Actions，不引入 cargo-dist

首版 release kit 使用仓库内脚本和 GitHub Actions 完成构建、打包、checksum 和 artifact upload。

Why:
- 当前只需要一个 macOS alpha artifact，手写脚本足够透明。
- 避免在 bootstrap 阶段引入 cargo-dist 的配置复杂度。
- 后续需要多平台、installer、Homebrew 时，可以再切换或叠加更专业的 release 工具。

Alternative considered:
- 直接引入 cargo-dist。拒绝原因是当前收益主要在多平台与 installer 生态，而 PrismTrace 现在只需要 macOS alpha 包。

### Decision: 用户命令名为 `prismtrace`

Cargo package 可以继续叫 `prismtrace-host`，但 release archive 里的可执行文件必须叫 `prismtrace`。实现方式优先选择新增一个 `[[bin]] name = "prismtrace"` 指向同一个 `src/main.rs`，这样开发者也能本地运行 `cargo run -p prismtrace-host --bin prismtrace -- --discover`。

Why:
- `prismtrace-host` 是内部架构名，不适合用户安装后直接暴露。
- `prismtrace` 更符合产品命令和 README 叙述。
- 保留旧 `prismtrace-host` bin 能避免破坏现有 CI 和开发者命令。

Alternative considered:
- 打包时只把 `prismtrace-host` 重命名成 `prismtrace`。拒绝原因是本地开发和 release artifact 命名会分叉，测试覆盖不如显式 bin alias 清楚。

### Decision: Release archive 采用 tar.gz + install.sh + SHA256SUMS

首版 artifact 命名为 `prismtrace-<version>-aarch64-apple-darwin.tar.gz`。包内目录包含：

```text
prismtrace-<version>-aarch64-apple-darwin/
  bin/prismtrace
  install.sh
  SHA256SUMS
  README.md
  LICENSE
```

`install.sh` 默认安装到 `/usr/local/bin`；如果没有权限，用户可传入 `--prefix "$HOME/.local"` 或设置 `PREFIX`。

Why:
- tarball 是 GitHub Release 上最简单稳定的交付形式。
- `install.sh` 给用户明确入口，但仍允许手动复制 binary。
- checksum 让用户和后续自动化都能验证下载完整性。

Alternative considered:
- 只上传裸 binary。拒绝原因是没有安装体验、没有校验和、没有随包说明。

### Decision: Release workflow 仅 tag/manual 触发

新增 `.github/workflows/release.yml`，触发条件为：

- `push` tag：`v*`
- `workflow_dispatch`

workflow 在 macOS runner 上执行 baseline checks，然后安装 `aarch64-apple-darwin` Rust target，构建 `--release --bin prismtrace --target aarch64-apple-darwin`，调用打包脚本生成 artifact，并上传 GitHub Actions artifact。tag 触发时可以创建 GitHub Release 并附加 archive；manual dispatch 至少产出 Actions artifact 供验证。

Why:
- 普通 PR 不应该被 release 打包成本拖慢。
- tag 触发符合用户可下载版本的语义。
- manual dispatch 方便在正式发 tag 前试跑。

Alternative considered:
- 每个 PR 都构建 release archive。拒绝原因是浪费 CI 时间，且 PR artifact 不等价于发布版本。

## Risks / Trade-offs

- [未签名 binary 可能触发 macOS Gatekeeper 提示] → README 明确 alpha 未签名；正式分发再做 codesign / notarization。
- [只支持 Apple Silicon] → 首版只产出 `aarch64-apple-darwin`，README 明确范围；Intel / universal binary 后续独立做。
- [安装到 `/usr/local/bin` 可能需要权限] → `install.sh` 支持 `--prefix` 和 `PREFIX`。
- [release workflow 与普通 CI 产生重复检查] → release workflow 只在 tag/manual 触发，重复是可接受的发布前保护。

## Migration Plan

本次变更不涉及运行时数据迁移。

实施步骤：
- 新增 release kit OpenSpec artifacts。
- 新增 `prismtrace` bin alias。
- 新增 `scripts/package-release.sh` 和 `scripts/install-prismtrace.sh`。
- 新增 release workflow。
- 更新 README / README.zh-CN 的安装说明。
- 添加 focused tests 或 shell smoke checks 验证 archive 结构。
- 执行本地 CI baseline 和 release packaging smoke test。

回滚策略：
- 删除 release workflow、打包脚本和 README 安装段落。
- 移除 `prismtrace` bin alias；保留现有 `prismtrace-host` 开发入口。

## Open Questions

- 下一个 release 阶段是否优先做 Homebrew tap，还是 codesign / notarization。
- 是否需要在 `prismtrace doctor` 落地后再把安装后验证命令从 `--discover` 换成 `doctor`。
