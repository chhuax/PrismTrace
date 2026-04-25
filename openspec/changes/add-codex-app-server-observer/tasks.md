## 1. 收敛 Codex observer V1 边界

- [ ] 1.1 固定 `Codex` 不复用 attach controller
  - 验证：文档明确 `Codex` 走独立 source

- [ ] 1.2 固定第一版事件面
  - 验证：文档明确 `thread / turn / item / tool / approval / hook / capability_snapshot`

- [ ] 1.3 固定第一版产品入口
  - 验证：文档明确先做 CLI/host，不直接扩控制台

## 2. 实现最小 Codex observer host 入口

- [ ] 2.1 新增 `Codex` observer 模块
  - 验证：存在独立模块负责 socket 发现、握手和事件读取

- [ ] 2.2 新增 CLI 参数入口
  - 验证：可通过 `--codex-observe` 或 `--codex-socket <path>` 启动

- [ ] 2.3 完成最小初始化与事件读取
  - 验证：CLI 能输出 initialize 成功和后续事件摘要

## 3. 归一化与持久化

- [ ] 3.1 增加最小事件归一化
  - 验证：至少能生成 thread / turn / item / tool / approval / hook / capability snapshot 的统一读模型

- [ ] 3.2 保留 raw JSON
  - 验证：未知或暂未完全投影的事件不会被静默丢弃

- [ ] 3.3 落盘结构化 artifact
  - 验证：后续控制台或分析层可复用这些事件

## 4. 验证与收尾

- [ ] 4.1 增加聚焦测试
  - 验证：覆盖初始化、事件解析和未知事件保留

- [ ] 4.2 进行 live 验证
  - 验证：在运行中的 `Codex` 上至少读取到一组高层事件

- [ ] 4.3 运行本地 CI 基线
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`
