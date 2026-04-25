# Opencode Server Observer 实施计划

日期：2026-04-26  
状态：草案

## 1. 目标

把 `opencode` 的官方 `server + attach(url) + export + event` 路线推进到可实施状态，并在第一版通过最小 CLI/host slice 验证：

- `PrismTrace` 能把 `opencode` 当成一个新的官方观测后端接入
- 能稳定拿到高层运行时事件和结构化 session 数据
- 不需要再依赖危险的 Bun attach

## 2. 分阶段推进

### 阶段 A：设计收敛

- [ ] A.1 固定 `opencode` 走官方 observer 路线，不再走 attach
  - 验证：story 与 change 文档明确 `opencode` 不复用 `AttachController`

- [ ] A.2 固定第一版数据面
  - 验证：明确第一版优先读 `health / session list / export / global event`

- [ ] A.3 固定第一版产品入口
  - 验证：明确先做 CLI/host 验证入口，不直接扩散到控制台

### 阶段 B：最小 host 实现

- [ ] B.1 新增 `opencode` observer 模块
  - 验证：存在独立 `opencode_observer.rs`

- [ ] B.2 新增 CLI 入口
  - 验证：可通过 `--opencode-observe` 或 `--opencode-url <url>` 启动最小观察流程

- [ ] B.3 完成最小读取链路
  - 验证：CLI 能输出 health、session list、export 摘要和最小事件摘要

- [ ] B.4 事件落盘
  - 验证：高层事件可以按结构化 artifact 保存，供后续 UI/分析复用

### 阶段 C：聚焦验证

- [ ] C.1 协议层测试
  - 验证：受控输入可覆盖 health、空 session、未知事件和错误回包

- [ ] C.2 集成层验证
  - 验证：live `opencode` 环境下能稳定读取至少一组 session/export/event 数据

- [ ] C.3 基线验证
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`

## 3. 建议的最小实现顺序

1. 先建 `OpencodeServerSource` 独立模块
2. 先打通 CLI 与 health / session list
3. 再做 export 和 event 归一化
4. 最后再决定是否需要把结果接到控制台

## 4. 建议文件边界

### 文档

- `docs/stories/add-opencode-server-observer/design.md`
- `docs/stories/add-opencode-server-observer/plan.md`
- `openspec/changes/add-opencode-server-observer/*`

### 最小实现

- `crates/prismtrace-host/src/opencode_observer.rs`
- `crates/prismtrace-host/src/observer.rs`
- `crates/prismtrace-host/src/main.rs`
- `crates/prismtrace-host/src/lib.rs`

## 5. 当前建议

先让 `opencode` 挂上统一 observer 接口层，再逐步扩事件深度。第一刀最重要的是让 `PrismTrace` 真正拥有一条稳定、安全、明确的 `opencode` 官方接入路径。
