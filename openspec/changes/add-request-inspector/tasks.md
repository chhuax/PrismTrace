## 1. 收敛 request inspector 边界

- [x] 1.1 固定 request inspector 的最小字段集合
  - 验证：文档与实现明确 request overview、request payload detail 和 response detail 的字段边界

- [x] 1.2 固定单次 exchange 边界
  - 验证：文档中明确本轮不扩展到 session reconstruction / timeline replay

- [x] 1.3 固定 detail API 的过滤语义
  - 验证：带 target filter 时，detail 不暴露未匹配 request

## 2. 实现 host 侧 inspector 读模型

- [x] 2.1 扩展 request artifact 读取
  - 验证：detail 可返回 request headers、body_text、truncated、exchange_id 等事实

- [x] 2.2 增加 response artifact 读取与匹配
  - 验证：detail 可按 `exchange_id` 返回 matching response detail

- [x] 2.3 收口 detail API 的过滤约束
  - 验证：过滤视图下未匹配 request 返回 `not_found`

## 3. 升级控制台详情区

- [x] 3.1 升级 request detail panel 和前端渲染
  - 验证：页面可直接查看 request payload 和 response detail

- [x] 3.2 补齐空态与截断提示
  - 验证：无 response、空正文、已截断三类路径均有明确显示

## 4. 验证与收尾

- [x] 4.1 增加聚焦测试
  - 验证：覆盖 detail 读模型、API payload 和过滤行为

- [x] 4.2 运行本地 CI 基线
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`
