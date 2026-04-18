# 迭代 4：单目标请求采集器设计

日期：2026-04-18
状态：已评审草案
对应 change：`add-request-capture`

## 1. 背景

PrismTrace 已完成迭代 3 的 attach controller 与 probe bootstrap，当前已经可以：

- 发现候选 AI 相关进程
- 判断目标是否适合 attach
- 发起 attach / detach 并看到 probe 握手状态
- 通过 heartbeat 维持最小在线探针

但当前系统还不能回答最关键的问题：

`attach 之后，目标进程到底真实发了什么给模型？`

迭代 4 的目标是交付第一个真正体现 AI 可观测性价值的产品切片：在单个目标进程上，前台 attach 后实时看到高置信度模型请求摘要，并把原始请求 payload 落到本地 artifacts。

## 2. 本轮范围

本轮只做请求采集，不做响应和流式输出采集。

本轮用户可见能力：

- 用户执行 `cargo run -p prismtrace-host -- --attach <pid>` 后进入前台采集模式
- host 完成 attach 握手后持续监听 probe IPC
- 当目标进程发出高置信度“类模型请求”HTTP 请求时，终端立即打印一行摘要
- 同时把原始请求内容写入 `.prismtrace/state/artifacts/requests/`
- 用户通过 `Ctrl+C` 结束会话

本轮明确不做：

- 响应采集
- 流式分片采集
- 跨命令 attach session 持久化
- daemon / background service
- 本地控制台 UI
- 深度 provider/model 统一 schema
- 全 provider 覆盖
- 二进制、multipart、超大请求体的完整支持

## 3. 目标与非目标

### 3.1 目标

- 在不重启目标应用的前提下观测模型请求
- 第一版只采集高置信度模型请求，尽量减少普通 HTTP 噪音
- 采集失败不得改变目标进程原有请求语义
- 采集结果既能通过 CLI 即时验证，也能通过本地 artifact 回看
- 设计边界应支持下一轮继续扩展到响应采集

### 3.2 非目标

- 不追求“所有 HTTP 请求都能抓”
- 不追求“所有 AI provider 都能识别”
- 不追求“请求内容百分百完整无损”
- 不在本轮引入数据库事件表或控制台浏览界面

## 4. 方案对比与选择

评估过的方案：

### 4.1 Probe 重逻辑

由 probe 直接判断 provider、过滤噪音、组装完整领域事件，host 只负责展示和落盘。

不采用原因：

- 规则会固化在注入脚本中，后续变更成本高
- Node 版本差异与 hook 兼容性风险更集中
- probe 侧测试和演进成本高于 Rust host

### 4.2 Probe 采集原始事实，Host 负责归一化

由 probe 只采集原始 HTTP 请求事实，通过 IPC 发回 host；host 再负责：

- 判断是否为“类模型请求”
- 生成第一版领域事件
- 输出 CLI 摘要
- 写入 artifact

采用原因：

- 与现有 `probe -> IPC -> host` 分层一致
- 把策略性逻辑放在 Rust 侧，更容易测试和演进
- 可以在不改 probe 协议的前提下扩 provider 规则

### 4.3 只做 fetch / undici

可作为最短路径，但覆盖面不足，容易漏掉 `http/https` fallback 请求。

不采用原因：

- 不符合“单目标请求采集器”的产品完整感
- 很多 SDK 或 fallback 路径仍可能走 `http/https`

最终采用方案：`Probe 采集原始事实，Host 负责归一化`

## 5. 用户体验

### 5.1 CLI 形态

`--attach <pid>` 从“一次性 attach 演示”扩为“前台采集模式”：

1. host 完成 discovery / readiness 检查
2. attach backend 注入 probe 并完成 bootstrap
3. attach 成功后保持前台运行
4. 持续读取 IPC 事件
5. 遇到命中的请求时打印摘要并写 artifact
6. 用户 `Ctrl+C` 结束进程

### 5.2 终端反馈

终端输出保留两层信息：

- attach 成功和 probe 状态摘要
- 每一条已捕获请求的一行摘要

示例：

