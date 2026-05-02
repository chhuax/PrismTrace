## 1. CLI and source plumbing

- [x] 1.1 新增 `--opencode-observe` / `--opencode-url` 入口
- [x] 1.2 让 host session 通过 storage 驱动 `opencode` observer

## 2. Snapshot and artifact pipeline

- [x] 2.1 为握手与事件补 `observer_events/opencode` artifact 落盘
- [x] 2.2 归一化 `session / export / message` 快照为统一 observer 事件

## 3. Realtime events and verification

- [x] 3.1 补 `global/event` 的保守映射与 `unknown` 回退
- [x] 3.2 为协议层与 CLI 层补聚焦测试
- [x] 3.2a 修正 opencode capability 语义边界：`agent` / `mcp` / `provider` 独立投影，不与 Codex `skill` / `plugin` / `app` 混同
- [ ] 3.3 跑本地基线并做 live `opencode` 验证
  - 本地基线已通过：`cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`
  - live `opencode` 验证待执行：当前本机 `127.0.0.1:4096` 没有 server 监听
