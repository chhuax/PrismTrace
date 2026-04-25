# add-prompt-diff 设计

日期：2026-04-25
状态：草稿

## 概览

`add-prompt-diff` 的目标，是在已经具备 session timeline 和 request inspector 的基础上，把 PrismTrace 从“能看见每次调用的事实”推进到“能看懂相邻两次调用之间 prompt 到底变了什么”。

本轮只做一个保守、可验收的第一版：

- 只比较同一 session 内相邻两次 request
- 只比较 prompt-bearing 文本，不比较完整原始 request body
- 在 request inspector 中展示上一条 request 与当前 request 的 prompt diff
- 当无法提取或不存在上一条 request 时，返回明确的不可比较状态

它不尝试解释“为什么会变化”，也不做跨 session 对比。它先回答一个更基础的问题：

`在这段连续会话里，这一次调用的 prompt 相比上一条到底增减了什么。`

## 背景

当前仓库已经完成：

- request capture 第一版
- response capture 第一版
- local console 第一版
- request inspector 第一版
- tool visibility 第一版
- session reconstruction 第一版

这意味着 PrismTrace 已经能回答：

- 发了什么 request
- 收到了什么 response
- 当时暴露了哪些 tools
- 这些调用在 session 中的前后顺序

但用户依然很难直接回答：

- 第二次调用到底比第一次多了哪些上下文
- 某个 system / instructions / messages 内容是从哪一轮开始变化的
- 模型行为变化之前，prompt 本身是不是已经明显膨胀或收缩

当前的产品短板已经从“事实没有被组织成会话”转成“会话里的变化没有被提炼成可读的比较结果”。

## 目标 / 非目标

**目标：**
- 为单次 request 生成可比较的 prompt projection
- 在同一 session 内，把当前 request 与上一条 request 做 prompt diff
- 在 request inspector 中展示 diff 摘要和原始比较文本
- 当 diff 不可用时，返回明确的原因而不是静默缺失

**非目标：**
- 不做跨 session 或任意两条 request 的自由对比
- 不做 tool visibility diff
- 不做 response diff
- 不做 failure attribution 或“为什么变化”的解释
- 不做 provider-specific 深度语义归因

## 方案概览

### 1. 先生成 prompt projection，再做 diff

本轮不直接对完整 request body 做原样文本 diff，而是先从 request body 中提取“真正面向模型的 prompt-bearing 文本”，形成一个稳定的 `prompt_projection`。

第一版 projection 优先覆盖常见、通用的字段：

- `system`
- `instructions`
- `messages[*].content`
- `input`
- 内容数组中的 `type=text` / `text` 文本块

明确不纳入 projection 的内容：

- tools / functions 定义
- sampling 参数
- headers
- response_format
- 其他非 prompt-bearing 元数据

这样做的原因是：

- 用户问“prompt 怎么变了”，本质上更关心进入模型上下文的文本，而不是整个 JSON 请求体
- 直接 diff 原始 request body，往往会被 tools、参数和无关字段噪音淹没
- projection 可以为后续的 tool visibility diff 和更强分析层预留干净边界

### 2. 只比较同一 session 内相邻 request

第一版 prompt diff 固定使用：

- 当前 request 所在 session
- 时间上紧邻当前 request 的上一条 request

不提供自由选取任意两条 request 的能力。

采用这个边界，是因为：

- 当前 session reconstruction 已能稳定给出时间顺序
- “与上一条相比”是最自然、最容易理解的产品语义
- 能避免本轮扩散到复杂对比器 UI、pair 选择器和跨 session 比较语义

### 3. diff 结果挂在 request inspector 中

本轮不新建独立的“Diff 页面”，而是在已有 request inspector 中新增 `Prompt Diff` 区块。

每次打开某条 request detail 时，控制台可展示：

- 当前 request 是否有上一条可比较 request
- 上一条 request 的 `request_id`
- diff 状态
- 当前 projection 摘要
- 上一版 projection 摘要
- unified diff 文本

这样做的好处是：

- 用户查看一条 request 时，天然就会追问“它和上一条有什么不同”
- 继续复用已有 request inspector 路径，不需要额外扩张导航结构
- 路径最短，且与 session timeline 的“点开一条 request 深入看”形成自然闭环

### 4. 不可比较状态必须显式可见

prompt diff 第一版至少要有三类状态：

- `available`
- `no_previous_request`
- `unavailable_projection`

其中 `unavailable_projection` 适用于：

- request body 为空
- request body 不是可解析 JSON
- request body 中没有可提取的 prompt-bearing 文本

这样用户不会把“没有 diff”误解为“没有变化”。

## 数据模型

### Prompt Projection

第一版 `PromptProjection` 至少包含：

- `status`
- `section_count`
- `text_char_count`
- `rendered_text`

其中 `rendered_text` 是按稳定格式拼装出的可比较文本，例如：

- `system`
- `instructions`
- `messages[0] user`
- `messages[1] assistant`
- `input[0]`

### Prompt Diff

第一版 `PromptDiff` 至少包含：

- `status`
- `current_request_id`
- `previous_request_id`
- `current_session_id`
- `summary`
- `current_projection`
- `previous_projection`
- `diff_text`

其中 `summary` 至少说明：

- 是否存在新增文本
- 是否存在删除文本
- 是否发生了净增或净减

## 关键设计决策

### 决策 1：只比较 prompt-bearing 文本，不比较完整 request body

- 背景：完整 body diff 会被工具定义、参数和 provider 噪音淹没
- 方案：先做 projection，再做 diff
- 取舍：会丢掉部分非文本差异，但更贴近“prompt 怎么变了”的核心问题

### 决策 2：只做相邻 request diff

- 背景：一旦支持任意 pair 对比，UI 和 API 都会迅速复杂化
- 方案：固定为当前 request 对上一条 request
- 取舍：表达力更弱，但第一版语义最稳定、最容易验收

### 决策 3：diff 作为 request inspector 的一部分，而不是独立页面

- 背景：当前控制台已经有 request detail 和 session timeline 两层浏览结构
- 方案：把 diff 放进 request inspector
- 取舍：避免页面膨胀，同时保留将来独立 diff 视图的扩展空间

## 正确性与需求映射

### 属性 1：用户能直接看见 prompt 的变化
- 验证：打开有上一条 request 的 detail 时，可见 `diff_text`

### 属性 2：用户不会把“不可比较”误解成“没有变化”
- 验证：无上一条 request 或 projection 不可提取时，返回明确状态

### 属性 3：diff 重点落在 prompt 文本，而不是无关 JSON 噪音
- 验证：projection 只包含 prompt-bearing 文本，不包含 tools 和采样参数

## 风险 / 取舍

- [风险] 不同 provider 的 body 结构差异很大
  - 处理：第一版走 provider-agnostic best-effort projection，覆盖常见通用字段；无法提取时显式降级

- [风险] projection 可能丢失一部分上下文结构
  - 处理：保留稳定 section label，使 diff 仍能定位变化位置

- [风险] 很长的 prompt 会导致 diff 文本膨胀
  - 处理：本轮先复用 capture 阶段已有截断事实，不额外承诺超长 diff 优化
