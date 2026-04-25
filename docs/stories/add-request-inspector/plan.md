# add-request-inspector 实施计划

> 给执行代理的要求：本计划只推进 request inspector，不并行扩展 session reconstruction、tool visibility 或搜索筛选能力。以下步骤使用 `- [ ]` 语法追踪。

**目标：** 让本地控制台的 `/api/requests/:id` 和详情区，从“基础摘要”升级为“可直接检查一次 exchange 的 request / response inspector”。

**架构：** 继续复用 `prismtrace-host/src/console.rs` 里的控制台读模型和静态页面，由 request artifact 与 response artifact 作为唯一事实源，通过 `exchange_id` 做最小关联。

---

## 1. 收敛 request inspector 的边界

- [x] 1.1 明确 request inspector 的事实面与字段边界
  - 验证：文档中明确 request metadata、request payload detail 与 response detail 的最小字段集合

- [x] 1.2 明确只做单次 exchange inspector，不进入 session reconstruction
  - 验证：文档中明确本轮不扩展到 timeline / session grouping / tool visibility

- [x] 1.3 明确 detail API 必须遵守当前过滤范围
  - 验证：文档中明确 detail path 不得绕过 target filter

## 2. 实现 host 侧 inspector 读模型

- [x] 2.1 扩展 request artifact 读取逻辑
  - 验证：detail 可读取 request 的 headers、body_text、truncated、exchange_id 等事实

- [x] 2.2 增加 response artifact 读取与按 `exchange_id` 关联逻辑
  - 验证：匹配 response 时，detail 可返回 status、duration、headers、body_text；无匹配时安全降级

- [x] 2.3 收口 detail API 的过滤约束
  - 验证：带过滤上下文时，未匹配 request 的 detail 返回 `not_found`

## 3. 升级控制台详情区

- [x] 3.1 将 request detail panel 升级为 inspector 视图
  - 验证：页面可见 Request Overview、Request Payload、Response Detail

- [x] 3.2 为 headers / body / truncated / response 缺失补齐空态与展示文案
  - 验证：空正文、无 response、已截断三类路径都有明确显示

## 4. 验证与收尾

- [x] 4.1 补齐 request inspector 的聚焦测试
  - 验证：自动化测试覆盖 request detail 读取、response 关联、detail API 与过滤路径

- [x] 4.2 运行本地 CI 基线并同步必要文档
  - 验证：通过 `cargo fmt --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p prismtrace-host -- --discover`
