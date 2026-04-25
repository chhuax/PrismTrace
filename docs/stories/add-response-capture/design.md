# add-response-capture 设计

日期：2026-04-22
状态：草稿

## 概览

`add-response-capture` 的目标不是从零发明一条新链路，而是把仓库里已经出现的 response 观测萌芽，收敛成一个有清晰边界、可稳定验收、可继续向控制台和后续会话重建复用的正式能力切片。

当前代码已经具备几块基础：

- `prismtrace-core::IpcMessage::HttpResponseObserved` 已存在，协议能承载 response 事实。
- `probe/bootstrap.js` 已能发出 `http_response_observed` 事件。
- `prismtrace-host/src/request_capture.rs` 已会在前台消费 response 事件并打印摘要。
- `prismtrace-host/src/response_capture.rs` 已能把 response artifact 写入 `.prismtrace/state/artifacts/responses/`。

但这些实现还没有被收敛成一个正式的产品能力定义。当前缺的是：

- 明确本轮到底承诺哪些 response 事实，哪些留到后续。
- 明确 request / response 的关联语义和黑盒验收标准。
- 明确流式响应、错误响应、超大 body、脱敏与截断策略。
- 明确 local-console 和后续 session reconstruction 可以依赖的稳定输出。

因此，这个 change 的本质是：

`把“已有 response 事件骨架”推进成“可被正式验收的 response capture V1”。`

## 背景

路线图当前已经完成：

- 真实 attach 与前台持续采集
- request capture 第一版
- local console 第一版

这意味着 PrismTrace 已经能回答两类问题：

- 可以 attach 到谁。
- 它发出了什么 request。

但仍然不能稳定回答更关键的下一个问题：

- 它收到了什么 response。

如果没有 response facts，后续几件事都缺少地基：

- local console 无法展示一次 exchange 的闭环。
- request inspector 只能看到请求半边。
- session reconstruction 无法稳定建立 request-response 对。
- 后续的 failure attribution、fallback 解释、latency 分析都缺核心输入。

## 当前现状

从代码层面看，response capture 处于“已有实现痕迹，但尚未被正式定义”的状态：

- 协议面：已有 `HttpResponseObserved`。
- probe 面：已有 response 事件发送逻辑。
- host 面：已有 response artifact 落盘和 CLI 摘要输出。
- 文档面：没有独立 story 设计、没有 OpenSpec stable spec、没有黑盒验收文档。
- 产品面：local console 还没有把 response 作为稳定 surface 暴露出来。

这决定了本轮不应该把重点放在“再写更多底层代码”，而应该先把边界和验收定义收紧，再围绕这个定义做最小补强。

## 目标 / 非目标

**目标：**

- 正式定义 response capture V1 的边界与验收口径。
- 对单次 exchange 稳定采集 response 核心事实：状态码、头、正文摘要、时间信息。
- 复用现有 `exchange_id`，形成稳定的 request-response 关联键。
- 把 response artifact 持久化为可供后续 console / reconstruction 消费的稳定格式。
- 明确错误响应、空响应、超大 body 和脱敏策略。

**非目标：**

- 不在本轮做完整 stream chunk 时间线回放。
- 不在本轮做 response 深度渲染 UI。
- 不在本轮做完整 usage / finish reason / tool call 语义归一化。
- 不在本轮引入 session reconstruction、tool visibility 或 failure attribution。
- 不为了 response capture 改造一整套新的存储引擎或查询层。

## 本轮要回答的 5 个问题

### 1. 采哪些 response 事实

V1 至少稳定采集：

- `exchange_id`
- `hook_name`
- `method`
- `url`
- `status_code`
- `headers`（经过脱敏）
- `body_text`（仅限可安全转换的文本正文）
- `body_truncated`
- `started_at_ms`
- `completed_at_ms`
- `duration_ms`
- `provider_hint`
- `pid` / `target_display_name`

这些字段已经能支持：

- CLI 单行摘要
- response artifact 持久化
- 之后在 console 中做列表与基础详情展示
- 后续 request-response 配对与 latency 分析

### 2. 如何与 request 稳定关联

本轮继续复用现有 `exchange_id` 作为 request-response 的一等关联键，不额外引入新的 session 键或关联层。

关联规则：

- 同一 request 和 response 必须共享同一个 `exchange_id`。
- host 在消费 response 事件时，优先复用同 `exchange_id` 的 request provider hint。
- 若 request 先前未被识别，response 路径允许按 URL / header / body 重新推断 provider，但不能改变既有 request artifact。

这个选择的原因是最小化范围：

- 它与现有代码一致。
- 它足够支撑单次 exchange 闭环。
- 它不提前承诺 session reconstruction 的长期模型。

### 3. 流式响应本轮如何定义

本轮不做“chunk 级时间线回放”，也不承诺完整 SSE/stream 事件重建。

本轮对流式响应只承诺最小可用语义：

- 如果 probe 能在不破坏原语义的前提下得到最终可文本化的响应内容，则发出一条终态 `HttpResponseObserved`。
- 如果正文过大或只能部分观察，则用 `body_truncated = true` 明确表示截断。
- 如果是无法安全文本化的二进制或不可读 body，则 `body_text = null`，但仍保留状态码、头和时延信息。

也就是说，本轮目标是“response summary + terminal artifact”，不是“stream replay”。

### 4. 脱敏与截断策略是什么

response 侧沿用 request capture 的安全原则：

- header 先走脱敏，再落盘。
- cookie、set-cookie、authorization 等敏感值不保留原文。
- query 参数不作为摘要的一部分。
- body 只保留文本化后的安全裁剪内容。
- 无法安全文本化的 body 不强制序列化为字符串。

