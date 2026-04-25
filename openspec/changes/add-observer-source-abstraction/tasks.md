## 1. Design

- [ ] 1.1 固定统一 source backend 抽象与高层事件面
- [ ] 1.2 明确 attach/probe、Codex、opencode 三类 source 在新架构中的位置

## 2. Host interface

- [ ] 2.1 在 `prismtrace-host` 中收敛 `ObserverSource` / `ObserverEvent` / `ObserverSourceKind`
- [ ] 2.2 让现有 `Codex observer` 对齐新的统一接口层

## 3. Follow-up sources

- [ ] 3.1 为 `OpencodeServerSource` 开后续实现 change
- [ ] 3.2 明确控制台与分析层后续如何消费统一事件
