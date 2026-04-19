## 概览

`add-local-console` 的目标是把 PrismTrace 现有的 attach / probe / request capture 事实，通过一个最小但可产品化的本地控制台暴露出来。整体方案是：在现有 `prismtrace-host` 之上增加一个本地 HTTP server 和静态 UI surface，由 host 聚合 discovery、attach status、probe health 与最近 request 摘要，控制台以只读方式浏览这些信息。

这一设计刻意避免过早进入“完整前后端分离产品”或“复杂 SPA 框架”路线，而是优先交付一个能稳定演示与验证价值的本地控制台切片：用户能打开浏览器，看见目标、活动和请求摘要，而不必再直接依赖 CLI dump。

## 背景

当前仓库已经完成迭代 4：真实 attach、前台持续采集、request artifact 落盘和基础 CLI 摘要输出。但产品 surface 仍然主要是：

- `cargo run -p prismtrace-host -- --discover`
- `cargo run -p prismtrace-host -- --readiness`
- `cargo run -p prismtrace-host -- --attach <pid>`
- `cargo run -p prismtrace-host -- --attach-status`

这说明系统已经有了“事实层”，但还没有“控制台层”。路线图中迭代 5 被定义为“汽车 - 本地可观测性控制台”，因此第一步应该优先建立一个 local-first 的浏览入口，而不是马上扩展 response 可视化、复杂筛选或深度 request inspector。

当前约束：

- 仍是 macOS only，本地单机运行
- 仍以 `prismtrace-host` 为主入口，而不是 daemon + 多进程控制面
- `prismtrace-storage` 目前只有状态目录布局，没有完整查询抽象
- request capture 已可产出 artifacts，但尚无成熟的 request 列表读模型

## 目标 / 非目标

**目标：**
- 在 `prismtrace-host` 中提供本地 HTTP + 静态 UI 控制台入口
- 提供 target 列表、最近活动时间线、request 摘要列表和基础 request 详情的只读浏览能力
- 将 attach 状态、probe 健康和基础错误摘要纳入统一控制台数据模型
- 保持方案足够轻量，使后续 `add-request-inspector` 能在其上继续扩展

**非目标：**
- 完成完整 request inspector 或原始 payload 高保真渲染体验
- 完成 response / stream capture 的 UI 闭环
- 在本轮引入完整前端框架、复杂构建链或重型 API 分层
- 实现全文搜索、复杂过滤和跨 session 重建

## 架构与方案概览

整体方案分三层：

1. **Console HTTP surface（host 内）**
   - 在 `prismtrace-host` 内增加本地 HTTP server
   - 同时提供静态页面与最小 JSON API
   - 继续复用当前 `AppConfig.bind_addr`

2. **Console query/service layer**
   - 在 host 内增加面向控制台的聚合查询层
   - 将 discovery、attach status、probe health 与 request capture 摘要整形成控制台 view models
   - 第一版可以直接读取现有状态、内存快照和 artifact 元数据，不要求先做完整 repository 抽象

3. **Minimal local UI**
   - 以静态 HTML/CSS/JS 或极轻量模板渲染为主
   - 页面包含：target 列表、最近活动、request 列表、request 基础详情区
   - 所有交互以“只读浏览 + 路由切换”为核心

建议的第一版路由：

- `GET /`：控制台主页面
- `GET /api/targets`：target 与 attach/probe 状态摘要
- `GET /api/activity`：最近活动时间线
- `GET /api/requests`：request 摘要列表
- `GET /api/requests/:id`：单条 request 基础详情

## 关键设计决策

### 决策 1：先在 `prismtrace-host` 内直接提供最小 HTTP/UI surface
- 背景：当前只有 CLI 入口，但迭代 5 需要一个真实可见的控制台产品面。
- 方案：在现有 host 内直接增加最小 HTTP server，并由同一进程提供静态 UI 与 JSON API。
- 备选方案：单独新建 UI server crate，或立即采用完整前后端分离架构。
- 取舍：直接放在 host 中变更最小、路径最短，也更符合当前 bootstrap 阶段；缺点是后续模块边界需要在下一步再抽离，但不会阻碍当前迭代价值验证。

### 决策 2：先做“摘要级控制台”，不在本轮做深度 inspector
- 背景：路线图已建议将 `add-local-console` 与 `add-request-inspector` 分拆，否则 change 会迅速膨胀。
- 方案：本轮 request 详情只覆盖基础元数据与摘要，不承诺完整 payload inspector 体验。
- 备选方案：把列表和深度详情一起做完。
- 取舍：摘要级控制台已经足以证明“事实层已变成产品 surface”；将深度 inspector 延后，可以避免本轮同时处理 payload 渲染、安全显示和大文本交互复杂度。

### 决策 3：控制台读模型优先由 host 聚合，而不是先推动底层存储大改造
- 背景：`prismtrace-storage` 还处于状态布局阶段，如果先追求完整 DB schema/query layer，会把迭代 5 重新拖回基础设施建设。
- 方案：host 先增加 console-oriented query layer，直接消费当前可得的 discovery、attach status、probe health 与 request capture 结果。
- 备选方案：先补全数据库模型与 repository，再做控制台。
- 取舍：先聚合再抽象，能最快形成可浏览产品切片；代价是后续需要将临时 query layer 演进成更稳定的数据访问层，但这属于合理的阶段性技术债。

## 组件与接口

### 1. `prismtrace_host::console`
- 职责：承载本地 HTTP server、静态资源响应与 JSON API 路由。
- 输入：`BootstrapResult`、host 当前可见状态、console query layer 输出。
- 输出：HTML 页面、JSON 响应、结构化错误响应。
- 依赖：`prismtrace_host` 现有 discovery/attach/probe/request 模块。
- 变更点：新增 `run_console_server` 或等价入口，统一处理 `/` 和 `/api/*`。

