# Observer Source Abstraction 实施计划

日期：2026-04-26  
更新：2026-04-26  
状态：草案

## 1. 目标

把 `PrismTrace` 从“围绕 attach 想办法”推进到“统一上层观测协议 + 多 source backend”的架构。

## 2. 已知前提

- `prismtrace-host` 当前产品面已经清理 legacy attach 控制链
- `Codex` 已进入 `CodexAppServerSource` 的实现落地阶段
- `opencode` 已完成官方路线验证，适合后续接成 `OpencodeServerSource`
- `Claude Code` 已明确不应再走 live attach，后续应以 transcript / event / export 路线接入

## 3. 分阶段推进

### 阶段 A：接口层收敛

- [ ] A.1 定义统一的 source backend 抽象
  - 验证：明确 `ObserverSource`、`ObserverEvent`、`ObserverSourceKind`

- [ ] A.2 定义统一高层事件面
  - 验证：至少覆盖 `session / turn / item / tool / approval / hook / capability / error`

- [ ] A.3 明确 legacy attach 的当前定位
  - 验证：attach 被视为历史归档路线，不再代表当前 host 主入口或产品方向

### 阶段 B：让 Codex 挂到统一接口层

- [ ] B.1 将现有 `Codex observer` 实现对齐到统一 source 接口
  - 验证：`Codex` 不再使用临时私有接口，而能产出统一 observer event

- [ ] B.2 明确 `Codex` 第一版可交付事件
  - 验证：至少包括 handshake、capability snapshot、最小高层事件

### 阶段 C：为 opencode / Claude Code 预留接入位

- [ ] C.1 新开 `OpencodeServerSource` 设计
  - 验证：明确从 `server + event/export` 入手

- [ ] C.2 新开 `ClaudeCodeTranscriptSource` 设计
  - 验证：明确从 transcript / event / export 入手

## 4. 建议文件边界

### 文档

- `docs/stories/add-observer-source-abstraction/design.md`
- `docs/stories/add-observer-source-abstraction/plan.md`
- `openspec/changes/add-observer-source-abstraction/*`

### 可能的实现入口

- `crates/prismtrace-host/src/observer.rs`
- `crates/prismtrace-host/src/codex_observer.rs`
- `crates/prismtrace-host/src/opencode_observer.rs`
- `crates/prismtrace-host/src/lib.rs`
- `crates/prismtrace-host/src/main.rs`

## 5. 当前建议

先把这层架构明确下来，再继续各自推进 `Codex`、`opencode` 和 `Claude Code`。这样后续不会出现：

- 每接一个新目标都要重新定义 host 接入边界
- 控制台和分析层被单一 source 模型绑死
- 已经确认不可行的 attach 方案继续污染当前产品叙事
