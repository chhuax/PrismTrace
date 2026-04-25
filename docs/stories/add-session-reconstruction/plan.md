# add-session-reconstruction 实现计划

日期：2026-04-25
状态：已完成

## 目标

把 `add-session-reconstruction` 收口成一个可验收的最小产品切片：

- 基于现有 request / response / tool visibility artifacts 聚合 exchange
- 在同一 `pid` 内按固定时间窗口重建 session
- 在本地控制台增加 session 列表与 session timeline
- 继续复用已有 request inspector 作为单条 exchange 的深度入口

## 任务拆解

### 1. 文档与规格

- [x] 补齐 story 设计稿
- [x] 补齐 OpenSpec proposal / design / tasks / spec

### 2. host 侧 session / exchange 读模型

- [x] 新增 exchange 聚合逻辑
- [x] 新增 `pid + 时间窗口` 的 session 切分逻辑
- [x] 新增 session summary 与 session detail 结构

### 3. console 侧 API 与页面

- [x] 新增 `/api/sessions`
- [x] 新增 `/api/sessions/:id`
- [x] 在首页增加 `Sessions` 区域与 `Session Timeline` 区域
- [x] 让 timeline item 继续跳到已有 request inspector

### 4. 测试与验证

- [x] 增加 session reconstruction 聚焦测试
- [x] 通过 `cargo fmt --check`
- [x] 通过 `cargo clippy --workspace --all-targets -- -D warnings`
- [x] 通过 `cargo test --workspace`
- [x] 通过 `cargo run -p prismtrace-host -- --discover`
