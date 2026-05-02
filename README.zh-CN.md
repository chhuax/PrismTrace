# PrismTrace / 棱镜观测

英文名：`PrismTrace`

[English README](./README.md)

`棱镜观测` 是一个 AI 应用可观测性工具，目标是在不重启应用、不打断当前会话的前提下，拿到 AI 应用真实发给大模型的内容，并逐步扩展到会话重建、分析和解释能力。

第一版重点关注 macOS 上基于 Node / Electron 的 AI CLI 和桌面应用。第一阶段先解决最核心的问题：

- 附着到正在运行的目标进程
- 采集附着后的真实 LLM 请求与响应
- 在本地可观测性控制台里查看 payload、tools 和元数据

## 当前状态

仓库目前处于设计和初始化阶段。

当前设计文档在 [docs/总体设计与V1方案.md](./docs/总体设计与V1方案.md)。
当前产品路线图在 [docs/产品迭代路线图.md](./docs/产品迭代路线图.md)。

## V1 范围

PrismTrace V1 当前边界是：

- 仅支持 macOS
- 优先支持已经在运行中的 Node / Electron AI 应用
- 不要求重启被观测应用
- 先解决 payload 可见性
- 采用本地优先的存储与隐私策略

## Alpha 安装

PrismTrace alpha 版本支持 Apple Silicon 和 Intel macOS。推荐使用 Homebrew 安装：

```bash
brew install chhuax/tap/prismtrace
```

GitHub Releases 也会提供未签名的 macOS tarball，分别面向 Apple Silicon（`aarch64-apple-darwin`）和 Intel（`x86_64-apple-darwin`）。`.pkg`、`.dmg`、codesign 和 notarization 暂未提供。

如果使用 tarball，请先下载与你的 Mac 架构匹配的最新 archive，然后运行：

```bash
case "$(uname -m)" in
  arm64) target="aarch64-apple-darwin" ;;
  x86_64) target="x86_64-apple-darwin" ;;
  *) echo "unsupported macOS architecture: $(uname -m)" >&2; exit 1 ;;
esac

tar -xzf prismtrace-*-"$target".tar.gz
cd prismtrace-*-"$target"
./install.sh --prefix "$HOME/.local"
```

确认安装目录已经在 `PATH` 中：

```bash
export PATH="$HOME/.local/bin:$PATH"
```

安装后可以先运行 discovery smoke test：

```bash
prismtrace --discover
```

启动本地控制台：

```bash
prismtrace --console
```

observer 入口也通过同一个安装命令暴露：

```bash
prismtrace --codex-observe
prismtrace --claude-observe
prismtrace --opencode-observe
```

## 长期方向

PrismTrace 不只是一个 payload 抓取工具，它的长期方向是完整的 AI 应用可观测性：

- 信息采集
- 会话重建
- 分析与解释

第一步先把可信的本地事实层搭起来，后续再在此基础上叠加分析能力。

## 当前仓库结构

- `crates/prismtrace-core`：共享的运行时与进程领域类型
- `crates/prismtrace-storage`：本地状态目录布局和存储初始化
- `crates/prismtrace-host`：可运行的 host 二进制和启动引导
- `docs/`：设计文档和实现计划

## 本地开发

```bash
cargo test
cargo run -p prismtrace-host
cargo run -p prismtrace-host -- --discover
cargo run -p prismtrace-host --bin prismtrace -- --discover
cargo run -p prismtrace-host -- --readiness
cargo run -p prismtrace-host -- --attach <pid>
```

`--attach <pid>` 当前会进入前台 attach 会话，并对受支持的运行中 Node CLI 目标捕获 request 与 response artifacts。

## 本地控制台

使用下面的命令启动本地可观测性控制台：

```bash
cargo run -p prismtrace-host -- --console
cargo run -p prismtrace-host --bin prismtrace -- --console
```

如果只想看特定目标，可以重复传入 `--target`：

```bash
cargo run -p prismtrace-host -- --console --target opencode
cargo run -p prismtrace-host -- --console --target opencode --target codex
```

当传入 `--target` 后，首页和 `/api/*` payload 会保持一致的过滤视图；如果当前没有命中任何目标，控制台仍会正常打开，并显示带过滤上下文的空态说明，而不会回退到全局进程列表。

当前默认入口地址是 `http://127.0.0.1:7799`。

当前 bootstrap 阶段的控制台提供：

- target 摘要列表
- 最近活动时间线
- request 摘要列表
- 基础 request 详情与 observability health 面板