```text
[attached] TargetApp (pid 12345)
[captured] openai POST /v1/responses artifact=.prismtrace/state/artifacts/requests/1714000000000-12345-1.json
```

CLI 只承担“证明已捕获”的角色，不承担完整内容阅读体验。

## 6. 架构设计

### 6.1 总体链路

本轮链路分四段：

1. `probe hook`
   对 `fetch / undici / http / https` 请求发起点做透明包装，提取原始请求事实。
2. `IPC 协议`
   把原始请求事实编码成新的 `IpcMessage` 变体发给 host。
3. `host 归一化`
   识别是否为“类模型请求”，命中后归一化为第一版 `CapturedRequestEvent`。
4. `CLI + artifacts`
   输出摘要并将原始请求内容落盘。

### 6.2 模块职责

#### `prismtrace-core`

- 扩展 `IpcMessage`
- 保持 probe 和 host 之间的线协议稳定

#### `prismtrace-host/probe/bootstrap.js`

- 继续负责 hook 安装与生命周期管理
- 在 hook 中提取原始请求事实
- 通过 stdout JSON line 发回 host
- 所有采集异常都必须吞掉，不能影响原请求

#### `prismtrace-host`

- 监听新的 IPC 请求事件
- 判断是否为高置信度 LLM 请求
- 生成 `CapturedRequestEvent`
- 管理 artifact 路径和写盘
- 渲染 CLI 摘要

#### `prismtrace-storage`

本轮不引入独立 storage runtime；继续使用 `StorageLayout` 提供 artifact 根目录。

## 7. 事件模型

### 7.1 Probe 到 Host 的原始协议

在 `prismtrace-core::IpcMessage` 中新增：

`HttpRequestObserved`

建议字段：

- `hook_name: String`
- `method: String`
- `url: String`
- `headers: Vec<HttpHeader>`
- `body_text: Option<String>`
- `timestamp_ms: u64`

其中 `HttpHeader` 固定为：

- `name: String`
- `value: String`

设计约束：

- 只表达“probe 观察到的原始事实”
- 不包含 `provider_hint`、`model`、`summary` 等推断字段
- 允许缺失 `body_text`
- 必须保持 JSON 往返稳定

### 7.2 Host 侧第一版领域事件

host 在命中后生成 `CapturedRequestEvent`：

- `event_id`
- `pid`
- `target_display_name`
- `provider_hint`
- `hook_name`
- `method`
- `url`
- `captured_at_ms`
- `artifact_path`
- `body_size_bytes`
- `summary`

其中：

- `event_id` 只要求本地唯一，不要求全局分布式唯一
- `provider_hint` 由 host 根据规则推断，允许是保守猜测值
- `body_size_bytes` 表示写入 artifact 的 `body_text` 字节数；若未采到正文则为 `0`
- `summary` 只用于 CLI 展示，不作为稳定存储契约

## 8. Probe Hook 设计

### 8.1 采集点

第一版对以下入口做透明包装：

- `globalThis.fetch`
- `undici.request`
- `http.request`
- `https.request`

### 8.2 采集字段

probe 应尽量提取：

- `hook_name`
- `method`
- `url`
- `headers`
- `body_text`
- `timestamp_ms`

### 8.3 文本 body 策略

第一版只处理适合文本化的请求体：

- 字符串 body 直接采集
- 常见 JSON body 尽量转成字符串
- 不适合文本化的 body 记为 `None`

如果 body 超过阈值，按文本截断后发送，并在 artifact 中记录 `truncated: true`。

建议的第一版截断上限：`64 KB`

### 8.4 透明性约束

hook 必须满足：

- 不改变原始请求参数语义
- 不改变返回值和时序语义
- 采集异常不得抛到业务调用方
- 重复安装仍保持幂等

## 9. 类模型请求识别规则

第一版采用“保守命中”策略，只在高置信度时采集。

### 9.1 第一层：provider host/path 命中

优先识别以下 host：

- `api.openai.com`
- `api.anthropic.com`
- `generativelanguage.googleapis.com`
- `openrouter.ai`

