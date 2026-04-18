# PrismTrace

PrismTrace（棱镜观测）是一个 macOS 上的 AI 应用可观测性工具，目标是在不重启目标应用的前提下，观测运行中的 Node / Electron AI 应用真实发给模型的内容。当前仍处于 V1 bootstrap 阶段，真实动态注入后端尚未完成。

## 输出约定

- 默认用中文写方案、计划、评审结论、实现说明和进度同步。
- 代码、命令、标识符、提交信息可以保留英文。

## 先看这几个地方

- `docs/总体设计与V1方案.md`
- `docs/产品迭代路线图.md`
- `openspec/`

## 工作区边界

- `prismtrace-core`：共享领域模型和 IPC 协议，不做 I/O。
- `prismtrace-storage`：只管 `.prismtrace/state/` 目录布局与初始化。
- `prismtrace-host`：当前主要运行时入口，承载 discovery、readiness、attach、IPC、probe health。

## 关键约定

- 新能力优先沿着 `probe -> IPC -> host` 这条边界扩展，不要过早把策略逻辑塞进 probe。
- CLI 保持 `Snapshot + Report` 模式：先产出结构化 snapshot，再做展示。
- 测试优先用 trait 边界和静态 / 脚本化替身，不直接依赖真实进程。
- 做中大型改动、跨模块改动、接口或架构调整时，先做设计收敛，再进入实现。

## 本地验证

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
