# Stitch 子会话说明稿：Observer Console 模块细化

日期：2026-04-26  
状态：待用于 Stitch 子会话  
目的：在 `stitch-ia-brief.md` 已确认顶层信息架构后，继续把 `Sources / Events / Sessions / Timeline / Inspector` 五个模块的页面语义、内容结构、空态和错误态细化出来

## 1. 这份 brief 要解决什么

上一份 `stitch-ia-brief.md` 已经把 Observer Console 的顶层信息架构定下来了：

- 保留三栏布局
- 不推翻现有控制台视觉
- 把整体叙事从 `attach-first` 改成 `observer-first`
- 让 `Codex` 与 `opencode` 共用一套 console 壳

这一次不再讨论“顶层结构是否成立”，而是继续往下收一层，明确：

- 五个核心模块分别回答什么问题
- 每个模块在第一页应该展示哪些信息
- `Codex` 与 `opencode` 如何投影到同一模块
- 模块级空态、无选择态、source 不可用态、加载失败态应该怎么表达

一句话说，这份 brief 的目标是：

`把统一 observer console 的模块页定义清楚，让 Stitch 可以直接产出模块级页面稿，而不需要再自行猜业务语义。`

## 2. 范围与边界

本次只细化这些模块：

- `Sources`
- `Events`
- `Sessions`
- `Timeline`
- `Inspector`

本次不展开：

- 不重做品牌视觉
- 不改变三栏布局
- 不扩写 attach 底层实现
- 不增加分析层能力，如 `prompt diff`、`failure attribution`
- 不把 `Codex` 和 `opencode` 拆成两套独立页面

说明：

- 左栏中的 `Activity` 可以继续保留，但它在这一轮仍是次级辅助区，不是这份 brief 的重点模块
- `Observability Health` 继续作为右栏辅助状态区存在，但不与五个主模块抢主叙事

## 3. 模块细化时必须遵守的产品语义

1. `Sources` 表示观测来源，不表示“可 attach 目标”
2. `Events` 是统一事件流，不是 HTTP request 列表的简单改名
3. `Sessions` 表示 observer session 或上层会话边界，不要求与 attach session 一一对应
4. `Timeline` 表示当前选中 session 的时序展开，不是全局 activity feed 的重复
5. `Inspector` 是统一详情容器，但内容可以随选中对象类型自适应
6. 对 `Codex`，不能出现任何“可 attach”“等待 attach”的误导性暗示
7. 所有模块都必须允许部分降级：某个 source 不可用，不应拖垮整页

## 4. 统一状态语言

为避免 Stitch 在每个模块里各自发明状态，先统一四类状态：

### 4.1 空态

含义：系统正常，但当前没有可展示数据。

适用场景：

- 第一次启动，还没有任何 observer event
- 当前过滤条件下没有匹配记录
- 某个 source 已接入，但当前没有会话或事件

表达原则：

- 解释“为什么没有”
- 说明“下一步会在什么情况下出现内容”
- 不把空态写成错误

### 4.2 无选择态

含义：页面有数据，但右侧或下级模块依赖用户先选中一个 source / session / event。

适用场景：

- `Timeline` 尚未选中 session
- `Inspector` 尚未选中 event

表达原则：

- 强调“请选择什么”
- 可附带一句提示，说明选择后会看到哪些内容

### 4.3 source 不可用态

含义：source 概念存在，但当前连接失败、离线、权限不足或暂时未发现。

适用场景：

- `Codex App Server` 当前不可连接
- `opencode` source 当前没有活跃 runtime

表达原则：

- 这是 source 级状态，不等于全站错误
- 需要告诉用户是“该 source 不可用”，不是“整个控制台坏了”
- 应保留该 source 卡片/入口，让用户理解它的存在与当前状态

### 4.4 加载失败态

含义：模块本身的数据请求或渲染失败。

适用场景：

- 模块 API 返回错误
- 前端刷新局部数据失败

表达原则：

- 使用明确错误文案
- 局部失败局部展示，不做全页崩溃
- 可以提示“重试”或“稍后刷新”，但不要求在本轮设计复杂恢复流

## 5. 模块级定稿方向

