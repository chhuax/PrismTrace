# Tasks: use-global-local-store

## 1. State root and CLI

- [x] 1.1 新增默认用户级 state root 解析
- [x] 1.2 新增 `--state-root <path>` 与 `PRISMTRACE_STATE_ROOT`
- [x] 1.3 调整 startup summary，不再把 cwd 描述成 workspace 边界

## 2. Read model and console API

- [x] 2.1 Codex session list 默认不按 cwd 过滤
- [x] 2.2 Index read store 默认不从 storage root 反推 workspace root
- [x] 2.3 Console health / empty state 暴露当前 state root

## 3. Legacy compatibility

- [x] 3.1 导入当前目录旧 `.prismtrace/state/artifacts`
- [x] 3.2 添加不覆盖已有文件的回归测试

## 4. Verification

- [x] 4.1 `cargo fmt --check`
- [x] 4.2 `cargo clippy --workspace --all-targets -- -D warnings`
- [x] 4.3 `cargo test --workspace`
- [x] 4.4 `cargo run -p prismtrace-host -- --discover`
- [x] 4.5 `npx openspec validate use-global-local-store --strict`
