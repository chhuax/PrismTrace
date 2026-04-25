## 概览

`add-prompt-diff` 的目标，是让 PrismTrace 能在已有 session timeline 基础上，直接回答“当前这次 request 的 prompt 相比上一条 request 有什么变化”。

本轮只交付：

- prompt projection
- 同一 session 内相邻 request 的 diff
- request inspector 中的 diff 展示

不交付：

- 任意 request pair 对比
- tool visibility diff
- failure attribution
- skill diagnostics

## 背景

当前控制台已经能展示 request detail、response detail、tool visibility 和 session timeline，但还不能把“变化”提炼成结构化结果。用户必须手工点开两条 request 再自行比较，这已经成为进入分析层前最明显的产品缺口。

## 目标 / 非目标

**目标：**
- 从 request body 提取可比较的 prompt-bearing 文本
- 用上一条 request 作为当前 request 的默认比较基线
- 在 request inspector 中展示 diff 结果和降级状态

**非目标：**
- 不做跨 session 对比
- 不做自由选取 pair 的比较器
- 不做非 prompt 字段差异分析

## 方案

### 1. prompt projection

host 从 request body 中做 best-effort 提取，优先覆盖：

- `system`
- `instructions`
- `messages[*].content`
- `input`
- `type=text` 文本块

输出一个稳定的 `rendered_text`，供 diff 使用。

### 2. diff 基线

diff 固定使用同一 session 中紧邻当前 request 的上一条 request：

- 有上一条 request 时生成 diff
- 没有上一条 request 时返回 `no_previous_request`
- projection 无法生成时返回 `unavailable_projection`

### 3. UI 接入

request inspector 新增 `Prompt Diff` 区域，至少展示：

- diff 状态
- 上一条 request 标识
- unified diff 文本

## 验证策略

- 聚焦测试覆盖 projection 提取
- 聚焦测试覆盖相邻 request diff
- 聚焦测试覆盖无上一条 request 与 projection 不可提取时的降级路径
