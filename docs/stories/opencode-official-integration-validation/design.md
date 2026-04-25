# opencode 官方接入能力验证

日期：2026-04-25  
状态：验证中，已完成最小路线确认

## 1. 目的

这份文档回答两个问题：

1. `opencode` 现在还能不能继续沿用 `PrismTrace` 现有的 `attach + SIGUSR1 + inspector` 路线
2. 如果不能，`opencode` 有没有更安全、更官方的接入方式

目标不是立刻进入实现，而是先把路线判断收敛清楚，避免继续沿着会打崩目标应用的方向推进。

## 2. 当前结论

当前已经可以明确：

- `opencode` 不适合继续走现有 `attach + SIGUSR1` 路线
- 这条路线在真实机器上会直接把 `opencode` 打死
- `opencode` 自身提供了官方的 `server + attach(url) + SDK + plugin + export` 接入能力
- 对 `PrismTrace` 来说，`opencode` 应该优先作为“官方 server 数据源”接入，而不是继续做 Bun runtime attach

一句话总结：

`opencode` 有官方观测路线，而且这条路线比进程 attach 更成熟、更安全。

## 3. 为什么现有 attach 路线不该继续

当前 `PrismTrace` 的 Node attach 主线会在真正握手前先对目标进程发送 `SIGUSR1`，尝试唤醒 inspector。

对 `opencode` 的实机验证结果是：

- 发送 `SIGUSR1` 后，终端明确出现：

```text
zsh: user-defined signal 1  opencode
```

- 之后 `PrismTrace` 输出大量乱码
- 这说明目标进程并没有进入一个稳定可用的 inspector attach 状态，而是被信号打断或直接退出

因此当前可以明确判定：

- 现有 `attach` 路线对 `opencode` 不安全
- 不应再把它当作 `opencode` 的主方案

## 4. opencode 的官方接入方式

本机 CLI 和官方文档已经给出了比较完整的官方接入面。

### 4.1 CLI 能力

本机 `opencode --help` 明确包含：

- `opencode serve`
- `opencode attach <url>`
- `opencode run --attach <url>`
- `opencode export [sessionID]`
- `opencode plugin <module>`
- `opencode mcp`
- `opencode acp`
- `opencode web`

这说明 `opencode` 的产品形态本身就是：

- 一个可以独立启动的 server
- 一个可以连接运行中 server 的 client
- 一个允许插件、MCP 和导出 session 的平台

### 4.2 官方文档

官方文档明确支持：

