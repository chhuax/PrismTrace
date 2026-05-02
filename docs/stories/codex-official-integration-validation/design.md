# Codex 官方接入能力验证

日期：2026-04-25  
状态：验证中，已完成最小可行性确认

## 1. 目的

这份文档只回答一个问题：

`PrismTrace` 如果不再走危险的运行中 attach / `SIGUSR1` 路线，而是改走 `Codex.app` 自带的官方接入面，当前到底能拿到什么信息，能做到什么程度？

这份文档不讨论实现细节优雅与否，也不提前承诺“能抓到完整模型请求报文”。目标是先把能力边界说清楚。

## 2. 当前结论

当前已经确认：

- `Codex.app` 存在官方本地接入面，不需要再依赖 `SIGUSR1` attach
- 这条官方接入面以 `Codex App Server + 本地 IPC socket` 为核心
- 这条路线高概率可以把 `PrismTrace` 做成一个安全、稳定的 `Codex` 运行时观测器
- 但按当前证据，这条路线大概率拿不到 `Codex -> 后端模型服务` 的原始 HTTP 请求报文

一句话总结：

`Codex` 的官方接入更像“工作台日志和会话协议”，不是“后端抓包器”。

## 3. Codex 的官方接入方式

当前已经验证到的官方接入方式如下：

1. `Codex.app` 自带 `codex app-server`
2. `codex app-server proxy` 明确支持连接到运行中的 app-server control socket
3. 运行中的 `Codex` 主进程实际打开了本地 Unix socket
4. `codex app-server` 的最小 `initialize` 握手已经验证成功

当前推荐理解方式：

- `Codex.app`
  是用户面应用
- `codex app-server`
  是官方协议入口
- `proxy --sock <socket>`
  是连接运行中 `Codex` 的潜在正式通道

这意味着后续如果 `PrismTrace` 接 `Codex`，主线应该优先考虑：

- 作为一个 app-server client 接入
- 读取 thread / turn / item / tool / hook / plugin / skill 相关事件
- 避免再对 live `Codex` 进程发 attach 信号

## 4. 我们能看到什么

下面这张表用产品视角描述“当前高概率能看到的信息”和“它对我们有什么用”。

| 我们能看到什么 | 对我们有什么用 |
|---|---|
| 它什么时候开始处理你的问题，什么时候结束 | 还原一轮任务的时间线，知道整轮任务跑了多久 |
| 中间经历了哪些步骤 | 看懂它不是一下子出结果，而是在分阶段工作 |
| 每一步大概是什么类型 | 分清是在思考、在调用工具、在等待确认，还是在输出结果 |
| 它最后给你的回复内容 | 看到最终产物，而不是只知道“它执行过” |
| 它有没有调用工具 | 判断这轮任务是不是靠工具完成的 |
| 调用了哪些工具 | 分析它依赖了哪些能力，也能反查为什么没用某个工具 |
| 工具执行结果是什么 | 知道工具成功了、失败了，还是返回了什么内容 |
| 什么时候向用户要确认或权限 | 定位任务为什么停住、为什么没有继续执行 |
| 什么时候报错、中断、失败 | 用来做故障排查，不再只看到一个模糊的失败状态 |
| 当前有哪些 skills / 插件 / app 能力可用 | 分析它当时“看得到什么能力”，帮助解释行为差异 |
| 安装了哪些插件 | 了解这台 `Codex` 当前具备哪些扩展能力 |
| 某个插件或 skill 是否参与了当前任务 | 判断扩展能力有没有真正参与会话 |
| 整段会话里各个步骤的先后顺序 | 做会话回放和问题复盘 |
| 哪一步最耗时 | 找瓶颈，知道时间花在什么地方 |
| 哪一步触发了工具，哪一步出了错 | 快速定位关键节点，而不是整段盲猜 |

## 5. 当前对产品最有价值的信息

如果从 `PrismTrace` 现阶段的产品价值来看，这条官方接入路线最值得利用的是四类信息。

### 5.1 会话时间线