### 5.1 Sources

这个模块要回答的问题是：

`我现在能观测哪些来源，它们各自是否在线、是否健康、最近有没有输出。`

推荐结构：

- 模块名称：`Sources`
- 展示对象：source card / source row
- 推荐排序：活跃 source 在前，最近有事件的 source 优先

每个 source 至少展示：

- `display name`
  - 例如 `Codex App Server`、`opencode`
- `source type`
  - 例如 `official observer`、`runtime observer`
- `availability`
  - 如 `active`、`idle`、`unavailable`
- 最近事件时间或最近会话
- 一条简短状态摘要
  - 例如“正在接收 turn / tool 事件”
  - 或“已发现 source，但当前没有活跃会话”

可选辅助信息：

- 当前会话数
- 最近事件数
- 当前 capability snapshot 是否可见
- 最近错误摘要

`Codex` / `opencode` 映射要求：

- `Codex` 展示为 `Codex App Server`
- `opencode` 展示为统一 observer source，而不是“另一套专区”
- source 卡片的字段结构应尽量一致，不因来源不同而重做组件

空态建议：

- `尚未发现可观测 source`
- 辅助说明：`当 Codex App Server 或其他 observer source 可用时，这里会出现来源入口。`

source 不可用态建议：

- 标题：`Source 当前不可用`
- 说明：`已识别到该来源，但当前无法建立稳定观测连接。`

加载失败态建议：

- `Sources 加载失败：暂时无法读取来源状态`

### 5.2 Events

这个模块要回答的问题是：

`刚刚发生了什么，系统最近吐出了哪些关键事件。`

推荐结构：

- 模块名称：`Events`
- 形式：统一事件流列表
- 默认行为：列表是全局事件视图；若选中了 source 或 session，可自动收窄上下文

每条 event 至少展示：

- `Kind`
  - 例如 `thread`、`turn`、`item`、`tool`、`approval`、`hook`、`message`、`snapshot`
- `Summary`
- `Source`
- `Time`
- `Status`
  - 例如 `observed`、`completed`、`waiting approval`、`failed`

推荐的次级辅助信息：

- `thread_id / turn_id / session_id` 的轻量引用
- 当前 event 是否关联 tool result / approval / raw payload

`Codex` / `opencode` 映射要求：

- `Codex`
  - 主要映射到 `thread / turn / item / tool / approval / hook / capability`
- `opencode`
  - 主要映射到 `session / message / tool / snapshot`
- 即使字段来源不同，也必须能收敛成统一列表项：`Source + Kind + Summary + Time + Status`

交互原则：

- 点选 event 后，右侧 `Inspector` 切到 event detail
- 如果当前已有选中 session，点击 event 不应打断 session 上下文，只是更新 detail

空态建议：

- `尚无事件记录`
- 辅助说明：`当 observer source 开始产出 thread、tool、message 等事件后，这里会按时间顺序出现。`

加载失败态建议：

- `Events 加载失败：暂时无法读取事件流`

### 5.3 Sessions

这个模块要回答的问题是：

`当前有哪些可浏览的观测会话，它们各自覆盖了什么时间窗口和运行片段。`

推荐结构：

- 模块名称：`Sessions`
- 形式：session list
- 默认排序：最近活跃优先

每个 session 至少展示：

- `source name`
- `session title`
  - 可以是 source + 时间窗口，也可以是更具体的线程/任务摘要
- `started_at -> completed_at`
- `event count`
- 一条状态摘要
  - 例如“包含 14 个 observer events”
  - 或“当前仍在进行中”

推荐的次级辅助信息：

- 关联 `thread_id`
- 关联 `turn_count`
- 是否包含 approval/tool activity

交互原则：

- 点选 session 后，右侧 `Timeline` 切到该 session 的时序视图
- `Sessions` 负责会话边界选择，不承担 event detail 的深度职责

空态建议：

- `尚无会话记录`
- 辅助说明：`当 source 产生可归档的 observer session 后，这里会显示时间窗口和事件规模。`

source 不可用态建议：

- `该来源当前没有可读取的会话`
- 辅助说明：`来源已存在，但当前没有活跃或已落盘的 session。`

