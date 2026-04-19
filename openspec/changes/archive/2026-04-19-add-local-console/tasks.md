## 1. Host 控制台入口与 API surface

- [x] 1.1 在 `prismtrace-host` 中增加本地控制台启动入口与最小 HTTP server
  - 关联需求：Host 必须提供可本地访问的控制台入口
  - 验证：能够启动本地控制台，并在启动失败时返回结构化错误

- [x] 1.2 增加控制台所需的最小 API 路由
  - 关联需求：控制台必须展示 attach target 与其当前状态；控制台必须展示最近观测活动时间线；控制台必须提供 request 摘要列表与基础详情跳转能力
  - 验证：`/api/targets`、`/api/activity`、`/api/requests`、`/api/requests/:id` 返回稳定结构化响应

## 2. 控制台读模型与状态聚合

- [x] 2.1 为 target、attach status、probe health 建立控制台专用 view models 与聚合逻辑
  - 关联需求：控制台必须展示 attach target 与其当前状态；控制台必须暴露 probe 健康与基础错误可见性
  - 验证：存在 active session、无 active session、probe 异常三类路径均有明确输出

- [x] 2.2 为最近活动时间线建立聚合逻辑和空态语义
  - 关联需求：控制台必须展示最近观测活动时间线
  - 验证：有活动与无活动两类场景都能稳定输出

- [x] 2.3 为 request 摘要列表与基础详情建立控制台读模型
  - 约束：本 task 只覆盖摘要与基础详情，不扩展到完整 request inspector
  - 关联需求：控制台必须提供 request 摘要列表与基础详情跳转能力
  - 验证：已捕获 request 可在列表中展示，并能进入单条 request 基础详情

## 3. 本地控制台 UI

- [x] 3.1 建立最小静态控制台页面与基础布局
  - 关联需求：Host 必须提供可本地访问的控制台入口
  - 验证：浏览器打开控制台主页时，能渲染 target、activity、request 三个主区域

- [x] 3.2 接入 target 列表、活动时间线和 request 摘要列表的前端展示
  - 关联需求：控制台必须展示 attach target 与其当前状态；控制台必须展示最近观测活动时间线；控制台必须提供 request 摘要列表与基础详情跳转能力
  - 验证：页面能正确展示非空态与空态，不依赖 CLI dump

- [x] 3.3 增加基础 request 详情区以及 probe/错误提示区
  - 关联需求：控制台必须提供 request 摘要列表与基础详情跳转能力；控制台必须暴露 probe 健康与基础错误可见性
  - 验证：选择 request 后可展示基础详情；链路异常时页面有明确错误或健康提示

## 4. 验证与收尾

- [x] 4.1 补齐并运行与控制台入口、聚合逻辑和 API surface 直接相关的自动化验证
  - 验证：新增/更新相关单元测试与集成测试；运行 `cargo test`
  - 关联需求：Host 必须提供可本地访问的控制台入口；控制台必须展示 attach target 与其当前状态；控制台必须展示最近观测活动时间线；控制台必须提供 request 摘要列表与基础详情跳转能力；控制台必须暴露 probe 健康与基础错误可见性

- [x] 4.2 完成本地黑盒回归与必要文档同步
  - 验证：本地打开控制台主页并检查 target/activity/request 基础浏览链路；同步 README 或使用说明中的控制台入口
  - 关联需求：Host 必须提供可本地访问的控制台入口；控制台必须提供 request 摘要列表与基础详情跳转能力

## 备注

- 本 change 只交付“本地控制台最小闭环”，不在本轮扩展 `add-request-inspector`。
- 若 request 基础详情实现过程中发现需要完整 payload 检查体验，应另行留到后续 change，而不是在本任务中扩范围。
