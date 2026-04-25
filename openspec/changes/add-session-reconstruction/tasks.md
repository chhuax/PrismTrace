## 1. 收敛 session reconstruction V1 边界

- [ ] 1.1 固定第一版 session 定义
  - 验证：文档明确 session 只在同一 `pid` 内重建，并采用固定时间窗口切分

- [ ] 1.2 固定 exchange 作为 timeline 基本单元
  - 验证：文档明确 timeline item 是 request / response / tool visibility 的聚合摘要，而不是原始事件流

- [ ] 1.3 固定本轮非目标
  - 验证：文档明确不扩展到跨 pid 合并、attach 生命周期持久化、provider-specific thread 推断、stream replay 和分析层

## 2. 实现 host 侧 session / exchange 读模型

- [ ] 2.1 增加 exchange 聚合逻辑
  - 验证：request 主记录可稳定关联 matching response 与 tool visibility

- [ ] 2.2 增加 `pid + 时间窗口` 的 session 切分逻辑
  - 验证：同一 pid 且时间连续的 exchange 归入同一 session，超过阈值切新 session

- [ ] 2.3 增加 session summary 与 session detail 结构
  - 验证：host 可输出 session 列表和单个 session timeline

## 3. 扩展控制台 API 与页面

- [ ] 3.1 新增 `/api/sessions`
  - 验证：API 返回最近 session 摘要列表与过滤上下文

- [ ] 3.2 新增 `/api/sessions/:id`
  - 验证：API 返回单个 session timeline；缺失或过滤不匹配时返回 `not_found`

- [ ] 3.3 升级控制台首页
  - 验证：页面展示 `Sessions` 区域与 `Session Timeline` 区域，并支持从 timeline item 跳转到已有 request inspector

## 4. 验证与收尾

- [ ] 4.1 增加聚焦测试
  - 验证：覆盖 exchange 聚合、session 切分、session API 与过滤语义

- [ ] 4.2 运行本地 CI 基线
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`
