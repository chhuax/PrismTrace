## 1. 收敛 prompt diff V1 边界

- [ ] 1.1 固定比较对象为同一 session 内相邻 request
  - 验证：文档明确不支持任意 pair 对比

- [ ] 1.2 固定 projection 只覆盖 prompt-bearing 文本
  - 验证：文档明确不扩展到 tools / 参数 diff

- [ ] 1.3 固定不可比较状态
  - 验证：文档明确 `available`、`no_previous_request`、`unavailable_projection`

## 2. 实现 host 侧 prompt projection / diff

- [ ] 2.1 增加 projection 提取逻辑
  - 验证：常见 request body 结构可提取稳定文本

- [ ] 2.2 增加相邻 request diff 逻辑
  - 验证：当前 request 可关联上一条 request 并生成 diff

- [ ] 2.3 扩展 request detail 读模型
  - 验证：request detail 返回 prompt diff 字段

## 3. 扩展 request inspector

- [ ] 3.1 新增 `Prompt Diff` 展示区
  - 验证：用户打开 request detail 时可直接看到 diff

- [ ] 3.2 收口空态与降级态
  - 验证：无上一条 request 或 projection 不可用时，UI 返回明确说明

## 4. 验证与收尾

- [ ] 4.1 增加聚焦测试
  - 验证：覆盖 projection、diff 和降级路径

- [ ] 4.2 运行本地 CI 基线
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`
