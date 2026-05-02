## 概览

`add-codex-app-server-observer` 的目标，是把 `Codex` 接入 PrismTrace 的方式从“尝试 live attach”改成“通过官方 App Server 协议观察高层运行时事件”。

本轮只交付：

- `Codex` 官方观测后端设计
- 第一版最小事件面
- 与现有 attach 路线的并存策略
- 最小 CLI/host 入口的实现边界

本轮不交付：

- 原始后端请求报文抓取
- 复杂本地控制台 UI
- 与 attach 路线完全统一的通用 timeline

## 背景

现有 host 的核心抽象围绕：

- 候选目标发现
- readiness 判断
- attach controller
- inspector runtime
- probe request / response capture

这条路线适用于动态注入型观测，但不适用于 `Codex.app`。对 `Codex`，当前 attach 方法会触发崩溃，因此需要明确引入一条新的官方协议接入路径。

## 目标 / 非目标

**目标：**

- 让 `PrismTrace` 能通过官方协议连接 `Codex`
- 读取高层事件并做结构化保留
- 保持 `Codex` 接入与现有 attach 路线分层清晰
- 通过 CLI 先形成可验证产品切片

**非目标：**

- 不把 `Codex` 重新纳入 attach 控制器
- 不承诺抓到原始 inference request / response payload
- 不在本轮完成高级分析与复杂 UI

## 方案

### 1. 引入独立的 Codex observer source

host 中新增一条并行 source：

- `AttachProbeSource`
  - 现有 Node / Electron attach + probe 路线
- `CodexAppServerSource`
  - 新增 `Codex` 官方 observer 路线

`CodexAppServerSource` 不依赖：

- `AttachController`
- `InstrumentationRuntime`
- probe bootstrap

### 2. 最小 CLI 入口

第一版推荐提供：

- `--codex-observe`
  - 自动发现 live `Codex` socket
- `--codex-socket <path>`
  - 指定 socket 做协议调试

CLI 输出先以“摘要 + 持久化”为主，而不是直接绑定控制台 UI。

### 3. 第一版事件面

第一版只归一化以下事件：

- `thread`
- `turn`
- `item`
- `tool`
- `approval`
- `hook`
- `capability_snapshot`
  - 由 `mcp / plugin / skill / app` 组成

每条事件至少包含：

- 时间戳
- 事件种类
- 可见的 thread / turn / item 关联键
- 摘要文本
- 原始 JSON

### 4. 保守保留 raw JSON

由于当前仍处于 `Codex` 官方协议探索期，第一版必须保留原始服务端事件 JSON，避免投影过窄后丢失后续需要的信息。

### 5. readiness 保持保守

现有 `attach-readiness` 对 `Codex` 的保守判断应继续保留。`Codex` 的可用性应通过新的 observer 入口体现，而不是通过 attach-ready 状态体现。

## 验证策略

- 聚焦测试覆盖 socket 发现、初始化、未知事件保留
- 聚焦测试覆盖最小事件归一化
- live 验证覆盖至少一个真实 `Codex` 会话的高层事件读取

## 风险与降级

### 风险

- live `proxy + socket` 的行为细节仍需进一步确认
- 官方协议的事件种类可能在不同版本间变化

### 降级策略

- 未识别事件保留 `raw_json`
- 无法建立 live socket 连接时返回结构化失败
- 不影响现有 attach/probe 路线
