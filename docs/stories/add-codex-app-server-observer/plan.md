# Codex App Server Observer 实施计划

日期：2026-04-25  
状态：草案

## 1. 目标

把 `Codex App Server + IPC socket` 路线推进到可实施状态，并在第一版通过最小 CLI/host slice 验证：

- `PrismTrace` 能把 `Codex` 当成一个新的官方观测后端接入
- 能稳定拿到高层运行时事件
- 不需要再依赖危险的 attach 路线

## 2. 分阶段推进

### 阶段 A：设计收敛

- [ ] A.1 固定 `Codex` 走官方 observer 路线，不再走 attach
  - 验证：story 与 change 文档明确 `Codex` 不复用 `AttachController`

- [ ] A.2 固定第一版事件面
  - 验证：明确只收 `thread / turn / item / tool / approval / hook / plugin / skill / app`

- [ ] A.3 固定第一版产品入口
  - 验证：明确先做 CLI/host 验证入口，不直接扩散到控制台

### 阶段 B：最小 host 实现

- [ ] B.1 新增 `Codex` observer 模块
  - 验证：存在独立 `codex_observer.rs`，不改 attach 主链语义

- [ ] B.2 新增 CLI 入口
  - 验证：可通过 `--codex-observe` 或 `--codex-socket <path>` 启动最小观察流程

- [ ] B.3 完成最小握手与事件读取
  - 验证：CLI 能输出 initialize 成功和后续高层事件摘要

- [ ] B.4 事件落盘
  - 验证：高层事件可以按结构化 artifact 保存，供后续 UI/分析复用

### 阶段 C：聚焦验证

- [ ] C.1 协议层测试
  - 验证：受控输入可覆盖初始化、未知事件、错误回包

- [ ] C.2 集成层验证
  - 验证：live `Codex` 环境下能稳定读取至少一组高层事件

- [ ] C.3 基线验证
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`

## 3. 建议的最小实现顺序

1. 先建 `Codex` observer 独立模块
2. 先打通 CLI 与最小握手
3. 再做事件归一化和 artifact 落盘
4. 最后再决定是否需要把结果接到控制台

## 4. 建议文件边界

### 文档

- `docs/stories/add-codex-app-server-observer/design.md`
- `docs/stories/add-codex-app-server-observer/plan.md`
- `openspec/changes/add-codex-app-server-observer/*`

### 最小实现

- `crates/prismtrace-host/src/main.rs`
- `crates/prismtrace-host/src/lib.rs`
- `crates/prismtrace-host/src/observer.rs`
- `crates/prismtrace-host/src/codex_observer.rs`

### 可能的后续扩展

- `crates/prismtrace-host/src/codex_protocol.rs`
- `crates/prismtrace-host/src/codex_storage.rs`

## 5. 当前建议

当前建议停在“设计与实现边界已收敛，可进入 apply”的状态，不急于在这一轮继续扩 UI。第一刀最重要的是让 `Codex` 终于拥有一条安全、明确、独立的 host 接入路径。
