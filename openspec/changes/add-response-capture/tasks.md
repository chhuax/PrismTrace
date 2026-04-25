## 1. 收敛 response capture V1 边界

- [x] 1.1 固定 response artifact schema 与关联语义
  - 关联需求：response capture 必须稳定产出可复用 artifact；response capture 必须支持 request-response 的最小闭环
  - 验证：文档与实现字段对齐，`exchange_id` 成为本轮唯一一等配对键

- [x] 1.2 固定截断、脱敏与流式边界
  - 关联需求：response capture 必须在异常正文和大 body 下安全降级
  - 验证：文档中明确“terminal response artifact”范围，不把 stream replay 并入本轮

## 2. 补齐主路径实现与 focused validation

- [x] 2.1 核对 probe 在主路径 hook 上稳定发出 `HttpResponseObserved`
  - 关联需求：response capture 必须对主路径模型请求产生 response 事实
  - 验证：probe 测试或等价 focused validation 覆盖至少一条主路径 response 事件

- [x] 2.2 固定 host 侧 response artifact 落盘与 CLI 摘要行为
  - 关联需求：response capture 必须提供 response artifact 与最小摘要输出
  - 验证：Rust focused tests 覆盖 artifact 落盘、provider hint 复用、摘要输出

- [x] 2.3 打通真实 Node CLI 目标的最小黑盒闭环
  - 关联需求：response capture 必须能在真实 attach 路径上工作
  - 验证：至少一条真实 response 被捕获、打印摘要并写入 `.prismtrace/state/artifacts/responses/`

## 3. 补错误路径与后续接入约束

- [x] 3.1 覆盖错误响应、空 body、不可文本化 body 与截断 body
  - 关联需求：response capture 必须在错误路径下保持稳定
  - 验证：focused tests 覆盖关键降级路径

- [x] 3.2 明确 local console / request inspector 的后续接入边界
  - 关联需求：response artifact 必须可供后续产品面复用
  - 验证：设计文档与 spec 明确 artifact 是后续 UI 的稳定事实源

- [x] 3.3 完成文档与回归收尾
  - 关联需求：变更范围、验收口径与后续边界清晰
  - 验证：更新路线图与黑盒验收记录；完成针对 response capture 的 focused validation

## 备注

- 一次只推进一个 task，避免并行扩散到 request inspector、tool visibility 或 session reconstruction。
- 只有在实现与验证完成后，才把对应复选框从 `- [ ]` 改成 `- [x]`。
