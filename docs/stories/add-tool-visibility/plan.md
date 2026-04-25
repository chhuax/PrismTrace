# add-tool-visibility 实现计划

日期：2026-04-25
状态：已完成

## 目标

把 `add-tool-visibility` 收口成一个可验收的最小产品切片：

- request payload 中的 tools / functions 可被提取为独立 visibility artifact
- request inspector 可展示该次 request 的 tool visibility

## 任务拆解

### 1. 文档与规格

- [x] 补齐 story 设计稿
- [x] 补齐 OpenSpec proposal / design / tasks / spec
- [x] 更新路线图中的当前阶段与下一步建议

### 2. host 侧采集

- [x] 新增 request-embedded tool visibility 提取逻辑
- [x] 将 visibility artifact 写入 `.prismtrace/state/artifacts/tool_visibility/`
- [x] 在前台 attach 消费循环中输出基础 summary

### 3. console 侧展示

- [x] 扩展 request detail 模型，支持 tool visibility detail
- [x] 扩展 `/api/requests/:id` payload
- [x] 在 request inspector 增加 `Tool Visibility` 区块

### 4. 测试与验证

- [x] 增加 tool visibility 提取 / 落盘测试
- [x] 增加 request inspector 读取 / API 展示测试
- [x] 通过 `cargo fmt --check`
- [x] 通过 `cargo clippy --workspace --all-targets -- -D warnings`
- [x] 通过 `cargo test --workspace`
- [x] 通过 `cargo run -p prismtrace-host -- --discover`
