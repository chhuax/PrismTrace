# Stitch 子会话说明稿：Observer Console 信息架构定稿

日期：2026-04-26  
状态：待用于 Stitch 子会话  
目的：先定信息架构，不重做整套视觉

## 1. 背景

PrismTrace 原先有一版控制台设计，心智更偏 `attach-first`：

- 先找可 attach 的目标
- 再 attach
- 再看 request / response

但现在产品路线已经调整：

- `Codex` 已经证实不能走 attach
- `opencode` 也转向 observer 方式
- 当前产品主线应改为 `multi-source observer-first`

因此，这次给 Stitch 的任务不是“重新画一套 UI 风格”，而是：

`在保留现有控制台视觉语言的前提下，把信息架构从 attach-first 改成 observer-first。`

## 2. 设计目标

这次子会话只解决一个核心问题：

`PrismTrace 控制台顶层信息架构应该怎样组织，才能统一承载 Codex 与 opencode 这两类 observer source。`

需要做到：

- `Codex` 和 `opencode` 共用一套 UI 壳
- 控制台不再以 attach 作为主叙事
- 控制台能统一展示 `source / event / session / inspector`
- 保留当前 Stitch 视觉方向，不推翻整体气质

## 3. 当前实现约束

请基于这些真实约束出稿，不要按理想化全新产品重做：

- 当前控制台已经有一版三栏布局
- 当前控制台已有这些主要区域：
  - `Sources`
  - `Activity`
  - `Sessions`
  - `Events`
  - `Timeline`
  - `Inspector`
  - `Observability Health`
- 现有实现文件：
  - [`crates/prismtrace-host/assets/console.html`](/Volumes/MacData/workspace/PrismTrace/crates/prismtrace-host/assets/console.html)
  - [`crates/prismtrace-host/assets/console.js`](/Volumes/MacData/workspace/PrismTrace/crates/prismtrace-host/assets/console.js)
  - [`crates/prismtrace-host/assets/console.css`](/Volumes/MacData/workspace/PrismTrace/crates/prismtrace-host/assets/console.css)

## 4. 产品语义约束

这些是必须遵守的硬约束：

1. `attach` 不再是产品主路线
2. 对 `Codex`，不允许再出现“可以 attach”的设计暗示
3. `Codex` 与 `opencode` 必须统一展示，不能做成两个专区或两套页面
4. 控制台第一版是“统一 observer 运行时观测台”，不是“调试 attach 面板”
5. 允许保留 attach 兼容信息，但只能降级为次级状态信息，不能占主叙事

## 5. 当前推荐的信息架构

请优先围绕这套结构打磨，而不是另起炉灶：

- 左侧：
  - `Sources`
  - `Activity`
  - `Sessions`
- 中间主区：
  - `Events`
- 右侧：
  - `Timeline`
  - `Inspector`
  - `Observability Health`

其中语义应统一为：

- `Sources`
  - 观测来源，不等于 attach target
  - 例如 `Codex App Server`、`opencode`
- `Events`
  - 统一事件流
  - 对 `Codex` 是 `thread / turn / item / tool / approval / hook / capability`
  - 对 `opencode` 是 `session / message / tool / snapshot`
- `Sessions`
  - 一次 observer session 或上层会话
- `Timeline`
  - 当前会话的时序视图
- `Inspector`
  - 当前选中 event 的详情视图

## 6. Stitch 需要产出的内容

本次希望 Stitch 产出的是“信息架构定稿”，不是高保真视觉扩展包。

请至少给出：

1. 顶层导航与主区块命名定稿
2. 左中右三区的职责边界
3. `Codex` / `opencode` 在同一套壳中的映射方式
4. `Inspector` 的类型切换原则
5. 空态、无数据态、source 不可用态的结构
6. 至少一张“统一 observer console 首页”稿
7. 至少一张“选中 event 后的 inspector 状态”稿

## 7. 明确不要做的事

这次子会话不要扩散到下面这些方向：

- 不重新设计品牌视觉
- 不重做颜色系统
- 不重做组件库
- 不讨论 attach 底层实现细节
- 不把 `Codex` 和 `opencode` 拆成独立产品线
- 不直接跳到分析层，如 `prompt diff`、`failure attribution`

## 8. 我们希望 Stitch 回答的问题

请让这次子会话最终给出清晰答案：

1. 当前三栏布局是否足够承载 unified observer console？
2. `Sources / Events / Sessions / Timeline / Inspector` 这一组命名是否最合适？
3. `Activity` 与 `Events` 是否需要进一步区分，还是存在重叠？
4. `Inspector` 应该是“统一详情容器”，还是拆成多种 detail mode？
5. 对第一版来说，哪些区域必须保留，哪些可以降级或延后？

## 9. 推荐结论方向

如果 Stitch 需要一个明确方向，请优先朝这个结论推进：

- 保留现有 Stitch 控制台视觉稿的大框架
- 不推翻三栏布局
- 把顶层语义改成 observer-first
- 让 `Codex` 与 `opencode` 共用一个统一控制台
- 把 attach 相关信息降到次级状态层

一句话总结：

`这次不是重画，而是把控制台从 attach-first 的监控台，校正成 observer-first 的统一运行时观测台。`