加载失败态建议：

- `Sessions 加载失败：暂时无法读取会话列表`

### 5.4 Timeline

这个模块要回答的问题是：

`在当前选中的 session 内，事件是按什么顺序推进的。`

推荐结构：

- 模块名称：`Timeline`
- 形式：当前 session 的时序列表
- 数据来源：依赖 `Sessions` 的当前选中项

每个 timeline item 至少展示：

- `Summary`
- `Time range` 或单点时间
- `Kind`
- `Status`
- `Source`

推荐的次级辅助信息：

- 是否关联 tool / approval
- 当前 item 属于哪个 thread / turn
- 是否可继续钻取到 Inspector

`Codex` / `opencode` 映射要求：

- `Codex`
  - 体现 `thread -> turn -> item/tool/approval/hook` 的推进节奏
- `opencode`
  - 体现 `session -> message/tool/snapshot` 的推进节奏
- 时间线可使用统一列表样式，但允许在视觉上给不同 kind 做轻量标签区分

无选择态建议：

- `请选择一个 session 查看 timeline`
- 辅助说明：`选中左侧会话后，这里会展开该会话的事件顺序与关键节点。`

空态建议：

- `当前 session 尚无 timeline item`

source 不可用态建议：

- `该会话的时间线暂不可用`
- 辅助说明：`会话已被识别，但当前无法读取完整事件顺序。`

加载失败态建议：

- `Timeline 加载失败：暂时无法读取会话时序`

### 5.5 Inspector

这个模块要回答的问题是：

`当前选中的对象具体是什么，它有哪些关键字段，原始内容是否可回看。`

推荐结构：

- 模块名称：`Inspector`
- 角色：统一详情容器
- detail mode：按选中对象类型切换，而不是按 source 品牌切换

第一版建议支持三类 detail mode：

- `event detail`
  - 默认最重要
- `session detail`
  - 作为 timeline 上下文补充
- `source detail`
  - 可选，用于解释 source 状态与能力快照

其中 `event detail` 建议优先展示：

- `Summary`
- `Source / Channel`
- `Kind / Status`
- `Time`
- `thread_id / turn_id / item_id`
- 关联 session 或 hook 信息
- 原始 JSON 或最小 raw payload 区

Inspector 原则：

- 壳统一
- 内部 section 可随类型变化
- 不为 `Codex` 与 `opencode` 设计两套完全不同的 detail 页面

推荐 section 结构：

1. `Overview`
2. `Context`
3. `Payload / Raw Event`
4. `Related Links`
   - 如“跳到 session”“定位到 timeline item”

无选择态建议：

- `请选择一条 event 查看详情`
- 辅助说明：`选中事件后，这里会显示摘要、上下文和原始内容。`

空态建议：

- `当前对象暂无更多详情`

source 不可用态建议：

- `详情暂不可读取`
- 辅助说明：`基础摘要仍可展示，但更深层内容当前不可用。`

加载失败态建议：

- `Inspector 加载失败：暂时无法读取详情内容`

## 6. 五个模块之间的联动关系

为了避免 Stitch 在稿面上把联动关系画乱，这里先固定：

1. `Sources` 决定当前观察的是哪个来源范围
2. `Sessions` 决定当前 timeline 所属的会话范围
3. `Events` 决定当前 inspector 的主要详情对象
4. `Timeline` 是 session 内部视角，不替代全局 `Events`
5. `Inspector` 永远是右侧统一详情区，不另起浮层或独立页面

推荐默认链路：

- 用户先看 `Sources`
- 再扫 `Events`
- 如需理解完整上下文，再点 `Sessions`
- 通过 `Timeline` 理解顺序
- 最后在 `Inspector` 查看详情

## 7. Stitch 本次最好直接产出的页面

希望 Stitch 这次至少给出以下页面或状态稿：

1. `Sources` 非空态
2. `Sources` source 不可用态
3. `Events` 非空态
4. `Events` 空态
5. `Sessions + Timeline` 联动态
6. `Timeline` 无选择态
7. `Inspector` event detail 态
8. `Inspector` 无选择态
9. 一个“部分 source 不可用但页面整体可用”的组合态