优先识别以下模型入口路径模式：

- `/v1/chat/completions`
- `/v1/responses`
- `/v1/messages`
- `/v1beta/models/*:generateContent`

### 9.2 第二层：header/body 辅助信号

当 host/path 需要进一步确认时，再看：

- `authorization: Bearer ...`
- `x-api-key`
- `anthropic-version`
- body 中的 `model`
- body 中的 `messages`
- body 中的 `input`
- body 中的 `contents`

### 9.3 命中原则

- 高置信度命中则采集
- 低置信度或不确定则丢弃
- 宁可漏掉边缘请求，也不把普通业务 HTTP 噪音大量采进来

## 10. Artifact 落盘

### 10.1 路径

请求 artifact 统一写入：

`.prismtrace/state/artifacts/requests/`

建议文件名：

`<timestamp_ms>-<pid>-<seq>.json`

示例：

`1714000000000-12345-1.json`

### 10.2 文件内容

artifact 保存比 CLI 更完整的结构化内容，至少包括：

- `event_id`
- `pid`
- `target_display_name`
- `provider_hint`
- `hook_name`
- `method`
- `url`
- `headers`
- `body_text`
- `body_size_bytes`
- `truncated`
- `captured_at_ms`

artifact 是本轮的主要回看载体，但不承诺长期稳定为公共 API。

## 11. 失败处理

本轮所有失败都遵循同一原则：

`观测失败不能变成业务故障`

具体约束：

- hook 安装失败：继续沿用 `BootstrapReport.failed_hooks`
- 单次请求解析失败：probe 静默跳过，不阻断原请求
- 非文本 body：允许 `body_text = None`
- 超大 body：按阈值截断并标记
- artifact 写入失败：CLI 输出本地错误，但采集循环继续
- IPC 中出现非目标消息：忽略并继续循环

## 12. 测试策略

### 12.1 `prismtrace-core`

- `HttpRequestObserved` JSON 往返测试
- 兼容 trailing newline 的解析测试

### 12.2 probe

- `fetch` 包装器调用时会发送请求观察消息
- `http` / `https` 包装器调用时会发送请求观察消息
- 重复安装仍幂等
- 无法文本化的 body 不抛异常

### 12.3 host

- OpenAI / Anthropic / Gemini / OpenRouter 请求会被识别
- 普通 HTTP 请求不会误判
- artifact 文件名规则稳定
- artifact 内容结构稳定
- CLI 摘要格式稳定
- artifact 写入失败不会中断采集循环

## 13. 实现拆分

本轮实现建议按以下顺序推进：

1. `core 协议扩展`
   在 `prismtrace-core` 新增 `IpcMessage::HttpRequestObserved` 及测试。
2. `probe 请求观察`
   在 `bootstrap.js` 中把现有 no-op hook 扩成透明采集包装器。
3. `host 识别与落盘`
   在 `prismtrace-host` 新增请求事件归一化、过滤、artifact 写盘与摘要渲染。
4. `前台 attach 采集循环`
   让 `--attach <pid>` 从一次性演示模式变成持续前台采集模式。

## 14. 风险与后续演进

### 14.1 本轮主要风险

- Node 生态里同一请求入口参数形态差异较大，probe 侧归一化要尽量保守
- `http/https` 请求体可能在 `write/end` 阶段分片写入，第一版可能先覆盖有限场景
- 一些 provider 可能使用未覆盖的域名或代理层，第一版会漏抓

### 14.2 后续自然演进

本设计为后续能力留出清晰演进路径：

- 迭代 4.5 / 后续 change：响应采集
- 迭代 5：本地控制台消费 artifact 和结构化事件
- 迭代 6：把 request/response/tool visibility 串成会话

## 15. 结论

迭代 4 的定义收敛为：

一个前台运行的单目标请求采集器。它 attach 到指定 pid 后，只采集高置信度“类模型请求”HTTP 请求，在 CLI 中即时证明“已经看到真实请求”，并把原始 payload 写到本地 artifact 文件中。

这个切片不求完整，但必须形成第一次真实可验证的模型请求可见性闭环。
