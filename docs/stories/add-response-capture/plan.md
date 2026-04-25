# add-response-capture 实施计划

> 给执行代理的要求：实施本计划时，保持范围收敛在 response capture V1，不并行扩展 request inspector、session reconstruction 或分析层能力。以下步骤使用 `- [ ]` 语法追踪。

**目标：** 把当前已经存在的 `HttpResponseObserved` 协议、probe 发事件、host 落盘与摘要输出，收敛成一个可正式验收的 response capture 切片，使 PrismTrace 能对单次 exchange 稳定回答“它收到了什么 response”。

**架构：** 继续沿用现有四段链路：`probe/bootstrap.js` 负责发出 response 事件，`prismtrace-core` 承载稳定 IPC 协议，`request_capture.rs` 在前台消费循环里做 request-response 的最小关联，`response_capture.rs` 负责 artifact 落盘和摘要渲染。本轮重点是收敛边界、补齐黑盒与错误路径，而不是重做底层架构。

**技术栈：** Rust workspace、Node.js bootstrap probe、前台 attach 持续采集、OpenSpec 文档驱动开发。

---

## 1. 收敛 response capture 的边界与协议承诺

- [x] 1.1 固定 response artifact 字段与最小语义
  - 关联目标：正式定义 response capture V1 的事实面
  - 验证：文档与代码字段对齐，至少覆盖 `exchange_id`、`status_code`、`headers`、`body_text`、`started_at_ms`、`completed_at_ms`
  - 涉及文件：`docs/stories/add-response-capture/design.md`、`openspec/changes/add-response-capture/specs/response-capture/spec.md`

- [x] 1.2 固定 request-response 关联语义
  - 关联目标：单次 exchange 必须能稳定闭环
  - 验证：明确 `exchange_id` 是本轮唯一一等关联键，且 host 复用 request provider hint 的规则可解释
  - 涉及文件：`docs/stories/add-response-capture/design.md`、`openspec/changes/add-response-capture/design.md`

- [x] 1.3 固定流式响应、截断与脱敏边界
  - 关联目标：避免 response capture 范围失控
  - 验证：文档中明确“本轮交付 terminal response artifact，不交付 stream replay”
  - 涉及文件：`docs/stories/add-response-capture/design.md`、`openspec/changes/add-response-capture/specs/response-capture/spec.md`

## 2. 补齐 happy path 的实现与验收

- [x] 2.1 核对 probe 在主路径 hook 上的 response 事件发出行为
  - 关联目标：真实 attach 时至少一条 response 能进入 host
  - 验证：probe 单测或黑盒路径证明 `http_response_observed` 在主路径可见
  - 涉及文件：`crates/prismtrace-host/probe/bootstrap.js`、`crates/prismtrace-host/probe/bootstrap.test.js`

- [x] 2.2 固定 host 侧 response artifact 与 CLI 摘要输出
  - 关联目标：用户能在前台 attach 模式下看到 response 摘要，并在本地读到 artifact
  - 验证：聚焦测试覆盖 response artifact 落盘、provider hint 复用、摘要输出
  - 涉及文件：`crates/prismtrace-host/src/response_capture.rs`、`crates/prismtrace-host/src/request_capture.rs`

- [x] 2.3 建立最小黑盒闭环记录
  - 关联目标：把 response capture 从“内部实现”变成“可演示能力”
  - 验证：对真实 Node CLI 目标至少拿到一条 response 摘要和对应 artifact
  - 涉及文件：`openspec/changes/add-response-capture/blackbox-test.md` 或等价验收记录

## 3. 补错误路径与后续接入约束

- [x] 3.1 覆盖错误响应与不可文本化 body 的安全降级
  - 关联目标：response capture 不能因为异常正文把链路打崩
  - 验证：测试覆盖错误状态码、空 body、不可文本化 body、截断 body
  - 涉及文件：`crates/prismtrace-host/src/response_capture.rs`、相关测试文件

- [x] 3.2 明确 local console / inspector 的后续接入契约
  - 关联目标：本轮产物能被后续控制台与详情页复用
  - 验证：文档中明确 response artifact 是后续 UI 的稳定事实源
  - 涉及文件：`docs/stories/add-response-capture/design.md`、`openspec/changes/add-response-capture/design.md`

- [x] 3.3 完成文档与回归收尾
  - 关联目标：变更范围、验收口径与后续边界清晰
  - 验证：更新 README 或路线图中的必要引用；完成针对 response capture 的 focused validation
  - 涉及文件：按实现情况调整

## 备注

- 一次只推进一个 task，避免一边补 probe 一边扩 UI。
- 当前仓库已经有 response capture 骨架；本计划的重点是把它收敛成正式 change，而不是从零实现第二套链路。
- 只有在对应实现和验证完成后，才将复选框从 `- [ ]` 改为 `- [x]`。