### 2. `prismtrace_host::console::query`
- 职责：为控制台生成 target/activity/request/detail 等 view model。
- 输入：discovery 结果、attach status snapshot、probe health、request artifact 元数据。
- 输出：`ConsoleTargetSummary`、`ConsoleActivityItem`、`ConsoleRequestSummary`、`ConsoleRequestDetail` 等结构。
- 依赖：`discovery`、`attach`、`probe_health`、`request_capture`、`prismtrace_storage::StorageLayout`。
- 变更点：新增控制台专用聚合函数，不直接暴露内部领域对象给 UI。

### 3. `static console UI`
- 职责：展示列表、空态、基础详情与错误提示。
- 输入：`/api/*` JSON 响应。
- 输出：用户可见控制台界面。
- 依赖：浏览器原生能力与 host API。
- 变更点：新增静态资源目录或内嵌资源文件，支持最小 client-side 渲染。

## 数据模型

建议新增的控制台读模型：

- `ConsoleTargetSummary`
  - `pid`
  - `display_name`
  - `runtime_kind`
  - `attach_state`
  - `probe_state_summary`

- `ConsoleActivityItem`
  - `activity_id`
  - `activity_type`（attach / probe / request / error）
  - `occurred_at`
  - `title`
  - `subtitle`
  - `related_pid`
  - `related_request_id`

- `ConsoleRequestSummary`
  - `request_id`
  - `captured_at`
  - `provider`
  - `model`
  - `target_display_name`
  - `summary_text`

- `ConsoleRequestDetail`
  - `request_id`
  - `captured_at`
  - `provider`
  - `model`
  - `target`
  - `artifact_path` 或等价引用
  - `request_summary`
  - `probe_context`

这些结构的关键目的，是把控制台看到的“读模型”与底层领域模型分开，避免 UI 直接耦合到未来仍会变化的底层实现。

## 正确性与需求映射

### 属性 1：控制台入口是可明确发现且可失败解释的
- 说明：如果入口不稳定，用户仍会退回 CLI，迭代 5 的产品价值就无法成立。
- 覆盖需求：Requirement: Host 必须提供可本地访问的控制台入口
- 验证方式：单元测试验证启动参数与失败路径；黑盒测试验证浏览器可访问主页与错误语义

### 属性 2：控制台能稳定展示“当前目标 + 最近活动 + 请求摘要”
- 说明：这是本轮最核心的用户可见能力，也是 local-console 与后续 inspector 的分界线。
- 覆盖需求：Requirement: 控制台必须展示 attach target 与其当前状态；Requirement: 控制台必须展示最近观测活动时间线；Requirement: 控制台必须提供 request 摘要列表与基础详情跳转能力
- 验证方式：聚合层单测 + host 集成测试 + 黑盒页面检查

### 属性 3：用户能在控制台理解观测链路是否健康
- 说明：如果控制台只展示数据、不展示 probe/attach 健康状态，用户仍无法判断系统“为什么没看到东西”。
- 覆盖需求：Requirement: 控制台必须暴露 probe 健康与基础错误可见性
- 验证方式：状态聚合测试、错误映射测试、黑盒空态/错误态验证

## 错误处理与降级策略

- HTTP server 启动失败时，host 返回结构化启动错误，不静默退回 CLI-only 模式。
- 当 discovery、attach status 或 request 列表中某一部分暂时不可用时，控制台应按模块降级：页面仍可打开，但对应区域展示空态或错误卡片。
- 当 request artifact 已缺失或详情引用失效时，请求列表仍可展示摘要，但详情页返回“详情暂不可用”的明确提示。
- 静态资源加载失败时，至少保留最小文本化错误输出，避免浏览器出现空白页。

## 验证策略

- 单元验证：验证 console query layer 对 target、activity、request、probe/error 的聚合与空态输出。
- 集成验证：验证 host 能启动本地控制台、返回 `/api/*` 结构化 JSON，并与现有 bootstrap/discovery/attach 路径兼容。
- 黑盒验证：验证用户能打开主页、看到 target/request/activity 空态与非空态、进入基础 request 详情。
- 回归重点：不能破坏现有 `--discover`、`--readiness`、`--attach <pid>`、`--detach`、`--attach-status` CLI 能力。

## 风险 / 取舍

- [风险] 直接在 host 中加入 HTTP/UI 逻辑会让模块边界先变宽 -> 通过新增 `console` 模块和独立 view models 控制耦合，后续再抽离。
- [风险] 当前底层存储查询能力不足，可能导致 request 列表读模型临时性较强 -> 明确将本轮 query layer 视为控制台适配层，而非最终存储抽象。
- [风险] 如果把 request 详情做得过深，会把本轮拖入 inspector 范围 -> 在 spec 和任务中明确只做基础详情，不做深度 payload 体验。

## 发布 / 回滚

- 发布方式：先作为本地开发入口的一部分加入 `prismtrace-host`，通过新的命令行或默认控制台模式启动。
- 不涉及数据库迁移；主要是新增 host surface 与静态资源。
- 若实现失败，可回滚为移除 `console` 模块与入口，不影响现有 CLI attach/capture 流程。

## 开放问题

- 第一版控制台启动形态是默认启动即进入 Web 模式，还是增加显式参数（如 `--console`）更合适。
- request 摘要列表第一版是直接读 artifacts 元数据，还是需要先补一个最小索引文件。
- 基础 request 详情是否只展示摘要和 artifact 引用，还是允许渲染一小部分安全裁剪后的正文预览。