artifact 的核心原则是：

`宁可保留不完整但明确标记为截断的事实，也不要因为大包体或异常正文把整条链路打崩。`

### 5. 什么算本轮验收通过

黑盒口径收敛为一条最小闭环：

1. 对一个真实运行中的纯 Node CLI 目标执行 `--attach <pid>`。
2. 目标发出至少一条真实模型 request，并收到至少一条 response。
3. host 在前台打印 response 摘要。
4. `.prismtrace/state/artifacts/responses/` 中出现对应 artifact。
5. artifact 中能看到稳定的 `exchange_id`、状态码、时间信息和脱敏后的 headers。

## 架构与方案概览

整体链路继续沿用现有四段结构：

1. `probe/bootstrap.js`
   - 在请求生命周期结束时发出 `http_response_observed`。
2. `prismtrace-core`
   - 用 `IpcMessage::HttpResponseObserved` 作为稳定 IPC 载体。
3. `prismtrace-host/src/request_capture.rs`
   - 在前台事件循环里消费 response 事件，维持 `exchange_id -> provider_hint` 的最小关联状态。
4. `prismtrace-host/src/response_capture.rs`
   - 负责 provider hint 复用、artifact 落盘、单行摘要和响应读模型产出。

本轮重点不是重新切模块，而是把这条链路从“可运行”推进到“可依赖”。

## 关键设计决策

### 决策 1：复用现有 `exchange_id`，不抢跑 session model

- 背景：response capture 的下一跳会自然诱导到 session reconstruction。
- 方案：本轮仅用 `exchange_id` 形成单次 exchange 闭环。
- 不采用：提前引入 session grouping、父子请求关系或完整时间线模型。
- 取舍：这样可以先把基础事实做稳，避免第二层抽象反过来压垮第一层实现。

### 决策 2：先交付 terminal response artifact，不交付 stream replay

- 背景：流式响应是高复杂度点，但本轮的核心价值是让用户先看见“收到了什么”。
- 方案：只承诺终态 response 事件与 artifact；stream chunk 重放留待后续单独切片。
- 不采用：本轮同时做 SSE chunk、增量 token、时间线回放。
- 取舍：范围更稳，验收更清晰，也更符合当前 console 与 request capture 的成熟度。

### 决策 3：host 侧读模型继续以 artifact 为稳定事实源

- 背景：response 现在已经能在 host 中被观察和打印，但要支撑 console 和后续分析，需要稳定的本地事实源。
- 方案：继续把 artifact 作为 response 的一等持久化输出；console 和未来 reconstruction 从 artifact 读取。
- 不采用：只保留前台 stdout 输出，不保证本地持久化。
- 取舍：artifact 模型更适合作为后续产品面的共享事实层。

## 数据模型

建议把 response artifact 稳定到以下结构：

- `event_id`
- `exchange_id`
- `pid`
- `target_display_name`
- `provider_hint`
- `hook_name`
- `method`
- `url`
- `status_code`
- `headers`
- `body_text`
- `body_size_bytes`
- `truncated`
- `started_at_ms`
- `completed_at_ms`
- `duration_ms`

这组字段已经与当前 `response_capture.rs` 输出基本对齐，因此本轮重点是确认它们成为对外承诺，而不是再发明一版新 schema。

## 正确性与需求映射

### 属性 1：response 采集必须与 request 形成稳定配对

- 说明：如果不能稳定配对，response artifact 的分析价值会大幅下降。
- 验证：同一 exchange 至少能在 request artifact 和 response artifact 中看到相同 `exchange_id`。

### 属性 2：response capture 不能因为单个大 body 或异常正文破坏主链路

- 说明：response 往往比 request 更大、更复杂；若没有清晰的截断策略，系统稳定性会先崩。
- 验证：大 body、不可文本化 body、错误响应都不会导致 attach 前台循环失效。

### 属性 3：response artifact 必须可供后续控制台消费

- 说明：本轮虽然不做深度 UI，但产物必须能被 local console 和后续 request inspector 复用。
- 验证：artifact 包含列表展示和基础详情所需的最小字段集合。

## 分阶段落地建议

### 阶段 1：定义和收敛现有事实面

- 产物：story design + OpenSpec change 草案。
- 验收：团队对 response schema、关联键、脱敏/截断策略和验收口径达成一致。

### 阶段 2：补齐 happy path 黑盒闭环

- 产物：真实 Node CLI 目标的 response 黑盒测试或手动验收记录。
- 验收：至少一条真实 response 能被捕获、打印摘要并写入 artifact。

### 阶段 3：补错误路径与产品接入点

- 产物：错误响应/截断响应测试，和后续 console 接入约束。
- 验收：response capture 对 local console / request inspector 的输入契约稳定。

## 风险 / 取舍

- [风险] 代码已经先于文档落地，真实行为和文档边界可能不一致。
  - 处理：本轮先围绕现有实现表面收敛承诺，不强行把 change 写得大于已有能力。

- [风险] 流式响应会诱导范围膨胀。
  - 处理：明确把“终态 response artifact”与“stream replay”分开，后者不并入本 change。

- [风险] local console 可能很快要求展示 response。
  - 处理：本轮先把 artifact 和 schema 稳住，UI 展示可以在后续 change 中增量接入。

## 开放问题

- 当前 probe 对 `fetch`、`undici`、`http/https` 的 response 观察覆盖率，是否已经足够支撑统一黑盒验收。
- 本轮是否需要把部分 provider 特有字段（如 usage、finish reason）纳入 artifact，还是保持 provider-agnostic 核心集。
- response artifact 是否需要补最小索引，以便 local console 后续读取更高效。