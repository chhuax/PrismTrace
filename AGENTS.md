<!-- PRAXIS_DEVOS_START -->
> This block is maintained by Praxis DevOS. Run `npx praxis-devos@latest update` to refresh it.

## Flow Selection

- 根据请求性质选择 OpenSpec flow：

### 使用 explore / propose

当请求具备以下任一特征时，必须进入 OpenSpec（从 explore 或 propose 开始）：

- 中大型改动
- 跨模块 / 跨系统变更
- 涉及接口、兼容性或架构调整
- 存在不明确需求或未收敛的 open questions
- 存在多个可选方案需要对比或决策
- 引入新能力或新 workflow

典型示例：

- “帮我加一个 X”
- “新增 Y 能力”
- “我想做一套 Z workflow”
- “implement feature X”
- “add a release kit”

要求：

- 在 explore / propose 阶段完成需求澄清与方案收敛后，才能进入实现

---

### 使用 apply（直接实现）

仅当请求满足以下条件时，可以直接进入实现阶段：

- 改动范围小且局部
- 无设计歧义
- 不涉及架构或接口变化
- 不需要方案对比或前置设计

典型示例：

- “修一下这个 bug”
- “改一下这段文案”
- “update the version number”
- “fix the failing test”

---

### 使用 review flow

- 评审、审计、分析类请求应使用 review flow

---

## OpenSpec + SuperPowers Contract（简化）

- OpenSpec（explore / propose / apply / archive）是唯一对用户可见的流程层
- SuperPowers 仅作为阶段内嵌能力使用，不形成独立流程
- 所有产物必须收敛在当前 change 下，不得创建额外目录（如 `docs/superpowers/...`）
<!-- PRAXIS_DEVOS_END -->

# [项目名称]

> 请在此文件中描述项目上下文，帮助 AI 代理理解你的项目。

## 项目概述

<!-- 简要描述项目类型、核心业务、部署方式 -->

## 技术栈

<!-- 列出运行时、框架、持久层、缓存等关键技术选型 -->

## 模块结构

<!-- 列出核心模块及职责 -->

| 模块 | 职责 |
|------|------|
| | |

## 构建命令

<!-- 列出常用命令 -->

```bash
```

## 分支策略

<!-- 描述 Git 分支模型 -->

## 额外约定

<!-- 列出本项目特有的编码约定 -->