我们可以高概率拿到：

- thread
- turn
- item
- started / completed / interrupted / archived / resumed 这类生命周期事件

这足够做：

- `Codex` 会话时间线
- 单轮任务回放
- 出错节点定位

### 5.2 工具与本地执行链路

我们可以高概率拿到：

- 工具调用
- shell / command 执行相关事件
- 审批 / 权限确认事件
- hook 开始和结束事件

这足够做：

- “为什么停住了”的定位
- “它用了什么本地能力”的解释
- “哪一步失败导致整轮任务失败”的分析

### 5.3 技能、插件、应用可见性

我们已经确认官方面里存在：

- `skills/list`
- `apps/list`
- `plugin/list`
- `plugin/read`
- `plugin/install`

这对产品很重要，因为可以回答：

- 当前 `Codex` 看得到哪些能力
- 某个插件到底有没有安装
- 某个 skill / app 是否在这台机器上可用
- 一轮任务里，哪些能力参与了，哪些没有参与

### 5.4 高层响应结果

官方 schema 明确暴露了高层响应 item，例如：

- message
- reasoning summary / reasoning item
- function call
- function call output
- local shell call
- web search call
- image generation call
- tool search call

这意味着我们很可能可以看到“`Codex` 产出了什么”，而不只是知道“它在运行”。

## 6. 当前看不到什么

这部分必须说清楚，否则会再次产生“以为能抓报文，结果最后抓不到”的误判。

按当前验证，官方接入路线大概率看不到：

- `Codex` 发给模型后端的原始 HTTP 请求报文
- 完整 request headers / response headers
- 完整 wire-level response stream
- 原封不动的 model-facing JSON 请求体
- 最终 system prompt / messages / tools 全量原文快照

也就是说，这条路线当前不适合直接做：

- prompt 抓包器
- 原始请求镜像
- 后端线包取证

## 7. 为什么会有这个边界

当前有两组证据：

### 7.1 官方 schema 暴露的是高层会话协议

导出的官方 schema 中可以看到：

- thread / turn / item
- skills / apps / plugins
- hooks
- raw response item

但没有看到：

- `inference_request`
- `inference_response`
- `RawPayloadKind`
- `trace.jsonl`

这说明客户端正式协议当前暴露的是“运行时会话信息”，不是“后端线包信息”。

### 7.2 二进制内部仍然存在更底层 payload 痕迹

在 `codex` 二进制字符串中可以看到：

- `inference request payload`
- `inference response payload`
- `trace.jsonl`
- `RawPayloadKind`
- `RawTraceEventPayload`

这说明 `Codex` 内部很可能有更底层的 trace / payload 机制。

但由于这些内容没有出现在当前公开的 app-server client schema 中，现阶段更合理的判断是：

- 内部有
- 但没有作为稳定公开接口提供给外部 client

## 8. 当前最稳的产品判断

如果 `PrismTrace` 走 `Codex` 官方接入路线，当前最现实的产品定位是：

`一个安全、不打崩 Codex 的高层运行时观测器`

它最适合做：

- 会话时间线
- 工具与审批链路分析
- 插件 / skill / app 可见性分析
- 高层结果与错误定位

它当前不适合直接做：

- 原始 prompt 抓包
- 后端请求报文镜像
- 线包级响应流还原

## 9. 下一步建议

在不改主业务代码的前提下，下一步最值得继续验证的是：

1. 继续验证 `proxy + running socket` 是否能稳定连到 live `Codex`
2. 用最小 thread / turn 样本验证 app-server 实际能返回哪些 item
3. 判断这些 item 是否已经足够支撑第一版 `Codex` 观测台
4. 不再把“能否拿到原始后端报文”作为这条路线的前提假设

当前建议的决策原则：

- 如果目标是“先把 `Codex` 安全接进来并能看懂它在干什么”，继续推进这条路线
- 如果目标是“必须拿到完整原始请求报文”，则需要把这条路线和“抓包 / 代理 / 其他更底层方法”区分开评估