- [Server](https://dev.opencode.ai/docs/server/)
- [CLI](https://opencode.ai/docs/cli/)
- [SDK](https://opencode.ai/docs/sdk/)
- [Plugins](https://opencode.ai/docs/plugins/)

这些资料一起说明：

- `opencode serve` 启动的是官方 headless HTTP server
- server 会公开 OpenAPI 文档
- 官方 SDK 可以直接作为 type-safe client 接入
- 插件可以订阅丰富的运行时事件

### 4.3 本机运行时证据

当前已经验证：

- `opencode serve --hostname 127.0.0.1 --port 4096` 能正常启动
- `GET /global/health` 返回健康状态
- `GET /doc` 能返回完整 OpenAPI 文档
- `opencode attach http://127.0.0.1:4096` 能接到运行中的 server
- `opencode session list --format json` 能列出真实 session
- `opencode export <sessionID>` 能导出结构化 session JSON

这说明 `opencode` 的官方路线不是纸面能力，而是在本机上真实可用。

## 5. 我们能看到什么

当前通过 `server + export + plugin/event` 这条官方路线，已经确认或高概率可以拿到下面这些信息。

| 我们能看到什么 | 对我们有什么用 |
|---|---|
| session 列表 | 知道当前有哪些会话，不用从 TUI 手工翻找 |
| session 标题、目录、创建时间、更新时间 | 还原会话基本元信息，方便筛选和索引 |
| user message 文本 | 知道用户问了什么 |
| assistant message 文本 | 知道 `opencode` 回了什么 |
| message parts | 看到一条消息内部是如何分段组成的 |
| reasoning 文本 | 看到它高层的思考摘要或解释过程 |
| 工具调用记录 | 知道它用了哪些工具 |
| 工具调用输入 / 输出 | 做工具链路分析，知道某一步到底做了什么 |
| step-start / step-finish | 还原步骤时间线 |
| model / provider 信息 | 知道每轮任务用的是哪个模型和提供方 |
| token 统计 | 做成本和上下文规模分析 |
| finish reason | 知道这一轮为什么结束，比如 `tool-calls`、`stop` 等 |
| cwd / root 等路径信息 | 还原任务执行的工作目录上下文 |
| 插件事件 | 分析插件参与了哪些行为 |
| session 事件 | 观察会话状态变化 |
| permission 事件 | 知道它什么时候在等待确认或权限 |
| server 事件 | 观察 server 生命周期和连接行为 |

## 6. 已确认的插件事件面

官方插件文档明确暴露了大量事件，已经确认至少包括：

- `message.updated`
- `message.part.updated`
- `session.created`
- `session.updated`
- `session.diff`
- `session.error`
- `session.idle`
- `session.status`
- `tool.execute.before`
- `tool.execute.after`
- `permission.asked`
- `permission.replied`
- `command.executed`
- `server.connected`

这意味着 `opencode` 的插件面不只是“装插件”，而是已经提供了较完整的运行时事件订阅能力。

## 7. 当前对产品最有价值的信息

如果从 `PrismTrace` 的产品价值来看，`opencode` 这条官方路线最值得用的是四类信息。

### 7.1 会话与步骤时间线

通过 session 导出和运行时事件，我们可以高概率拿到：

- 会话开始、结束、更新
- 消息顺序
- 每一步的开始与结束
- 任务在哪一步停住或结束

这足够做：

- `opencode` 会话时间线
- 单轮任务回放
- 错误节点定位

### 7.2 工具链路

目前导出的 session 里已经能看到：

- 工具名
- 工具调用输入
- 工具调用输出
- call ID
- 工具执行前后关系

这足够做：

- 工具执行链分析
- “为什么结果不对”的排查
- “到底是模型错了还是工具错了”的分层定位

### 7.3 模型与成本信息

目前已经能看到：

- provider
- model
- tokens
- finish reason

这足够做：

- 模型使用统计
- 会话成本粗估
- 不同 agent / model 行为差异分析

### 7.4 插件 / 事件驱动观测

如果接入插件事件或 server 事件流，我们高概率可以做：

- session 变化实时观测
- 工具执行前后监听
- 权限审批观察
- 失败和空闲状态提醒

这对做“实时观测台”非常有价值。

## 8. 当前还没有验证到的边界

虽然 `opencode` 的官方能力面已经很强，但当前仍然没有证据表明它的官方 server / plugin 面一定会暴露：

- 发给模型提供方的原始 HTTP 请求报文
- 完整 request headers / response headers
- 原始 wire-level response stream
- 100% 等价于最终 model-facing JSON 的底层报文

也就是说，当前还不能直接下结论说：

`opencode` 的官方路线已经等于后端抓包器。

当前更稳的判断是：

- 它已经足够做高层运行时观测
- 至于能不能进一步做到“原始模型报文观测”，还需要额外验证

## 9. 当前最稳的产品判断

如果 `PrismTrace` 接 `opencode`，当前最现实的定位是：

`把 opencode 作为一个成熟的官方 server 数据源接入`

它适合优先做：

- 会话时间线
- 工具链分析
- session / message / reasoning / step 观测
- 插件 / permission / command / server 事件观测
- 离线 session 导出分析

它当前不应该优先做：

- 继续 Bun attach
- 继续发 `SIGUSR1`
- 把“能不能拿到原始后端 HTTP 包”当作接入前提

## 10. 建议路线

`opencode` 这条线建议按下面优先级推进：

1. `serve + SDK/client`
   把 `opencode` 作为官方 server source 接入
2. `session export`
   先做离线结构化分析
3. `global/event + plugin events`
   再补实时观测
4. 如仍有必要，再评估更底层 payload 能力

当前不建议再把 Bun runtime attach 当成 `opencode` 的主路线。
