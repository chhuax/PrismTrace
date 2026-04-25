## 变更概述

`tool-visibility` 能力为 PrismTrace 增加“单次 request 最终暴露给模型的工具集合”这一层事实，使控制台能够在 request / response 之外，再展示与该次 request 相关的 tool visibility。

本次变更只要求收口 request-embedded visibility，不要求完成多阶段 visibility 或解释层。

## ADDED Requirements

### Requirement: host 必须在可见时持久化 request-embedded tool visibility
PrismTrace host MUST 在已捕获 request payload 中可见 `tools`、`functions` 或 `tool_choice` 时，生成对应的 tool visibility artifact，而不是仅把这部分信息埋在原始 request body 文本里。

#### Scenario: request payload 包含 tools 数组
- **WHEN** host 捕获到一条 request，且其 `body_text` 中包含 `tools` 数组
- **THEN** host 写入一条 tool visibility artifact，并记录 `final_tools_json`、`tool_count_final` 与 `visibility_stage = request-embedded`

#### Scenario: request payload 包含 functions 数组
- **WHEN** host 捕获到一条 request，且其 `body_text` 中包含 `functions` 数组
- **THEN** host 写入一条 tool visibility artifact，并将该数组视为最终暴露给模型的工具集合之一部分

#### Scenario: request payload 不包含任何 tool visibility 线索
- **WHEN** host 捕获到一条 request，但其 `body_text` 中既不包含 `tools`、也不包含 `functions`、也不包含 `tool_choice`
- **THEN** host 不写入 tool visibility artifact

### Requirement: request inspector 必须展示 matching tool visibility
PrismTrace local console MUST 在单条 request 详情中展示与该 request 对应的 tool visibility detail，使用户可以直接看到 final tools、tool choice 与基础摘要。

#### Scenario: request 存在 matching visibility artifact
- **WHEN** 用户在控制台中打开一条已捕获 request，且该 request 已关联 visibility artifact
- **THEN** 控制台详情中展示 visibility stage、tool count、tool choice 与 final tools 摘要

#### Scenario: request 不存在 visibility artifact
- **WHEN** 用户在控制台中打开一条未关联 visibility artifact 的 request
- **THEN** 控制台仍展示 request detail，并对 tool visibility 区域给出明确空态说明

## 非目标与边界

- 本次不要求展示 candidate / filtered / final 多阶段 visibility。
- 本次不要求解释为什么某个 tool 没有出现。
- 本次不要求完成跨 request 的 visibility diff 或 session timeline 关联。