## 8. 推荐结论

如果 Stitch 需要明确方向，请优先朝这个定稿方向推进：

- `Sources` 负责“来源可见性”
- `Events` 负责“全局事件事实”
- `Sessions` 负责“会话边界”
- `Timeline` 负责“当前会话顺序”
- `Inspector` 负责“统一详情”

同时，所有模块都遵守同一条降级原则：

`局部缺失、局部报错、局部 source 不可用，都不应把整个 observer console 打成不可用。`

一句话总结：

`这份模块 brief 的目标，不是发明更多页面，而是让现有三栏 observer console 的每一块都说清楚自己在回答什么问题。`

## 9. 可直接提交给 Stitch 的设计任务

下面这段可以直接作为 Stitch 子会话输入，目标是让它基于现有控制台视觉继续细化模块页，而不是重新发明一套产品。

```text
请基于已有的 PrismTrace Local Console 视觉方向，继续细化一套 observer-first 的模块级页面设计。

这次不要重做品牌视觉，也不要推翻现有三栏布局。请在现有设计语言上继续工作，把控制台从旧的 attach-first 语义，调整成统一 observer console。

设计目标：
- 统一展示 Codex 和 opencode
- 不为 Codex 和 opencode 分别设计两套页面
- 保持三栏布局、顶部导航、左侧导航、右侧 inspector 的整体结构
- 强化 observer-first 语义，而不是 attach target 语义

请重点设计以下五个模块：
- Sources
- Events
- Sessions
- Timeline
- Inspector

这五个模块分别回答：
- Sources: 我现在能观测哪些来源，它们是否在线、是否健康、最近有没有输出
- Events: 最近发生了什么，系统刚刚吐出了哪些关键事件
- Sessions: 当前有哪些可浏览的观测会话，它们覆盖了什么时间窗口和运行片段
- Timeline: 当前选中 session 内，事件是按什么顺序推进的
- Inspector: 当前选中的对象具体是什么，它有哪些关键字段，原始内容是否可回看

统一产品语义要求：
- Sources 表示观测来源，不表示“可 attach 目标”
- Events 是统一事件流，不是 HTTP request 列表的简单改名
- Sessions 表示 observer session 或上层会话边界，不要求与 attach session 一一对应
- Timeline 是当前 session 的时序展开，不是全局 activity feed 的重复
- Inspector 是统一详情容器，但内容可以随选中对象类型自适应
- 对 Codex 不能出现任何“等待 attach”“可 attach”的误导
- 所有模块都必须支持局部降级：某个 source 不可用，不应拖垮整个页面

Codex / opencode 统一映射要求：
- Codex 主要映射到 thread / turn / item / tool / approval / hook / capability
- opencode 主要映射到 session / message / tool / snapshot
- 即使事件字段不同，也必须投影到同一套 UI 壳：Source + Kind + Summary + Time + Status

统一状态要求：
- 空态：系统正常，但当前没有数据；要解释为什么没有，不要写成错误
- 无选择态：例如 Timeline 未选中 session、Inspector 未选中 event；要明确提示用户先选什么
- source 不可用态：说明是某个 source 当前离线或连接失败，不是整个控制台坏了
- 加载失败态：局部失败局部展示，不做全页崩溃

请至少产出以下页面或状态稿：
1. Sources 非空态
2. Sources source 不可用态
3. Events 非空态
4. Events 空态
5. Sessions + Timeline 联动态
6. Timeline 无选择态
7. Inspector event detail 态
8. Inspector 无选择态
9. 一个“部分 source 不可用但页面整体可用”的组合态

设计偏好：
- 保留现有 PrismTrace Local Console 的视觉风格
- 不新增独立的 Codex 专区或 opencode 专区
- 尽量用统一组件系统覆盖不同 source
- 允许通过标签、状态色、轻量图标区分事件类型，但不要把页面做成复杂分析台

最终希望看到：
- 模块间信息架构清晰
- 空态、错误态、无选择态完整
- 同一套 observer console 能自然容纳 Codex 和 opencode
- 页面语义已经彻底摆脱 attach-first 心智
```
