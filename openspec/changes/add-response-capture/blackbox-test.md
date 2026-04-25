# 黑盒测试说明

## 测试目标

- 验证 PrismTrace host 已经从“只看 request”推进到“可以稳定看见 response”
- 验证已 attach 的 Node CLI 目标在一次真实 exchange 中，能够形成 request-response 最小闭环
- 验证 response capture 不只是 stdout 调试输出，而是会生成可复用的 response artifact
- 验证错误响应、空正文和脱敏边界不会破坏主链路

## 测试范围

- `prismtrace-host -- --attach <pid>` 的前台持续采集路径
- probe 发出 `http_response_observed` 的主路径行为
- host 对 response 的摘要输出、artifact 落盘与 `exchange_id` 关联
- response capture 对既有 request capture / attach 主链路的非回退要求

## 前置条件

- 在 macOS 本机运行 PrismTrace workspace
- 已具备 `attach-controller`、`probe-bootstrap`、`request-capture` 与 `local-console` 第一版能力
- 可以执行 `cargo test --workspace`
- 本机可用 Node.js 运行受控测试目标

## 操作约束

- 本次验证只关注 terminal response artifact，不验证 stream chunk 回放
- 本次不要求完成 response inspector UI，只要求 CLI 摘要与 artifact 可见
- 若 response 正文无法安全文本化，允许以 `body_text = null` 的形式完成降级，但不得破坏主链路

## 核心场景

### 1. 已 attach 的真实 Node CLI 目标产生 response 摘要与 artifact

- 场景类型：成功
- 输入：执行 `cargo test --workspace`
- 关注点：
  - attach 成功后出现 `[attached]`
  - request capture 出现 `[captured]`
  - response capture 出现 `[response]`
  - `.prismtrace/state/artifacts/responses/` 下生成 response artifact
- 预期：
  - 不应只有 request artifact、没有 response artifact
  - 不应要求手工拼接内部状态才能证明 response 被捕获

### 2. request 与 response 共享同一 exchange 标识

- 场景类型：成功
- 输入：运行 probe / host 相关自动化测试
- 关注点：
  - 同一 exchange 的 request 与 response 使用相同 `exchange_id`
  - host 在 response 路径优先复用 request 识别出的 provider hint
- 预期：
  - 不应在同一次 exchange 上生成不一致的 provider 归类
  - 不应在 response 到达后丢失 request-response 的最小配对能力

### 3. 错误响应和空正文仍被安全持久化

- 场景类型：降级成功
- 输入：运行 response capture 聚焦测试
- 关注点：
  - 4xx / 5xx 等错误状态码仍会被写入 artifact
  - 空正文或不可文本化正文可用 `body_text = null` 安全降级
  - body 大小与时延字段仍保持可用
- 预期：
  - 不应因为错误响应或空正文而中断前台采集循环
  - 不应因为正文缺失而丢失状态码与时延事实

### 4. 敏感头字段被脱敏后再持久化

- 场景类型：安全
- 输入：运行 response capture 聚焦测试
- 关注点：
  - `cookie` / `set-cookie` 等敏感头不会以原文写入 artifact
  - artifact 中保留可解释的脱敏结果
- 预期：
  - 不应把敏感头原值直接落盘

## 通过标准

- 真实 Node CLI attach 黑盒路径中可以稳定看到 `[response]` 摘要
- `.prismtrace/state/artifacts/responses/` 下可以读到对应 artifact
- request 与 response 使用同一 `exchange_id` 形成最小闭环
- 错误响应、空正文和敏感头路径都能安全降级而不破坏主链路

## 回归重点

- `request-capture` 既有摘要与 artifact 行为不回退
- attach / detach 与 probe heartbeat 主链路不回退
- response capture 的引入不会把普通非模型 HTTP response 误记为模型 response

## 自动化验证对应

- `crates/prismtrace-host/tests/node_cli_attach.rs`
  - 覆盖真实 Node CLI attach、request/response 摘要输出和 request/response artifact 落盘
- `crates/prismtrace-host/src/request_capture.rs`
  - 覆盖 response 路径对 request provider hint 的复用
- `crates/prismtrace-host/src/response_capture.rs`
  - 覆盖错误响应、空正文、截断元数据、敏感头脱敏和 provider hint 行为
- `crates/prismtrace-host/probe/bootstrap.test.js`
  - 覆盖 probe 主路径会发出匹配的 request / response 事件且共享 `exchange_id`

## 当前自动化验收口径

- `cargo test --workspace`
- `node --test crates/prismtrace-host/probe/bootstrap.test.js`
